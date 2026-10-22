//! The next message Codex's composer suggests after a completed turn.
//!
//! The Codex desktop app shows a predicted next user message as its composer
//! placeholder but keeps it in memory only: it is not in the rollout, the
//! app's stores or the Accessibility tree. The service therefore asks a
//! short-lived `codex app-server` for one, the way the app does: an
//! ephemeral, read-only fork of the rollout runs one turn that answers
//! `{"suggestion": …}`. The fork is never persisted, the source thread is
//! not resumed (the app holds its writer), and the process exits once the
//! turn has completed.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};

/// One suggestion request: the completed turn of a rollout it follows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub thread: String,
    pub turn: String,
    pub rollout: PathBuf,
}

/// Produces the suggestion that follows a completed turn.
#[async_trait::async_trait]
pub trait Predictor: Send + Sync {
    /// The suggested next user message; `None` when there is none.
    async fn predict(&self, request: Request) -> Option<String>;
}

/// The newest turn marker of a Codex rollout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Turn {
    /// The newest turn completed (`task_complete`), by turn id.
    Completed(String),
    /// Running, aborted, or no marker in the scanned tail.
    Other,
}

/// How far back from its end a rollout is scanned for the newest marker.
const TAIL: u64 = 4 * 1024 * 1024;

/// The newest `task_started` / `task_complete` / `turn_aborted` record.
pub(crate) fn latest_turn(rollout: &Path) -> Turn {
    crate::recency::last_record_within(rollout, TAIL, |record| {
        if record["type"] != "event_msg" {
            return None;
        }
        let payload = &record["payload"];
        match payload["type"].as_str()? {
            "task_complete" => Some(match payload["turn_id"].as_str() {
                Some(turn) if !turn.is_empty() => Turn::Completed(turn.to_owned()),
                _ => Turn::Other,
            }),
            "task_started" | "turn_aborted" => Some(Turn::Other),
            _ => None,
        }
    })
    .ok()
    .flatten()
    .unwrap_or(Turn::Other)
}

/// The longest suggestion kept.
const MAX_CHARS: usize = 500;

/// The suggestion in the turn's answer, on one line.
pub(crate) fn parse(answer: &str) -> Option<String> {
    let value: Value = serde_json::from_str(answer.trim()).ok()?;
    let text = value.get("suggestion")?.as_str()?.trim();
    (!text.is_empty()).then(|| {
        text.chars()
            .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
            .take(MAX_CHARS)
            .collect()
    })
}

/// Codex fetches its own prompt from its service; this one asks for the
/// same thing in the shape the app parses.
const PROMPT: &str = "Predict the next message the user is most likely to send in this \
conversation. Write it as the user would type it, in the language the user has been using, \
as one short sentence. Use null when there is no useful prediction.\n\n\
Return only JSON matching this schema: \
{\"type\":\"object\",\"properties\":{\"suggestion\":{\"type\":[\"string\",\"null\"]}},\
\"required\":[\"suggestion\"],\"additionalProperties\":false}.";

/// The longest one prediction may take, from start to answer.
const DEADLINE: Duration = Duration::from_secs(60);
/// How long the server may take to exit once its input is closed.
const EXIT_GRACE: Duration = Duration::from_secs(5);
/// The longest protocol line read.
const LINE_LIMIT: u64 = 8 * 1024 * 1024;
/// At most this many app-servers run at once; further requests wait.
const CONCURRENT: usize = 2;

/// Starts one `codex app-server` per prediction and stops it afterwards.
pub struct AppServer {
    /// A fixed program (tests); otherwise the CLI the desktop app ships,
    /// resolved per request so a later installation is picked up.
    program: Option<PathBuf>,
    deadline: Duration,
    running: tokio::sync::Semaphore,
}

impl Default for AppServer {
    fn default() -> Self {
        Self {
            program: None,
            deadline: DEADLINE,
            running: tokio::sync::Semaphore::new(CONCURRENT),
        }
    }
}

impl AppServer {
    pub fn with_program(program: PathBuf, deadline: Duration) -> Self {
        Self {
            program: Some(program),
            deadline,
            ..Self::default()
        }
    }
}

