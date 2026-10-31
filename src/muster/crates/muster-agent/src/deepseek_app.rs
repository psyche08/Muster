//! DeepSeek Harness driven through Muster's own plugin inside it
//! (`deploy/deepseek-harness`, ADR 0074). The plugin serves one owner-only
//! socket under the app's home (`profiles/deepseek.toml`, `[plugin]`): the
//! app's live state, prompts and interrupts through its session controller,
//! and the approvals and question cards waiting for a person.
//!
//! Every exchange is bound to the process serving the socket: its peer pid
//! must be the one the [`Host`] verified as genuinely DeepSeek Harness, and
//! a write is refused before it is sent when another process serves. A
//! request line the plugin never received whole is never acted on, so a
//! failed write is "not sent"; only a lost reply leaves the outcome unknown.
//! This module adapts the plugin; who may operate a session and whether the
//! Mac must be unlocked are decided by the daemon as for any controller.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use muster_proto::v1::{InteractionQuestion, QuestionAnswer, QuestionOption};
use serde_json::{Value, json};

use crate::controller::{Controller, ControllerEvent, ProcessIdentity, ProcessState};
use crate::model::{ControllerKind, PendingInteraction, Provider, SessionState};
use crate::{AgentService, Error};

const TIMEOUT: Duration = Duration::from_secs(2);
/// The plugin waits up to 30 s for the app to admit a prompt.
const PROMPT_TIMEOUT: Duration = Duration::from_secs(35);
const MAX_REPLY: u64 = 4 << 20;

/// The options of a DeepSeek Harness approval, as Muster offers them.
pub const ALLOW: &str = "Allow once";
pub const REJECT: &str = "Reject";

const APPROVAL: &str = "dsh-approval:";
const QUESTION: &str = "dsh-question:";

/// The plugin's socket under the app's home.
pub fn socket(home: &Path) -> Option<PathBuf> {
    Some(home.join(Provider::Deepseek.profile().plugin?.socket))
}

/// Who the process serving the socket is. musterd answers with the
/// platform's code signature and launch-time checks.
pub trait Host: Send + Sync {
    /// The launch identity of `pid` when it is genuinely DeepSeek Harness.
    fn verify(&self, pid: u32) -> Option<ProcessIdentity>;
    /// Whether `process` has exited (its pid is gone or was reused).
    fn exited(&self, process: ProcessIdentity) -> bool;
}

/// Why an exchange with the plugin failed.
#[derive(Debug, PartialEq, Eq)]
pub enum CallError {
    /// The request never reached the plugin whole: nothing happened.
    NotSent(String),
    /// The request was sent and its reply was lost.
    Unknown(String),
    /// The plugin, or the app behind it, refused the request.
    Refused(String),
}

impl From<CallError> for Error {
    fn from(error: CallError) -> Self {
        match error {
            CallError::NotSent(why) => {
                Error::Unavailable(format!("DeepSeek Harness: {why}; nothing was sent"))
            }
            CallError::Unknown(why) => Error::UnknownOutcome(format!("DeepSeek Harness: {why}")),
            CallError::Refused(why) => {
                Error::Unavailable(format!("DeepSeek Harness refused: {why}"))
            }
        }
    }
}

/// The pid of the process at the other end of a Unix socket.
#[cfg(target_os = "macos")]
fn peer_pid(stream: &UnixStream) -> Option<u32> {
    use std::os::fd::AsRawFd;
    let mut pid: libc::pid_t = 0;
    let mut len = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
    // SAFETY: a connected socket and an out-parameter of the size passed.
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_LOCAL,
            libc::LOCAL_PEERPID,
            (&mut pid as *mut libc::pid_t).cast(),
            &mut len,
        )
    };
    (rc == 0 && pid > 0).then_some(pid as u32)
}

#[cfg(target_os = "linux")]
fn peer_pid(stream: &UnixStream) -> Option<u32> {
    use std::os::fd::AsRawFd;
    let mut cred = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: a connected socket and an out-parameter of the size passed.
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut cred as *mut libc::ucred).cast(),
            &mut len,
        )
    };
    (rc == 0 && cred.pid > 0).then_some(cred.pid as u32)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn peer_pid(_stream: &UnixStream) -> Option<u32> {
    None
}

