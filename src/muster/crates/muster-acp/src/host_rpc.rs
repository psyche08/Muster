//! The [`Host`] `musterd` implements: every turn runs in `NativeRuntime`
//! behind the daemon's own socket (ADR 0009).
//!
//! Nothing here owns session state or decides authorization. A session id is
//! the daemon's, an operation carries an `op_id` so the daemon's ledger can
//! refuse a replay, and a turn ends only on the provider-confirmed turn row
//! the daemon projects — output arriving is not an outcome.

use std::collections::HashMap;

use async_trait::async_trait;
use muster_proto::v1::{self, request::Req, response::Resp};
use muster_rpc::Client;
use tokio::sync::mpsc;

use crate::{Host, StopReason, TurnEvent};

/// How many turn events may queue before the reader slows down. A turn's rows
/// are small and the writer drains them immediately.
const EVENT_QUEUE: usize = 256;

/// A [`Host`] backed by one `musterd` connection.
pub struct RpcHost {
    client: Client,
    /// Which provider a new session uses: the daemon validates the value.
    agent: String,
    /// Environment each new session gives its provider child. This is how the
    /// scheduler's per-task credential reaches the agent, which needs it to
    /// read its own issue and post its result.
    env: HashMap<String, String>,
}

impl RpcHost {
    pub fn new(client: Client, agent: &str, env: HashMap<String, String>) -> Self {
        Self {
            client,
            agent: agent.to_owned(),
            env,
        }
    }

    /// A per-operation id. The daemon's ledger keys replay protection on it,
    /// so it must be fresh for a new intent and identical for a retry of the
    /// same one; this server never retries, so fresh is always right here.
    fn op_id(&self) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("acp-{nanos}-{}", std::process::id())
    }

    /// Send text into `session` as a write the daemon ledgers.
    async fn send(&self, session: &str, text: &str) -> Result<(), String> {
        let response = self
            .client
            .call(Req::AgentSend(v1::AgentSendRequest {
                op_id: self.op_id(),
                session: session.to_owned(),
                text: text.to_owned(),
                attachments: Vec::new(),
                release_unconfirmed: String::new(),
            }))
            .await
            .map_err(|error| error.to_string())?;
        match response {
            Resp::OpResult(result) => accept_op(result),
            other => Err(format!("unexpected reply to agent.send: {other:?}")),
        }
    }
}

#[async_trait]
impl Host for RpcHost {
    async fn create(&self, cwd: &str) -> Result<String, String> {
        if cwd.is_empty() {
            return Err("the scheduler supplied no working directory".to_owned());
        }
        let response = self
            .client
            .call(Req::OrcaCreate(v1::OrcaCreateRequest {
                op_id: self.op_id(),
                directory: cwd.to_owned(),
                agent: self.agent.clone(),
                env: self.env.clone(),
            }))
            .await
            .map_err(|error| error.to_string())?;
        match response {
            Resp::OpResult(result) => {
                let session = result.result_id.clone();
                accept_op(result)?;
                if session.is_empty() {
                    return Err("the daemon applied orca.create without a session id".to_owned());
                }
                Ok(session)
            }
            other => Err(format!("unexpected reply to orca.create: {other:?}")),
        }
    }

    async fn prompt(&self, session: &str, text: &str) -> Result<mpsc::Receiver<TurnEvent>, String> {
        // Watch before sending: a turn that starts between the send and the
        // subscription would stream its first rows to nobody.
        let mut call = self
            .client
            .start(Req::AgentWatch(v1::AgentWatchRequest {
                session: session.to_owned(),
                ..Default::default()
            }))
            .await
            .map_err(|error| error.to_string())?;
        if let Err(error) = self.send(session, text).await {
            call.cancel().await;
            return Err(error);
        }
        let (tx, rx) = mpsc::channel(EVENT_QUEUE);
        tokio::spawn(async move {
            while let Some(event) = call.events.recv().await {
                let v1::event::Ev::AgentEvent(agent) = event else {
                    continue;
                };
                let Some(v1::agent_event::Ev::Message(message)) = agent.ev else {
                    continue;
                };
                let Some(turn) = turn_event(&message) else {
                    continue;
                };
                let settled = matches!(turn, TurnEvent::Settled(_));
                if tx.send(turn).await.is_err() || settled {
                    break;
                }
            }
            // Dropping the sender ends the turn stream. When no settled row
            // arrived, the server reports the turn as having no recorded
            // outcome rather than inventing one.
            call.cancel().await;
        });
        Ok(rx)
    }

