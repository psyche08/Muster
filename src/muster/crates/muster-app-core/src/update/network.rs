//! Devices, the network, administration, the browser and the desktop app.

use super::*;

pub(super) fn browser_result(response: Resp) -> Result<v1::OpResult, Error> {
    match response {
        Resp::OpResult(r) => Ok(r),
        _ => Err(unexpected()),
    }
}

pub(super) fn browser_setup(state: &mut State, ctx: &Ctx, install: bool) {
    if state.browser.busy || state.browser.device.is_empty() {
        return;
    }
    if install && !state.browser.local {
        state.toast("Install the extension on the target Mac", true);
        return;
    }
    state.browser.busy = true;
    let device = state.browser.device.clone();
    let self_id = state.self_id.clone();
    let backend = ctx.backend.clone();
    spawn(ctx, async move {
        let result = async {
            let client = backend.client_for(&device, &self_id).await?;
            match client
                .call(Req::BrowserSetup(v1::BrowserSetupRequest { install }))
                .await?
            {
                Resp::BrowserStatus(s) => Ok(s),
                _ => Err(unexpected()),
            }
        }
        .await;
        Msg::BrowserStatus { device, result }
    });
}

/// Runs a network/policy request on `device`; remote admin requests go
/// through local authentication first (a presence proof).
pub(super) fn admin_request(
    state: &mut State,
    ctx: &Ctx,
    what: &'static str,
    device: String,
    req: Req,
) {
    let self_id = state.self_id.clone();
    let remote = !state.is_self(&device);
    if remote && musterd::handler::needs_presence(&req) {
        let ctx_backend = ctx.backend.clone();
        spawn(ctx, async move {
            match ctx_backend.client_for(&device, &self_id).await {
                Ok(client) => {
                    let call_id = client.next_id();
                    let message = muster_proto::presence_message(call_id, &req);
                    Msg::PresenceNeeded {
                        response_origin: None,
                        what,
                        device,
                        call_id,
                        request: Box::new(req),
                        message,
                    }
                }
                Err(error) => Msg::Done {
                    what,
                    result: Err(error),
                },
            }
        });
        return;
    }
    state.busy += 1;
    let backend = ctx.backend.clone();
    spawn(ctx, async move {
        let result = async {
            let client = backend.client_for(&device, &self_id).await?;
            match unary(&client, req).await? {
                Resp::NetworkStatus(status) => Ok(Some(status)),
                Resp::Ack(_) => Ok(None),
                Resp::Policy(_) | Resp::OpResult(_) => Ok(None),
                _ => Err(unexpected()),
            }
        }
        .await;
        Msg::Done { what, result }
    });
}