/// One request and its reply. `expected` refuses, before writing, a socket
/// another process serves.
fn call(
    socket: &Path,
    expected: Option<u32>,
    request: Value,
    timeout: Duration,
) -> Result<(u32, Value), CallError> {
    let stream = UnixStream::connect(socket)
        .map_err(|e| CallError::NotSent(format!("its Muster plugin is not reachable ({e})")))?;
    let _ = stream.set_write_timeout(Some(TIMEOUT));
    let _ = stream.set_read_timeout(Some(timeout));
    let peer = peer_pid(&stream)
        .ok_or_else(|| CallError::NotSent("the plugin socket's process is unknown".into()))?;
    if expected.is_some_and(|pid| pid != peer) {
        return Err(CallError::NotSent(
            "another process serves the plugin socket now".into(),
        ));
    }
    let mut line = request.to_string().into_bytes();
    line.push(b'\n');
    // The newline goes last: a failed write leaves a line the plugin never
    // reads.
    (&stream)
        .write_all(&line)
        .map_err(|e| CallError::NotSent(format!("the request could not be written ({e})")))?;
    let mut reply = Vec::new();
    BufReader::new((&stream).take(MAX_REPLY))
        .read_until(b'\n', &mut reply)
        .map_err(|e| CallError::Unknown(format!("no reply ({e})")))?;
    if reply.last() != Some(&b'\n') {
        return Err(CallError::Unknown("no reply".into()));
    }
    let reply: Value = serde_json::from_slice(&reply)
        .map_err(|e| CallError::Unknown(format!("an unreadable reply ({e})")))?;
    if reply["ok"] == true {
        Ok((peer, reply["result"].clone()))
    } else {
        Err(CallError::Refused(
            reply["error"]
                .as_str()
                .unwrap_or("no reason given")
                .chars()
                .take(500)
                .collect(),
        ))
    }
}

/// An approval waiting for a person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Approval {
    pub id: String,
    pub session: String,
    pub tool: String,
    pub reason: String,
}

/// A question card waiting for a person: the app's questions, as Muster
/// offers them (option ids are their positions; the app takes labels).
#[derive(Clone, Debug, PartialEq)]
pub struct Questions {
    pub id: String,
    pub session: String,
    pub questions: Vec<InteractionQuestion>,
}

/// What the running app reported.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    /// Sessions the app holds live, and whether a turn runs in each.
    pub sessions: HashMap<String, bool>,
    pub approvals: Vec<Approval>,
    pub questions: Vec<Questions>,
}

fn text(value: &Value, max: usize) -> Option<String> {
    let s = value.as_str()?;
    (s.chars().count() <= max).then(|| s.to_string())
}

fn bridge_id(value: &Value) -> Option<String> {
    text(value, 128).filter(|s| !s.is_empty())
}

fn parse_questions(value: &Value) -> Option<Vec<InteractionQuestion>> {
    let raw = value.as_array()?;
    if raw.is_empty() || raw.len() > 16 {
        return None;
    }
    let mut ids = HashSet::new();
    raw.iter()
        .map(|q| {
            let id = bridge_id(&q["id"])?;
            if !ids.insert(id.clone()) {
                return None;
            }
            let mut prompt = text(&q["question"], 16_384).filter(|p| !p.is_empty())?;
            if let Some(detail) = text(&q["detail"], 65_536).filter(|d| !d.is_empty()) {
                prompt = format!("{prompt}\n\n{detail}");
            }
            let options = q["options"]
                .as_array()
                .map_or(Some(Vec::new()), |options| {
                    (options.len() <= 32).then_some(())?;
                    options
                        .iter()
                        .enumerate()
                        .map(|(i, o)| {
                            Some(QuestionOption {
                                id: i.to_string(),
                                label: text(&o["label"], 4096).filter(|l| !l.is_empty())?,
                                description: text(&o["description"], 8192).unwrap_or_default(),
                            })
                        })
                        .collect()
                })?;
            Some(InteractionQuestion {
                id,
                header: text(&q["header"], 256).unwrap_or_default(),
                prompt,
                options,
                multi_select: q["multiSelect"] == true,
                allow_text: true,
            })
        })
        .collect()
}