    /// Mid-turn steering is **not** available through this path yet.
    ///
    /// The runtime holds one turn per session: a second write while a turn is
    /// live is refused with `conflict: Wait for the current turn, answer its
    /// prompt, or interrupt it`, verified against a running daemon on
    /// 2026-10-21. Landing a reply inside the turn needs the provider's own
    /// steering (Codex `turn/steer`, Claude's pre-tool context), which no
    /// Muster RPC exposes today, so this reports the refusal instead of
    /// silently queueing the text for a later turn.
    ///
    /// Until Shared Rust grows that operation, the version this server
    /// reports must stay below a scheduler's steering floor, so the scheduler
    /// never believes the capability is there (see `Identity::version` and
    /// ADR 0009).
    async fn interject(&self, session: &str, text: &str) -> Result<(), String> {
        self.send(session, text).await.map_err(|error| {
            format!("{error} (mid-turn steering is not implemented; see ADR 0009)")
        })
    }

    async fn interrupt(&self, session: &str) -> Result<(), String> {
        let response = self
            .client
            .call(Req::AgentInterrupt(v1::AgentInterruptRequest {
                op_id: self.op_id(),
                session: session.to_owned(),
            }))
            .await
            .map_err(|error| error.to_string())?;
        match response {
            Resp::OpResult(result) => accept_op(result),
            other => Err(format!("unexpected reply to agent.interrupt: {other:?}")),
        }
    }
}

/// An operation counts as done only when the daemon applied it, or returned
/// the result of the identical earlier one. A rejection and an undecidable
/// outcome are both reported; neither is retried here.
fn accept_op(result: v1::OpResult) -> Result<(), String> {
    use v1::op_result::Status;
    match Status::try_from(result.status) {
        Ok(Status::Applied) | Ok(Status::Duplicate) => Ok(()),
        Ok(Status::Rejected) => Err(if result.message.is_empty() {
            "the daemon rejected the operation".to_owned()
        } else {
            result.message
        }),
        Ok(Status::UserConfirmed) => Err(
            "Delivery was manually confirmed by the user; no provider receipt, do not resend"
                .into(),
        ),
        Ok(Status::Unknown) => Err(format!(
            "the operation's outcome is unknown and was not retried: {}",
            result.message
        )),
        Err(_) => Err(format!("unknown operation status {}", result.status)),
    }
}

/// Project one daemon message row into a turn event, or nothing when the row
/// carries no turn signal this server reports.
fn turn_event(message: &v1::AgentMessage) -> Option<TurnEvent> {
    match message.kind.as_str() {
        "turn_status" => turn_outcome(&message.text).map(TurnEvent::Settled),
        "text" if message.role == "assistant" => {
            (!message.text.is_empty()).then(|| TurnEvent::Text(message.text.clone()))
        }
        "tool_call" => {
            let title = message.text.lines().next().unwrap_or_default().to_owned();
            (!title.is_empty()).then_some(TurnEvent::Tool(title))
        }
        _ => None,
    }
}

