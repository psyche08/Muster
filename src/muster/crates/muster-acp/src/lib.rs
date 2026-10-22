//! muster-acp: the Agent Client Protocol surface an external task scheduler
//! drives when it launches Muster as its runtime command (ADR 0009).
//!
//! The scheduler owns the queue, the working directory and the reporting; it
//! spawns this command in that directory and speaks newline-delimited
//! JSON-RPC 2.0 on stdio. Everything a turn needs — the provider child, the
//! journal, the ledger, approvals — stays behind [`Host`], which `musterd`
//! implements over its own socket. This crate decides nothing about
//! authorization and owns no session state beyond the ids it is handed.
//!
//! A turn runs as its own task while stdin keeps being served, because the
//! scheduler delivers a mid-turn interjection as a request on the same pipe:
//! a server that only reads between turns cannot receive one.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, mpsc};

mod host_rpc;
pub use host_rpc::RpcHost;

/// What this build answers a version probe with, and what `agentInfo` carries.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The ACP protocol revision this server speaks.
const PROTOCOL_VERSION: u32 = 1;

/// Longest single frame read from the scheduler. A turn's prompt is the
/// largest thing it sends, and the scheduler already bounds that; this only
/// stops an unbounded line from exhausting memory.
const MAX_FRAME: u64 = 8 * 1024 * 1024;

/// How long a turn still recording may keep this process alive after the
/// scheduler closed stdin.
const DRAIN_AFTER_EOF: std::time::Duration = std::time::Duration::from_secs(30);

/// Why a turn stopped, in the protocol's own vocabulary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    Cancelled,
    Refusal,
    /// The turn did not settle; the message is reported as the turn's error.
    Error(String),
}

impl StopReason {
    fn wire(&self) -> &str {
        match self {
            StopReason::EndTurn => "end_turn",
            StopReason::Cancelled => "cancelled",
            StopReason::Refusal => "refusal",
            StopReason::Error(_) => "refusal",
        }
    }
}

/// One thing that happened inside a turn, already projected by the host.
///
/// The host reports what the provider recorded. Arrival of output is not
/// success and absence of output is not failure: only [`TurnEvent::Settled`]
/// ends a turn, and a stream that ends without it is reported as unsettled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TurnEvent {
    /// Assistant prose for the reader.
    Text(String),
    /// Reasoning the provider marked as such.
    Thought(String),
    /// A tool call, by the title the provider gave it.
    Tool(String),
    /// The provider-confirmed end of the turn.
    Settled(StopReason),
}

/// Everything this server needs from Muster. One session id is one Muster
/// session; this crate never invents one.
#[async_trait]
pub trait Host: Send + Sync {
    /// Open a session that works in `cwd`. The returned id is opaque here.
    async fn create(&self, cwd: &str) -> Result<String, String>;

    /// Send `text` as a new turn and stream what the turn records.
    async fn prompt(&self, session: &str, text: &str) -> Result<mpsc::Receiver<TurnEvent>, String>;

    /// Deliver `text` into the turn already running in `session`.
    ///
    /// A host that cannot land a reply inside a live turn reports that here.
    /// Accepting the text and holding it for the next turn would tell the
    /// scheduler the reply was taken when it was not.
    async fn interject(&self, session: &str, text: &str) -> Result<(), String>;

    /// Ask the running turn to stop.
    async fn interrupt(&self, session: &str) -> Result<(), String>;

    /// Adopt a session the scheduler resumed. The default accepts the id as
    /// given, which is what a host with durable sessions wants.
    async fn resume(&self, session: &str) -> Result<String, String> {
        Ok(session.to_owned())
    }
}

/// How this server presents itself to the scheduler.
#[derive(Clone, Debug)]
pub struct Identity {
    /// The version a probe and `agentInfo` report.
    ///
    /// A scheduler may gate features on a protocol family's version floor, so
    /// this is load-bearing, not cosmetic: reporting a version whose
    /// behaviour this build does not implement would have the scheduler
    /// dispatch work this runtime then fails to honour. The default is
    /// Muster's own version, which claims nothing.
    pub version: String,
    /// Authentication methods to advertise. Some schedulers refuse to start a
    /// runtime that advertises none, so this is configurable rather than
    /// empty; Muster's own trust decisions are not made here.
    pub auth_methods: Vec<String>,
}