/// The plugin's `state` reply; malformed entries are left out.
fn parse_snapshot(value: &Value) -> Snapshot {
    let sessions = value["sessions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| Some((bridge_id(&s["id"])?, s["running"] == true)))
        .collect();
    let approvals = value["approvals"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| {
            Some(Approval {
                id: bridge_id(&a["id"])?,
                session: bridge_id(&a["sessionId"])?,
                tool: text(&a["toolName"], 256).unwrap_or_default(),
                reason: text(&a["reason"], 16_384).unwrap_or_default(),
            })
        })
        .collect();
    let questions = value["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|q| {
            Some(Questions {
                id: bridge_id(&q["id"])?,
                session: bridge_id(&q["sessionId"])?,
                questions: parse_questions(&q["questions"])?,
            })
        })
        .collect();
    Snapshot {
        sessions,
        approvals,
        questions,
    }
}

impl Snapshot {
    /// What waits for a person in `session`, as interactions.
    fn interactions(&self, session: &str) -> Vec<PendingInteraction> {
        let approvals = self
            .approvals
            .iter()
            .filter(|a| a.session == session)
            .map(|a| {
                let prompt = if a.reason.is_empty() {
                    a.tool.clone()
                } else {
                    format!("{}: {}", a.tool, a.reason)
                };
                PendingInteraction {
                    id: format!("{APPROVAL}{}", a.id),
                    session: session.into(),
                    kind: "approval".into(),
                    prompt,
                    options: vec![ALLOW.into(), REJECT.into()],
                    questions: Vec::new(),
                    optional: false,
                }
            });
        let questions = self
            .questions
            .iter()
            .filter(|q| q.session == session)
            .map(|q| PendingInteraction {
                id: format!("{QUESTION}{}", q.id),
                session: session.into(),
                kind: "question".into(),
                prompt: String::new(),
                options: Vec::new(),
                questions: q.questions.clone(),
                optional: false,
            });
        approvals.chain(questions).collect()
    }

    fn state(&self, session: &str) -> SessionState {
        if self.approvals.iter().any(|a| a.session == session)
            || self.questions.iter().any(|q| q.session == session)
        {
            SessionState::WaitingApproval
        } else if self.sessions.get(session) == Some(&true) {
            SessionState::Running
        } else {
            SessionState::Idle
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Status {
    /// The plugin could not be read.
    Unreachable,
    Live(ProcessIdentity, Snapshot),
    /// The process that served it exited.
    Gone(ProcessIdentity),
}

/// The plugin as last read, shared by the controllers of its sessions.
pub struct Link {
    socket: PathBuf,
    status: Mutex<Status>,
}

impl Link {
    pub fn new(socket: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            socket,
            status: Mutex::new(Status::Unreachable),
        })
    }

    fn live(&self, process: ProcessIdentity) -> Option<Snapshot> {
        match &*self.status.lock().unwrap() {
            Status::Live(serving, snapshot) if *serving == process => Some(snapshot.clone()),
            _ => None,
        }
    }

    /// Reads the plugin's state, bound to the process that serves it.
    fn read(&self, host: &dyn Host) -> Status {
        let previous = self.status.lock().unwrap().clone();
        let protocol = Provider::Deepseek
            .profile()
            .plugin
            .map_or(0, |plugin| plugin.protocol);
        let read =
            call(&self.socket, None, json!({"op": "state"}), TIMEOUT).and_then(|(pid, state)| {
                let process = host.verify(pid).ok_or_else(|| {
                    CallError::Refused(format!("pid {pid} is not DeepSeek Harness"))
                })?;
                Ok((process, state))
            });
        match read {
            Ok((process, state)) => {
                // A plugin speaking another protocol drives nothing.
                if state["protocol"].as_u64() != Some(u64::from(protocol)) {
                    log::warn!("DeepSeek Harness plugin speaks another protocol; not driven");
                    return Status::Unreachable;
                }
                Status::Live(process, parse_snapshot(&state))
            }
            Err(error) => {
                log::debug!("DeepSeek Harness plugin: {error:?}");
                match previous {
                    Status::Live(process, _) | Status::Gone(process) if host.exited(process) => {
                        Status::Gone(process)
                    }
                    _ => Status::Unreachable,
                }
            }
        }
    }

    /// One operation on `process`'s plugin.
    fn operate(
        &self,
        process: ProcessIdentity,
        request: Value,
        timeout: Duration,
    ) -> Result<Value, CallError> {
        if self.live(process).is_none() {
            return Err(CallError::NotSent(
                "its Muster plugin is not running in this app launch".into(),
            ));
        }
        call(&self.socket, Some(process.pid), request, timeout).map(|(_, result)| result)
    }
}

/// One DeepSeek Harness session, driven through the plugin of one app
/// launch.
pub struct PluginController {
    session: String,
    process: ProcessIdentity,
    link: Arc<Link>,
}

impl PluginController {
    pub fn new(session: String, process: ProcessIdentity, link: Arc<Link>) -> Arc<Self> {
        Arc::new(Self {
            session,
            process,
            link,
        })
    }

