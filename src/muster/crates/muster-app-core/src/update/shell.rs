//! What the shell reports back: effect results and lifecycle events.

use super::*;

pub fn effect_result(state: &mut State, ctx: &Ctx, id: u64, result: app::EffectResult) {
    let Some(kind) = state.effects.remove(&id) else {
        return;
    };
    match kind {
        EffectKind::ScanQr => {
            if !result.ok {
                if !result.error.is_empty() {
                    state.toast(result.error, true);
                }
                return;
            }
            let text = result.text.trim().to_string();
            if !text.starts_with("muster1:") {
                state.toast("that is not a Muster invite", true);
                return;
            }
            let (name, kind) = state
                .self_info
                .as_ref()
                .map(|i| (i.name.clone(), i.kind.clone()))
                .unwrap_or_default();
            intent(
                state,
                ctx,
                app::Intent {
                    it: Some(It::JoinNetwork(app::JoinNetwork {
                        invite_text: text,
                        device_name: name,
                        device_kind: kind,
                    })),
                },
            );
        }
        EffectKind::Uncover => {
            if !result.ok {
                state.toast(
                    if result.error.is_empty() {
                        "authentication cancelled".into()
                    } else {
                        result.error
                    },
                    true,
                );
                return;
            }
            let device = state.self_id.clone();
            admin_request(
                state,
                ctx,
                "cover",
                device,
                Req::DeviceSetCover(v1::DeviceSetCoverRequest { on: false }),
            );
        }
        EffectKind::Authenticate => {
            let Some(pending) = state.presence.take() else {
                return;
            };
            if pending.effect_id != id {
                state.presence = Some(pending);
                return;
            }
            if !result.ok || result.data.is_empty() {
                if matches!(pending.request, Req::BrowserCall(_)) {
                    state.browser.busy = false;
                    state.browser.result = "Authentication cancelled".into();
                }
                cancel_locked(state, ctx, CancelReason::Authentication);
                state.toast(
                    if result.error.is_empty() {
                        "authentication cancelled".into()
                    } else {
                        result.error
                    },
                    true,
                );
                return;
            }
            if matches!(pending.request, Req::BrowserCall(_)) {
                let backend = ctx.backend.clone();
                let self_id = state.self_id.clone();
                spawn(ctx, async move {
                    let result = async {
                        let client = backend.client_for(&pending.device, &self_id).await?;
                        let call = client
                            .start_presigned(pending.call_id, pending.request, result.data)
                            .await?;
                        browser_result(call.finish().await?)
                    }
                    .await;
                    Msg::BrowserResult {
                        device: pending.device,
                        result,
                    }
                });
                return;
            }
            if let Req::LockedUseExecute(mut request) = pending.request.clone() {
                request.signature = result.data;
                let what = match pending.action_id.split('-').next() {
                    Some("send") => "send",
                    Some("terminal_result") => "terminal_result",
                    Some("respond") => "respond",
                    Some("chat") => "chat",
                    Some("manual") => "manual",
                    _ => "interrupt",
                };
                op_request_from(
                    state,
                    ctx,
                    what,
                    pending.device,
                    Req::LockedUseExecute(request),
                    pending.response_origin,
                );
                return;
            }
            state.busy += 1;
            let self_id = state.self_id.clone();
            let backend = ctx.backend.clone();
            let what: &'static str = match pending.action_id.split('-').next() {
                Some("policy") => "policy",
                Some("remove") => "remove",
                Some("scopes") => "scopes",
                Some("decide") => "decide",
                _ => "admin",
            };
            spawn(ctx, async move {
                let result = async {
                    let client = backend.client_for(&pending.device, &self_id).await?;
                    let call = client
                        .start_presigned(pending.call_id, pending.request, result.data)
                        .await?;
                    match call.finish().await? {
                        Resp::NetworkStatus(status) => Ok(Some(status)),
                        _ => Ok(None),
                    }
                }
                .await;
                Msg::Done { what, result }
            });
        }
        EffectKind::PrepareUpdate => {
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            {
                let prepared = state.update_prepared.clone();
                spawn(ctx, async move {
                    Msg::UpdateApplied(if result.ok {
                        tokio::task::spawn_blocking(move || {
                            prepared
                                .ok_or_else(|| "没有已验证的更新包".to_owned())
                                .and_then(|p| p.apply())
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                    } else {
                        Err(result.error)
                    })
                });
            }
        }
        EffectKind::DesktopHelper => {
            if !result.ok {
                state.desktop_setup_error = result.error;
            }
        }
        EffectKind::ActivateNetwork => {
            if result.ok {
                // Only a retry hint: roster, TLS and availability remain
                // owned by the daemon's authenticated network transport.
                ctx.backend.network_changed();
            } else {
                log::warn!("OS network activation: {}", result.error);
            }
        }
        EffectKind::Other => {}
    }
}

pub fn lifecycle(state: &mut State, ctx: &Ctx, event: app::Lifecycle) {
    match event {
        app::Lifecycle::Foreground => {
            state.foreground = true;
            // Before the reconnect: failures while suspended count again,
            // and members found unreachable meanwhile get another round.
            ctx.backend.suspend(false);
            reconnect(state, ctx);
            terminals::foreground_terminal(state, ctx);
        }
        app::Lifecycle::Background => {
            state.foreground = false;
            // Suspended links fail by design: no evidence about anyone.
            ctx.backend.suspend(true);
            if state.embedded {
                // The next foreground catch-up repairs the stream from history.
                // Do not continuously decode/render/cache a hidden phone chat.
                flush_chat_cache(state, ctx);
                if let Some(chat) = &mut state.chat {
                    chat.watch = None;
                }
                state.device_watch = None;
            }
            if state
                .presence
                .as_ref()
                .is_some_and(|p| matches!(p.request, Req::BrowserCall(_)))
            {
                if let Some(p) = state.presence.take() {
                    state.effects.remove(&p.effect_id);
                }
                state.browser.busy = false;
                state.browser.result = "Browser authentication cancelled in background".into();
            }
            cancel_locked(state, ctx, CancelReason::Background);
            terminals::background_terminal(state);
        }
        app::Lifecycle::NetworkChanged => {
            if state.foreground || !state.embedded {
                reconnect(state, ctx);
            } else {
                ctx.backend.network_changed();
            }
        }
        app::Lifecycle::Terminating => {
            flush_chat_cache(state, ctx);
            cancel_locked(state, ctx, CancelReason::Terminating);
            state.cancel_all();
        }
    }
}

/// After a network change or a return from the background: the link
/// re-registers and re-probes, the basics reload (the device watch
/// re-opens by itself), and the chat starts a fresh watch plus history
/// catch-up. Cancelled watch results cannot touch the new view. Terminals stay as they were: an ended one is re-opened by the
/// user, an SSH session runs on its own stream.
pub(super) fn reconnect(state: &mut State, ctx: &Ctx) {
    activate_network(state, ctx);
    ctx.backend.network_changed();
    load_basics(state, ctx);
    if matches!(state.screen, Screen::Home | Screen::Agents) {
        open_agents(state, ctx);
    }
    if let Some((device, session)) = state.chat_target()
        && state.on_path(Screen::Chat)
    {
        watch_agent(state, ctx, device, session);
    }
}

/// After Back: streams that only a left screen shows stop. The chat's live
/// watch ends (its cache is flushed; opening it again catches up from
/// history) and an attached remote terminal detaches (the session keeps
/// running on its device). An in-app SSH session keeps its connection; only
/// its frames leave the view. Returning to a chat whose watch was stopped
/// meanwhile (a background return on a child screen) watches it again.
pub(super) fn release_hidden(state: &mut State, ctx: &Ctx) {
    if !state.on_path(Screen::Chat) && state.chat.as_ref().is_some_and(|c| c.watch.is_some()) {
        flush_chat_cache(state, ctx);
        if let Some(chat) = &mut state.chat {
            chat.watch = None;
        }
    }
    if state.screen == Screen::Chat
        && state.chat.as_ref().is_some_and(|c| c.watch.is_none())
        && let Some((device, session)) = state.chat_target()
    {
        watch_agent(state, ctx, device, session);
    }
    if !state.on_path(Screen::Terminal)
        && state
            .terminal
            .as_ref()
            .is_some_and(|t| matches!(t.kind, TerminalKind::Remote { .. }))
    {
        state.terminal = None;
    }
}

pub(super) fn activate_network(state: &mut State, ctx: &Ctx) {
    if !state.embedded
        || !state.foreground
        || state
            .effects
            .values()
            .any(|k| *k == EffectKind::ActivateNetwork)
        || state
            .last_network_activation
            .is_some_and(|last| last.elapsed() < std::time::Duration::from_secs(30))
    {
        return;
    }
    state.last_network_activation = Some(std::time::Instant::now());
    let endpoint = muster_net::doh::bootstrap_endpoint();
    effect(
        state,
        ctx,
        EffectKind::ActivateNetwork,
        app::effect::Ef::ActivateNetwork(app::ActivateNetwork {
            address: endpoint.ip().to_string(),
            port: u32::from(endpoint.port()),
            timeout_ms: 3000,
        }),
    );
}

/// A desktop draws (or clears) the privacy cover its daemon records; the
/// shell never decides on its own, it only follows the state.
pub(super) fn follow_cover(state: &mut State, ctx: &Ctx, covered: bool) {
    if ctx.backend.daemon().is_some() {
        return;
    }
    if state.cover_shown == Some(covered) {
        return;
    }
    if state.cover_shown.is_none() && !covered {
        state.cover_shown = Some(false);
        return;
    }
    state.cover_shown = Some(covered);
    effect(
        state,
        ctx,
        EffectKind::Other,
        app::effect::Ef::Cover(app::Cover { on: covered }),
    );
}
