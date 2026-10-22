//! Independent CLI adapter, porting Orca's revisioned journal and fenced
//! mutation design into Rust. No Orca process or JavaScript runtime is used.
mod diagnostic;
mod render;
pub mod runtime;
use crate::{Error, Event};
use muster_proto::v1::{AgentMessage, AgentSession, PendingInteraction};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

pub struct Reply {
    pub runtime: String,
    pub result: Value,
}
#[async_trait::async_trait]
pub trait Rpc: Send + Sync {
    fn shutdown(&self) {}
    async fn call(
        &self,
        runtime: Option<&str>,
        method: &str,
        params: Value,
        mutation: bool,
    ) -> Result<Reply, Error>;
}

pub fn is_orca(id: &str) -> bool {
    id.starts_with("orca:")
}
fn id(runtime: &str, session: &str) -> String {
    format!("orca:{}:{}", hex::encode(runtime), hex::encode(session))
}
fn invalid() -> Error {
    Error::Unavailable("Orca returned an unsupported session response".into())
}
fn string(v: &Value, key: &str) -> Result<String, Error> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 8192)
        .map(str::to_owned)
        .ok_or_else(invalid)
}

#[derive(Clone)]
struct Entry {
    runtime: String,
    remote: String,
    info: AgentSession,
    epoch: String,
    fence: Option<u64>,
    cursor: Value,
    oldest: Value,
    has_older: bool,
    items: Vec<Value>,
    messages: Vec<AgentMessage>,
    pending: Vec<PendingInteraction>,
    opened: bool,
}