    async fn operate(&self, request: Value, timeout: Duration) -> Result<Value, Error> {
        let link = self.link.clone();
        let process = self.process;
        tokio::task::spawn_blocking(move || link.operate(process, request, timeout))
            .await
            .map_err(|e| Error::UnknownOutcome(e.to_string()))?
            .map_err(Error::from)
    }

    fn interaction(&self, id: &str) -> Result<PendingInteraction, Error> {
        self.pending()
            .into_iter()
            .find(|i| i.id == id)
            .ok_or_else(|| Error::NotFound(format!("interaction {id}")))
    }
}

/// A random version-4 UUID: a prompt's request id, which the app records
/// as the message's `rpcId`.
fn request_id() -> String {
    let mut bytes = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = hex::encode(bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

#[async_trait]
impl Controller for PluginController {
    fn kind(&self) -> ControllerKind {
        ControllerKind::Plugin
    }
    fn session_id(&self) -> String {
        self.session.clone()
    }
    fn state(&self) -> SessionState {
        self.link
            .live(self.process)
            .map_or(SessionState::Unknown, |s| s.state(&self.session))
    }
    fn process_identity(&self) -> ProcessIdentity {
        self.process
    }
    fn process_state(&self) -> ProcessState {
        match &*self.link.status.lock().unwrap() {
            Status::Live(process, _) if *process == self.process => ProcessState::Running,
            Status::Live(..) => ProcessState::Ended,
            Status::Gone(process) if *process == self.process => ProcessState::Ended,
            _ => ProcessState::Unknown,
        }
    }
    fn pending(&self) -> Vec<PendingInteraction> {
        self.link
            .live(self.process)
            .map(|s| s.interactions(&self.session))
            .unwrap_or_default()
    }
    async fn send(&self, text: &str) -> Result<(), Error> {
        let request = json!({
            "op": "prompt",
            "sessionId": self.session,
            "requestId": request_id(),
            "text": text,
            "mode": "queue",
        });
        self.operate(request, PROMPT_TIMEOUT).await.map(|_| ())
    }
    async fn respond(&self, interaction: &str, option: &str, answer: &str) -> Result<(), Error> {
        let pending = self.interaction(interaction)?;
        if pending.kind != "approval" {
            return Err(Error::Invalid("Use the structured question card".into()));
        }
        let outcome = match option {
            ALLOW if answer.is_empty() => "allowed-once",
            REJECT if answer.is_empty() => "rejected",
            _ => return Err(Error::Invalid("This option is not offered".into())),
        };
        let id = &interaction[APPROVAL.len()..];
        self.operate(
            json!({"op": "approve", "approvalId": id, "outcome": outcome}),
            TIMEOUT,
        )
        .await
        .map(|_| ())
    }
    async fn answer_questions(
        &self,
        interaction: &str,
        answers: &[QuestionAnswer],
    ) -> Result<(), Error> {
        let pending = self.interaction(interaction)?;
        let plans = crate::questions::validate(&pending, answers)?;
        let answers: Vec<Value> = plans
            .into_iter()
            .map(|plan| {
                let mut answer = json!({"id": plan.question.id, "selected": plan.labels});
                if !plan.text.is_empty() {
                    answer["custom"] = plan.text.into();
                }
                answer
            })
            .collect();
        let id = &interaction[QUESTION.len()..];
        self.operate(
            json!({"op": "answer", "questionId": id, "answers": answers}),
            TIMEOUT,
        )
        .await
        .map(|_| ())
    }
    async fn interrupt(&self) -> Result<(), Error> {
        self.operate(json!({"op": "cancel", "sessionId": self.session}), TIMEOUT)
            .await
            .map(|_| ())
    }
    async fn stop(&self) {}
}

/// Reads the plugin once and brings the service up to date: controllers
/// for the app's listed sessions bound to the launch that serves it, their
/// state and waiting interactions, and their detachment when that launch
/// exits or another replaces it. Blocking; musterd runs it off its runtime.
pub fn refresh(agent: &AgentService, link: &Arc<Link>, host: &dyn Host) {
    let previous = link.status.lock().unwrap().clone();
    let status = link.read(host);
    *link.status.lock().unwrap() = status.clone();
    let events = agent.controller_events();
    let send = |session: &str, event: ControllerEvent| {
        let _ = events.try_send((session.to_string(), event));
    };
    if let Status::Live(process, _) = &status {
        for session in agent.list(Some(Provider::Deepseek), 0) {
            let controller = PluginController::new(session.id, *process, link.clone());
            agent.attach_native(controller as Arc<dyn Controller>);
        }
    }
    // What each launch's sessions show: a launch that is no longer read
    // shows nothing waiting.
    let shown = |status: &Status, process: ProcessIdentity| match status {
        Status::Live(serving, snapshot) if *serving == process => Some(snapshot.clone()),
        _ => None,
    };
    let mut launches = Vec::new();
    for status in [&previous, &status] {
        if let Status::Live(process, _) = status
            && !launches.contains(process)
        {
            launches.push(*process);
        }
    }
    let controllers = agent.controllers_of(ControllerKind::Plugin);
    for process in launches {
        let (old, new) = (shown(&previous, process), shown(&status, process));
        if old == new {
            continue;
        }
        let (old, new) = (old.unwrap_or_default(), new.unwrap_or_default());
        for (session, _) in controllers.iter().filter(|(_, p)| *p == process) {
            let (was, now) = (old.interactions(session), new.interactions(session));
            for interaction in now.iter().filter(|i| !was.iter().any(|w| w.id == i.id)) {
                send(session, ControllerEvent::Interaction(interaction.clone()));
            }
            for interaction in was.iter().filter(|w| !now.iter().any(|i| i.id == w.id)) {
                let interaction = interaction.id.clone();
                send(session, ControllerEvent::Resolved { interaction });
            }
            send(session, ControllerEvent::State(new.state(session)));
        }
    }
    // A launch that exited, or that another replaced, takes its
    // controllers with it.
    let ended = match (&previous, &status) {
        (Status::Live(old, _), Status::Live(new, _)) if old != new => Some(*old),
        (Status::Live(old, _), Status::Gone(_)) => Some(*old),
        _ => None,
    };
    if let Some(process) = ended {
        for (session, _) in controllers.iter().filter(|(_, p)| *p == process) {
            send(session, ControllerEvent::Detached { process });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_snapshot_offers_approvals_and_questions_as_interactions() {
        let snapshot = parse_snapshot(&json!({
            "sessions": [{"id": "session-a", "running": true}, {"id": "session-b", "running": false}],
            "approvals": [{"id": "a1", "sessionId": "session-a", "toolName": "bash", "reason": "rm build"}],
            "questions": [{"id": "q1", "sessionId": "session-b", "questions": [
                {"id": "x", "question": "Which?", "detail": "The plan", "header": "Pick",
                 "options": [{"label": "A", "description": "first"}, {"label": "B"}], "multiSelect": false}
            ]}, {"id": "bad", "sessionId": "session-b", "questions": []}],
        }));
        assert_eq!(snapshot.state("session-a"), SessionState::WaitingApproval);
        assert_eq!(snapshot.state("session-b"), SessionState::WaitingApproval);
        assert_eq!(snapshot.state("session-c"), SessionState::Idle);
        let [approval] = &snapshot.interactions("session-a")[..] else {
            panic!("one approval")
        };
        assert_eq!(approval.id, "dsh-approval:a1");
        assert_eq!(approval.prompt, "bash: rm build");
        assert_eq!(approval.options, [ALLOW, REJECT]);
        let [card] = &snapshot.interactions("session-b")[..] else {
            panic!("one well-formed card")
        };
        assert_eq!(card.kind, "question");
        let q = &card.questions[0];
        assert_eq!(
            (q.id.as_str(), q.prompt.as_str()),
            ("x", "Which?\n\nThe plan")
        );
        assert_eq!(q.options[1].id, "1");
        assert!(q.allow_text && !q.multi_select);
        let running = parse_snapshot(&json!({"sessions": [{"id": "session-a", "running": true}]}));
        assert_eq!(running.state("session-a"), SessionState::Running);
    }

    #[test]
    fn errors_say_whether_anything_was_sent() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("bridge.sock");
        assert!(matches!(
            call(&missing, None, json!({"op": "state"}), TIMEOUT),
            Err(CallError::NotSent(_))
        ));
        assert!(matches!(
            Error::from(CallError::Unknown("x".into())),
            Error::UnknownOutcome(_)
        ));
        assert!(matches!(
            Error::from(CallError::NotSent("x".into())),
            Error::Unavailable(_)
        ));
    }

    /// A one-shot plugin stand-in on a real socket: the reply it gives, and
    /// the request it read.
    fn serve_once(socket: &Path, reply: Option<&'static str>) -> std::thread::JoinHandle<String> {
        let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            if let Some(reply) = reply {
                (&stream).write_all(reply.as_bytes()).unwrap();
            }
            line
        })
    }

    #[test]
    fn a_call_is_bound_to_the_serving_process() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("bridge.sock");
        let server = serve_once(
            &socket,
            Some("{\"id\":null,\"ok\":true,\"result\":{\"accepted\":true}}\n"),
        );
        let me = std::process::id();
        let (pid, result) = call(&socket, Some(me), json!({"op": "cancel"}), TIMEOUT).unwrap();
        assert_eq!((pid, &result["accepted"]), (me, &Value::Bool(true)));
        assert!(server.join().unwrap().contains("\"cancel\""));
        // Another serving process is refused before anything is written.
        std::fs::remove_file(&socket).unwrap();
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let refused = call(&socket, Some(me + 1), json!({"op": "cancel"}), TIMEOUT);
        assert!(matches!(refused, Err(CallError::NotSent(_))));
        let (stream, _) = listener.accept().unwrap();
        let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
        let mut written = Vec::new();
        let _ = (&stream).read_to_end(&mut written);
        assert!(written.is_empty(), "nothing reached the other process");
    }

    #[test]
    fn a_lost_reply_leaves_the_outcome_unknown_and_a_refusal_is_named() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("bridge.sock");
        let server = serve_once(&socket, None);
        assert!(matches!(
            call(
                &socket,
                None,
                json!({"op": "prompt"}),
                Duration::from_millis(200)
            ),
            Err(CallError::Unknown(_))
        ));
        server.join().unwrap();
        std::fs::remove_file(&socket).unwrap();
        let server = serve_once(
            &socket,
            Some("{\"id\":null,\"ok\":false,\"error\":\"session/not-found\"}\n"),
        );
        assert_eq!(
            call(&socket, None, json!({"op": "prompt"}), TIMEOUT),
            Err(CallError::Refused("session/not-found".into()))
        );
        server.join().unwrap();
    }
}
