//! Foreground, bounded reads of missing local Claude permission cards.
use super::*;

fn missing(chat: &Chat) -> bool {
    chat.session.provider == "claude"
        && chat.session.mode == "code"
        && muster_agent::claude_identity::local_id(&chat.session.id)
        && !chat.session.read_only
        && (chat.session.state == "waiting_approval" || chat.session.process_state == "running")
        && !chat
            .interactions
            .iter()
            .any(|i| !i.optional && matches!(i.kind.as_str(), "approval" | "question"))
}

/// A waiting-episode boundary between two definite provider states admits one
/// more guarded read: entering an episode means its card has never been read
/// here, and leaving one ends the episode an earlier read covered. Repeated
/// running/idle frames do not rearm, and an unknown state is lost sight of the
/// same episode rather than a boundary, in either direction.
pub(super) fn episode_boundary(chat: &mut Chat, next: &str) {
    let definite = |s: &str| matches!(s, "running" | "idle" | "ended" | "waiting_approval");
    let waiting = |s: &str| s == "waiting_approval";
    if !chat.approval_refresh_pending
        && definite(&chat.session.state)
        && definite(next)
        && waiting(&chat.session.state) != waiting(next)
    {
        chat.approval_refresh_attempted = false;
    }
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
    chat.approval_refresh_attempted = true;
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
    ctx: &Ctx,
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
        if error == format!("unavailable: {}", muster_proto::INSPECT_LOCAL_INPUT) {
            chat.approval_refresh_attempted = false;
            chat.approval_refresh_pending = true;
            spawn(ctx, async move {
                tokio::select! {
                    _ = token.cancelled()=> Msg::Ignored,
                    _ = tokio::time::sleep(Duration::from_secs(35)) => Msg::RefreshApprovals{token,device,session},
                }
            });
            return;
        }
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
        c.session.process_state = "running".into();
        assert!(
            missing(&c),
            "a bound live worker may have an unobserved AX card"
        );
        c.session.state = "idle".into();
        assert!(
            missing(&c),
            "idle metadata cannot prove absence of an AX card"
        );
    }

    #[test]
    fn a_waiting_episode_opening_after_the_entry_read_is_inspected_once() {
        // Entered while the worker was running: the entry read found no card
        // because none existed yet. The episode that opens next is new.
        let mut c = waiting();
        c.session.state = "running".into();
        c.session.process_state = "running".into();
        c.approval_refresh_attempted = true;
        episode_boundary(&mut c, "waiting_approval");
        assert!(!c.approval_refresh_attempted);
        assert!(missing(&c), "the new episode still has no observed card");

        // One read per episode: repeated waiting frames do not rearm.
        c.session.state = "waiting_approval".into();
        c.approval_refresh_attempted = true;
        episode_boundary(&mut c, "waiting_approval");
        assert!(c.approval_refresh_attempted);

        // Losing sight of a session is not an episode boundary either way.
        for (from, to) in [
            ("waiting_approval", "unknown"),
            ("unknown", "waiting_approval"),
        ] {
            c.session.state = from.into();
            c.approval_refresh_attempted = true;
            episode_boundary(&mut c, to);
            assert!(
                c.approval_refresh_attempted,
                "{from} -> {to} is not a boundary"
            );
        }

        // A read already in flight owns this episode.
        c.session.state = "running".into();
        c.approval_refresh_attempted = true;
        c.approval_refresh_pending = true;
        episode_boundary(&mut c, "waiting_approval");
        assert!(c.approval_refresh_attempted);

        // Leaving an episode still ends the one an earlier read covered.
        c.approval_refresh_pending = false;
        for ended in ["running", "idle", "ended"] {
            c.session.state = "waiting_approval".into();
            c.approval_refresh_attempted = true;
            episode_boundary(&mut c, ended);
            assert!(!c.approval_refresh_attempted, "waiting -> {ended}");
        }
    }
}
