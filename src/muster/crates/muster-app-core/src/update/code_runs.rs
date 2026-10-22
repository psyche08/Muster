//! User-selected code execution and result delivery stay bound to their origin.
use super::*;
use muster_terminal::commands;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(crate) struct Workbench {
    pub preview: Option<Prepared>,
    pub runs: HashMap<String, Run>,
    polling: HashSet<String>,
    starting: HashSet<String>,
    pub errors: HashMap<String, String>,
    status_errors: HashSet<String>,
}
pub(crate) struct Prepared {
    pub device: String,
    pub session: String,
    pub provider: String,
    pub message: String,
    pub view: app::CodePreview,
}
pub(crate) struct Run {
    pub device: String,
    pub provider: String,
    pub view: app::CodeRunView,
    auto_return: bool,
    attempted: bool,
}
pub(crate) fn origin(device: &str, session: &str) -> String {
    crate::cache::Cache::key(&["code", device, session])
}
fn key(device: &str, id: &str) -> String {
    crate::cache::Cache::key(&["run", device, id])
}
pub(crate) fn eligible(chat: &Chat) -> bool {
    !chat.session.cwd.is_empty() && !chat.session.read_only && chat.session.mode != "chat"
}
pub(crate) fn preview(state: &State) -> Option<app::CodePreview> {
    state
        .code
        .preview
        .as_ref()
        .filter(|p| state.chat_target() == Some((p.device.clone(), p.session.clone())))
        .map(|p| p.view.clone())
}
pub(crate) fn views(state: &State, device: &str, session: &str) -> Vec<app::CodeRunView> {
    let mut rows: Vec<_> = state
        .code
        .runs
        .values()
        .filter(|r| r.device == device && r.view.run.as_ref().is_some_and(|r| r.session == session))
        .map(|r| r.view.clone())
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.run.as_ref().map(|r| r.started_unix).unwrap_or(0)));
    rows
}
pub(super) fn action(state: &mut State, ctx: &Ctx, action: app::CodeAction) {
    if action.action == 2 {
        state.code.preview = None;
        return;
    }
    if action.action == 4 {
        poll(state, ctx);
        return;
    }
    let Some(chat) = state.chat.as_ref().filter(|c| eligible(c)) else {
        return;
    };
    let device = chat.device.clone();
    let session = chat.session.id.clone();
    let scope = origin(&device, &session);
    match action.action {
        0 => {
            if state.code.starting.contains(&scope) {
                return;
            }
            let found = chat.messages.iter().find_map(|message| {
                commands::parts(message, true)
                    .into_iter()
                    .find(|p| p.command_id == action.id && !p.command_id.is_empty())
                    .map(|p| (message.id.clone(), p))
            });
            if let Some((message, part)) = found {
                state.code.preview = Some(Prepared {
                    device,
                    session,
                    provider: chat.session.provider.clone(),
                    message,
                    view: app::CodePreview {
                        id: part.command_id,
                        code: part.text,
                        language: part.language,
                        cwd: chat.session.cwd.clone(),
                        device_name: chat.session.device_name.clone(),
                    },
                });
            }
        }
        1 => {
            let Some(prepared) =
                state.code.preview.take().filter(|p| {
                    p.device == device && p.session == session && p.view.id == action.id
                })
            else {
                return;
            };
            // Reject stale previews after a streamed edit, workspace change or navigation.
            if prepared.view.cwd != chat.session.cwd
                || !chat
                    .messages
                    .iter()
                    .flat_map(|m| commands::parts(m, true))
                    .any(|p| p.command_id == action.id)
            {
                state
                    .code
                    .errors
                    .insert(scope, "Code changed; open its preview again".into());
                return;
            }
            if !state.code.starting.insert(scope.clone()) {
                return;
            }
            state.code.status_errors.remove(&scope);
            state.code.errors.remove(&scope);
            let request = v1::TerminalRunRequest {
                session: session.clone(),
                op_id: state.op_id(),
                code: prepared.view.code.clone(),
                language: prepared.view.language.clone(),
                source_message: prepared.message.clone(),
                cwd: prepared.view.cwd.clone(),
                auto_return: action.auto_return,
            };
            let backend = ctx.backend.clone();
            let self_id = state.self_id.clone();
            spawn(ctx, async move {
                let result = async {
                    let client = backend.client_for(&device, &self_id).await?;
                    match unary(&client, Req::TerminalRun(request)).await? {
                        Resp::OpResult(r) if matches!(r.status, 0 | 1) => Ok(r.result_id),
                        Resp::OpResult(r) => Err(Error::Unavailable(r.message)),
                        _ => Err(unexpected()),
                    }
                }
                .await;
                Msg::CodeStarted {
                    prepared: Box::new(prepared),
                    auto_return: action.auto_return,
                    result,
                }
            });
        }
        3 => {
            let k = key(&device, &action.id);
            if state
                .code
                .runs
                .get(&k)
                .is_some_and(|r| r.view.run.as_ref().is_some_and(|r| r.session == session))
            {
                deliver(state, ctx, &k);
            }
        }
        _ => {}
    }
}
pub(super) fn started(
    state: &mut State,
    ctx: &Ctx,
    p: Prepared,
    auto_return: bool,
    result: Result<String, Error>,
) {
    let scope = origin(&p.device, &p.session);
    state.code.starting.remove(&scope);
    match result {
        Ok(id) => {
            state.code.runs.insert(
                key(&p.device, &id),
                Run {
                    device: p.device.clone(),
                    provider: p.provider,
                    view: app::CodeRunView {
                        run: Some(v1::TerminalRun {
                            id,
                            session: p.session.clone(),
                            code: p.view.code,
                            language: p.view.language,
                            source_message: p.message,
                            cwd: p.view.cwd,
                            state: "running".into(),
                            ..Default::default()
                        }),
                        delivery: "ready".into(),
                        ..Default::default()
                    },
                    auto_return,
                    attempted: false,
                },
            );
            if state.chat_target() == Some((p.device, p.session)) {
                terminals::session_terminal_action(
                    state,
                    ctx,
                    app::SessionTerminalAction { action: 0 },
                );
            }
            poll(state, ctx);
        }
        Err(e) => {
            state.code.errors.insert(
                scope,
                format!("Run was not confirmed: {e}. Refresh records before starting it again."),
            );
        }
    }
}
pub(super) fn poll(state: &mut State, ctx: &Ctx) {
    if !state.foreground {
        return;
    }
    let mut origins: HashMap<String, (String, String, String)> = HashMap::new();
    if state.screen == Screen::Chat
        && let Some(c) = state.chat.as_ref().filter(|c| eligible(c))
    {
        origins.insert(
            origin(&c.device, &c.session.id),
            (
                c.device.clone(),
                c.session.id.clone(),
                c.session.provider.clone(),
            ),
        );
    }
    for r in state.code.runs.values() {
        if let Some(run) = &r.view.run
            && (matches!(run.state.as_str(), "starting" | "running")
                || r.view.delivery == "sending"
                || (r.auto_return && !r.attempted && run.state != "unavailable"))
        {
            origins.insert(
                origin(&r.device, &run.session),
                (r.device.clone(), run.session.clone(), r.provider.clone()),
            );
        }
    }
    for (scope, (device, session, provider)) in origins {
        if !state.code.polling.insert(scope) {
            continue;
        }
        let backend = ctx.backend.clone();
        let self_id = state.self_id.clone();
        spawn(ctx, async move {
            let result = tokio::time::timeout(Duration::from_secs(8), async {
                let client = backend.client_for(&device, &self_id).await?;
                match unary(
                    &client,
                    Req::TerminalRuns(v1::TerminalRunsRequest {
                        session: session.clone(),
                    }),
                )
                .await?
                {
                    Resp::TerminalRuns(list) => Ok(list.runs),
                    _ => Err(unexpected()),
                }
            })
            .await
            .unwrap_or_else(|_| Err(Error::Unavailable("Run status timed out".into())));
            Msg::CodeRuns {
                device,
                session,
                provider,
                result,
            }
        });
    }
}
pub(super) fn received(
    state: &mut State,
    ctx: &Ctx,
    device: String,
    session: String,
    provider: String,
    result: Result<Vec<v1::TerminalRun>, Error>,
) {
    let scope = origin(&device, &session);
    state.code.polling.remove(&scope);
    match result {
        Err(e) => {
            if !state.code.errors.contains_key(&scope) || state.code.status_errors.contains(&scope)
            {
                state.code.status_errors.insert(scope.clone());
                state.code.errors.insert(scope, e.to_string());
            }
        }
        Ok(records) => {
            if state.code.status_errors.remove(&scope) {
                state.code.errors.remove(&scope);
            }
            for record in records {
                if record.session != session {
                    continue;
                }
                let k = key(&device, &record.id);
                let r = state.code.runs.entry(k).or_insert_with(|| Run {
                    device: device.clone(),
                    provider: provider.clone(),
                    view: app::CodeRunView {
                        delivery: "ready".into(),
                        ..Default::default()
                    },
                    auto_return: !state.self_id.is_empty() && record.return_owner == state.self_id,
                    attempted: false,
                });
                match record.return_state.as_str() {
                    "applied" => {
                        r.view.delivery = "sent".into();
                        r.attempted = true;
                        r.view.error.clear();
                    }
                    "pending" | "unknown" => {
                        if r.view.delivery != "sending" {
                            r.view.delivery = "unknown".into();
                        }
                        r.attempted = true;
                    }
                    "rejected" => {
                        r.view.delivery = "rejected".into();
                        r.attempted = true;
                    }
                    _ => {}
                }
                r.view.can_return = record.state == "completed"
                    && record.exit_known
                    && record.return_ready
                    && !r.attempted;
                if !r.attempted && record.state == "completed" {
                    r.view.delivery = if record.return_ready {
                        "ready"
                    } else {
                        "waiting"
                    }
                    .into();
                }
                r.view.run = Some(record);
            }
        }
    }
    if state.foreground && state.presence.is_none() && state.locked.is_none() {
        let candidate = state
            .code
            .runs
            .iter()
            .find(|(_, r)| r.auto_return && r.view.can_return)
            .map(|(k, _)| k.clone());
        if let Some(k) = candidate {
            deliver(state, ctx, &k);
        }
    }
}
fn deliver(state: &mut State, ctx: &Ctx, k: &str) {
    if !state.foreground || state.presence.is_some() || state.locked.is_some() {
        return;
    }
    let Some(r) = state
        .code
        .runs
        .get_mut(k)
        .filter(|r| r.view.can_return && !r.attempted)
    else {
        return;
    };
    let Some(run) = &r.view.run else { return };
    let request = v1::AgentSendRequest {
        session: run.session.clone(),
        op_id: commands::return_id(&run.id),
        text: commands::result_message(run),
        ..Default::default()
    };
    let device = r.device.clone();
    let origin = (run.session.clone(), run.id.clone());
    r.attempted = true;
    r.view.delivery = "sending".into();
    r.view.can_return = false;
    operations::op_request_from(
        state,
        ctx,
        "terminal_result",
        device,
        Req::AgentSend(request),
        Some(origin),
    );
}
pub(super) fn delivery(
    state: &mut State,
    device: &str,
    origin: Option<(String, String)>,
    result: Result<v1::OpResult, Error>,
) {
    let Some((session, id)) = origin else { return };
    let Some(r) = state
        .code
        .runs
        .get_mut(&key(device, &id))
        .filter(|r| r.view.run.as_ref().is_some_and(|r| r.session == session))
    else {
        return;
    };
    match result {
        Ok(v) if v.status == 0 => {
            r.view.delivery = "sent".into();
            r.view.error.clear();
        }
        Ok(v) => {
            r.view.delivery = if v.status == 2 { "rejected" } else { "unknown" }.into();
            r.view.error = v.message;
        }
        Err(e) => {
            r.view.delivery = "unknown".into();
            r.view.error = format!(
                "Delivery was not confirmed: {e}. Check the original conversation before sending again."
            );
        }
    }
}
pub(super) fn provider(state: &State, device: &str, origin: &Option<(String, String)>) -> String {
    origin
        .as_ref()
        .and_then(|(_, id)| state.code.runs.get(&key(device, id)))
        .map(|r| r.provider.clone())
        .unwrap_or_default()
}

