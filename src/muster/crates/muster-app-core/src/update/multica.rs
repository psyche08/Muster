use super::*;

/// Claims the single in-flight Multica slot. A second intent while one is
/// outstanding (a status check, a save or an install) must not be dropped
/// silently: the user gets a visible notice and the running request keeps
/// ownership of the projection.
fn begin_request(state: &mut State) -> bool {
    if state.multica.busy {
        state.toast("Multica 正在处理上一个请求，请稍候", true);
        return false;
    }
    state.multica.busy = true;
    true
}

pub(super) fn request(state: &mut State, ctx: &Ctx, request: v1::MulticaRequest) {
    if state.embedded || !begin_request(state) {
        return;
    }
    let installing = request.action == v1::multica_request::Action::Install as i32;
    if installing {
        state.multica.installation = Some(v1::MulticaInstallation {
            busy: true,
            stage: "正在连接安装服务…".into(),
            ..Default::default()
        });
    }
    let client = ctx.backend.local().clone();
    let tx = ctx.tx.clone();
    spawn(ctx, async move {
        let result = async {
            let pending = unary(&client, Req::Multica(request));
            tokio::pin!(pending);
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut status = loop {
                tokio::select! {
                    result = &mut pending => break match result? {
                        Resp::Multica(status) => status,
                        _ => return Err(unexpected()),
                    },
                    _ = interval.tick(), if installing => {
                        if let Ok(status) = progress(&client).await {
                            if let Some(progress) = status.installation.filter(|p| p.busy) {
                                let _ = tx.send(loop_message(Msg::MulticaProgress(progress)));
                            }
                        }
                    }
                }
            };
            // Publish the full projection so Settings is visible during recovery.
            if status.installation.as_ref().is_some_and(|p| p.busy) {
                let _ = tx.send(loop_message(Msg::Multica(Ok(status.clone()))));
            }
            // Recover a daemon-owned installation after the app reconnects.
            while status.installation.as_ref().is_some_and(|p| p.busy) {
                if let Some(progress) = status.installation.take() {
                    let _ = tx.send(loop_message(Msg::MulticaProgress(progress)));
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                status = progress(&client).await?;
                if !status.installation.as_ref().is_some_and(|p| p.busy) {
                    status =
                        match unary(&client, Req::Multica(v1::MulticaRequest::default())).await? {
                            Resp::Multica(status) => status,
                            _ => return Err(unexpected()),
                        };
                }
            }
            Ok(status)
        }
        .await;
        Msg::Multica(result)
    });
}

async fn progress(client: &Client) -> Result<v1::MulticaStatus, Error> {
    match tokio::time::timeout(
        std::time::Duration::from_secs(2),
        unary(
            client,
            Req::Multica(v1::MulticaRequest {
                action: v1::multica_request::Action::Progress as i32,
                ..Default::default()
            }),
        ),
    )
    .await
    .map_err(|_| Error::Rpc("Multica progress query timed out".into()))??
    {
        Resp::Multica(status) => Ok(status),
        _ => Err(unexpected()),
    }
}

pub(super) fn apply_result(state: &mut State, result: Result<v1::MulticaStatus, Error>) {
    state.multica.busy = false;
    match result {
        Ok(status) => state.multica = status,
        Err(error) => {
            state.multica.message = error.to_string();
            if let Some(progress) = &mut state.multica.installation {
                if progress.busy {
                    progress.busy = false;
                    progress.finished = true;
                    progress.succeeded = false;
                    progress.stage = "连接中断，安装结果尚未确认；请刷新状态，不要重复安装".into();
                    progress
                        .log
                        .push_str(&format!("\n{}\n", state.multica.message));
                }
            }
        }
    }
}

pub(super) fn browse(state: &mut State, ctx: &Ctx, request: v1::MulticaBrowseRequest) {
    let device = state.multica_selected.clone();
    let local = !state.embedded && state.is_self(&device);
    if !local
        && !state
            .multica_targets
            .get(&device)
            .is_some_and(|t| t.connected && t.service.as_ref().is_some_and(|s| s.can_view))
    {
        clear_page(state, "目标设备离线或没有 Multica 查看权限");
        return;
    }
    state.multica_generation = state.multica_generation.wrapping_add(1);
    let generation = state.multica_generation;
    if state.multica_page.workspace_id != request.workspace_id && !request.workspace_id.is_empty() {
        let workspaces = std::mem::take(&mut state.multica_page.workspaces);
        state.multica_page = v1::MulticaPage {
            workspaces,
            workspace_id: request.workspace_id.clone(),
            ..Default::default()
        };
    }
    state.multica_page.issue = None;
    state.multica_page.agent = None;
    state.multica_page.busy = true;
    state.multica_page.error.clear();
    state.multica_query = request.clone();
    let backend = ctx.backend.clone();
    let self_id = state.self_id.clone();
    spawn(ctx, async move {
        let result = async {
            let client = if local {
                backend.local().clone()
            } else {
                backend.client_for(&device, &self_id).await?
            };
            match unary(&client, Req::MulticaBrowse(request)).await? {
                Resp::MulticaPage(page) => Ok(page),
                _ => Err(unexpected()),
            }
        }
        .await;
        Msg::MulticaPage { generation, result }
    });
}
pub(super) fn apply_page(
    state: &mut State,
    generation: u64,
    result: Result<v1::MulticaPage, Error>,
) {
    if generation != state.multica_generation {
        return;
    }
    state.multica_page.busy = false;
    match result {
        Ok(mut page) => {
            state.multica_query.workspace_id = page.workspace_id.clone();
            if page.issue.is_some() || page.agent.is_some() {
                state.multica_page.issue = page.issue.take();
                state.multica_page.agent = page.agent.take();
                state.multica_page.runs = page.runs;
                state.multica_page.comments = page.comments;
                state.multica_page.status_choices = page.status_choices;
                state.multica_page.error.clear();
            } else {
                state.multica_page = page;
            }
        }
        Err(error) => state.multica_page.error = error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_busy_multica_request_shows_a_toast_and_keeps_the_owner() {
        let mut state = State::new(false, Some("local".into()));
        state.multica.busy = true;
        state.multica.installation = Some(v1::MulticaInstallation {
            busy: true,
            stage: "正在安装".into(),
            ..Default::default()
        });
        assert!(!begin_request(&mut state));
        assert!(state.multica.busy, "the running request keeps the slot");
        assert_eq!(
            state.multica.installation.as_ref().unwrap().stage,
            "正在安装",
            "the in-flight projection is untouched"
        );
        let toast = state.toasts.last().expect("a dropped request is visible");
        assert!(toast.error);
        assert!(toast.text.contains("正在处理上一个请求"));
    }

    #[test]
    fn the_first_multica_request_claims_the_slot() {
        let mut state = State::new(false, Some("local".into()));
        assert!(begin_request(&mut state));
        assert!(state.multica.busy);
        assert!(state.toasts.is_empty());
    }

    #[test]
    fn interrupted_install_keeps_logs_and_requires_status_check() {
        let mut state = State::new(false, Some("local".into()));
        state.multica.busy = true;
        state.multica.installation = Some(v1::MulticaInstallation {
            busy: true,
            log: "Downloading package\n".into(),
            ..Default::default()
        });
        apply_result(&mut state, Err(Error::Rpc("connection closed".into())));
        assert!(!state.multica.busy);
        let progress = state.multica.installation.unwrap();
        assert!(!progress.busy && !progress.succeeded && progress.finished);
        assert!(progress.log.contains("Downloading package"));
        assert!(progress.stage.contains("尚未确认"));
    }

    #[test]
    fn late_workspace_and_detail_responses_cannot_replace_the_current_selection() {
        let mut state = State::new(false, Some("local".into()));
        state.multica_generation = 2;
        state.multica_page = v1::MulticaPage {
            workspace_id: "workspace-b".into(),
            busy: true,
            ..Default::default()
        };
        apply_page(
            &mut state,
            1,
            Ok(v1::MulticaPage {
                workspace_id: "workspace-a".into(),
                issue: Some(v1::MulticaIssue {
                    id: "old-issue".into(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        );
        assert_eq!(state.multica_page.workspace_id, "workspace-b");
        assert!(state.multica_page.issue.is_none());
        assert!(state.multica_page.busy);
        apply_page(
            &mut state,
            2,
            Ok(v1::MulticaPage {
                workspace_id: "workspace-b".into(),
                ..Default::default()
            }),
        );
        assert!(!state.multica_page.busy);
        apply_page(&mut state, 1, Err(Error::Rpc("old error".into())));
        assert!(state.multica_page.error.is_empty());
    }
    #[test]
    fn a_detail_does_not_replace_the_selected_list_page() {
        let mut state = State::new(false, Some("local".into()));
        state.multica_generation = 7;
        state.multica_page = v1::MulticaPage {
            workspace_id: "w".into(),
            offset: 50,
            has_more: true,
            issues: vec![v1::MulticaIssue {
                id: "visible".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        apply_page(
            &mut state,
            7,
            Ok(v1::MulticaPage {
                workspace_id: "w".into(),
                issue: Some(v1::MulticaIssue {
                    id: "selected".into(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        );
        assert_eq!(state.multica_page.offset, 50);
        assert!(state.multica_page.has_more);
        assert_eq!(state.multica_page.issues[0].id, "visible");
        assert_eq!(state.multica_page.issue.unwrap().id, "selected");
    }
}

pub(crate) fn view(state: &State) -> app::MulticaWorkspaceView {
    let mut devices: Vec<_> = state.multica_targets.values().cloned().collect();
    devices.sort_by(|a, b| a.name.cmp(&b.name).then(a.device_id.cmp(&b.device_id)));
    app::MulticaWorkspaceView {
        visible: state.multica.installed || !devices.is_empty(),
        selected_device: state.multica_selected.clone(),
        devices,
        local_service: Some(state.multica_service.clone()),
        selected_is_local: !state.embedded && state.is_self(&state.multica_selected),
        mutation_busy: state.multica_mutation_busy,
        mutation_message: state.multica_mutation_message.clone(),
        mutation_result: state.multica_mutation_result,
        mutation_applied: state.multica_mutation_applied,
        mutation_uncertain: state.multica_mutation_uncertain,
    }
}
fn clear_page(state: &mut State, message: &str) {
    state.multica_generation = state.multica_generation.wrapping_add(1);
    state.multica_page = v1::MulticaPage {
        error: message.into(),
        ..Default::default()
    };
}

pub(super) fn start_discovery(state: &mut State, ctx: &Ctx) {
    let (watch, token) = state.new_watch();
    state.multica_discovery = Some(watch);
    let tx = ctx.tx.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(2));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = token.cancelled() => break,
                _ = tick.tick() => { if tx.send(loop_message(Msg::MulticaTick)).is_err() { break; } }
            }
        }
    });
}

pub(super) fn background(state: &mut State) {
    state.multica_discovery_epoch = state.multica_discovery_epoch.wrapping_add(1);
    state.multica_probing = false;
    for target in state.multica_targets.values_mut() {
        if target.device_id != state.self_id {
            target.connected = false;
        }
    }
    if state.multica_selected != state.self_id {
        clear_page(state, "返回前台后重新验证 Multica 权限");
    }
}

pub(super) fn discover(state: &mut State, ctx: &Ctx) {
    if !state.foreground {
        return;
    }
    let members = state.service_devices();
    let own = state.self_id.clone();
    let embedded = state.embedded;
    state.multica_targets.retain(|id, _| {
        (!embedded && id == &own && state.multica.installed)
            || members.iter().any(|m| &m.device_id == id && !m.is_self)
    });
    if !state.multica_selected.is_empty()
        && !state.multica_targets.contains_key(&state.multica_selected)
        && (!state.multica_page.workspaces.is_empty() || state.multica_page.busy)
    {
        clear_page(state, "目标设备已移除或停止共享");
    }
    for target in state.multica_targets.values_mut() {
        if members
            .iter()
            .any(|m| m.device_id == target.device_id && crate::state::member_offline(m))
        {
            target.connected = false;
        }
    }
    if state
        .multica_targets
        .get(&state.multica_selected)
        .is_some_and(|t| !t.connected)
        && (!state.multica_page.workspaces.is_empty() || state.multica_page.busy)
    {
        clear_page(state, "目标设备离线；重新连接后会重新验证权限");
    }
    if state.multica_probing {
        return;
    }
    state.multica_probing = true;
    let epoch = state.multica_discovery_epoch;
    let mut targets: Vec<_> = members
        .into_iter()
        .filter(|m| !m.is_self && !crate::state::member_offline(m))
        .map(|m| (m.device_id, m.name, false))
        .collect();
    if !embedded {
        targets.push((own.clone(), "本机".into(), true));
    }
    let backend = ctx.backend.clone();
    spawn(ctx, async move {
        let mut jobs = tokio::task::JoinSet::new();
        for (device, name, local) in targets {
            let backend = backend.clone();
            let own = own.clone();
            jobs.spawn(async move {
                let result = tokio::time::timeout(Duration::from_secs(3), async {
                    let client = if local {
                        backend.local().clone()
                    } else {
                        backend.client_for(&device, &own).await?
                    };
                    match unary(
                        &client,
                        Req::MulticaService(v1::MulticaServiceRequest::default()),
                    )
                    .await?
                    {
                        Resp::MulticaService(service) => Ok(service),
                        _ => Err(unexpected()),
                    }
                })
                .await
                .unwrap_or_else(|_| Err(Error::Unavailable("Multica 连接超时".into())));
                (device, name, result)
            });
        }
        let mut results = Vec::new();
        while let Some(Ok(result)) = jobs.join_next().await {
            results.push(result);
        }
        Msg::MulticaServices { epoch, results }
    });
}

pub(super) fn apply_services(
    state: &mut State,
    ctx: &Ctx,
    results: Vec<(String, String, Result<v1::MulticaService, Error>)>,
) {
    state.multica_probing = false;
    let before = state.multica_targets.get(&state.multica_selected).cloned();
    let members = state.service_devices();
    for (device, name, result) in results {
        let local = !state.embedded && state.is_self(&device);
        if !local && !members.iter().any(|m| m.device_id == device && !m.is_self) {
            continue;
        }
        match result {
            Ok(service) if service.version == 1 => {
                if local {
                    state.multica_service = service.clone();
                }
                if service.can_view && (!local || state.multica.installed) {
                    state.multica_targets.insert(
                        device.clone(),
                        app::MulticaDevice {
                            device_id: device,
                            name,
                            connected: true,
                            service: Some(service),
                        },
                    );
                } else {
                    state.multica_targets.remove(&device);
                }
            }
            Ok(_) => {
                state.multica_targets.remove(&device);
            }
            Err(_) => {
                if let Some(target) = state.multica_targets.get_mut(&device) {
                    target.connected = false;
                }
            }
        }
    }
    if !state.multica_targets.contains_key(&state.multica_selected) {
        state.multica_selected = state
            .multica_targets
            .keys()
            .min()
            .cloned()
            .unwrap_or_default();
        state.multica_query = v1::MulticaBrowseRequest::default();
    }
    let after = state.multica_targets.get(&state.multica_selected).cloned();
    if before != after {
        clear_page(
            state,
            if after.as_ref().is_some_and(|t| t.connected) {
                ""
            } else {
                "Multica 不可用或权限已撤回"
            },
        );
        if state.screen == Screen::Multica && after.as_ref().is_some_and(|t| t.connected) {
            browse(state, ctx, state.multica_query.clone());
        }
    }
    if state.screen == Screen::Multica && !view(state).visible {
        state.go(Screen::Agents);
    }
}

pub(super) fn select(state: &mut State, ctx: &Ctx, device: String) {
    if !state.multica_targets.contains_key(&device) {
        return;
    }
    if state.multica_selected != device {
        state.multica_selected = device;
        state.multica_query = v1::MulticaBrowseRequest::default();
        clear_page(state, "");
    }
    browse(state, ctx, state.multica_query.clone());
}

pub(super) fn service_request(state: &mut State, ctx: &Ctx, request: v1::MulticaServiceRequest) {
    if state.embedded {
        state.toast("请在安装 Multica 的设备上管理共享", true);
        return;
    }
    let client = ctx.backend.local().clone();
    spawn(ctx, async move {
        let result = match unary(&client, Req::MulticaService(request)).await {
            Ok(Resp::MulticaService(service)) => Ok(service),
            Ok(_) => Err(unexpected()),
            Err(error) => Err(error),
        };
        Msg::MulticaServiceSaved(result)
    });
}

#[cfg(test)]
mod service_tests {
    use super::*;
    #[tokio::test]
    async fn phone_discovers_remote_workbench_and_clears_content_on_disconnect_and_revocation() {
        let (socket, _peer) = tokio::io::duplex(1024);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let dir = tempfile::tempdir().unwrap();
        let ctx = Ctx {
            shell: Arc::new(crate::send_diagnostics::tests::Capture::default()),
            cache: Arc::new(crate::cache::Cache::new(dir.path().into())),
            backend: Arc::new(crate::backend::Backend::Socket {
                path: dir.path().into(),
                local: Client::new(socket),
            }),
            tx,
        };
        let mut state = State::new(true, Some("phone".into()));
        state.network = Some(v1::NetworkStatus {
            members: vec![v1::MemberView {
                device_id: "host".into(),
                kind: "desktop".into(),
                connected: true,
                presence: "online".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
        let offered = v1::MulticaService {
            version: 1,
            offered: true,
            can_view: true,
            workspaces: vec!["workspace-a".into()],
            ..Default::default()
        };
        apply_services(
            &mut state,
            &ctx,
            vec![("host".into(), "Example Mac".into(), Ok(offered.clone()))],
        );
        assert!(view(&state).visible && !view(&state).selected_is_local);
        assert!(
            !state.multica.installed,
            "a phone needs no local Multica installation"
        );
        assert_eq!(state.multica_selected, "host");
        state.multica_page = v1::MulticaPage {
            workspace_id: "workspace-a".into(),
            issues: vec![v1::MulticaIssue {
                title: "private task".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let stale_generation = state.multica_generation;
        apply_services(
            &mut state,
            &ctx,
            vec![(
                "host".into(),
                "Example Mac".into(),
                Err(Error::Unavailable("offline".into())),
            )],
        );
        assert!(
            view(&state).visible,
            "offline keeps only the known device entry"
        );
        assert!(state.multica_page.issues.is_empty());
        assert!(!state.multica_targets["host"].connected);
        apply_page(
            &mut state,
            stale_generation,
            Ok(v1::MulticaPage {
                issues: vec![v1::MulticaIssue::default()],
                ..Default::default()
            }),
        );
        assert!(
            state.multica_page.issues.is_empty(),
            "late history cannot undo invalidation"
        );
        apply_services(
            &mut state,
            &ctx,
            vec![("host".into(), "Example Mac".into(), Ok(offered))],
        );
        assert!(state.multica_targets["host"].connected);
        state.multica_page = v1::MulticaPage {
            workspaces: vec![v1::MulticaWorkspace::default()],
            ..Default::default()
        };
        let old_epoch = state.multica_discovery_epoch;
        let old_page = state.multica_generation;
        state.foreground = false;
        background(&mut state);
        assert!(state.multica_page.workspaces.is_empty());
        assert!(!state.multica_targets["host"].connected);
        state.foreground = true;
        super::super::msg(
            &mut state,
            &ctx,
            Msg::MulticaServices {
                epoch: old_epoch,
                results: vec![(
                    "host".into(),
                    "Example Mac".into(),
                    Ok(v1::MulticaService {
                        version: 1,
                        can_view: true,
                        ..Default::default()
                    }),
                )],
            },
        );
        assert!(
            !state.multica_targets["host"].connected,
            "pre-suspension discovery is stale even after foreground"
        );
        apply_page(
            &mut state,
            old_page,
            Ok(v1::MulticaPage {
                workspaces: vec![v1::MulticaWorkspace::default()],
                ..Default::default()
            }),
        );
        assert!(state.multica_page.workspaces.is_empty());
        apply_services(
            &mut state,
            &ctx,
            vec![(
                "host".into(),
                "Example Mac".into(),
                Ok(v1::MulticaService {
                    version: 1,
                    ..Default::default()
                }),
            )],
        );
        assert!(!view(&state).visible);
        assert!(state.multica_targets.is_empty() && state.multica_page.workspaces.is_empty());
        // A removed roster member's late discovery cannot resurrect its entry.
        state.network.as_mut().unwrap().members.clear();
        apply_services(
            &mut state,
            &ctx,
            vec![(
                "host".into(),
                "Example Mac".into(),
                Ok(v1::MulticaService {
                    version: 1,
                    can_view: true,
                    ..Default::default()
                }),
            )],
        );
        assert!(!view(&state).visible);
    }
}

pub(super) fn mutate(state: &mut State, ctx: &Ctx, mut request: v1::MulticaMutation) {
    if state.embedded || !state.is_self(&state.multica_selected) {
        state.toast("请在提供 Multica 服务的 Mac 上操作", true);
        return;
    }
    if state.multica_mutation_busy || state.multica_mutation_uncertain {
        state.toast("请等待当前操作完成，或先核对未确认的结果", true);
        return;
    }
    if request.workspace_id != state.multica_page.workspace_id || request.workspace_id.is_empty() {
        state.toast("工作区已改变，请重新打开详情", true);
        return;
    }
    request.op_id = state.op_id();
    state.multica_mutation_busy = true;
    state.multica_mutation_message = "正在提交…".into();
    state.multica_mutation_applied = false;
    let workspace = request.workspace_id.clone();
    let query = state.multica_query.clone();
    let client = ctx.backend.local().clone();
    spawn(ctx, async move {
        let result = match unary(&client, Req::MulticaMutation(request)).await {
            Ok(Resp::OpResult(result)) => Ok(result),
            Ok(_) => Err(unexpected()),
            Err(error) => Err(error),
        };
        Msg::MulticaMutation {
            workspace,
            query,
            result,
        }
    });
}

pub(super) fn mutation_result(
    state: &mut State,
    ctx: &Ctx,
    workspace: String,
    query: v1::MulticaBrowseRequest,
    result: Result<v1::OpResult, Error>,
) {
    use v1::op_result::Status;
    state.multica_mutation_busy = false;
    state.multica_mutation_result = state.multica_mutation_result.wrapping_add(1);
    let (applied, uncertain, message) = match result {
        Ok(r)
            if matches!(
                Status::try_from(r.status),
                Ok(Status::Applied | Status::Duplicate)
            ) =>
        {
            (
                true,
                false,
                "已提交；任务执行结果请查看执行历史和评论".into(),
            )
        }
        Ok(r) if r.status == Status::Rejected as i32 => (false, false, r.message),
        _ => (
            false,
            true,
            "操作结果尚未确认。请刷新详情和执行历史核对；不会自动重试。".into(),
        ),
    };
    state.multica_mutation_applied = applied;
    state.multica_mutation_uncertain = uncertain;
    state.multica_mutation_message = message;
    // Never overwrite a different target/workspace/detail opened during the call.
    if applied
        && state.is_self(&state.multica_selected)
        && workspace == state.multica_page.workspace_id
        && query == state.multica_query
    {
        browse(state, ctx, query);
    }
}

#[cfg(test)]
mod mutation_tests {
    use super::*;
    #[tokio::test]
    async fn host_only_submit_is_single_flight_and_unknown_results_require_review() {
        let (socket, _peer) = tokio::io::duplex(1024);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let dir = tempfile::tempdir().unwrap();
        let ctx = Ctx {
            shell: Arc::new(crate::send_diagnostics::tests::Capture::default()),
            cache: Arc::new(crate::cache::Cache::new(dir.path().into())),
            backend: Arc::new(crate::backend::Backend::Socket {
                path: dir.path().into(),
                local: Client::new(socket),
            }),
            tx,
        };
        let mut state = State::new(true, Some("phone".into()));
        state.multica_selected = "host".into();
        state.multica_page.workspace_id = "workspace-a".into();
        let request = v1::MulticaMutation {
            workspace_id: "workspace-a".into(),
            ..Default::default()
        };
        mutate(&mut state, &ctx, request.clone());
        assert!(
            !state.multica_mutation_busy,
            "a phone cannot become a local owner"
        );
        state.embedded = false;
        state.multica_selected = "phone".into();
        mutate(&mut state, &ctx, request.clone());
        assert!(state.multica_mutation_busy);
        let op = state.next_op;
        mutate(&mut state, &ctx, request.clone());
        assert_eq!(
            state.next_op, op,
            "duplicate clicks do not allocate another submission"
        );
        mutation_result(
            &mut state,
            &ctx,
            "workspace-a".into(),
            v1::MulticaBrowseRequest::default(),
            Err(Error::Unavailable("lost reply".into())),
        );
        assert!(state.multica_mutation_uncertain && !state.multica_mutation_busy);
        mutate(&mut state, &ctx, request);
        assert_eq!(state.next_op, op, "an ambiguous write is not retried");
        // A late success must not replace another target's selected page.
        state.multica_selected = "other-host".into();
        state.multica_page.workspace_id = "workspace-b".into();
        let generation = state.multica_generation;
        mutation_result(
            &mut state,
            &ctx,
            "workspace-a".into(),
            v1::MulticaBrowseRequest::default(),
            Ok(v1::OpResult {
                status: v1::op_result::Status::Applied as i32,
                ..Default::default()
            }),
        );
        assert_eq!(state.multica_page.workspace_id, "workspace-b");
        assert_eq!(state.multica_generation, generation);
        assert!(state.multica_mutation_applied);
    }
}
