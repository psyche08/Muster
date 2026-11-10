//! Owned CLI sessions driven through the native provider protocols.
mod diagnostic;
mod render;
pub mod runtime;
use crate::{Error, Event};
use muster_proto::v1::{AgentMessage, AgentSession, PendingInteraction};
use runtime::NativeRuntime;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

/// The deployed session namespace is also used by saved bindings and older peers.
pub fn is_cli(id: &str) -> bool {
    id.starts_with("orca:")
}
fn invalid() -> Error {
    Error::Unavailable("Invalid native CLI session record".into())
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
    remote: String,
    info: AgentSession,
    messages: Vec<AgentMessage>,
    pending: Vec<PendingInteraction>,
}

pub struct Source {
    runtime: Arc<NativeRuntime>,
    entries: Mutex<HashMap<String, Entry>>,
    serial: tokio::sync::Mutex<()>,
    events: broadcast::Sender<Event>,
}
impl Source {
    pub fn shutdown(&self) {
        self.runtime.shutdown();
    }
    pub fn new(runtime: Arc<NativeRuntime>, events: broadcast::Sender<Event>) -> Arc<Self> {
        Arc::new(Self {
            runtime,
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
        let remote = self.runtime.create(directory, agent, op, env)?;
        let key = self.runtime.session_key(&remote);
        // Creation is already durable even if a later catalog refresh fails.
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
    pub fn session_provider(&self, id: &str) -> Option<String> {
        self.entries
            .lock()
            .unwrap()
            .get(id)
            .map(|e| e.info.provider.clone())
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
            .ok_or_else(|| Error::NotFound("CLI session; refresh the session list".into()))
    }

    pub async fn discover(&self) -> Result<(), Error> {
        let _guard = self.serial.lock().await;
        let found = self.runtime.catalog().inspect_err(|_| self.offline())?;
        let mut entries = self.entries.lock().unwrap();
        for (remote, info) in found {
            let entry = entries.entry(info.id.clone()).or_insert_with(|| Entry {
                remote,
                info: info.clone(),
                messages: Vec::new(),
                pending: Vec::new(),
            });
            entry.info.updated_unix = entry.info.updated_unix.max(info.updated_unix);
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
                "The CLI runtime is unavailable. Cached history is read-only.".into();
            let _ = self.events.send(Event::State(entry.info.clone()));
        }
    }
    pub async fn refresh(&self, id: &str) -> Result<(), Error> {
        let _guard = self.serial.lock().await;
        self.read(id)
    }
    fn read(&self, id: &str) -> Result<(), Error> {
        let entry = self.entry(id)?;
        if entry.info.read_only {
            return Err(Error::Unavailable("CLI session is unavailable".into()));
        }
        let (state, items) = self.runtime.snapshot(&entry.remote)?;
        let (messages, pending) = render::project(id, &items)?;
        let mut next = Entry {
            remote: entry.remote,
            info: entry.info,
            messages,
            pending,
        };
        next.info.message_count = next.messages.len() as u32;
        if let Some(updated) = items.iter().filter_map(|i| i["observedAt"].as_u64()).max() {
            next.info.updated_unix = next.info.updated_unix.max(updated / 1000);
        }
        next.info.state = if !next.pending.is_empty() {
            "waiting_approval"
        } else {
            match state.as_str() {
                "running" => "running",
                "completed" | "interrupted" => "idle",
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
        self.read(&request.session)?;
        let entry = self.entry(&request.session)?;
        Ok(muster_proto::history::page(&entry.messages, request))
    }

    pub async fn operation(&self, id: &str, op: &str, action: Action) -> Result<String, Error> {
        let remote = {
            let entries = self.entries.lock().unwrap();
            let entry = entries
                .get(id)
                .ok_or_else(|| Error::NotFound("CLI session".into()))?;
            if entry.info.read_only {
                return Err(Error::Unavailable("CLI session is unavailable".into()));
            }
            entry.remote.clone()
        };
        self.runtime.apply(&remote, op, action).await?;
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
