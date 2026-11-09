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
use crate::plugin_control::{self, Contract};
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
    let mutation = matches!(
        request["op"].as_str(),
        Some("prompt" | "cancel" | "approve" | "answer" | "task.create.v1" | "task.resume.v1")
    );
    if mutation {
        crate::control_guard::check().map_err(|e| CallError::NotSent(e.to_string()))?;
        crate::control_guard::delivery_attempt();
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
    if !reply.is_object()
        || reply["id"] != request["id"]
        || request
            .get("control")
            .is_some_and(|binding| reply.get("control") != Some(binding))
    {
        return Err(CallError::Unknown(
            "reply does not match the App operation".into(),
        ));
    }
    if reply["ok"] == true {
        Ok((peer, reply["result"].clone()))
    } else if reply["ok"] == false {
        let error = reply["error"]
            .as_str()
            .unwrap_or("no reason given")
            .chars()
            .take(500)
            .collect();
        if reply["delivery"] == "unknown"
            || (request.get("control").is_some() && reply["delivery"] != "not_started")
        {
            Err(CallError::Unknown(error))
        } else {
            Err(CallError::Refused(error))
        }
    } else {
        Err(CallError::Unknown("reply has no explicit result".into()))
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
    /// Local App-only negotiation; absent for the legacy bridge.
    pub contract: Option<Contract>,
    /// Sessions the app holds live, and whether a turn runs in each.
    pub sessions: HashMap<String, bool>,
    /// Whether `sessions` is the app's whole registry, every entry read. A
    /// session missing from a whole registry is one the app has not loaded,
    /// which it shows as not running.
    pub complete: bool,
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
    let listed = value["sessions"].as_array();
    let sessions: HashMap<_, _> = listed
        .into_iter()
        .flatten()
        .filter_map(|s| Some((bridge_id(&s["id"])?, s["running"].as_bool()?)))
        .collect();
    let complete = listed.is_some_and(|l| l.len() == sessions.len());
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
        contract: None,
        sessions,
        complete,
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
        } else if self.sessions.contains_key(session) || self.complete {
            SessionState::Idle
        } else {
            SessionState::Unknown
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

/// A plugin capability contract bound to a specific process launch.
pub struct TaskHost {
    pub process: ProcessIdentity,
    contract: Contract,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskAction {
    Create,
    Resume,
    Inspect,
}

pub fn fresh_task_session() -> String {
    format!("session-{}", request_id())
}

/// The plugin as last read, shared by the controllers of its sessions.
pub struct Link {
    socket: PathBuf,
    status: Mutex<Status>,
    pending: Mutex<HashMap<String, PendingSend>>,
    confirmed: Mutex<HashMap<String, String>>,
}

#[derive(Clone)]
struct PendingSend {
    operation: String,
    confirmation: Option<crate::control_guard::Confirmation>,
}

impl Link {
    pub fn new(socket: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            socket,
            status: Mutex::new(Status::Unreachable),
            pending: Mutex::new(HashMap::new()),
            confirmed: Mutex::new(HashMap::new()),
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
                let mut snapshot = parse_snapshot(&state);
                match Contract::parse(&state, "deepseek") {
                    Ok(contract) => snapshot.contract = contract,
                    Err(error) => {
                        log::warn!("DeepSeek Harness plugin: {error}");
                        return Status::Unreachable;
                    }
                }
                Status::Live(process, snapshot)
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

    /// Read-only evidence, including after a daemon restart. A fresh verified
    /// binding may read an old request's host record; it never repeats the send.
    fn lookup(
        &self,
        process: ProcessIdentity,
        session: &str,
        operation: &str,
    ) -> Result<bool, Error> {
        let Some(contract) = self.live(process).and_then(|s| s.contract) else {
            return Ok(false);
        };
        if !contract.offers("prompt.lookup") {
            return Ok(false);
        }
        let request_id = plugin_control::prompt_id(session, operation);
        let mut request = json!({"id":request_id,"op":"prompt.lookup",
            "sessionId":session,"requestId":request_id});
        contract.bind(session, operation, &mut request)?;
        let Ok(result) = self.operate(process, request, TIMEOUT) else {
            return Ok(false);
        };
        if !plugin_control::confirms_prompt(&result, session, &request_id) {
            return Ok(false);
        }
        self.confirm_pending(session, operation)?;
        Ok(true)
    }

    /// Negotiate the exact signed App instance before the caller reserves an ID.
    pub fn task_host(&self) -> Result<TaskHost, Error> {
        let Status::Live(process, snapshot) = self.status.lock().unwrap().clone() else {
            return Err(Error::Unavailable(
                "Open DSH App with the Muster plugin".into(),
            ));
        };
        let contract = snapshot
            .contract
            .filter(|c| {
                ["task.create.v1", "task.resume.v1", "task.inspect.v1"]
                    .iter()
                    .all(|op| c.offers(op))
            })
            .ok_or_else(|| {
                Error::Unavailable("Update the DSH desktop profile's Muster plugin".into())
            })?;
        Ok(TaskHost { process, contract })
    }

    /// Blocking, addressed only to the peer PID and epoch verified for this request.
    pub fn task_session(
        &self,
        host: &TaskHost,
        action: TaskAction,
        session: &str,
        operation: &str,
        cwd: &str,
        store: &str,
    ) -> Result<(), Error> {
        let op = match action {
            TaskAction::Create => "task.create.v1",
            TaskAction::Resume => "task.resume.v1",
            TaskAction::Inspect => "task.inspect.v1",
        };
        let mut request =
            json!({"id":request_id(), "op":op, "sessionId":session, "cwd":cwd, "store":store});
        host.contract.bind(session, operation, &mut request)?;
        let result = self.operate(host.process, request, PROMPT_TIMEOUT)?;
        if result["sessionId"] != session || result["cwd"] != cwd || result["store"] != store {
            return Err(Error::UnknownOutcome(
                "DSH App did not confirm the reserved identity and directory".into(),
            ));
        }
        Ok(())
    }

    /// Creates `session` in `cwd` through `task.create.v1`, then sends
    /// `text` as its first message under the request id
    /// `prompt_id(session, operation)` (ADR 0167). Blocking, addressed only to
    /// `host`'s launch. Until the App confirms the session, a refusal is "not
    /// sent" and a lost reply unknown. After it, a refused first message
    /// names the blank session left behind, and a lost one is unknown unless
    /// the App's own record shows it. Nothing is ever replayed.
    pub fn create(
        &self,
        host: &TaskHost,
        session: &str,
        operation: &str,
        cwd: &str,
        store: &str,
        text: &str,
    ) -> Result<(), Error> {
        self.task_session(host, TaskAction::Create, session, operation, cwd, store)?;
        let mut request = json!({
            "id": request_id(),
            "op": "prompt",
            "sessionId": session,
            "requestId": plugin_control::prompt_id(session, operation),
            "text": text,
            "mode": "queue",
        });
        host.contract.bind(session, operation, &mut request)?;
        match self.operate(host.process, request, PROMPT_TIMEOUT) {
            Ok(result) if result["accepted"] == true => Ok(()),
            Err(CallError::NotSent(why) | CallError::Refused(why)) => {
                Err(Error::Unavailable(format!(
                    "DeepSeek Harness created {session}, but its first message was not \
                     sent ({why})"
                )))
            }
            _ if self.lookup(host.process, session, operation)? => Ok(()),
            _ => Err(Error::UnknownOutcome(
                "DeepSeek Harness did not acknowledge the first message; no replay".into(),
            )),
        }
    }

    fn confirm_pending(&self, session: &str, operation: &str) -> Result<(), Error> {
        let mut pending = self.pending.lock().unwrap();
        if let Some(row) = pending
            .get(session)
            .filter(|row| row.operation == operation)
        {
            if let Some(confirm) = &row.confirmation {
                confirm()?;
            }
            pending.remove(session);
            self.confirmed
                .lock()
                .unwrap()
                .insert(session.into(), operation.into());
        }
        Ok(())
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
        let operation = crate::control_guard::current_operation()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(request_id);
        let session = self.session.clone();
        let mut request = request;
        if let Some(contract) = link.live(process).and_then(|s| s.contract) {
            request["id"] = json!(request_id());
            contract.bind(&session, &operation, &mut request)?;
        }
        crate::control_guard::spawn_blocking(move || link.operate(process, request, timeout))
            .await
            .map_err(|e| Error::UnknownOutcome(e.to_string()))?
            .map_err(Error::from)
            .and_then(|value| {
                if value["accepted"] == true {
                    Ok(value)
                } else {
                    Err(Error::UnknownOutcome(
                        "App did not acknowledge acceptance".into(),
                    ))
                }
            })
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
    fn reports_delivery(&self) -> bool {
        true
    }
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
        let operation = crate::control_guard::current_operation()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(request_id);
        let session = self.session.clone();
        let process = self.process;
        let link = self.link.clone();
        let confirmation = crate::control_guard::current_delivery().and_then(|d| d.confirmation());
        let mut request = json!({
            "op": "prompt",
            "sessionId": self.session,
            "requestId": plugin_control::prompt_id(&session, &operation),
            "text": text,
            "mode": "queue",
        });
        if let Some(contract) = link.live(process).and_then(|s| s.contract) {
            request["id"] = json!(request_id());
            contract.bind(&session, &operation, &mut request)?;
        }
        crate::control_guard::spawn_blocking(move || {
            let old = link.pending.lock().unwrap().get(&session).cloned();
            if let Some(old) = old {
                if !link.lookup(process, &session, &old.operation)? {
                    return Err(Error::Unconfirmed {
                        operation: old.operation,
                        message: "The previous App send is still unconfirmed".into(),
                    });
                }
                if old.operation == operation {
                    return Ok(());
                }
            }
            if link.confirmed.lock().unwrap().get(&session) == Some(&operation) {
                return Ok(());
            }
            crate::control_guard::check()?;
            link.pending.lock().unwrap().insert(
                session.clone(),
                PendingSend {
                    operation: operation.clone(),
                    confirmation,
                },
            );
            let outcome = link.operate(process, request, PROMPT_TIMEOUT);
            match outcome {
                Ok(ref result) if result["accepted"] == true => {
                    link.confirm_pending(&session, &operation)
                }
                Err(CallError::NotSent(reason) | CallError::Refused(reason)) => {
                    link.pending.lock().unwrap().remove(&session);
                    Err(Error::Unavailable(reason))
                }
                _ => {
                    if link.lookup(process, &session, &operation)? {
                        Ok(())
                    } else {
                        Err(Error::UnknownOutcome(
                            "Waiting for the App's send receipt; no replay".into(),
                        ))
                    }
                }
            }
        })
        .await
        .map_err(|e| Error::UnknownOutcome(e.to_string()))?
    }

    async fn lookup_send(&self, operation: &str) -> Result<bool, Error> {
        let (link, process, session, operation) = (
            self.link.clone(),
            self.process,
            self.session.clone(),
            operation.to_owned(),
        );
        tokio::task::spawn_blocking(move || link.lookup(process, &session, &operation))
            .await
            .map_err(|e| Error::Unavailable(e.to_string()))?
    }

    fn release_unconfirmed(&self, operation: &str) -> Result<(), Error> {
        if self.link.lookup(self.process, &self.session, operation)?
            || self
                .link
                .confirmed
                .lock()
                .unwrap()
                .get(&self.session)
                .is_some_and(|op| op == operation)
        {
            return Err(Error::Conflict(
                "The original App send was already accepted".into(),
            ));
        }
        let mut pending = self.link.pending.lock().unwrap();
        match pending.get(&self.session) {
            Some(row) if row.operation == operation => {
                pending.remove(&self.session);
                Ok(())
            }
            Some(_) => Err(Error::Conflict("Another App send is unconfirmed".into())),
            None => Ok(()),
        }
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
        // The app has not loaded it: not running, as the app shows it.
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

    #[test]
    fn malformed_running_state_is_not_idle() {
        let snapshot = parse_snapshot(&json!({"sessions":[{"id":"session-a","running":"false"}]}));
        assert_eq!(snapshot.state("session-a"), SessionState::Unknown);
        // A registry with an unread entry says nothing of the others.
        assert_eq!(snapshot.state("session-b"), SessionState::Unknown);
        assert_eq!(
            parse_snapshot(&json!({})).state("session-b"),
            SessionState::Unknown
        );
    }

    #[tokio::test]
    async fn revoked_authority_reaches_the_socket_worker_before_any_write() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("bridge.sock");
        let server = serve_once(&socket, None);
        let result = crate::control_guard::scope(Arc::new(|| false), async move {
            crate::control_guard::spawn_blocking(move || {
                call(&socket, None, json!({"op":"prompt"}), TIMEOUT)
            })
            .await
            .unwrap()
        })
        .await;
        assert!(matches!(result, Err(CallError::NotSent(_))));
        assert!(server.join().unwrap().is_empty());
    }

    #[test]
    fn unmatched_or_ambiguous_bound_replies_never_confirm_or_release_a_send() {
        for reply in [
            "{\"id\":\"other\",\"ok\":true,\"result\":{\"accepted\":true}}\n",
            "{\"id\":\"request\",\"ok\":true,\"control\":{\"epoch\":\"old\"},\"result\":{\"accepted\":true}}\n",
            "{\"id\":\"request\",\"ok\":false,\"control\":{\"epoch\":\"current\"},\"error\":\"host failed\"}\n",
            "{\"id\":\"request\",\"control\":{\"epoch\":\"current\"}}\n",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let socket = dir.path().join("bridge.sock");
            let server = serve_once(&socket, Some(reply));
            let result = call(
                &socket,
                None,
                json!({"id":"request","op":"prompt","control":{"epoch":"current"}}),
                TIMEOUT,
            );
            assert!(matches!(result, Err(CallError::Unknown(_))));
            assert!(!server.join().unwrap().is_empty());
        }
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
    fn task_session_requires_live_capabilities_and_an_exact_receipt() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("bridge.sock");
        let link = Link::new(socket.clone());
        assert!(link.task_host().is_err());
        let process = ProcessIdentity {
            pid: std::process::id(),
            start: 1,
        };
        *link.status.lock().unwrap() = Status::Live(process, Snapshot::default());
        assert!(link.task_host().is_err());
        let contract = Contract::parse(
            &json!({"control":{"version":1,"domain":"app","provider":"deepseek",
            "epoch":"test","capabilities":["task.create.v1","task.inspect.v1","task.resume.v1"]}}),
            "deepseek",
        )
        .unwrap();
        *link.status.lock().unwrap() = Status::Live(
            process,
            Snapshot {
                contract,
                ..Default::default()
            },
        );
        let host = link.task_host().unwrap();
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["op"], "task.resume.v1");
            assert_eq!(request["control"]["sessionId"], request["sessionId"]);
            writeln!(
                stream,
                "{}",
                json!({"id":request["id"],"ok":true,"control":request["control"],
                "result":{"sessionId":request["sessionId"],"cwd":"/original","store":"other"}})
            )
            .unwrap();
        });
        assert!(matches!(
            link.task_session(
                &host,
                TaskAction::Resume,
                &fresh_task_session(),
                "op",
                "/original",
                "expected"
            ),
            Err(Error::UnknownOutcome(_))
        ));
        thread.join().unwrap();
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

    /// A plugin stand-in that answers each of `replies` connections from
    /// the request it read, echoing its id and binding; the requests read.
    fn serve(
        socket: &Path,
        replies: Vec<fn(&Value) -> Value>,
    ) -> std::thread::JoinHandle<Vec<Value>> {
        let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
        std::thread::spawn(move || {
            let mut read = Vec::new();
            for reply in replies {
                let (stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                let mut answer = reply(&request);
                answer["id"] = request["id"].clone();
                answer["control"] = request["control"].clone();
                (&stream)
                    .write_all(format!("{answer}\n").as_bytes())
                    .unwrap();
                read.push(request);
            }
            read
        })
    }

    fn creating(socket: &Path, capabilities: Value) -> Arc<Link> {
        let link = Link::new(socket.to_path_buf());
        let state = json!({"control":{"version":1,"domain":"app","provider":"deepseek",
            "epoch":"launch-a","capabilities":capabilities}});
        let snapshot = Snapshot {
            contract: Contract::parse(&state, "deepseek").unwrap(),
            ..Snapshot::default()
        };
        let process = ProcessIdentity {
            pid: std::process::id(),
            start: 1,
        };
        *link.status.lock().unwrap() = Status::Live(process, snapshot);
        link
    }

    const TASKS: [&str; 5] = [
        "prompt",
        "prompt.lookup",
        "task.create.v1",
        "task.resume.v1",
        "task.inspect.v1",
    ];

    /// One creation of `session-a` against a stand-in answering `replies`:
    /// its result and the operations the stand-in read.
    fn create_against(
        link: &Link,
        socket: &Path,
        replies: Vec<fn(&Value) -> Value>,
    ) -> (Result<(), Error>, Vec<String>) {
        let _ = std::fs::remove_file(socket);
        let server = serve(socket, replies);
        let host = link.task_host().unwrap();
        let result = link.create(&host, "session-a", "op-1", "/tmp/tasks", "/store", "hi");
        let read = server.join().unwrap();
        let ops = read.iter().map(|r| r["op"].as_str().unwrap().to_owned());
        (result, ops.collect())
    }

    /// The App's confirmation of the session a `task.create.v1` asked for.
    fn made(r: &Value) -> Value {
        json!({"ok":true,"result":{"sessionId":r["sessionId"],"cwd":r["cwd"],"store":r["store"]}})
    }

    #[test]
    fn a_session_is_created_through_the_task_contract_then_prompted() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("bridge.sock");
        assert!(matches!(
            Link::new(socket.clone()).task_host(),
            Err(Error::Unavailable(_))
        ));
        let old = creating(&socket, json!(["prompt", "prompt.lookup"]));
        assert!(matches!(old.task_host(), Err(Error::Unavailable(_))));

        let link = creating(&socket, json!(TASKS));
        let host = link.task_host().unwrap();
        let server = serve(
            &socket,
            vec![made, |_| json!({"ok":true,"result":{"accepted":true}})],
        );
        link.create(
            &host,
            "session-a",
            "op-1",
            "/tmp/tasks",
            "/store",
            "read the task",
        )
        .unwrap();
        let [create, prompt] = &server.join().unwrap()[..] else {
            panic!("two requests")
        };
        assert_eq!(create["op"], "task.create.v1");
        assert_eq!(
            (&create["sessionId"], &create["cwd"], &create["store"]),
            (&json!("session-a"), &json!("/tmp/tasks"), &json!("/store"))
        );
        assert_eq!(prompt["op"], "prompt");
        assert_eq!(
            (&prompt["sessionId"], &prompt["text"], &prompt["mode"]),
            (
                &json!("session-a"),
                &json!("read the task"),
                &json!("queue")
            )
        );
        assert_eq!(
            prompt["requestId"],
            plugin_control::prompt_id("session-a", "op-1")
        );
        for request in [create, prompt] {
            assert_eq!(request["control"]["sessionId"], "session-a");
            assert_eq!(request["control"]["operationId"], "op-1");
        }
    }

    #[test]
    fn a_refused_creation_is_not_sent_and_a_lost_one_is_never_replayed() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("bridge.sock");
        let link = creating(&socket, json!(TASKS));
        let refused: fn(&Value) -> Value =
            |_| json!({"ok":false,"delivery":"not_started","error":"session/conflict"});
        let lost: fn(&Value) -> Value =
            |_| json!({"ok":false,"delivery":"unknown","error":"lost host response"});

        // Until the App confirms the session, nothing is prompted.
        let (result, ops) = create_against(&link, &socket, vec![refused]);
        assert!(matches!(result, Err(Error::Unavailable(m)) if m.contains("session/conflict")));
        assert_eq!(ops, ["task.create.v1"]);
        let (result, ops) = create_against(&link, &socket, vec![lost]);
        assert!(matches!(result, Err(Error::UnknownOutcome(_))));
        assert_eq!(ops, ["task.create.v1"]);
        let (result, ops) = create_against(
            &link,
            &socket,
            vec![
                |r| json!({"ok":true,"result":{"sessionId":"session-other","cwd":r["cwd"],"store":r["store"]}}),
            ],
        );
        assert!(matches!(result, Err(Error::UnknownOutcome(_))));
        assert_eq!(ops, ["task.create.v1"]);

        // A refused first message names the blank session; nothing else is sent.
        let (result, ops) = create_against(&link, &socket, vec![made, refused]);
        assert!(matches!(result, Err(Error::Unavailable(m)) if m.contains("session-a")));
        assert_eq!(ops, ["task.create.v1", "prompt"]);

        // A lost first message: only the App's own record settles it.
        let (result, ops) = create_against(
            &link,
            &socket,
            vec![made, lost, |_| json!({"ok":true,"result":{"records":[]}})],
        );
        assert!(matches!(result, Err(Error::UnknownOutcome(_))));
        assert_eq!(ops, ["task.create.v1", "prompt", "prompt.lookup"]);
        let (result, ops) = create_against(
            &link,
            &socket,
            vec![made, lost, |r| {
                json!({"ok":true,"result":{"sessionId":r["sessionId"],"requestId":r["requestId"],
                "records":[{"source":{"kind":"user","rpcId":r["requestId"]},"location":"session-event"}]}})
            }],
        );
        result.unwrap();
        assert_eq!(ops, ["task.create.v1", "prompt", "prompt.lookup"]);
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