pub struct Source {
    rpc: Arc<dyn Rpc>,
    entries: Mutex<HashMap<String, Entry>>,
    serial: tokio::sync::Mutex<()>,
    events: broadcast::Sender<Event>,
}
impl Source {
    pub fn shutdown(&self) {
        self.rpc.shutdown();
    }
    pub fn new(rpc: Arc<dyn Rpc>, events: broadcast::Sender<Event>) -> Arc<Self> {
        Arc::new(Self {
            rpc,
            entries: Mutex::new(HashMap::new()),
            serial: tokio::sync::Mutex::new(()),
            events,
        })
    }
    /// Open a session. `env` is this session's provider-child environment,
    /// held in memory by the runtime and never journaled; see
    /// `OrcaCreateRequest.env`.
    pub async fn create(
        &self,
        directory: &str,
        agent: &str,
        op: &str,
        env: &std::collections::HashMap<String, String>,
    ) -> Result<String, Error> {
        let reply = self
            .rpc
            .call(
                None,
                "muster.session.create",
                json!({
                    "directory": directory,
                    "agent": agent,
                    "operation": op,
                    "env": env,
                }),
                true,
            )
            .await?;
        let key = id(&reply.runtime, &string(&reply.result, "sessionId")?);
        let _ = self.discover().await;
        Ok(key)
    }
    pub fn list(&self) -> Vec<AgentSession> {
        self.entries
            .lock()
            .unwrap()
            .values()
            .map(|e| e.info.clone())
            .collect()
    }
    pub fn session(&self, id: &str) -> Option<AgentSession> {
        self.entries.lock().unwrap().get(id).map(|e| e.info.clone())
    }
    pub fn pending(&self, id: &str) -> Vec<PendingInteraction> {
        self.entries
            .lock()
            .unwrap()
            .get(id)
            .map(|e| e.pending.clone())
            .unwrap_or_default()
    }
    fn entry(&self, id: &str) -> Result<Entry, Error> {
        self.entries
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| Error::NotFound("Orca session; refresh the session list".into()))
    }

    /// Discovery only uses Orca's structured session tabs, never a terminal
    /// title or cached CLI transcript as evidence of a writable agent.
    pub async fn discover(&self) -> Result<(), Error> {
        let _guard = self.serial.lock().await;
        let result = self
            .rpc
            .call(None, "session.tabs.listAll", json!({}), false)
            .await;
        let reply = match result {
            Ok(r) => r,
            Err(e) => {
                self.offline();
                return Err(e);
            }
        };
        let snapshots = reply.result["snapshots"].as_array().ok_or_else(invalid)?;
        let mut found = Vec::new();
        for snapshot in snapshots {
            if snapshot["agentSessionsUnverifiable"] == true {
                self.offline();
                return Err(invalid());
            }
            let workspace = string(snapshot, "worktree")?;
            for tab in snapshot["tabs"].as_array().ok_or_else(invalid)? {
                if tab["type"] != "agent-session" {
                    continue;
                }
                let remote = string(tab, "sessionId")?;
                let agent = string(tab, "agent")?;
                let key = id(&reply.runtime, &remote);
                found.push((
                    key,
                    remote,
                    workspace.clone(),
                    format!(
                        "{} · {}",
                        agent,
                        tab["title"].as_str().unwrap_or("Orca session")
                    ),
                    tab["updatedAt"].as_u64().unwrap_or(0) / 1000,
                ));
            }
        }
        let mut entries = self.entries.lock().unwrap();
        // Local CLI callers do not negotiate authoritative inventory. Missing
        // tabs become unavailable, not "ended"; keep their readable cached rows.
        let seen: std::collections::HashSet<_> =
            found.iter().map(|(key, ..)| key.clone()).collect();
        for (key, entry) in entries.iter_mut() {
            if !seen.contains(key) {
                entry.info.read_only = true;
                entry.info.state = "unknown".into();
                entry.fence = None;
            }
        }
        for (key, remote, workspace, title, updated_unix) in found {
            let entry = entries.entry(key.clone()).or_insert_with(|| Entry {
                runtime: reply.runtime.clone(),
                remote,
                info: AgentSession {
                    id: key,
                    provider: "orca".into(),
                    mode: "orca".into(),
                    controller: "orca".into(),
                    process_state: "unknown".into(),
                    state: "unknown".into(),
                    ..Default::default()
                },
                epoch: String::new(),
                fence: None,
                cursor: Value::Null,
                oldest: Value::Null,
                has_older: false,
                items: Vec::new(),
                messages: Vec::new(),
                pending: Vec::new(),
                opened: false,
            });
            entry.info.title = title;
            entry.info.cwd = workspace;
            entry.info.updated_unix = entry.info.updated_unix.max(updated_unix);
            entry.info.read_only = false;
            entry.info.transcript_notice.clear();
        }
        drop(entries);
        let _ = self.events.send(Event::ListChanged);
        Ok(())
    }
    fn offline(&self) {
        for entry in self.entries.lock().unwrap().values_mut() {
            entry.info.read_only = true;
            entry.info.state = "unknown".into();
            entry.info.transcript_notice =
                "Orca is unavailable. Cached history is read-only.".into();
            entry.fence = None;
            let _ = self.events.send(Event::State(entry.info.clone()));
        }
    }
    pub async fn refresh(&self, id: &str) -> Result<(), Error> {
        let _guard = self.serial.lock().await;
        self.read(id, false).await
    }
    async fn read(&self, id: &str, older: bool) -> Result<(), Error> {
        let entry = self.entry(id)?;
        if entry.info.read_only {
            return Err(Error::Unavailable("Orca session is unavailable".into()));
        }
        let direction = if older {
            "before"
        } else if entry.opened {
            "after"
        } else {
            "tail"
        };
        let mut params = json!({"sessionId":entry.remote,"direction":direction,"limit":200});
        if direction != "tail" {
            params["cursor"] = if older {
                entry.oldest.clone()
            } else {
                entry.cursor.clone()
            };
        }
        let reply = self
            .rpc
            .call(Some(&entry.runtime), "agentSession.history", params, false)
            .await?;
        let page = &reply.result["page"];
        if page["sessionId"] != entry.remote {
            return Err(invalid());
        }
        let epoch = string(page, "epoch")?;
        let items = page["items"].as_array().ok_or_else(invalid)?;
        let reset = !entry.opened || entry.epoch != epoch || reply.result["ok"] == false;
        let mut next = entry.clone();
        if reset {
            next.items.clear();
        }
        let removed = page["removedItemIds"].as_array().ok_or_else(invalid)?;
        next.items.retain(|item| !removed.contains(&item["itemId"]));
        for item in items {
            string(item, "itemId")?;
            if item["revision"].as_u64().filter(|v| *v > 0).is_none()
                || item["sequence"].as_u64().is_none()
            {
                return Err(invalid());
            }
            if let Some(old) = next
                .items
                .iter_mut()
                .find(|old| old["itemId"] == item["itemId"])
            {
                *old = item.clone();
            } else {
                next.items.push(item.clone());
            }
        }
        next.items.sort_by_key(|i| {
            (
                i["sequence"].as_u64().unwrap_or(0),
                i["sequenceIndex"].as_u64().unwrap_or(0),
            )
        });
        // Bound memory. The UI can load older pages again rather than retaining
        // unbounded histories in a permanently running daemon.
        if next.items.len() > 10000 {
            return Err(Error::Unavailable(
                "Orca history exceeds the adapter's 10000-item limit; reopen the session".into(),
            ));
        }
        next.epoch = epoch;
        next.fence = page["fence"].as_u64().filter(|v| *v > 0);
        if !older || reset {
            next.cursor = page["window"]["nextCursor"].clone();
        }
        if older || reset {
            next.oldest = page["window"]["oldest"].clone();
            next.has_older = page["hasOlder"].as_bool().ok_or_else(invalid)?;
        }
        next.opened = true;
        let (messages, pending) = render::project(id, &next.items)?;
        next.messages = messages;
        next.pending = pending;
        next.info.message_count = next.messages.len() as u32;
        next.info.updated_unix = next.info.updated_unix.max(
            next.items
                .iter()
                .filter_map(|i| i["observedAt"].as_u64())
                .max()
                .unwrap_or(0)
                / 1000,
        );
        next.info.state = if !next.pending.is_empty() {
            "waiting_approval"
        } else {
            match page["latestTurn"]["turn"]["state"].as_str() {
                Some("running") => "running",
                Some("completed" | "interrupted") => "idle",
                _ => "unknown",
            }
        }
        .into();
        let old_messages = entry.messages;
        let old_pending = entry.pending;
        self.entries.lock().unwrap().insert(id.into(), next.clone());
        if old_messages
            .iter()
            .any(|old| !next.messages.iter().any(|new| new.id == old.id))
        {
            let _ = self.events.send(Event::Reset { session: id.into() });
        } else {
            for message in &next.messages {
                if !old_messages.contains(message) {
                    let _ = self.events.send(Event::Message {
                        session: id.into(),
                        message: message.clone(),
                    });
                }
            }
        }
        for interaction in &old_pending {
            if !next.pending.iter().any(|p| p.id == interaction.id) {
                let _ = self.events.send(Event::InteractionResolved {
                    session: id.into(),
                    interaction: interaction.id.clone(),
                });
            }
        }
        for interaction in &next.pending {
            if !old_pending.contains(interaction) {
                let _ = self.events.send(Event::Interaction(interaction.clone()));
            }
        }
        let _ = self.events.send(Event::State(next.info));
        Ok(())
    }
    pub async fn history(
        &self,
        request: &muster_proto::v1::AgentMessagesRequest,
    ) -> Result<muster_proto::v1::AgentMessagePage, Error> {
        let _guard = self.serial.lock().await;
        self.read(&request.session, false).await?;
        // Fetch another provider page only when the caller asks past the cache.
        for _ in 0..12 {
            let e = self.entry(&request.session)?;
            let at = e
                .messages
                .iter()
                .position(|m| m.id == request.before_id)
                .unwrap_or(e.messages.len());
            if request.before_id.is_empty() || !e.has_older || at >= request.tail.max(50) as usize {
                break;
            }
            let previous = e.oldest;
            self.read(&request.session, true).await?;
            if self.entry(&request.session)?.oldest == previous {
                break;
            }
        }
        let e = self.entry(&request.session)?;
        let mut page = muster_proto::history::page(&e.messages, request);
        page.has_more |= e.has_older;
        Ok(page)
    }
    pub async fn operation(&self, id: &str, op: &str, action: Action) -> Result<String, Error> {
        let _guard = self.serial.lock().await;
        self.read(id, false).await?;
        let e = self.entry(id)?;
        let fence = e.fence.ok_or_else(|| {
            Error::Unavailable("The CLI session has no active runtime fence".into())
        })?;
        let (method, fields) = match action {
            Action::Send(text) => {
                if text.trim().is_empty() || text.len() > 256 * 1024 {
                    return Err(Error::Invalid(
                        "Orca needs a nonempty message of at most 256 KiB".into(),
                    ));
                }
                (
                    "agentSession.send",
                    json!({"body":{"kind":"message","role":"user","blocks":[{"type":"text","text":text}]}}),
                )
            }
            Action::Answers {
                interaction,
                answers,
            } => {
                let prompt = e
                    .pending
                    .iter()
                    .find(|p| p.id == interaction && p.kind == "question")
                    .ok_or_else(|| {
                        Error::Conflict("Question changed; refresh before answering".into())
                    })?;
                let item = e
                    .items
                    .iter()
                    .find(|i| render::interaction_id(i) == prompt.id)
                    .ok_or_else(invalid)?;
                let answers: Vec<_> = answers.iter().map(|a| json!({"questionId":a.question_id,"optionIds":a.option_ids,"other":a.text})).collect();
                (
                    "agentSession.respondToQuestion",
                    json!({"itemId":item["itemId"],"expectedRevision":item["revision"],"answers":answers}),
                )
            }
            Action::Interrupt => ("agentSession.cancel", json!({})),
            Action::Respond {
                interaction,
                option,
            } => {
                if !e.pending.iter().any(|p| p.id == interaction) {
                    return Err(Error::Conflict(
                        "Orca interaction changed or is unsupported; refresh before answering"
                            .into(),
                    ));
                }
                let item = e
                    .items
                    .iter()
                    .find(|i| render::interaction_id(i) == interaction)
                    .ok_or_else(|| {
                        Error::Conflict("Orca interaction changed; refresh before answering".into())
                    })?;
                let body = &item["body"];
                if body["kind"] != "approval" || body["resolution"]["state"] != "pending" {
                    return Err(Error::Unavailable(
                        "This CLI prompt shape is unsupported".into(),
                    ));
                }
                let options = body["options"].as_array().ok_or_else(invalid)?;
                let matches: Vec<_> = options
                    .iter()
                    .filter(|o| o["label"].as_str() == Some(&option))
                    .collect();
                if matches.len() != 1 {
                    return Err(Error::Invalid(
                        "Choose one of the uniquely named Orca approval options".into(),
                    ));
                }
                (
                    "agentSession.respondToApproval",
                    json!({"itemId":item["itemId"],"expectedRevision":item["revision"],"optionId":matches[0]["id"]}),
                )
            }
        };
        // Orca's IDs are timestamped; Muster's outer ledger forbids replaying
        // unknown operations, including across daemon restarts.
        let operation = operation_id(op);
        let hash = fingerprint(method, &e.remote, &fields);
        let mut params = fields.clone();
        params["envelope"] = json!({"sessionId":e.remote,"clientOperationId":operation,"expectedRuntimeFence":fence,"payloadFingerprint":hash});
        let response = self
            .rpc
            .call(Some(&e.runtime), method, params, true)
            .await?
            .result;
        if response["ok"] == false {
            return Err(Error::Conflict(
                "Orca refused the operation; refresh the session before trying a new action".into(),
            ));
        }
        if response["ok"] != true || response["fence"].as_u64() != Some(fence) {
            return Err(Error::UnknownOutcome(
                "Orca returned an unverified operation receipt".into(),
            ));
        }
        let value = &response["value"];
        let accepted = match method {
            "agentSession.send" => {
                value["clientMessageId"] == operation
                    && value["submission"]["dispatchState"] == "accepted"
            }
            "agentSession.cancel" => value["cancelled"] == true,
            _ => {
                value["resolution"]["state"] == "resolved"
                    && value["itemId"] == fields["itemId"]
                    && value["revision"]
                        .as_u64()
                        .zip(fields["expectedRevision"].as_u64())
                        .is_some_and(|(new, old)| new > old)
            }
        };
        if !accepted {
            return Err(Error::UnknownOutcome(
                "Orca has not confirmed this operation; do not resend automatically".into(),
            ));
        }
        Ok(id.into())
    }
}

pub enum Action {
    Send(String),
    Answers {
        interaction: String,
        answers: Vec<muster_proto::v1::QuestionAnswer>,
    },
    Interrupt,
    Respond {
        interaction: String,
        option: String,
    },
}
/// serde_json's default map order recursively sorts keys, matching Orca's
/// canonical fingerprint for the fixed ASCII-key request structures we send.
pub fn fingerprint(method: &str, session: &str, fields: &Value) -> String {
    hex::encode(Sha256::digest(
        serde_json::to_vec(&json!({"method":method,"sessionId":session,"fields":fields}))
            .expect("JSON value"),
    ))
}

fn operation_id(op: &str) -> String {
    let entropy = hex::encode(Sha256::digest(op.as_bytes()));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{now}-{}", &entropy[..32])
}