pub(super) fn load_basics(state: &mut State, ctx: &Ctx) {
    let local = ctx.backend.local();
    let c = local.clone();
    spawn(ctx, async move {
        Msg::Info(
            match unary(&c, Req::DeviceInfo(v1::DeviceInfoRequest {})).await {
                Ok(Resp::DeviceInfo(info)) => Ok(info),
                Ok(_) => Err(unexpected()),
                Err(e) => Err(e),
            },
        )
    });
    let c = local.clone();
    spawn(ctx, async move {
        match unary(&c, Req::NetworkStatus(v1::NetworkStatusRequest {})).await {
            Ok(Resp::NetworkStatus(status)) => Msg::Network(status),
            other => Msg::Done {
                what: "status",
                result: other.map(|_| None),
            },
        }
    });
    load_pending(ctx);
    let c = local;
    spawn(ctx, async move {
        Msg::Policy(
            match unary(&c, Req::PolicyGet(v1::PolicyGetRequest {})).await {
                Ok(Resp::Policy(policy)) => Ok(policy),
                Ok(_) => Err(unexpected()),
                Err(e) => Err(e),
            },
        )
    });
    if state.device_watch.is_none() {
        let (watch, token) = state.new_watch();
        state.device_watch = Some(watch);
        let tx = ctx.tx.clone();
        let local = ctx.backend.local();
        tokio::spawn(async move {
            loop {
                let Ok(mut call) = local
                    .start(Req::DeviceWatch(v1::DeviceWatchRequest {}))
                    .await
                else {
                    return;
                };
                loop {
                    tokio::select! {
                        _ = token.cancelled() => { call.cancel().await; return; }
                        ev = call.events.recv() => match ev {
                            Some(v1::event::Ev::NetworkStatus(s)) => { let _ = tx.send(loop_message(Msg::Network(s))); }
                            Some(v1::event::Ev::DeviceState(s)) => { let _ = tx.send(loop_message(Msg::DeviceState(s))); }
                            Some(v1::event::Ev::Policy(p)) => { let _ = tx.send(loop_message(Msg::Policy(Ok(p)))); }
                            Some(_) => {}
                            None => break,
                        }
                    }
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    }
}

pub(super) fn load_pending(ctx: &Ctx) {
    let c = ctx.backend.local();
    spawn(ctx, async move {
        match unary(&c, Req::NetworkPending(v1::NetworkPendingRequest {})).await {
            Ok(Resp::PendingJoins(p)) => Msg::Pending(p.joins),
            _ => Msg::Pending(Vec::new()),
        }
    });
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(super) fn desktop_update_intent(
    state: &mut State,
    ctx: &Ctx,
    action: app::DesktopUpdateAction,
) {
    use app::desktop_update_action::Action;
    if !state.desktop_update.supported {
        return;
    }
    match Action::try_from(action.action) {
        Ok(Action::Check) if state.desktop_update.can_check => {
            state.desktop_update.error.clear();
            state.desktop_update.status = "checking".into();
            state.desktop_update.can_check = false;
            state.desktop_update.can_install = false;
            state.desktop_update.can_download = false;
            state.update_candidate = None;
            state.update_prepared = None;
            spawn(ctx, async {
                Msg::UpdateChecked(
                    tokio::task::spawn_blocking(crate::desktop_update::check)
                        .await
                        .unwrap_or_else(|e| Err(e.to_string())),
                )
            });
        }
        Ok(Action::Download) if state.desktop_update.can_download => {
            let Some(candidate) = state.update_candidate.take() else {
                return;
            };
            state.desktop_update.status = "downloading".into();
            state.desktop_update.can_check = false;
            state.desktop_update.can_download = false;
            spawn(ctx, async {
                Msg::UpdateDownloaded(
                    tokio::task::spawn_blocking(move || candidate.download())
                        .await
                        .unwrap_or_else(|e| Err(e.to_string())),
                )
            });
        }
        Ok(Action::Install) if state.desktop_update.can_install => {
            if !state.can_restart_desktop() {
                state.desktop_update.error =
                    "请先结束当前发送、终端连接或 Locked Use，再安装更新。".into();
                return;
            }
            state.desktop_update.error.clear();
            state.desktop_update.status = "waiting".into();
            state.desktop_update.can_check = false;
            state.desktop_update.can_install = false;
            let client = ctx.backend.local();
            spawn(ctx, async move {
                Msg::UpdateLease(crate::desktop_update::Lease::acquire(client).await)
            });
        }
        _ => {}
    }
}

pub(super) fn intent_open_browser(state: &mut State, ctx: &Ctx, open: app::OpenBrowser) {
    if state.browser.busy || state.presence.is_some() {
        state.toast("Wait for the current operation", true);
        return;
    }
    let devices = state.service_devices();
    let wanted = if open.device.is_empty() {
        state.target()
    } else {
        open.device
    };
    let matches: Vec<_> = devices
        .iter()
        .filter(|m| m.device_id == wanted || m.name == wanted)
        .collect();
    if matches.len() != 1 {
        state.toast("Choose one desktop device for the browser", true);
        return;
    }
    let member = matches[0];
    state.browser = app::BrowserView {
        device: member.device_id.clone(),
        local: member.is_self,
        draft: if open.draft.is_empty() {
            "{\"op\":\"tabs\"}".into()
        } else {
            open.draft
        },
        ..Default::default()
    };
    state.go(Screen::Browser);
    browser_setup(state, ctx, false);
}

pub(super) fn intent_browser_run(state: &mut State, ctx: &Ctx, r: app::BrowserRun) {
    if state.browser.busy || state.presence.is_some() || !state.foreground {
        return;
    }
    if let Err(error) = musterd::browser::parse(&r.json) {
        state.toast(error.to_string(), true);
        return;
    }
    state.browser.draft = r.json.clone();
    state.browser.result.clear();
    state.browser.busy = true;
    let device = state.browser.device.clone();
    let remote = !state.is_self(&device);
    let self_id = state.self_id.clone();
    let backend = ctx.backend.clone();
    let request = Req::BrowserCall(v1::BrowserCallRequest {
        op_id: state.op_id(),
        json: r.json,
    });
    spawn(ctx, async move {
        let result = async {
            let client = backend.client_for(&device, &self_id).await?;
            if remote {
                let call_id = client.next_id();
                return Ok(Msg::PresenceNeeded {
                    response_origin: None,
                    what: "browser",
                    device: device.clone(),
                    call_id,
                    message: muster_proto::presence_message(call_id, &request),
                    request: Box::new(request),
                });
            }
            let response = client.call(request).await?;
            Ok(Msg::BrowserResult {
                device: device.clone(),
                result: browser_result(response),
            })
        }
        .await;
        result.unwrap_or_else(|error| Msg::BrowserResult {
            device,
            result: Err(error),
        })
    });
}

pub(super) fn intent_create_network(state: &mut State, ctx: &Ctx, c: app::CreateNetwork) {
    admin_request(
        state,
        ctx,
        "create",
        String::new(),
        Req::NetworkCreate(v1::NetworkCreateRequest {
            name: c.name,
            vnet_cidr: c.vnet_cidr,
        }),
    );
}

pub(super) fn intent_show_invite(state: &mut State, ctx: &Ctx, i: app::ShowInvite) {
    if !state.can_invite() {
        state.toast("Only an admin device can invite", true);
        return;
    }
    state.invite = None;
    state.go(Screen::Invite);
    let c = ctx.backend.local();
    spawn(ctx, async move {
        Msg::Invite(
            match unary(
                &c,
                Req::NetworkInvite(v1::NetworkInviteRequest {
                    ttl_secs: i.ttl_secs,
                }),
            )
            .await
            {
                Ok(Resp::InviteText(t)) => Ok(t),
                Ok(_) => Err(unexpected()),
                Err(e) => Err(e),
            },
        )
    });
}

pub(super) fn intent_join_network(state: &mut State, ctx: &Ctx, j: app::JoinNetwork) {
    if j.invite_text.trim().is_empty() {
        effect(
            state,
            ctx,
            EffectKind::ScanQr,
            app::effect::Ef::ScanQr(app::ScanQr {}),
        );
        return;
    }
    state.go(Screen::Join);
    state.join = app::JoinView {
        phase: "connecting".into(),
        message: String::new(),
    };
    let tx = ctx.tx.clone();
    let local = ctx.backend.local();
    tokio::spawn(async move {
        let result = async {
            let mut call = local
                .start(Req::NetworkJoin(v1::NetworkJoinRequest {
                    invite_text: j.invite_text.trim().to_string(),
                    device_name: j.device_name,
                    device_kind: j.device_kind,
                }))
                .await?;
            while let Some(ev) = call.events.recv().await {
                if let v1::event::Ev::JoinProgress(p) = ev {
                    let phase = match v1::join_progress::Phase::try_from(p.phase) {
                        Ok(v1::join_progress::Phase::Connecting) => "connecting",
                        Ok(v1::join_progress::Phase::WaitingApproval) => "waiting_approval",
                        Ok(v1::join_progress::Phase::Joined) => "joined",
                        _ => "failed",
                    };
                    let _ = tx.send(loop_message(Msg::JoinProgress(phase.into(), p.message)));
                }
            }
            match call.finish().await? {
                Resp::NetworkStatus(s) => Ok(s),
                _ => Err(unexpected()),
            }
        }
        .await;
        let _ = tx.send(loop_message(Msg::JoinDone(result)));
    });
}

pub(super) fn intent_decide_join(state: &mut State, ctx: &Ctx, d: app::DecideJoin) {
    admin_request(
        state,
        ctx,
        "decide",
        String::new(),
        Req::NetworkDecide(v1::NetworkDecideRequest {
            device_id: d.device_id,
            approve: d.approve,
            scopes: d.scopes,
            reason: d.reason,
        }),
    )
}

pub(super) fn intent_remove_device(state: &mut State, ctx: &Ctx, r: app::RemoveDevice) {
    admin_request(
        state,
        ctx,
        "remove",
        String::new(),
        Req::NetworkRemove(v1::NetworkRemoveRequest {
            device_id: r.device_id,
        }),
    )
}

pub(super) fn intent_set_scopes(state: &mut State, ctx: &Ctx, s: app::SetScopes) {
    admin_request(
        state,
        ctx,
        "scopes",
        String::new(),
        Req::NetworkSetScopes(v1::NetworkSetScopesRequest {
            device_id: s.device_id,
            scopes: s.scopes,
            role: s.role,
        }),
    )
}

pub(super) fn intent_set_policy(state: &mut State, ctx: &Ctx, p: app::SetPolicy) {
    let device = state.target();
    admin_request(
        state,
        ctx,
        "policy",
        device,
        Req::PolicySet(v1::PolicySetRequest { policy: p.policy }),
    );
}

pub(super) fn intent_select_device(state: &mut State, s: app::SelectDevice) {
    let known = state.is_self(&s.device_id)
        || state
            .network
            .as_ref()
            .is_some_and(|n| n.members.iter().any(|m| m.device_id == s.device_id));
    if !known {
        state.toast("unknown device", true);
        return;
    }
    state.selected = if state.is_self(&s.device_id) {
        String::new()
    } else {
        s.device_id
    };
    state.go(Screen::Device);
}

pub(super) fn intent_set_cover(state: &mut State, ctx: &Ctx, c: app::SetCover) {
    let device = if c.device_id.is_empty() || state.is_self(&c.device_id) {
        state.self_id.clone()
    } else {
        c.device_id.clone()
    };
    if state.is_self(&device) && !c.on && state.covered() {
        // Nobody drops this desktop's cover without being here.
        effect(
            state,
            ctx,
            EffectKind::Uncover,
            app::effect::Ef::LocalAuth(app::LocalAuth {
                reason: "Uncover the screen".into(),
            }),
        );
        return;
    }
    admin_request(
        state,
        ctx,
        "cover",
        device,
        Req::DeviceSetCover(v1::DeviceSetCoverRequest { on: c.on }),
    );
}

pub(super) fn intent_desktop_update_action(
    state: &mut State,
    ctx: &Ctx,
    action: app::DesktopUpdateAction,
) {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    desktop_update_intent(state, ctx, action);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = (state, ctx, action);
}

pub(super) fn intent_desktop_setup_action(
    state: &mut State,
    ctx: &Ctx,
    action: app::DesktopSetupAction,
) {
    if state.embedded
        || !state
            .self_info
            .as_ref()
            .is_some_and(|i| i.platform == "macos")
    {
        return;
    }
    match app::desktop_setup_action::Action::try_from(action.action) {
        Ok(app::desktop_setup_action::Action::Restart) => {
            if !state.can_restart_desktop() {
                state.desktop_setup_error =
                    "请先结束当前发送、终端连接或 Locked Use，再重启。".into();
                return;
            }
            state.desktop_setup_error.clear();
            effect(
                state,
                ctx,
                EffectKind::DesktopHelper,
                app::effect::Ef::RestartDesktop(app::RestartDesktop {}),
            );
        }
        Ok(app::desktop_setup_action::Action::Check) => {
            load_basics(state, ctx);
        }
        Ok(app::desktop_setup_action::Action::Show) => state.desktop_setup_dismissed = false,
        Ok(app::desktop_setup_action::Action::Dismiss) => state.desktop_setup_dismissed = true,
        Ok(app::desktop_setup_action::Action::Install) => {
            state.desktop_setup_error.clear();
            effect(
                state,
                ctx,
                EffectKind::DesktopHelper,
                app::effect::Ef::AuthorizeDesktopHelper(app::AuthorizeDesktopHelper {}),
            );
        }
        Ok(app::desktop_setup_action::Action::Iterm) => {
            if state.busy > 0 {
                return;
            }
            state.desktop_setup_error.clear();
            state.busy += 1;
            let client = ctx.backend.local();
            spawn(ctx, async move {
                let result = unary(
                    &client,
                    Req::DeviceRequestAutomation(v1::DeviceRequestAutomationRequest {}),
                )
                .await
                .map(|_| None);
                Msg::Done {
                    what: "accessibility",
                    result,
                }
            });
            effect(
                state,
                ctx,
                EffectKind::Other,
                app::effect::Ef::OpenUrl(app::OpenUrl {
                    url:
                        "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation"
                            .into(),
                }),
            );
        }
        Err(_) => {}
    }
}

pub(super) fn intent_request_accessibility(state: &mut State, ctx: &Ctx) {
    if ctx.backend.daemon().is_some() {
        log::debug!("accessibility is a desktop matter; ignored");
        return;
    }
    state.busy += 1;
    let client = ctx.backend.local();
    spawn(ctx, async move {
        let result = async {
            unary(
                &client,
                Req::DeviceRequestAccessibility(v1::DeviceRequestAccessibilityRequest {}),
            )
            .await?;
            Ok(None)
        }
        .await;
        Msg::Done {
            what: "accessibility",
            result,
        }
    });
    // The prompt only appears once per denial; the pane is where the
    // user finishes.
    if state
        .self_info
        .as_ref()
        .is_some_and(|i| i.platform == "macos")
    {
        effect(
            state,
            ctx,
            EffectKind::Other,
            app::effect::Ef::OpenUrl(app::OpenUrl {
                url:
                    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
                        .into(),
            }),
        );
    }
}

pub(super) fn intent_set_relay(state: &mut State, ctx: &Ctx, r: app::SetRelay) {
    let relay = r.relay.trim().to_string();
    if relay.is_empty() {
        state.toast("relay: an address is required", true);
        return;
    }
    state.busy += 1;
    match ctx.backend.daemon() {
        Some(daemon) => {
            let daemon = daemon.clone();
            spawn(ctx, async move {
                let result = daemon
                    .restart_node(&relay)
                    .await
                    .map(|_| None)
                    .map_err(|e| Error::Unavailable(e.to_string()));
                Msg::Done {
                    what: "relay",
                    result,
                }
            });
        }
        None => {
            let client = ctx.backend.local();
            spawn(ctx, async move {
                let result = async {
                    unary(
                        &client,
                        Req::DeviceSetRelay(v1::DeviceSetRelayRequest { relay }),
                    )
                    .await?;
                    Ok(None)
                }
                .await;
                Msg::Done {
                    what: "relay",
                    result,
                }
            });
        }
    }
}

pub(super) fn intent_set_push_token(state: &mut State, ctx: &Ctx, t: app::SetPushToken) {
    match ctx.backend.daemon() {
        Some(daemon) => {
            if let Err(error) = daemon.set_push_token(t.kind as u8, &t.token) {
                state.toast(format!("push token: {error}"), true);
            }
        }
        None => log::debug!("push tokens are for the embedded link; ignored"),
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(super) fn msg_update_checked(
    state: &mut State,
    result: Result<Option<crate::desktop_update::Candidate>, String>,
) {
    state.desktop_update.can_check = true;
    match result {
        Ok(Some(candidate)) => {
            state.desktop_update.version = candidate.version().into();
            state.desktop_update.status = "available".into();
            state.desktop_update.can_download = true;
            state.update_candidate = Some(candidate);
        }
        Ok(None) => state.desktop_update.status = "current".into(),
        Err(e) => {
            state.desktop_update.status = "error".into();
            state.desktop_update.error = e;
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(super) fn msg_update_downloaded(
    state: &mut State,
    result: Result<crate::desktop_update::Prepared, String>,
) {
    state.desktop_update.can_check = true;
    match result {
        Ok(prepared) => {
            state.update_prepared = Some(Arc::new(prepared));
            state.desktop_update.status = "ready".into();
            state.desktop_update.can_install = true;
        }
        Err(e) => {
            state.desktop_update.status = "error".into();
            state.desktop_update.error = e;
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(super) fn msg_update_lease(
    state: &mut State,
    ctx: &Ctx,
    result: Result<crate::desktop_update::Lease, String>,
) {
    match result {
        Ok(lease) => {
            state.update_lease = Some(lease);
            state.desktop_update.status = "installing".into();
            effect(
                state,
                ctx,
                EffectKind::PrepareUpdate,
                app::effect::Ef::PrepareDesktopUpdate(app::PrepareDesktopUpdate {}),
            );
        }
        Err(error) => {
            state.desktop_update.status = "ready".into();
            state.desktop_update.error = error;
            state.desktop_update.can_check = true;
            state.desktop_update.can_install = state.update_prepared.is_some();
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(super) fn msg_update_applied(state: &mut State, ctx: &Ctx, result: Result<(), String>) {
    state.update_lease = None;
    let exit = result.is_ok();
    if let Err(e) = result {
        state.desktop_update.error = e;
        state.desktop_update.status = "error".into();
        state.desktop_update.can_check = true;
        state.desktop_update.can_install = state.update_prepared.is_some();
    }
    effect(
        state,
        ctx,
        EffectKind::Other,
        app::effect::Ef::FinishDesktopUpdate(app::FinishDesktopUpdate { exit }),
    );
}

pub(super) fn msg_info(state: &mut State, ctx: &Ctx, result: Result<v1::DeviceInfo, Error>) {
    match result {
        Ok(info) => {
            if state.self_id.is_empty() {
                state.self_id = info.device_id.clone();
            }
            if info.platform == "macos" {
                if let Some(device) = &info.state {
                    state.observe_desktop_permissions(device);
                }
            }
            let covered = info.state.as_ref().is_some_and(|s| s.covered);
            follow_cover(state, ctx, covered);
            state.self_info = Some(info);
        }
        Err(error) => state.toast(format!("daemon: {error}"), true),
    }
}

pub(super) fn msg_network(state: &mut State, ctx: &Ctx, status: v1::NetworkStatus) {
    let had = state.has_network();
    let previous_members: Vec<_> = state
        .network
        .as_ref()
        .map(|n| n.members.iter().map(|m| m.device_id.clone()).collect())
        .unwrap_or_default();
    let members_changed = previous_members
        != status
            .members
            .iter()
            .map(|m| m.device_id.clone())
            .collect::<Vec<_>>();
    // Members whose offline verdict changed: their sessions appear (after a
    // fresh load) or disappear.
    let offline = |status: Option<&v1::NetworkStatus>| -> Vec<(String, bool)> {
        status
            .map(|n| {
                n.members
                    .iter()
                    .map(|m| (m.device_id.clone(), crate::state::member_offline(m)))
                    .collect()
            })
            .unwrap_or_default()
    };
    let presence_changed = offline(state.network.as_ref()) != offline(Some(&status));
    if status.pending_joins as usize != state.pending.len() {
        load_pending(ctx);
    }
    state.network = Some(status);
    super::manual_locked_use::watch(state, ctx);
    if !state.can_invite() {
        state.invite = None;
        state.stack.retain(|screen| *screen != Screen::Invite);
        if state.screen == Screen::Invite {
            state.screen = Screen::Agents;
        }
    }
    let has = state.has_network();
    if has && !had && matches!(state.screen, Screen::Onboarding) {
        state.screen = Screen::Agents;
        state.stack.clear();
        state.agents.device_id = "*".into();
        open_agents(state, ctx);
    }
    if has && had && (members_changed || presence_changed) {
        match state.screen {
            Screen::Home | Screen::Agents => open_agents(state, ctx),
            _ if state.workspace() == Screen::Agents => load_catalog(state, ctx),
            Screen::Terminals if presence_changed => super::terminals::open_terminals(state, ctx),
            _ if presence_changed && state.workspace() == Screen::DesktopChats => {
                super::desktop_chats::load_desktop_chats(state, ctx)
            }
            _ => {}
        }
    }
    if !has && !matches!(state.screen, Screen::Onboarding | Screen::Join) {
        state.screen = Screen::Onboarding;
        state.stack.clear();
        state.selected.clear();
    }
}

pub(super) fn msg_device_state(state: &mut State, ctx: &Ctx, s: v1::DeviceState) {
    if state
        .self_info
        .as_ref()
        .is_some_and(|i| i.platform == "macos")
    {
        state.observe_desktop_permissions(&s);
    }
    let covered = s.covered;
    if let Some(info) = &mut state.self_info {
        info.state = Some(s);
    }
    follow_cover(state, ctx, covered);
}

pub(super) fn msg_pending(state: &mut State, ctx: &Ctx, p: Vec<v1::PendingJoin>) {
    // A device that just scanned the invite: the admin is usually
    // still looking at the QR code, so bring the approval into view
    // and say who is waiting.
    let known: std::collections::HashSet<&str> =
        state.pending.iter().map(|j| j.device_id.as_str()).collect();
    let new: Vec<_> = p
        .iter()
        .filter(|j| !known.contains(j.device_id.as_str()))
        .cloned()
        .collect();
    state.pending = p;
    for join in new {
        let name = if join.name.is_empty() {
            join.fingerprint.clone()
        } else {
            join.name.clone()
        };
        state.toast(format!("{name} wants to join; approve it on Home"), false);
        effect(
            state,
            ctx,
            EffectKind::Other,
            app::effect::Ef::Notify(app::Notify {
                title: "Join request".into(),
                body: format!("{name} ({}) is waiting for approval", join.fingerprint),
                session: String::new(),
            }),
        );
    }
    if !state.pending.is_empty() && matches!(state.screen, Screen::Invite) {
        state.back();
    }
}

pub(super) fn msg_browser_status(
    state: &mut State,
    device: String,
    result: Result<v1::BrowserStatus, Error>,
) {
    if state.browser.device != device {
        return;
    }
    state.browser.busy = false;
    match result {
        Ok(status) => state.browser.status = Some(status),
        Err(error) => state.browser.result = error.to_string(),
    }
}

pub(super) fn msg_browser_result(
    state: &mut State,
    device: String,
    result: Result<v1::OpResult, Error>,
) {
    if state.browser.device != device {
        return;
    }
    state.browser.busy = false;
    state.browser.result = match result {
        Ok(r)
            if matches!(
                v1::op_result::Status::try_from(r.status),
                Ok(v1::op_result::Status::Applied | v1::op_result::Status::Duplicate)
            ) =>
        {
            r.result_id
        }
        Ok(r) => format!(
            "{:?}: {}",
            v1::op_result::Status::try_from(r.status),
            r.message
        ),
        Err(e) => e.to_string(),
    };
}

pub(super) fn msg_invite(state: &mut State, result: Result<v1::InviteText, Error>) {
    match result {
        Ok(invite) => {
            if !state.can_invite() {
                return;
            }
            state.invite = Some(app::InviteView {
                text: invite.text,
                expires_unix: invite.expires_unix,
            })
        }
        Err(error) => {
            state.toast(format!("invite: {error}"), true);
            state.back();
        }
    }
}

pub(super) fn msg_join_done(
    state: &mut State,
    ctx: &Ctx,
    result: Result<v1::NetworkStatus, Error>,
) {
    match result {
        Ok(status) => {
            state.join = app::JoinView {
                phase: "joined".into(),
                message: status.name.clone(),
            };
            state.network = Some(status);
            super::manual_locked_use::watch(state, ctx);
            state.screen = Screen::Agents;
            state.stack.clear();
            state.agents.device_id = "*".into();
            open_agents(state, ctx);
            load_basics(state, ctx);
        }
        Err(error) => {
            state.join = app::JoinView {
                phase: "failed".into(),
                message: error.to_string(),
            };
        }
    }
}

pub(super) fn msg_done(
    state: &mut State,
    ctx: &Ctx,
    what: &'static str,
    result: Result<Option<v1::NetworkStatus>, Error>,
) {
    if what == "accessibility" {
        state.desktop_setup_error = result
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap_or_default();
    }
    state.busy = state.busy.saturating_sub(1);
    match result {
        Ok(Some(status)) => {
            state.network = Some(status);
            super::manual_locked_use::watch(state, ctx);
            if what == "create" {
                state.screen = Screen::Agents;
                state.stack.clear();
                state.agents.device_id = "*".into();
                open_agents(state, ctx);
                load_basics(state, ctx);
            }
        }
        Ok(None) => {
            if what == "native terminal" {
                state.toast("Native window opened on the session’s device", false);
            }
            if what == "relay" || what == "accessibility" {
                load_basics(state, ctx);
            }
        }
        Err(error) => state.toast(format!("{what}: {error}"), true),
    }
    if what == "decide" {
        load_pending(ctx);
    }
    if what == "policy" {
        let c = ctx.backend.local();
        spawn(ctx, async move {
            Msg::Policy(
                match unary(&c, Req::PolicyGet(v1::PolicyGetRequest {})).await {
                    Ok(Resp::Policy(policy)) => Ok(policy),
                    Ok(_) => Err(unexpected()),
                    Err(e) => Err(e),
                },
            )
        });
    }
}

/// Sends this device's diagnostics (the shell read them, e.g. its iOS
/// performance log) to a member, whose daemon saves them under
/// /tmp/muster-reports and answers with the path.
pub(super) fn intent_upload_diagnostics(state: &mut State, ctx: &Ctx, u: app::UploadDiagnostics) {
    let name = state
        .network
        .as_ref()
        .and_then(|n| n.members.iter().find(|m| m.device_id == u.device_id))
        .map(|m| m.name.clone())
        .unwrap_or_else(|| u.device_id.chars().take(8).collect());
    if u.data.is_empty() || u.data.len() > muster_proto::MAX_DIAGNOSTICS {
        state.toast(
            format!(
                "Log not sent: it must be 1 byte to {} MB",
                muster_proto::MAX_DIAGNOSTICS / (1024 * 1024)
            ),
            true,
        );
        return;
    }
    let self_id = state.self_id.clone();
    let backend = ctx.backend.clone();
    spawn(ctx, async move {
        let result = tokio::time::timeout(Duration::from_secs(60), async {
            let client = backend.client_for(&u.device_id, &self_id).await?;
            match unary(
                &client,
                Req::DiagnosticsUpload(v1::DiagnosticsUploadRequest {
                    kind: u.kind,
                    data: u.data,
                }),
            )
            .await?
            {
                Resp::DiagnosticsSaved(saved) => Ok(saved.path),
                _ => Err(unexpected()),
            }
        })
        .await
        .unwrap_or_else(|_| Err(Error::Unavailable("timed out".into())));
        Msg::DiagnosticsUploaded { name, result }
    });
}

pub(super) fn msg_diagnostics_uploaded(
    state: &mut State,
    name: String,
    result: Result<String, Error>,
) {
    match result {
        Ok(path) => state.toast(format!("Log sent to {name}: {path}"), false),
        Err(error) => state.toast(format!("Log not sent to {name}: {error}"), true),
    }
}