/// Read the provider-confirmed turn outcome out of a `turn_status` row.
///
/// The row's text is the daemon's own rendering of the journal's turn item
/// (`muster_agent::orca::render`), `Turn: <state>` followed by any provider
/// error. A state this build does not know returns `None`: the turn then
/// counts as unsettled, which is reported as an unknown outcome rather than
/// presented as success. A typed outcome field on the row would remove this
/// parse; that is a protocol change, recorded as follow-up in ADR 0009.
fn turn_outcome(text: &str) -> Option<StopReason> {
    let rest = text.strip_prefix("Turn: ")?;
    let (state, detail) = match rest.split_once('\n') {
        Some((state, detail)) => (state, detail.trim()),
        None => (rest, ""),
    };
    match state.trim() {
        "completed" => Some(StopReason::EndTurn),
        "interrupted" | "cancelled" => Some(StopReason::Cancelled),
        "failed" => Some(StopReason::Error(if detail.is_empty() {
            "the provider reported a failed turn".to_owned()
        } else {
            detail.to_owned()
        })),
        // "running" is progress, not an outcome; anything else is a state
        // this build has not been taught.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(kind: &str, role: &str, text: &str) -> v1::AgentMessage {
        v1::AgentMessage {
            id: "r1".into(),
            role: role.into(),
            text: text.into(),
            kind: kind.into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_completed_turn_row_settles_the_turn() {
        assert_eq!(turn_outcome("Turn: completed"), Some(StopReason::EndTurn));
        assert_eq!(
            turn_event(&row("turn_status", "system", "Turn: completed")),
            Some(TurnEvent::Settled(StopReason::EndTurn))
        );
    }

    #[test]
    fn a_failed_turn_carries_the_providers_own_message() {
        assert_eq!(
            turn_outcome("Turn: failed\nPlease upgrade Codex."),
            Some(StopReason::Error("Please upgrade Codex.".to_owned()))
        );
        assert_eq!(
            turn_outcome("Turn: failed"),
            Some(StopReason::Error(
                "the provider reported a failed turn".to_owned()
            ))
        );
    }

    #[test]
    fn interrupted_and_cancelled_turns_are_cancellations() {
        assert_eq!(
            turn_outcome("Turn: interrupted"),
            Some(StopReason::Cancelled)
        );
        assert_eq!(turn_outcome("Turn: cancelled"), Some(StopReason::Cancelled));
    }

    #[test]
    fn a_running_or_unknown_turn_state_does_not_settle() {
        // Reporting either as end_turn would present progress, or a state
        // this build has not been taught, as a finished turn.
        assert_eq!(turn_outcome("Turn: running"), None);
        assert_eq!(turn_outcome("Turn: teleported"), None);
        assert_eq!(turn_outcome("Turn: unknown"), None);
        assert_eq!(turn_outcome("something else entirely"), None);
    }

    #[test]
    fn assistant_text_streams_and_other_roles_do_not() {
        assert_eq!(
            turn_event(&row("text", "assistant", "READY")),
            Some(TurnEvent::Text("READY".to_owned()))
        );
        assert_eq!(turn_event(&row("text", "user", "go")), None);
        assert_eq!(turn_event(&row("text", "assistant", "")), None);
    }

    #[test]
    fn a_tool_row_reports_its_first_line_as_the_title() {
        assert_eq!(
            turn_event(&row(
                "tool_call",
                "tool",
                "read readme.txt\n{\"path\":\"…\"}"
            )),
            Some(TurnEvent::Tool("read readme.txt".to_owned()))
        );
        assert_eq!(turn_event(&row("tool_result", "tool", "ok")), None);
    }

    #[test]
    fn an_applied_or_duplicate_operation_is_accepted_and_the_rest_reported() {
        let op = |status: i32, message: &str| v1::OpResult {
            status,
            message: message.into(),
            ..Default::default()
        };
        assert!(accept_op(op(0, "")).is_ok());
        assert!(accept_op(op(1, "")).is_ok());
        assert_eq!(accept_op(op(2, "locked")).unwrap_err(), "locked".to_owned());
        // An undecidable write must never read as success.
        assert!(
            accept_op(op(3, "crashed"))
                .unwrap_err()
                .contains("outcome is unknown")
        );
    }
}
