//! Foreground, bounded reads of missing local Claude permission cards.
use super::*;

fn missing(chat: &Chat) -> bool {
    chat.session.provider == "claude"
        && chat.session.mode == "code"
        && muster_agent::claude_identity::local_id(&chat.session.id)
        && !chat.session.read_only
        && chat.session.state == "waiting_approval"
        && !chat
            .interactions
            .iter()
            .any(|i| !i.optional && matches!(i.kind.as_str(), "approval" | "question"))
}

pub(super) fn schedule(state: &mut State, ctx: &Ctx, token: CancellationToken) {
    if !state.foreground || state.screen != Screen::Chat || token.is_cancelled() {
        return;
    }
    let Some(chat) = &mut state.chat else { return };
    if chat.approval_refresh_attempted || chat.approval_refresh_pending || !missing(chat) {
        return;
    }
    chat.approval_refresh_attempted = true;
    chat.approval_refresh_pending = true;
    let device = chat.device.clone();
    let session = chat.session.id.clone();
    spawn(ctx, async move {
        // Initial interaction events follow the subscription's State. Let them
        // arrive before deciding a covered read is actually necessary.
        tokio::select! {
            _ = token.cancelled() => Msg::Ignored,
            _ = tokio::time::sleep(Duration::from_millis(350)) => Msg::RefreshApprovals { token, device, session },
        }
    });
}

pub(super) fn refresh(
    state: &mut State,
    ctx: &Ctx,
    token: CancellationToken,
    device: String,
    session: String,
) {
    if token.is_cancelled() {
        return;
    }
    let Some(chat) = &mut state.chat else { return };
    if chat.device != device || chat.session.id != session {
        return;
    }
    if !state.foreground || state.screen != Screen::Chat || !missing(chat) {
        chat.approval_refresh_pending = false;
        return;
    }
    let epoch = state.chat_call_epoch;
    let tx = ctx.tx.clone();
    spawn(ctx, async move {
        let result = tokio::select! {
            _ = token.cancelled() => return Msg::Ignored,
            result = operations::protected_chat_call(tx, epoch, device.clone(), "claude".into(), Req::AgentInspect(v1::AgentInspectRequest {
                session: session.clone(), op_id: String::new(),
            })) => result,
        };
        Msg::ApprovalsRefreshed {
            token,
            device,
            session,
            error: result.err().map(|e| e.to_string()),
        }
    });
}

pub(super) fn finished(
    state: &mut State,
    token: CancellationToken,
    device: String,
    session: String,
    error: Option<String>,
) {
    if token.is_cancelled() {
        return;
    }
    let Some(chat) = &mut state.chat else { return };
    if chat.device != device || chat.session.id != session {
        return;
    }
    chat.approval_refresh_pending = false;
    if missing(chat)
        && let Some(error) = error
    {
        state.toast(
            format!("Could not read Claude's permission card: {error}"),
            true,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn waiting() -> Chat {
        Chat {
            session: v1::AgentSession {
                provider: "claude".into(),
                mode: "code".into(),
                id: "11111111-1111-4111-8111-111111111111".into(),
                state: "waiting_approval".into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
    #[test]
    fn only_missing_required_local_claude_interactions_need_inspection() {
        let mut c = waiting();
        assert!(missing(&c));
        for kind in ["approval", "question"] {
            c.interactions = vec![v1::PendingInteraction {
                kind: kind.into(),
                ..Default::default()
            }];
            assert!(!missing(&c));
            c.interactions[0].optional = true;
            assert!(missing(&c));
        }
        c.interactions.clear();
        c.session.read_only = true;
        assert!(!missing(&c));
        c.session.read_only = false;
        for id in ["session_remote", "cloud:claude:example", "orca:example"] {
            c.session.id = id.into();
            assert!(!missing(&c));
        }
        c = waiting();
        c.session.provider = "codex".into();
        assert!(!missing(&c));
        c = waiting();
        c.session.mode = "chat".into();
        assert!(!missing(&c));
        c = waiting();
        c.session.state = "running".into();
        assert!(!missing(&c));
    }
}