impl Default for Identity {
    fn default() -> Self {
        Self {
            version: VERSION.to_owned(),
            auth_methods: vec!["cached_token".to_owned()],
        }
    }
}

/// A line-delimited JSON-RPC writer shared by the reader and every turn task.
struct Writer<W> {
    inner: Mutex<W>,
}

impl<W: AsyncWrite + Unpin + Send> Writer<W> {
    async fn send(&self, frame: Value) {
        let mut line = frame.to_string();
        line.push('\n');
        let mut guard = self.inner.lock().await;
        // A scheduler that closed the pipe is reported by the read side; a
        // failed write here must not abort a turn that is still recording.
        let _ = guard.write_all(line.as_bytes()).await;
        let _ = guard.flush().await;
    }

    async fn result(&self, id: Value, result: Value) {
        self.send(json!({"jsonrpc": "2.0", "id": id, "result": result}))
            .await;
    }

    async fn error(&self, id: Value, code: i32, message: &str) {
        self.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": code, "message": message},
        }))
        .await;
    }

    async fn update(&self, session: &str, update: Value) {
        self.send(json!({
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": {"sessionId": session, "update": update},
        }))
        .await;
    }
}

/// Serve one scheduler connection until it closes stdin or closes the session.
pub async fn serve<R, W, H>(
    reader: R,
    writer: W,
    host: Arc<H>,
    identity: Identity,
) -> anyhow::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
    H: Host + 'static,
{
    let writer = Arc::new(Writer {
        inner: Mutex::new(writer),
    });
    let mut lines = BufReader::new(reader.take(MAX_FRAME)).lines();
    // Turns in flight, so a cancel can reach the right one and a closing
    // session does not leave a task writing into a dead pipe.
    let mut turns: HashMap<String, tokio::task::JoinHandle<()>> = HashMap::new();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let frame: Value = match serde_json::from_str(&line) {
            Ok(frame) => frame,
            Err(error) => {
                // A frame we cannot parse has no id to answer, so say so once
                // with the null id the protocol reserves for exactly this.
                writer
                    .error(Value::Null, -32700, &format!("parse error: {error}"))
                    .await;
                continue;
            }
        };
        let id = frame.get("id").cloned().unwrap_or(Value::Null);
        let method = frame
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let params = frame.get("params").cloned().unwrap_or(Value::Null);
        let session_of = |params: &Value| {
            params
                .get("sessionId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };

        match method.as_str() {
            "initialize" => {
                let methods: Vec<Value> = identity
                    .auth_methods
                    .iter()
                    .map(|id| json!({"id": id, "name": id, "description": ""}))
                    .collect();
                writer
                    .result(
                        id,
                        json!({
                            "protocolVersion": PROTOCOL_VERSION,
                            "agentCapabilities": {
                                "loadSession": true,
                                "promptCapabilities": {
                                    "image": false,
                                    "audio": false,
                                    "embeddedContext": false,
                                },
                            },
                            "authMethods": methods,
                            "agentInfo": {"name": "muster", "version": identity.version},
                        }),
                    )
                    .await;
            }
            // Muster's own trust model decides what a session may do; there is
            // no separate credential to present here.
            "authenticate" => writer.result(id, json!({})).await,
            "session/new" => {
                let cwd = params
                    .get("cwd")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                match host.create(&cwd).await {
                    Ok(session) => {
                        writer
                            .result(
                                id,
                                json!({"sessionId": session, "models": {"availableModels": []}}),
                            )
                            .await;
                    }
                    Err(error) => writer.error(id, -32603, &error).await,
                }
            }
            "session/load" | "session/resume" => {
                let session = session_of(&params);
                match host.resume(&session).await {
                    Ok(session) => writer.result(id, json!({"sessionId": session})).await,
                    Err(error) => writer.error(id, -32603, &error).await,
                }
            }
            // Accepted and ignored: the model and the permission preset are
            // the provider configuration Muster already owns, and claiming to
            // have changed them would be a lie to the scheduler.
            "session/set_model" | "session/set_config_option" => writer.result(id, json!({})).await,
            "session/prompt" => {
                let session = session_of(&params);
                let text = prompt_text(&params);
                match host.prompt(&session, &text).await {
                    Ok(events) => {
                        let writer = Arc::clone(&writer);
                        let handle = tokio::spawn(async move {
                            run_turn(writer, session, id, events).await;
                        });
                        turns.insert(session_of(&params), handle);
                    }
                    Err(error) => writer.error(id, -32603, &error).await,
                }
            }
            // The scheduler's mid-turn interjection. Grok's ACP extension
            // wraps its acknowledgement one level deeper than the JSON-RPC
            // result, and "queued" is the only value it accepts.
            "_x.ai/interject" | "_session/steer" => {
                let session = session_of(&params);
                let text = params
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                match host.interject(&session, &text).await {
                    Ok(()) => {
                        writer
                            .result(id, json!({"result": {"status": "queued"}}))
                            .await;
                    }
                    Err(error) => writer.error(id, -32603, &error).await,
                }
            }
            "session/cancel" => {
                let session = session_of(&params);
                let outcome = host.interrupt(&session).await;
                if !id.is_null() {
                    match outcome {
                        Ok(()) => writer.result(id, json!({})).await,
                        Err(error) => writer.error(id, -32603, &error).await,
                    }
                }
            }
            "session/close" => {
                let session = session_of(&params);
                if let Some(handle) = turns.remove(&session) {
                    handle.abort();
                }
                if !id.is_null() {
                    writer.result(id, json!({})).await;
                }
                break;
            }
            other => {
                if !id.is_null() {
                    writer
                        .error(id, -32601, &format!("method not found: {other}"))
                        .await;
                }
            }
        }
    }

    // The scheduler closed stdin. A turn still recording has output the
    // scheduler is waiting for, so let it finish writing rather than dropping
    // what the provider already produced — but bounded, because a turn that
    // never settles must not keep this process alive for ever.
    for (_, handle) in turns {
        let _ = tokio::time::timeout(DRAIN_AFTER_EOF, handle).await;
    }
    Ok(())
}

/// Stream one turn's events, then answer the prompt request that opened it.
async fn run_turn<W>(
    writer: Arc<Writer<W>>,
    session: String,
    id: Value,
    mut events: mpsc::Receiver<TurnEvent>,
) where
    W: AsyncWrite + Unpin + Send + 'static,
{
    let mut settled = None;
    while let Some(event) = events.recv().await {
        match event {
            TurnEvent::Text(text) => {
                writer
                    .update(
                        &session,
                        json!({
                            "sessionUpdate": "agent_message_chunk",
                            "content": {"type": "text", "text": text},
                        }),
                    )
                    .await;
            }
            TurnEvent::Thought(text) => {
                writer
                    .update(
                        &session,
                        json!({
                            "sessionUpdate": "agent_thought_chunk",
                            "content": {"type": "text", "text": text},
                        }),
                    )
                    .await;
            }
            TurnEvent::Tool(title) => {
                writer
                    .update(
                        &session,
                        json!({
                            "sessionUpdate": "tool_call",
                            "toolCallId": format!("{session}-{title}"),
                            "title": title,
                            "status": "completed",
                        }),
                    )
                    .await;
            }
            TurnEvent::Settled(reason) => {
                settled = Some(reason);
                break;
            }
        }
    }
    match settled {
        Some(StopReason::Error(message)) => writer.error(id, -32603, &message).await,
        Some(reason) => {
            writer
                .result(id, json!({"stopReason": reason.wire()}))
                .await;
        }
        // The stream ended without a provider-confirmed outcome. Reporting
        // end_turn here would present an unknown as a success.
        None => {
            writer
                .error(id, -32603, "turn ended without a recorded outcome")
                .await;
        }
    }
}

/// Flatten the prompt blocks the scheduler sends into the text of one turn.
/// Only text blocks are declared in `promptCapabilities`, so anything else
/// arriving here is skipped rather than guessed at.
fn prompt_text(params: &Value) -> String {
    let blocks = match params.get("prompt").and_then(Value::as_array) {
        Some(blocks) => blocks,
        None => return String::new(),
    };
    blocks
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    #[derive(Default)]
    struct Fake {
        sent: StdMutex<Vec<(String, String)>>,
        interjected: StdMutex<Vec<(String, String)>>,
        interrupted: StdMutex<Vec<String>>,
        /// Events each prompt should stream, in order of call.
        script: StdMutex<Vec<Vec<TurnEvent>>>,
        /// Held so a turn stays open until the test pushes its events.
        held: StdMutex<Option<mpsc::Sender<TurnEvent>>>,
        hold: bool,
    }

    #[async_trait]
    impl Host for Fake {
        async fn create(&self, cwd: &str) -> Result<String, String> {
            if cwd.is_empty() {
                return Err("cwd is required".into());
            }
            Ok(format!("session-for-{cwd}"))
        }

        async fn prompt(
            &self,
            session: &str,
            text: &str,
        ) -> Result<mpsc::Receiver<TurnEvent>, String> {
            self.sent
                .lock()
                .unwrap()
                .push((session.to_owned(), text.to_owned()));
            let (tx, rx) = mpsc::channel(16);
            if self.hold {
                *self.held.lock().unwrap() = Some(tx);
                return Ok(rx);
            }
            let events = {
                let mut script = self.script.lock().unwrap();
                if script.is_empty() {
                    vec![TurnEvent::Settled(StopReason::EndTurn)]
                } else {
                    script.remove(0)
                }
            };
            for event in events {
                let _ = tx.send(event).await;
            }
            Ok(rx)
        }

        async fn interject(&self, session: &str, text: &str) -> Result<(), String> {
            self.interjected
                .lock()
                .unwrap()
                .push((session.to_owned(), text.to_owned()));
            // A real host folds the interjection into the running turn, which
            // then finishes. Settle the held turn so the test does not wait
            // out the post-EOF drain.
            if let Some(sender) = self.held.lock().unwrap().take() {
                let _ = sender.try_send(TurnEvent::Settled(StopReason::EndTurn));
            }
            Ok(())
        }

        async fn interrupt(&self, session: &str) -> Result<(), String> {
            self.interrupted.lock().unwrap().push(session.to_owned());
            Ok(())
        }
    }

    /// Drive the server with the given frames and return what it wrote.
    async fn exchange(host: Arc<Fake>, input: &str) -> Vec<Value> {
        // The server owns its writer, so read what it writes through a pipe.
        let reader = std::io::Cursor::new(input.as_bytes().to_vec());
        let (client, server) = tokio::io::duplex(64 * 1024);
        let serving =
            tokio::spawn(async move { serve(reader, server, host, Identity::default()).await });
        let mut frames = Vec::new();
        let mut lines = BufReader::new(client).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            frames.push(serde_json::from_str(&line).expect("server wrote valid JSON"));
        }
        serving.await.expect("server task").expect("server run");
        frames
    }

    fn frame(id: u32, method: &str, params: Value) -> String {
        format!(
            "{}\n",
            json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
        )
    }

    #[tokio::test]
    async fn initialize_advertises_an_auth_method_and_muster_identity() {
        let frames = exchange(
            Arc::new(Fake::default()),
            &frame(0, "initialize", json!({"protocolVersion": 1})),
        )
        .await;
        let result = &frames[0]["result"];
        assert_eq!(result["protocolVersion"], 1);
        assert_eq!(result["agentInfo"]["name"], "muster");
        assert_eq!(result["agentInfo"]["version"], VERSION);
        // A scheduler that sees no method refuses to start the runtime.
        assert_eq!(result["authMethods"][0]["id"], "cached_token");
        assert_eq!(result["agentCapabilities"]["loadSession"], true);
    }

    #[tokio::test]
    async fn session_new_returns_the_hosts_own_session_id() {
        let frames = exchange(
            Arc::new(Fake::default()),
            &frame(1, "session/new", json!({"cwd": "/w"})),
        )
        .await;
        assert_eq!(frames[0]["result"]["sessionId"], "session-for-/w");
    }

    #[tokio::test]
    async fn session_new_reports_a_host_refusal_instead_of_inventing_a_session() {
        let frames = exchange(
            Arc::new(Fake::default()),
            &frame(1, "session/new", json!({"cwd": ""})),
        )
        .await;
        assert_eq!(frames[0]["error"]["message"], "cwd is required");
        assert!(frames[0].get("result").is_none());
    }

    #[tokio::test]
    async fn a_turn_streams_text_then_answers_the_prompt() {
        let host = Arc::new(Fake {
            script: StdMutex::new(vec![vec![
                TurnEvent::Text("READY".into()),
                TurnEvent::Settled(StopReason::EndTurn),
            ]]),
            ..Default::default()
        });
        let frames = exchange(
            Arc::clone(&host),
            &frame(
                2,
                "session/prompt",
                json!({"sessionId": "s1", "prompt": [{"type": "text", "text": "go"}]}),
            ),
        )
        .await;
        assert_eq!(frames[0]["method"], "session/update");
        assert_eq!(
            frames[0]["params"]["update"]["sessionUpdate"],
            "agent_message_chunk"
        );
        assert_eq!(frames[0]["params"]["update"]["content"]["text"], "READY");
        assert_eq!(frames[1]["id"], 2);
        assert_eq!(frames[1]["result"]["stopReason"], "end_turn");
        assert_eq!(
            host.sent.lock().unwrap().as_slice(),
            [("s1".to_owned(), "go".to_owned())]
        );
    }

    #[tokio::test]
    async fn a_turn_that_never_settles_is_an_error_not_an_end_turn() {
        let host = Arc::new(Fake {
            script: StdMutex::new(vec![vec![TurnEvent::Text("partial".into())]]),
            ..Default::default()
        });
        let frames = exchange(
            host,
            &frame(
                3,
                "session/prompt",
                json!({"sessionId": "s1", "prompt": [{"type": "text", "text": "go"}]}),
            ),
        )
        .await;
        assert_eq!(frames[1]["id"], 3);
        assert_eq!(
            frames[1]["error"]["message"],
            "turn ended without a recorded outcome"
        );
    }

    #[tokio::test]
    async fn an_interjection_is_served_while_a_turn_is_still_open() {
        // The turn is held open, so this only passes if stdin keeps being
        // read during a turn — the whole reason a turn runs as its own task.
        let host = Arc::new(Fake {
            hold: true,
            ..Default::default()
        });
        let input = format!(
            "{}{}",
            frame(
                4,
                "session/prompt",
                json!({"sessionId": "s1", "prompt": [{"type": "text", "text": "go"}]}),
            ),
            frame(
                5,
                "_x.ai/interject",
                json!({"sessionId": "s1", "text": "also this"}),
            ),
        );
        let frames = exchange(Arc::clone(&host), &input).await;
        let ack = frames
            .iter()
            .find(|frame| frame["id"] == 5)
            .expect("the interjection was answered");
        // The scheduler rejects anything but this nested shape.
        assert_eq!(ack["result"]["result"]["status"], "queued");
        assert_eq!(
            host.interjected.lock().unwrap().as_slice(),
            [("s1".to_owned(), "also this".to_owned())]
        );
    }

    #[tokio::test]
    async fn cancel_interrupts_the_session_and_close_ends_the_server() {
        let host = Arc::new(Fake::default());
        let input = format!(
            "{}{}",
            frame(6, "session/cancel", json!({"sessionId": "s1"})),
            frame(7, "session/close", json!({"sessionId": "s1"})),
        );
        let frames = exchange(Arc::clone(&host), &input).await;
        assert_eq!(frames[0]["id"], 6);
        assert_eq!(frames[1]["id"], 7);
        assert_eq!(host.interrupted.lock().unwrap().as_slice(), ["s1"]);
    }

    #[tokio::test]
    async fn an_unknown_method_is_refused_by_name() {
        let frames = exchange(
            Arc::new(Fake::default()),
            &frame(8, "session/teleport", json!({})),
        )
        .await;
        assert_eq!(frames[0]["error"]["code"], -32601);
        assert_eq!(
            frames[0]["error"]["message"],
            "method not found: session/teleport"
        );
    }

    #[tokio::test]
    async fn an_unparseable_frame_is_answered_without_ending_the_server() {
        let input = format!(
            "{}{}",
            "{not json\n",
            frame(9, "initialize", json!({"protocolVersion": 1})),
        );
        let frames = exchange(Arc::new(Fake::default()), &input).await;
        assert_eq!(frames[0]["error"]["code"], -32700);
        assert_eq!(frames[1]["id"], 9);
    }

    #[test]
    fn prompt_text_joins_text_blocks_and_skips_the_rest() {
        let params = json!({"prompt": [
            {"type": "text", "text": "first"},
            {"type": "image", "data": "…"},
            {"type": "text", "text": "second"},
        ]});
        assert_eq!(prompt_text(&params), "first\nsecond");
        assert_eq!(prompt_text(&json!({})), "");
    }
}