/// Cancellation has no provider receipt; preserve the run and never resend it.
pub(super) fn cancelled(state: &mut State, device: &str) {
    for r in state
        .code
        .runs
        .values_mut()
        .filter(|r| r.device == device && r.view.delivery == "sending")
    {
        r.view.delivery = "unknown".into();
        r.view.error="Return cancelled or unconfirmed. Check the original conversation before sending again.".into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn run_delivery_is_origin_bound_and_unknown_never_becomes_retryable() {
        let mut state = State::new(false, Some("self".into()));
        let k = key("mac", "run");
        state.code.runs.insert(
            k.clone(),
            Run {
                device: "mac".into(),
                provider: "claude".into(),
                view: app::CodeRunView {
                    run: Some(v1::TerminalRun {
                        id: "run".into(),
                        session: "original".into(),
                        state: "completed".into(),
                        exit_known: true,
                        ..Default::default()
                    }),
                    delivery: "sending".into(),
                    ..Default::default()
                },
                auto_return: true,
                attempted: true,
            },
        );
        state.chat = Some(Chat {
            device: "other-mac".into(),
            session: v1::AgentSession {
                id: "other-chat".into(),
                ..Default::default()
            },
            ..Default::default()
        });
        delivery(
            &mut state,
            "mac",
            Some(("wrong".into(), "run".into())),
            Ok(v1::OpResult::default()),
        );
        assert_eq!(state.code.runs[&k].view.delivery, "sending");
        delivery(
            &mut state,
            "mac",
            Some(("original".into(), "run".into())),
            Err(Error::Unavailable("lost receipt".into())),
        );
        assert_eq!(state.code.runs[&k].view.delivery, "unknown");
        assert!(state.code.runs[&k].attempted);
        assert!(!state.code.runs[&k].view.can_return);
        assert!(views(&state, "other-mac", "other-chat").is_empty());
        assert_eq!(views(&state, "mac", "original").len(), 1);
        delivery(
            &mut state,
            "mac",
            Some(("original".into(), "run".into())),
            Ok(v1::OpResult::default()),
        );
        assert_eq!(state.code.runs[&k].view.delivery, "sent");
        assert_eq!(state.chat.unwrap().session.id, "other-chat");
    }
}