#[async_trait::async_trait]
impl Predictor for AppServer {
    async fn predict(&self, request: Request) -> Option<String> {
        let _running = self.running.acquire().await.ok()?;
        let program = self
            .program
            .clone()
            .or_else(|| crate::orca::runtime::program::resolve("codex"))?;
        let mut child = match spawn(&program) {
            Ok(child) => child,
            Err(error) => {
                log::debug!(
                    "codex prediction: {} did not start: {error}",
                    program.display()
                );
                return None;
            }
        };
        let session = Session {
            stdin: child.stdin.take()?,
            stdout: BufReader::new(child.stdout.take()?),
            next: 0,
            fork: None,
            answer: None,
            status: None,
        };
        // The session owns the server's input: dropping it, finished or timed
        // out, is the end of input the server exits on.
        let outcome = tokio::time::timeout(self.deadline, converse(session, &request)).await;
        stop(&mut child).await;
        match outcome {
            Ok(Ok(text)) => text,
            Ok(Err(error)) => {
                log::debug!("codex prediction for {}: {error}", request.thread);
                None
            }
            Err(_) => {
                log::debug!("codex prediction for {} timed out", request.thread);
                None
            }
        }
    }
}

fn spawn(program: &Path) -> std::io::Result<Child> {
    let mut command = tokio::process::Command::new(program);
    // A prediction uses no tools: start none of the configured MCP servers.
    command
        .args(["app-server", "-c", "mcp_servers={}"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    command.spawn()
}

/// Waits for the server to exit after its input closed; one that lingers is
/// killed with its process group, so nothing it started outlives it.
async fn stop(child: &mut Child) {
    if tokio::time::timeout(EXIT_GRACE, child.wait()).await.is_ok() {
        return;
    }
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        // SAFETY: signals the process group this still-running child leads.
        unsafe {
            libc::killpg(pid as libc::pid_t, libc::SIGKILL);
        }
    }
    let _ = child.kill().await;
}

async fn converse(mut session: Session, request: &Request) -> Result<Option<String>, String> {
    session
        .call(
            "initialize",
            json!({
                "clientInfo": {"name": "muster", "title": "Muster", "version": env!("CARGO_PKG_VERSION")},
                "capabilities": {"experimentalApi": true},
            }),
        )
        .await?;
    session
        .send(json!({"jsonrpc": "2.0", "method": "initialized"}))
        .await?;
    // A fork of the rollout file, not of the loaded thread: the desktop app
    // holds that thread's writer.
    let fork = session
        .call(
            "thread/fork",
            json!({
                "threadId": request.thread,
                "path": request.rollout,
                "ephemeral": true,
                "excludeTurns": true,
                "threadSource": "composer_predictions",
                "approvalPolicy": "never",
                "permissions": ":read-only",
                "runtimeWorkspaceRoots": [],
                "config": {
                    "features.enable_fanout": false,
                    "features.multi_agent": false,
                    "features.multi_agent_v2": false,
                    "web_search": "disabled",
                },
            }),
        )
        .await?;
    let fork = fork["thread"]["id"]
        .as_str()
        .ok_or("thread/fork returned no thread")?
        .to_owned();
    session.fork = Some(fork.clone());
    session
        .call(
            "turn/start",
            json!({
                "threadId": fork,
                "turnTrigger": "composer_predictions",
                "responsesapiClientMetadata": {
                    "prediction_source_thread_id": request.thread,
                    "prediction_source_turn_id": request.turn,
                    "prediction_trigger": "live",
                },
                "input": [{"type": "text", "text": PROMPT, "text_elements": []}],
                "effort": "low",
                "approvalPolicy": "never",
                "permissions": ":read-only",
            }),
        )
        .await?;
    while session.status.is_none() {
        let message = session.read().await?;
        session.observe(message).await?;
    }
    match session.status.as_deref() {
        Some("completed") => Ok(session.answer.as_deref().and_then(parse)),
        status => Err(format!("prediction turn ended {status:?}")),
    }
}

/// One app-server connection over its standard input and output.
struct Session {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u64,
    /// The forked thread, once known; what follows is read for it only.
    fork: Option<String>,
    /// The fork's last completed agent message.
    answer: Option<String>,
    /// The fork's turn status once it completed.
    status: Option<String>,
}

impl Session {
    async fn send(&mut self, message: Value) -> Result<(), String> {
        let mut line = serde_json::to_vec(&message).map_err(|e| e.to_string())?;
        line.push(b'\n');
        self.stdin
            .write_all(&line)
            .await
            .map_err(|e| e.to_string())?;
        self.stdin.flush().await.map_err(|e| e.to_string())
    }

    async fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next += 1;
        let id = self.next;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await?;
        loop {
            let message = self.read().await?;
            if message.get("method").is_none() && message["id"] == id {
                if let Some(error) = message.get("error") {
                    return Err(format!("{method}: {error}"));
                }
                return Ok(message["result"].clone());
            }
            self.observe(message).await?;
        }
    }

    async fn read(&mut self) -> Result<Value, String> {
        let mut line = Vec::new();
        loop {
            line.clear();
            let read = (&mut self.stdout)
                .take(LINE_LIMIT)
                .read_until(b'\n', &mut line)
                .await
                .map_err(|e| e.to_string())?;
            if read == 0 {
                return Err("app-server closed its output".into());
            }
            if read as u64 == LINE_LIMIT && !line.ends_with(b"\n") {
                return Err("app-server line too long".into());
            }
            if let Ok(value) = serde_json::from_slice(&line) {
                return Ok(value);
            }
        }
    }

    /// Notes the fork's answer and completion; declines every request the
    /// server makes, since a read-only prediction approves nothing.
    async fn observe(&mut self, message: Value) -> Result<(), String> {
        let Some(method) = message["method"].as_str() else {
            return Ok(());
        };
        if let Some(id) = message.get("id") {
            return self
                .send(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {"code": -32601, "message": "not supported by this client"},
                }))
                .await;
        }
        let params = &message["params"];
        if self.fork.is_none() || params["threadId"].as_str() != self.fork.as_deref() {
            return Ok(());
        }
        match method {
            "item/completed" if params["item"]["type"] == "agentMessage" => {
                self.answer = params["item"]["text"].as_str().map(str::to_owned);
            }
            "turn/completed" => {
                self.status = Some(
                    params["turn"]["status"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_owned(),
                );
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_suggestion_on_one_line() {
        assert_eq!(
            parse(r#" {"suggestion":"  Commit it\nand install  "} "#).as_deref(),
            Some("Commit it and install")
        );
        assert_eq!(parse(r#"{"suggestion":null}"#), None);
        assert_eq!(parse(r#"{"suggestion":"   "}"#), None);
        assert_eq!(parse("not json"), None);
        assert_eq!(parse(r#"{"other":"x"}"#), None);
        let long = format!(r#"{{"suggestion":"{}"}}"#, "字".repeat(MAX_CHARS + 20));
        assert_eq!(parse(&long).unwrap().chars().count(), MAX_CHARS);
    }

    fn rollout(lines: &[&str]) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        for line in lines {
            std::io::Write::write_all(&mut file, format!("{line}\n").as_bytes()).unwrap();
        }
        file
    }

    #[test]
    fn the_newest_turn_marker_decides() {
        let started = r#"{"type":"event_msg","payload":{"type":"task_started","turn_id":"t2"}}"#;
        let complete = r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"t1"}}"#;
        let aborted = r#"{"type":"event_msg","payload":{"type":"turn_aborted","turn_id":"t1"}}"#;
        let settings = r#"{"type":"event_msg","payload":{"type":"thread_settings_applied"}}"#;
        let meta = r#"{"type":"session_meta","payload":{"id":"s"}}"#;
        assert_eq!(
            latest_turn(rollout(&[meta, complete, settings]).path()),
            Turn::Completed("t1".into())
        );
        assert_eq!(
            latest_turn(rollout(&[meta, complete, started]).path()),
            Turn::Other
        );
        assert_eq!(latest_turn(rollout(&[meta, aborted]).path()), Turn::Other);
        assert_eq!(latest_turn(rollout(&[meta]).path()), Turn::Other);
        assert_eq!(
            latest_turn(Path::new("/nonexistent/rollout.jsonl")),
            Turn::Other
        );
    }

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    fn request() -> Request {
        Request {
            thread: "source-thread".into(),
            turn: "turn-1".into(),
            rollout: "/tmp/rollout.jsonl".into(),
        }
    }

    #[tokio::test]
    async fn asks_an_app_server_and_lets_it_exit() {
        let server =
            AppServer::with_program(fixture("codex-app-server.py"), Duration::from_secs(20));
        assert_eq!(
            server.predict(request()).await.as_deref(),
            Some("Commit and install it")
        );
    }

    #[tokio::test]
    async fn a_failed_turn_or_a_silent_server_gives_nothing() {
        let failing =
            AppServer::with_program(fixture("codex-app-server.py"), Duration::from_secs(20));
        let mut failed = request();
        failed.turn = "fail".into();
        assert_eq!(failing.predict(failed).await, None);
        let mut silent = request();
        silent.turn = "hang".into();
        let started = std::time::Instant::now();
        let slow = AppServer::with_program(fixture("codex-app-server.py"), Duration::from_secs(1));
        assert_eq!(slow.predict(silent).await, None);
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
