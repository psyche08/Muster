//! muster-agent (ARCHITECTURE §3.6.2): the agent service behind musterd. Every
//! session has one viewer (its transcript on disk) and at most one
//! controller (the desktop Accessibility channel attached by musterd).
//! Desktop Chat-mode conversations (`desktop_chat`) have no transcript: the
//! platform reports what the app shows and the service keeps it.

pub mod attachments;
mod cache;
pub mod catdesk;
pub mod catdesk_app;
pub mod catdesk_ui;
mod changes;
pub mod chat_catalog;
pub mod claude;
pub mod claude_cards;
pub mod claude_runtime;
pub mod cloud;
pub mod codex;
pub mod controller;
pub mod desktop_chat;
pub mod desktop_lock;
pub mod files;
pub mod jsonl;
pub mod model;
pub mod native_options;
pub mod native_scan;
pub mod native_send;
pub mod native_sidebar;
pub mod native_step;
pub mod native_tree;
pub mod orca;
pub mod prediction;
pub mod profile;
pub mod questions;
pub mod queue;
pub mod receipt;
mod recency;
pub mod text;
pub mod transcript;
pub mod unconfirmed;
mod usage;
pub mod viewer;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use tokio::sync::{broadcast, mpsc};

use crate::controller::{Controller, ControllerEvent, ProcessState};
use crate::desktop_chat::{ChatApp, ChatKey, ChatLine, ChatTranscript};
pub use crate::model::{AgentMessage, AgentSession, PendingInteraction};
use crate::model::{ControllerKind, Message, MessageKind, Provider, Role, SessionState};
use crate::viewer::{Discovered, Viewer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid: {0}")]
    Invalid(String),
    #[error("unavailable: {0}")]
    Unavailable(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("outcome unknown: {0}")]
    UnknownOutcome(String),
    /// Not sent: an earlier write to the session has no provider receipt.
    /// `operation` binds an explicit user release before another send.
    #[error("conflict: {message}")]
    Unconfirmed { operation: String, message: String },
    #[error("io: {0}")]
    Io(String),
}

#[derive(Clone)]
pub struct AgentConfig {
    pub codex_home: Option<PathBuf>,
    pub claude_home: Option<PathBuf>,
    /// Claude.app's registry of desktop conversations, read for the
    /// sessions it has archived.
    pub claude_desktop: Option<PathBuf>,
    /// CatDesk's data directory (its SQLite store lives there).
    pub catdesk_home: Option<PathBuf>,
    pub poll: Duration,
    pub rescan: Duration,
    /// Parsed sessions nobody asked about for this long are dropped.
    pub idle_evict: Duration,
    /// The HTTPS transport for the providers' cloud sessions (`cloud`);
    /// `None` keeps the service to local sessions.
    pub cloud_http: Option<Arc<dyn cloud::Http>>,
    /// Independent Rust CLI runtime (Orca port); never an AX fallback.
    pub orca_rpc: Option<Arc<dyn orca::Rpc>>,
    /// Suggests the next message after a watched Codex session's completed
    /// turn (the composer placeholder); `None` suggests nothing.
    pub predictor: Option<Arc<dyn prediction::Predictor>>,
}

impl std::fmt::Debug for AgentConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentConfig")
            .field("codex_home", &self.codex_home)
            .field("claude_home", &self.claude_home)
            .field("claude_desktop", &self.claude_desktop)
            .field("catdesk_home", &self.catdesk_home)
            .field("poll", &self.poll)
            .field("rescan", &self.rescan)
            .field("idle_evict", &self.idle_evict)
            .field("cloud", &self.cloud_http.is_some())
            .field("predictor", &self.predictor.is_some())
            .finish()
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            codex_home: codex::home(),
            claude_home: claude::home(),
            claude_desktop: claude::desktop_registry(),
            catdesk_home: catdesk::home(),
            poll: Duration::from_millis(500),
            rescan: Duration::from_secs(5),
            idle_evict: Duration::from_secs(30 * 60),
            cloud_http: None,
            orca_rpc: None,
            predictor: None,
        }
    }
}

/// What the service pushes to whoever watches.
#[derive(Clone, Debug)]
pub enum Event {
    ListChanged,
    Message {
        session: String,
        message: AgentMessage,
    },
    State(AgentSession),
    Interaction(PendingInteraction),
    InteractionResolved {
        session: String,
        interaction: String,
    },
    /// A desktop chat's known messages were replaced (a new generation of
    /// message ids); watchers reload its history.
    Reset {
        session: String,
    },
}

struct Open {
    viewer: Box<dyn Viewer>,
    messages: Vec<Message>,
    last_used: Instant,
    stat: (SystemTime, u64),
}

/// A desktop Chat-mode conversation as last seen on screen.
struct ChatEntry {
    app: ChatApp,
    key: ChatKey,
    title: String,
    transcript: ChatTranscript,
    updated_unix: u64,
}

struct RuntimeObservation {
    observed: Instant,
    activity: SessionState,
    process_state: ProcessState,
}

/// A cloud session as last listed, and its transcript once someone read it.
struct CloudEntry {
    provider: Provider,
    remote: cloud::RemoteSession,
    /// Serialized read state; `None` until the first successful fetch.
    transcript: Arc<tokio::sync::Mutex<Option<cloud::Transcript>>>,
    opened: bool,
    messages: Vec<Message>,
    last_used: Option<Instant>,
    /// The last read's failure, logged once.
    error: Option<String>,
}

struct Inner {
    catalog: HashMap<String, Discovered>,
    open: HashMap<String, Open>,
    controllers: HashMap<String, Arc<dyn Controller>>,
    interactions: HashMap<String, String>,
    exited: HashSet<String>,
    runtime: HashMap<String, RuntimeObservation>,
    chats: HashMap<String, ChatEntry>,
    queued: HashMap<String, Vec<PendingInteraction>>,
    /// Project directories the running CatDesk reported, by session: its
    /// store keeps them only in an encrypted file.
    projects: HashMap<String, PathBuf>,
    /// The providers' cloud sessions, by service id (`cloud:<provider>:…`).
    cloud: HashMap<String, CloudEntry>,
    watching: HashMap<String, usize>,
    /// The suggested next message after a Codex session's newest completed
    /// turn, by session.
    predictions: HashMap<String, Prediction>,
}

/// A suggestion for one completed turn; `text` is `None` while it is asked
/// for or when there is none.
struct Prediction {
    turn: String,
    text: Option<String>,
}

/// Keeps a transcript alive only while an authorized RPC watch is open.
pub struct SessionWatch {
    service: Arc<AgentService>,
    session: String,
}

impl Drop for SessionWatch {
    fn drop(&mut self) {
        let mut inner = self.service.inner.lock().unwrap();
        if let Some(count) = inner.watching.get_mut(&self.session) {
            *count -= 1;
            if *count == 0 {
                inner.watching.remove(&self.session);
            }
        }
        if let Some(open) = inner.open.get_mut(&self.session) {
            open.last_used = Instant::now();
        }
        if let Some(entry) = inner.cloud.get_mut(&self.session) {
            entry.last_used = Some(Instant::now());
        }
    }
}

pub struct AgentService {
    runtime: tokio::runtime::Handle,
    config: AgentConfig,
    queue_reader: Mutex<queue::Reader>,
    chatgpt_catalog: Mutex<chat_catalog::LocalCatalog>,
    inner: Mutex<Inner>,
    events: broadcast::Sender<Event>,
    controller_events: mpsc::Sender<(String, ControllerEvent)>,
    /// One source per provider whose profile declares cloud sessions.
    cloud: Vec<Arc<cloud::Source>>,
    /// Wakes the cloud loop after a send or a newly opened session watch.
    cloud_wake: tokio::sync::Notify,
    orca: Option<Arc<orca::Source>>,
    /// Sessions whose suggestion may be due; `None` without a predictor.
    predict: Option<mpsc::UnboundedSender<String>>,
}

/// Watches the transcript trees of both providers; every event wakes the
/// poll loop, a create/remove/rename also asks for a rescan. `None` when
/// the platform offers no watcher: polling alone then does the job.
fn watch_homes(config: &AgentConfig, tx: mpsc::Sender<bool>) -> Option<notify::RecommendedWatcher> {
    use notify::event::{EventKind, ModifyKind};
    use notify::{RecursiveMode, Watcher};
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        if let Ok(event) = result {
            let structural = matches!(
                event.kind,
                EventKind::Create(_)
                    | EventKind::Remove(_)
                    | EventKind::Modify(ModifyKind::Name(_))
            );
            if structural || matches!(event.kind, EventKind::Modify(_)) {
                let _ = tx.try_send(true);
            }
        }
    })
    .map_err(|error| log::info!("file watcher unavailable, polling only: {error}"))
    .ok()?;
    let roots = [
        (config.codex_home.clone(), RecursiveMode::NonRecursive),
        (
            config.codex_home.as_ref().map(|h| h.join("sessions")),
            RecursiveMode::Recursive,
        ),
        (
            config
                .codex_home
                .as_ref()
                .map(|h| h.join("archived_sessions")),
            RecursiveMode::Recursive,
        ),
        (
            config.claude_home.as_ref().map(|h| h.join("projects")),
            RecursiveMode::Recursive,
        ),
        (config.claude_desktop.clone(), RecursiveMode::Recursive),
        (config.catdesk_home.clone(), RecursiveMode::Recursive),
    ];
    let mut watching = 0;
    for (root, mode) in roots {
        let Some(root) = root else { continue };
        if !root.is_dir() {
            continue;
        }
        match watcher.watch(&root, mode) {
            Ok(()) => {
                log::info!("watching {}", root.display());
                watching += 1;
            }
            Err(error) => log::info!("watch {}: {error}", root.display()),
        }
    }
    (watching > 0).then_some(watcher)
}

impl AgentService {
    pub fn start(config: AgentConfig) -> Arc<Self> {
        let (events, _) = broadcast::channel(1024);
        let (ctl_tx, ctl_rx) = mpsc::channel(256);
        let cloud = Self::cloud_sources(&config);
        let orca = config
            .orca_rpc
            .clone()
            .map(|rpc| orca::Source::new(rpc, events.clone()));
        let (predict, predict_rx) = match config.predictor {
            Some(_) => {
                let (tx, rx) = mpsc::unbounded_channel();
                (Some(tx), Some(rx))
            }
            None => (None, None),
        };
        let service = Arc::new(Self {
            runtime: tokio::runtime::Handle::current(),
            queue_reader: Mutex::new(queue::Reader::default()),
            chatgpt_catalog: Mutex::new(chat_catalog::LocalCatalog::default()),
            config,
            inner: Mutex::new(Inner {
                catalog: HashMap::new(),
                open: HashMap::new(),
                controllers: HashMap::new(),
                interactions: HashMap::new(),
                exited: HashSet::new(),
                runtime: HashMap::new(),
                chats: HashMap::new(),
                queued: HashMap::new(),
                projects: HashMap::new(),
                cloud: HashMap::new(),
                watching: HashMap::new(),
                predictions: HashMap::new(),
            }),
            events,
            controller_events: ctl_tx,
            cloud,
            cloud_wake: tokio::sync::Notify::new(),
            orca,
            predict,
        });
        let (wake_tx, wake_rx) = mpsc::channel(1);
        let watcher = watch_homes(&service.config, wake_tx.clone());
        tokio::spawn(service.clone().poll_loop(wake_rx, wake_tx, watcher));
        tokio::spawn(service.clone().controller_loop(ctl_rx));
        if let Some(rx) = predict_rx {
            tokio::spawn(service.clone().prediction_loop(rx));
        }
        if !service.cloud.is_empty() {
            tokio::spawn(service.clone().cloud_loop());
        }
        if let Some(source) = service.orca.clone() {
            let weak = Arc::downgrade(&service);
            tokio::spawn(async move {
                let mut tick = 0u64;
                loop {
                    let Some(service) = weak.upgrade() else {
                        break;
                    };
                    if tick % 3 == 0 {
                        let _ = source.discover().await;
                    }
                    let watched: Vec<_> = service
                        .inner
                        .lock()
                        .unwrap()
                        .watching
                        .keys()
                        .filter(|id| orca::is_orca(id))
                        .cloned()
                        .collect();
                    for id in watched {
                        let _ = source.refresh(&id).await;
                    }
                    drop(service);
                    tick = tick.wrapping_add(1);
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            });
        }
        service
    }

    pub fn orca_source(&self) -> Result<&Arc<orca::Source>, Error> {
        self.orca
            .as_ref()
            .ok_or_else(|| Error::Unavailable("CLI sessions are unavailable on this device".into()))
    }

    /// A cloud source for every provider whose profile declares one and
    /// whose CLI home (where its credentials live) is known.
    fn cloud_sources(config: &AgentConfig) -> Vec<Arc<cloud::Source>> {
        let Some(http) = &config.cloud_http else {
            return Vec::new();
        };
        Provider::ALL
            .iter()
            .filter_map(|provider| {
                let cloud = provider.profile().cloud?;
                let home = match provider {
                    Provider::Codex => config.codex_home.clone(),
                    Provider::Claude => config.claude_home.clone(),
                    Provider::Catdesk => None,
                }?;
                Some(Arc::new(cloud::Source::new(
                    *provider,
                    cloud,
                    home,
                    http.clone(),
                )))
            })
            .collect()
    }

    /// The cloud source of a session id, if it is a cloud session.
    fn cloud_source(&self, id: &str) -> Result<(Arc<cloud::Source>, String), Error> {
        let (provider, remote) =
            cloud::parse(id).ok_or_else(|| Error::NotFound(format!("session {id}")))?;
        self.cloud
            .iter()
            .find(|s| s.provider() == provider)
            .cloned()
            .map(|s| (s, remote.to_string()))
            .ok_or_else(|| {
                Error::Unavailable(format!(
                    "{} cloud sessions are off on this device",
                    provider.profile().app.agent_label
                ))
            })
    }

    /// Keeps catalog polling at each profile's cadence while refreshing
    /// visible transcripts every two seconds.
    async fn cloud_loop(self: Arc<Self>) {
        let mut failures: HashMap<Provider, String> = HashMap::new();
        let mut listed: HashMap<Provider, Instant> = HashMap::new();
        let mut read: HashMap<String, Instant> = HashMap::new();
        loop {
            for source in &self.cloud {
                let provider = source.provider();
                if listed
                    .get(&provider)
                    .is_some_and(|at| at.elapsed() < source.poll())
                {
                    continue;
                }
                listed.insert(provider, Instant::now());
                match source.list().await {
                    Ok(sessions) => {
                        if failures.remove(&provider).is_some() {
                            log::info!("{} reachable again", source.cloud().label);
                        }
                        self.merge_cloud(provider, sessions);
                    }
                    Err(error) => {
                        let text = error.to_string();
                        if failures.get(&provider) != Some(&text) {
                            log::warn!("{}: {text}", source.cloud().label);
                            failures.insert(provider, text);
                        }
                    }
                }
            }
            let ids: Vec<String> = {
                let inner = self.inner.lock().unwrap();
                inner
                    .cloud
                    .iter()
                    .filter(|(id, e)| {
                        let watched = inner.watching.contains_key(*id);
                        let cadence = if watched {
                            Duration::from_secs(2)
                        } else {
                            self.cloud
                                .iter()
                                .find(|s| s.provider() == e.provider)
                                .unwrap()
                                .poll()
                        };
                        e.opened
                            && (watched || Self::cloud_followed(e, self.config.idle_evict))
                            && read.get(*id).is_none_or(|at| at.elapsed() >= cadence)
                    })
                    .map(|(id, _)| id.clone())
                    .collect()
            };
            for id in ids {
                read.insert(id.clone(), Instant::now());
                if let Err(error) = self.read_cloud(&id).await {
                    log::debug!("cloud read {id}: {error}");
                }
            }
            let interval = self
                .cloud
                .iter()
                .map(|s| s.poll())
                .min()
                .unwrap_or(Duration::from_secs(60));
            let followed = {
                let inner = self.inner.lock().unwrap();
                inner
                    .cloud
                    .values()
                    .any(|e| Self::cloud_followed(e, self.config.idle_evict))
            };
            let watching_cloud = {
                let inner = self.inner.lock().unwrap();
                read.retain(|id, _| inner.cloud.contains_key(id));
                inner.watching.keys().any(|id| cloud::is_cloud(id))
            };
            let wait = if watching_cloud {
                Duration::from_secs(2)
            } else if self.watched() || followed {
                interval
            } else {
                interval * 4
            };
            tokio::select! {
                _ = tokio::time::sleep(wait) => {}
                _ = self.cloud_wake.notified() => {}
            }
        }
    }

    fn cloud_followed(entry: &CloudEntry, idle: Duration) -> bool {
        entry.last_used.is_some_and(|at| at.elapsed() < idle)
    }

    /// Takes a provider's listing into the catalog of cloud sessions.
    fn merge_cloud(&self, provider: Provider, sessions: Vec<cloud::RemoteSession>) {
        let mut emitted = Vec::new();
        {
            let mut inner = self.inner.lock().unwrap();
            let listed: HashSet<String> = sessions
                .iter()
                .map(|s| cloud::session_id(provider, &s.remote_id))
                .collect();
            let before = inner.cloud.len();
            inner
                .cloud
                .retain(|id, e| e.provider != provider || listed.contains(id));
            let mut changed = inner.cloud.len() != before;
            for remote in sessions {
                let id = cloud::session_id(provider, &remote.remote_id);
                match inner.cloud.get_mut(&id) {
                    Some(entry) => {
                        if entry.remote != remote {
                            let state_changed = entry.remote.state != remote.state;
                            // A read knows the state and title better than
                            // the listing while it follows the session.
                            let followed = entry.opened;
                            let (state, title) = if followed {
                                (entry.remote.state, entry.remote.title.clone())
                            } else {
                                (remote.state, remote.title.clone())
                            };
                            entry.remote = cloud::RemoteSession {
                                state,
                                title,
                                ..remote
                            };
                            changed = true;
                            if state_changed && !followed {
                                emitted.push(id.clone());
                            }
                        }
                    }
                    None => {
                        inner.cloud.insert(
                            id,
                            CloudEntry {
                                provider,
                                remote,
                                transcript: Arc::new(tokio::sync::Mutex::new(None)),
                                opened: false,
                                messages: Vec::new(),
                                last_used: None,
                                error: None,
                            },
                        );
                        changed = true;
                    }
                }
            }
            let states: Vec<Event> = emitted
                .iter()
                .filter_map(|id| Self::project_cloud(&inner, id))
                .map(Event::State)
                .collect();
            emitted.clear();
            for ev in states {
                let _ = self.events.send(ev);
            }
            if changed {
                let _ = self.events.send(Event::ListChanged);
            }
        }
    }

    /// Refreshes a cloud session immediately when history is requested.
    async fn ensure_cloud_open(&self, id: &str) -> Result<(), Error> {
        {
            let mut inner = self.inner.lock().unwrap();
            let Some(entry) = inner.cloud.get_mut(id) else {
                return Err(Error::NotFound(format!("session {id}")));
            };
            entry.last_used = Some(Instant::now());
        }
        // Opening/reopening asks the provider now, never waits for the next poll.
        self.read_cloud(id).await?;
        Ok(())
    }

    /// One read of a cloud session, applying what it brought.
    async fn read_cloud(&self, id: &str) -> Result<(), Error> {
        let (source, remote_id) = self.cloud_source(id)?;
        let (transcript, remote) = {
            let inner = self.inner.lock().unwrap();
            let Some(entry) = inner.cloud.get(id) else {
                return Err(Error::NotFound(format!("session {id}")));
            };
            (entry.transcript.clone(), entry.remote.clone())
        };
        // Serialize reads and keep the cursor in its guard even if a caller is
        // cancelled. A concurrent first read must not return an empty cache.
        let mut cursor = transcript.lock().await;
        let mut next = cursor.clone().unwrap_or_else(|| source.transcript(&remote));
        let fetched = source.read(&remote_id, &mut next).await;
        let mut emitted = Vec::new();
        {
            let mut inner = self.inner.lock().unwrap();
            let Some(entry) = inner.cloud.get_mut(id) else {
                return Ok(());
            };
            entry.opened = true;
            let fetched = match fetched {
                Ok(fetched) => {
                    entry.error = None;
                    *cursor = Some(next);
                    fetched
                }
                Err(error) => {
                    let text = error.to_string();
                    if entry.error.as_deref() != Some(&text) {
                        log::warn!("{} read {}: {text}", source.cloud().label, remote_id);
                        entry.error = Some(text.clone());
                    }
                    return Err(error.into());
                }
            };
            let mut touched = false;
            if fetched.replace {
                if entry.messages != fetched.messages {
                    entry.messages = fetched.messages;
                    emitted.push(Event::Reset {
                        session: id.to_string(),
                    });
                    touched = true;
                }
            } else if !fetched.messages.is_empty() {
                for m in &fetched.messages {
                    emitted.push(Event::Message {
                        session: id.to_string(),
                        message: m.to_proto(),
                    });
                }
                if let Some(last) = fetched.messages.last() {
                    entry.remote.updated_unix = entry.remote.updated_unix.max(last.ts_unix);
                }
                entry.messages.extend(fetched.messages);
                touched = true;
            }
            if let Some(state) = fetched.state
                && entry.remote.state != state
                && !entry.remote.archived
            {
                entry.remote.state = state;
                touched = true;
            }
            if let Some(title) = fetched.title
                && !title.is_empty()
                && entry.remote.title != title
            {
                entry.remote.title = title;
                touched = true;
            }
            if touched {
                if let Some(session) = Self::project_cloud(&inner, id) {
                    emitted.push(Event::State(session));
                }
                emitted.push(Event::ListChanged);
            }
        }
        for ev in emitted {
            let _ = self.events.send(ev);
        }
        Ok(())
    }

    fn project_cloud(inner: &Inner, id: &str) -> Option<AgentSession> {
        let entry = inner.cloud.get(id)?;
        Some(AgentSession {
            id: id.to_string(),
            provider: entry.provider.as_str().into(),
            title: entry.remote.title.clone(),
            // Shown where a local session shows its directory.
            cwd: entry.remote.repo.clone(),
            state: entry.remote.state.as_str().into(),
            process_state: if entry.remote.archived {
                ProcessState::Ended
            } else {
                ProcessState::Running
            }
            .as_str()
            .into(),
            updated_unix: entry.remote.updated_unix,
            message_count: entry.messages.len() as u32,
            controller: ControllerKind::Cloud.as_str().into(),
            archived: entry.remote.archived,
            device_id: String::new(),
            device_name: String::new(),
            mode: "cloud".into(),
            ..Default::default()
        })
    }

    pub fn queued_messages(&self, session: &str) -> Result<Vec<queue::QueuedMessage>, Error> {
        let discovered = self.inner.lock().unwrap().catalog.get(session).cloned();
        let Some(discovered) = discovered.filter(|d| !d.hidden) else {
            return Ok(Vec::new());
        };
        if discovered.provider == Provider::Claude {
            return queue::read_claude(
                &mut self.queue_reader.lock().unwrap(),
                &discovered.path,
                session,
            );
        }
        if discovered.provider != Provider::Codex {
            return Ok(Vec::new());
        }
        let home = self
            .config
            .codex_home
            .as_ref()
            .ok_or_else(|| Error::Unavailable("Codex queue unavailable".into()))?;
        queue::read_session(&mut self.queue_reader.lock().unwrap(), home, session)
    }

    pub fn queue_source(self: &Arc<Self>, session: &str) -> queue::Source {
        let agent = Arc::downgrade(self);
        let session = session.to_owned();
        Arc::new(move || {
            agent
                .upgrade()
                .ok_or_else(|| Error::Unavailable("Agent stopped".into()))?
                .queued_messages(&session)
        })
    }

    fn poll_queues(&self) {
        if !self.watched() {
            return;
        }
        let ids: Vec<_> = self.inner.lock().unwrap().open.keys().cloned().collect();
        for session in ids {
            let Ok(messages) = self.queued_messages(&session) else {
                continue;
            };
            let current: Vec<_> = messages.iter().map(|m| m.interaction(&session)).collect();
            let old = self
                .inner
                .lock()
                .unwrap()
                .queued
                .insert(session.clone(), current.clone())
                .unwrap_or_default();
            for item in &old {
                if !current.iter().any(|q| q.id == item.id) {
                    let _ = self.events.send(Event::InteractionResolved {
                        session: session.clone(),
                        interaction: item.id.clone(),
                    });
                }
            }
            for item in current {
                if !old.contains(&item) {
                    let _ = self.events.send(Event::Interaction(item));
                }
            }
        }
    }

    pub fn send_receipt_source(self: &Arc<Self>, session: &str) -> receipt::Source {
        let agent = Arc::downgrade(self);
        let session = session.to_string();
        Arc::new(move |text| {
            let agent = agent
                .upgrade()
                .ok_or_else(|| Error::Unavailable("Agent stopped".into()))?;
            let record = agent
                .inner
                .lock()
                .unwrap()
                .catalog
                .get(&session)
                .cloned()
                .ok_or_else(|| Error::NotFound(format!("session {session}")))?;
            receipt::Receipt::for_session(&record, text).map_err(|e| {
                Error::Unavailable(format!("Cannot observe send receipt; nothing sent: {e}"))
            })
        })
    }

    pub fn question_source(self: &Arc<Self>, session: &str) -> questions::Source {
        let agent = Arc::downgrade(self);
        let session = session.to_string();
        type Cache = (
            (std::path::PathBuf, SystemTime, u64),
            Option<questions::Snapshot>,
        );
        let cache: Mutex<Option<Cache>> = Mutex::new(None);
        let claude: Mutex<Option<claude_cards::TrackedFile>> = Mutex::new(None);
        Arc::new(move || {
            let agent = agent.upgrade()?;
            let d = agent.inner.lock().ok()?.catalog.get(&session)?.clone();
            if d.provider == Provider::Claude {
                let mut reader = claude.lock().ok()?;
                return reader
                    .get_or_insert_with(|| claude_cards::TrackedFile::new(&d.path, &session))
                    .read(&d.path, &session);
            }
            let meta = std::fs::metadata(&d.path).ok()?;
            let key = (d.path.clone(), meta.modified().ok()?, meta.len());
            let mut cache = cache.lock().ok()?;
            if let Some((old_key, value)) = &*cache {
                if old_key == &key {
                    return value.clone();
                }
            }
            let value = questions::read_snapshot(&d.path, d.provider, &session);
            *cache = Some((key, value.clone()));
            value
        })
    }

    pub fn watch_session(self: &Arc<Self>, session: &str) -> SessionWatch {
        *self
            .inner
            .lock()
            .unwrap()
            .watching
            .entry(session.to_owned())
            .or_default() += 1;
        self.cloud_wake.notify_one();
        // A suggestion for the turn that ended before anyone watched.
        self.want_prediction(session);
        SessionWatch {
            service: self.clone(),
            session: session.to_owned(),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    fn scan_all(config: &AgentConfig, cache: &HashMap<String, Discovered>) -> Vec<Discovered> {
        let mut out = Vec::new();
        if let Some(home) = &config.codex_home {
            out.extend(codex::scan(home));
        }
        if let Some(home) = &config.claude_home {
            let mut found = claude::scan(home);
            // Claude.app archives conversations in its own registry and then
            // lists none of their sessions.
            if let Some(root) = &config.claude_desktop {
                claude::apply_desktop_relationships(root, &mut found);
                let archived = claude::desktop_archived(root);
                for d in found.iter_mut().filter(|d| archived.contains(&d.id)) {
                    d.archived = true;
                    d.meta.archived = true;
                }
            }
            out.extend(found);
        }
        if let Some(home) = &config.catdesk_home {
            out.extend(catdesk::scan(home));
        }
        for d in &mut out {
            if !d.provider.file_transcripts() {
                // The store reports its own titles and message times; no file
                // to scan. Keep a project directory the running app reported.
                if let Some(known) = cache.get(&d.id)
                    && d.meta.cwd.as_os_str().is_empty()
                {
                    d.meta.cwd = known.meta.cwd.clone();
                }
                continue;
            }
            let known = cache.get(&d.id).filter(|known| known.path == d.path);
            let unchanged = known.filter(|k| k.modified == d.modified && k.size == d.size);
            d.meta.updated_unix = match unchanged {
                Some(known) => known.meta.updated_unix,
                None => recency::last_message_time(&d.path, d.provider).unwrap_or(0),
            };
            match d.provider {
                // Codex names come from its session index, read every scan.
                // Keep a title a viewer refined (first prompt) when it has none.
                Provider::Codex => {
                    if let Some(known) = cache.get(&d.id)
                        && d.meta.title.starts_with("session ")
                        && !known.meta.title.starts_with("session ")
                    {
                        d.meta.title = known.meta.title.clone();
                    }
                }
                // A Claude name lives in its transcript, and the head holds
                // only the first one. Start from what a scan or a viewer last
                // knew about this file, then look for a later rename.
                Provider::Claude => {
                    if let Some(known) = unchanged {
                        d.meta.title = known.meta.title.clone();
                        d.custom_title = known.custom_title;
                        continue;
                    }
                    let mut prior = (d.meta.title.clone(), d.custom_title);
                    if let Some(known) = known
                        && !(d.custom_title && !known.custom_title)
                        && !known.meta.title.starts_with("session ")
                    {
                        prior = (known.meta.title.clone(), known.custom_title);
                    }
                    (d.meta.title, d.custom_title) = claude::refreshed_title(&d.path, prior);
                }
                Provider::Catdesk => {}
            }
        }
        out
    }

    /// Rescans the transcript roots now.
    pub async fn rescan(&self) {
        let config = self.config.clone();
        let cache = self.inner.lock().unwrap().catalog.clone();
        let found = tokio::task::spawn_blocking(move || Self::scan_all(&config, &cache))
            .await
            .unwrap_or_default();
        let (changed, replaced) = {
            let mut inner = self.inner.lock().unwrap();
            // Desktop resumes may create multiple rollout files for one ID.
            // Directory iteration order must not select an obsolete segment.
            let mut new_catalog: HashMap<String, Discovered> = HashMap::new();
            for d in found {
                let rank = |d: &Discovered| (d.meta.updated_unix, d.modified, d.path.clone());
                if new_catalog
                    .get(&d.id)
                    .is_none_or(|old| rank(&d) > rank(old))
                {
                    new_catalog.insert(d.id.clone(), d);
                }
            }
            for (id, d) in new_catalog.iter_mut() {
                if d.meta.cwd.as_os_str().is_empty()
                    && let Some(project) = inner.projects.get(id)
                {
                    d.meta.cwd = project.clone();
                }
            }
            let changed = new_catalog.len() != inner.catalog.len()
                || new_catalog.iter().any(|(id, d)| {
                    inner.catalog.get(id).is_none_or(|old| {
                        old.meta.title != d.meta.title
                            || old.meta.updated_unix != d.meta.updated_unix
                            || old.meta.cwd != d.meta.cwd
                            || old.path != d.path
                            || old.modified != d.modified
                            || old.size != d.size
                            || old.archived != d.archived
                            || old.parent_id != d.parent_id
                            || old.relation != d.relation
                    })
                });
            let replaced: Vec<_> = new_catalog
                .iter()
                .filter_map(|(id, d)| {
                    inner
                        .catalog
                        .get(id)
                        .filter(|old| old.path != d.path)
                        .map(|_| id.clone())
                })
                .collect();
            for id in &replaced {
                inner.open.remove(id);
            }
            inner.catalog = new_catalog;
            (changed, replaced)
        };
        for session in replaced {
            let _ = self.events.send(Event::Reset { session });
        }
        if changed {
            let _ = self.events.send(Event::ListChanged);
        }
    }

    /// Polls the open transcripts when the filesystem watcher wakes it,
    /// and on a timer as the fallback (a watcher may be missing or may miss
    /// events on some filesystems). The watcher lives as long as the loop;
    /// keeping a sender here means `recv` never ends.
    async fn poll_loop(
        self: Arc<Self>,
        mut wake: mpsc::Receiver<bool>,
        _keep: mpsc::Sender<bool>,
        _watcher: Option<notify::RecommendedWatcher>,
    ) {
        self.rescan().await;
        let mut last_rescan = Instant::now();
        // A structural event gets a rescan now and one more shortly after:
        // a file created inside a directory that was itself just created
        // can land before the watcher covers the new directory.
        let mut followup: Option<Instant> = None;
        let mut catalog_dirty = false;
        loop {
            let mut wait = match followup {
                Some(at) => at
                    .saturating_duration_since(Instant::now())
                    .min(self.config.poll),
                None => self.config.poll,
            };
            if catalog_dirty {
                wait = wait.min(Duration::from_millis(500).saturating_sub(last_rescan.elapsed()));
            }
            let mut structural = false;
            tokio::select! {
                _ = tokio::time::sleep(wait) => {}
                got = wake.recv() => {
                    structural |= got.unwrap_or(false);
                    // One write produces a burst of events; take them together.
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    while let Ok(more) = wake.try_recv() {
                        structural |= more;
                    }
                }
            }
            catalog_dirty |= structural;
            self.poll_queues();
            self.poll_open().await;
            let followup_due = followup.is_some_and(|at| Instant::now() >= at);
            if followup_due {
                followup = None;
            }
            if (catalog_dirty && last_rescan.elapsed() >= Duration::from_millis(500))
                || followup_due
                || last_rescan.elapsed() >= self.config.rescan
            {
                let changed_files = catalog_dirty;
                catalog_dirty = false;
                self.rescan().await;
                last_rescan = Instant::now();
                if changed_files {
                    followup = Some(Instant::now() + Duration::from_millis(500));
                }
            }
        }
    }

    async fn poll_open(&self) {
        let ids: Vec<String> = self.inner.lock().unwrap().open.keys().cloned().collect();
        self.poll_transcripts(ids);
    }

    fn poll_transcripts(&self, ids: Vec<String>) {
        for id in ids {
            let mut emitted = Vec::new();
            let mut predict = false;
            {
                let mut inner = self.inner.lock().unwrap();
                let watched = inner.watching.contains_key(&id);
                let retained = inner.controllers.contains_key(&id) || watched;
                let Some(open) = inner.open.get_mut(&id) else {
                    continue;
                };
                if open.last_used.elapsed() > self.config.idle_evict && !retained {
                    inner.open.remove(&id);
                    continue;
                }
                let Ok(stat) = open.viewer.stamp() else {
                    continue;
                };
                if stat == open.stat {
                    continue;
                }
                match open.viewer.poll() {
                    Ok(polled) => {
                        open.stat = stat;
                        if polled.reset {
                            emitted.push(Event::Reset {
                                session: id.clone(),
                            });
                            open.messages.clear();
                        }
                        for m in &polled.messages {
                            emitted.push(Event::Message {
                                session: id.clone(),
                                message: m.to_proto(),
                            });
                        }
                        // A turn ended or a new one began: the suggestion is due
                        // or stale.
                        let turn_changed = polled
                            .messages
                            .iter()
                            .any(|m| m.kind == MessageKind::TurnSummary || m.role == Role::User);
                        open.messages.extend(polled.messages);
                        let touched = !emitted.is_empty() || polled.meta_changed;
                        if touched {
                            let (title, custom) =
                                (open.viewer.meta().title.clone(), open.viewer.custom_title());
                            if let Some(d) = inner.catalog.get_mut(&id) {
                                // Keep a rename the viewer read for the next scan.
                                Self::refine_catalog_title(d, title, custom);
                                // The stat is also scan_all's recency cache key.
                                // Never mark fresh bytes as cached while retaining
                                // the old last-message time (even for an open viewer).
                                // A store-backed provider has no transcript file.
                                if d.provider.file_transcripts()
                                    && let Ok(time) =
                                        recency::last_message_time(&d.path, d.provider)
                                {
                                    d.meta.updated_unix = time;
                                    d.modified = stat.0;
                                    d.size = stat.1;
                                }
                            }
                            if let Some(session) = Self::project_locked(&inner, &id) {
                                emitted.push(Event::State(session));
                            }
                            emitted.push(Event::ListChanged);
                        }
                        predict = turn_changed
                            && watched
                            && inner
                                .catalog
                                .get(&id)
                                .is_some_and(|d| d.provider == Provider::Codex);
                    }
                    Err(error) => log::debug!("poll {id}: {error}"),
                }
            }
            for ev in emitted {
                let _ = self.events.send(ev);
            }
            if predict {
                self.want_prediction(&id);
            }
        }
    }

    async fn controller_loop(self: Arc<Self>, mut rx: mpsc::Receiver<(String, ControllerEvent)>) {
        while let Some((session, event)) = rx.recv().await {
            match event {
                ControllerEvent::State(_) => {
                    let projected = Self::project_locked(&self.inner.lock().unwrap(), &session);
                    if let Some(s) = projected {
                        let _ = self.events.send(Event::State(s));
                    }
                }
                ControllerEvent::Interaction(interaction) => {
                    self.inner
                        .lock()
                        .unwrap()
                        .interactions
                        .insert(interaction.id.clone(), session.clone());
                    let _ = self.events.send(Event::Interaction(interaction));
                }
                ControllerEvent::Resolved { interaction } => {
                    self.inner.lock().unwrap().interactions.remove(&interaction);
                    let _ = self.events.send(Event::InteractionResolved {
                        session,
                        interaction,
                    });
                }
                ControllerEvent::Detached { process } => {
                    let projected = {
                        let mut inner = self.inner.lock().unwrap();
                        // A late exit from an older app launch cannot detach its replacement.
                        if !inner
                            .controllers
                            .get(&session)
                            .is_some_and(|c| c.process_identity() == process)
                        {
                            continue;
                        }
                        inner.controllers.remove(&session);
                        inner.exited.insert(session.clone());
                        inner.interactions.retain(|_, s| s != &session);
                        Self::project_locked(&inner, &session)
                    };
                    if let Some(s) = projected {
                        let _ = self.events.send(Event::State(s));
                    }
                }
            }
        }
    }

    /// Read-only OS evidence; it never attaches a controller or grants input.
    pub fn observe_runtime_files(&self, files: Vec<(controller::ProcessIdentity, PathBuf)>) {
        self.observe_runtime(files, Vec::new(), None);
    }

    /// `catdesk`: what the running CatDesk reported through its socket, for
    /// the verified process serving it. Its live status is the app's own
    /// view of activity; the store holds no process binding.
    pub fn observe_runtime(
        &self,
        files: Vec<(controller::ProcessIdentity, PathBuf)>,
        claude: Vec<(claude_runtime::Registration, ProcessState)>,
        catdesk: Option<catdesk_app::Runtime>,
    ) {
        let catalog = self.inner.lock().unwrap().catalog.clone();
        let mut observed = HashMap::new();
        let mut projects = Vec::new();
        for session in catdesk.iter().flat_map(|r| r.sessions.iter()) {
            if let Some(project) = &session.project {
                projects.push((session.id.clone(), project.clone()));
            }
            if catalog
                .get(&session.id)
                .is_some_and(|d| d.provider == Provider::Catdesk)
            {
                observed.insert(
                    session.id.clone(),
                    RuntimeObservation {
                        observed: Instant::now(),
                        activity: catdesk.as_ref().unwrap().activity(session),
                        process_state: ProcessState::Running,
                    },
                );
            }
        }
        for (process, path) in files {
            for (id, d) in catalog.iter().filter(|(_, d)| d.path == path) {
                observed.insert(
                    id.clone(),
                    RuntimeObservation {
                        observed: Instant::now(),
                        activity: recency::activity(&d.path, d.provider, process.start),
                        process_state: ProcessState::Running,
                    },
                );
            }
        }
        for (registration, process_state) in claude {
            let Some(d) = catalog
                .get(&registration.session)
                .filter(|d| d.provider == Provider::Claude)
            else {
                continue;
            };
            let (transcript, timestamp) =
                recency::activity_at(&d.path, d.provider, registration.process.start);
            // An assistant end_turn finishes its reply, not the worker's
            // lifecycle: Stop hooks, queued continuations and approvals can
            // still be active. A verified busy/waiting worker owns that phase.
            let active_after_reply = transcript == SessionState::Idle
                && matches!(
                    registration.activity,
                    SessionState::Running | SessionState::WaitingApproval
                );
            let activity = if active_after_reply
                || (registration.status_unix >= timestamp
                    && registration.activity != SessionState::Unknown)
            {
                registration.activity
            } else {
                transcript
            };
            // Claude's inline subagents share this verified worker, but not its
            // turn state: a parent can be idle while background work continues.
            // Only provider-declared children and records from this launch count.
            for child in catalog.values().filter(|child| {
                child.provider == Provider::Claude
                    && child.hidden
                    && !child.archived
                    && child.parent_id.as_deref() == Some(registration.session.as_str())
            }) {
                observed.insert(
                    child.id.clone(),
                    RuntimeObservation {
                        observed: Instant::now(),
                        activity: recency::subagent_activity_at(
                            &child.path,
                            child.provider,
                            registration.process.start,
                        )
                        .0,
                        process_state,
                    },
                );
            }
            observed.insert(
                registration.session,
                RuntimeObservation {
                    observed: Instant::now(),
                    activity,
                    process_state,
                },
            );
        }
        {
            let mut inner = self.inner.lock().unwrap();
            inner.runtime = observed;
            for (id, project) in projects {
                if let Some(d) = inner.catalog.get_mut(&id) {
                    d.meta.cwd = project.clone();
                }
                inner.projects.insert(id, project);
            }
        }
        let _ = self.events.send(Event::ListChanged);
    }

    // Codex's index is read on every scan. A viewer opened before its rename
    // must not replace that current name with the original prompt.
    fn refine_catalog_title(d: &mut Discovered, title: String, custom: bool) {
        if d.provider != Provider::Codex || d.meta.title.starts_with("session ") {
            d.meta.title = title;
            d.custom_title = custom;
        }
    }

    /// Codex uses its current index; transcript-named providers use their viewer.
    fn title_locked<'a>(inner: &'a Inner, d: &'a Discovered) -> &'a str {
        if d.provider == Provider::Codex {
            return &d.meta.title;
        }
        inner.open.get(&d.id).map_or(d.meta.title.as_str(), |open| {
            open.viewer.meta().title.as_str()
        })
    }

    /// Asks the prediction loop to bring a session's suggestion up to date.
    fn want_prediction(&self, session: &str) {
        if let Some(predict) = &self.predict {
            let _ = predict.send(session.to_owned());
        }
    }

    async fn prediction_loop(self: Arc<Self>, mut wanted: mpsc::UnboundedReceiver<String>) {
        while let Some(session) = wanted.recv().await {
            self.refresh_prediction(session).await;
        }
    }

    /// One suggestion per completed turn of a watched Codex session: asked
    /// for once, kept while that turn stays the newest, dropped when a new
    /// turn starts.
    async fn refresh_prediction(self: &Arc<Self>, id: String) {
        let Some(predictor) = self.config.predictor.clone() else {
            return;
        };
        let rollout = {
            let inner = self.inner.lock().unwrap();
            match inner.catalog.get(&id) {
                Some(d)
                    if d.provider == Provider::Codex
                        && !d.hidden
                        && inner.watching.contains_key(&id) =>
                {
                    d.path.clone()
                }
                _ => return,
            }
        };
        let prediction::Turn::Completed(turn) = Self::latest_turn(&rollout).await else {
            self.drop_prediction(&id);
            return;
        };
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.predictions.get(&id).is_some_and(|p| p.turn == turn) {
                return;
            }
            let pending = Prediction {
                turn: turn.clone(),
                text: None,
            };
            if inner
                .predictions
                .insert(id.clone(), pending)
                .is_some_and(|p| p.text.is_some())
                && let Some(session) = Self::project_locked(&inner, &id)
            {
                let _ = self.events.send(Event::State(session));
            }
        }
        let service = self.clone();
        tokio::spawn(async move {
            let request = prediction::Request {
                thread: id.clone(),
                turn: turn.clone(),
                rollout: rollout.clone(),
            };
            let Some(text) = predictor.predict(request).await else {
                return;
            };
            // Kept only if no newer turn started meanwhile.
            let current = Self::latest_turn(&rollout).await;
            let session = {
                let mut inner = service.inner.lock().unwrap();
                if !inner.predictions.get(&id).is_some_and(|p| p.turn == turn) {
                    return;
                }
                if current != prediction::Turn::Completed(turn) {
                    inner.predictions.remove(&id);
                    return;
                }
                if let Some(entry) = inner.predictions.get_mut(&id) {
                    entry.text = Some(text);
                }
                Self::project_locked(&inner, &id)
            };
            if let Some(session) = session {
                let _ = service.events.send(Event::State(session));
            }
        });
    }

    async fn latest_turn(rollout: &std::path::Path) -> prediction::Turn {
        let rollout = rollout.to_owned();
        tokio::task::spawn_blocking(move || prediction::latest_turn(&rollout))
            .await
            .unwrap_or(prediction::Turn::Other)
    }

    fn drop_prediction(&self, id: &str) {
        let mut inner = self.inner.lock().unwrap();
        if inner
            .predictions
            .remove(id)
            .is_some_and(|p| p.text.is_some())
            && let Some(session) = Self::project_locked(&inner, id)
        {
            let _ = self.events.send(Event::State(session));
        }
    }

    /// Summarize verified activity without changing any session's own turn or
    /// controller state. Walk upward so descendants outside the 200-child wire
    /// summary still count, with provider boundaries and cycle protection.
    fn descendant_activity(inner: &Inner) -> HashMap<String, String> {
        let mut activity = HashMap::new();
        for child in inner
            .catalog
            .values()
            .filter(|d| matches!(d.relation, "subagent" | "subsession") && !d.archived)
        {
            let Some(summary) = Self::project_summary(inner, &child.id) else {
                continue;
            };
            if !matches!(summary.state.as_str(), "running" | "waiting_approval") {
                continue;
            }
            let mut visited = HashSet::from([child.id.as_str()]);
            let mut current = child;
            while matches!(current.relation, "subagent" | "subsession") {
                let Some(parent) = current
                    .parent_id
                    .as_deref()
                    .and_then(|id| inner.catalog.get(id))
                    .filter(|p| p.provider == child.provider && visited.insert(p.id.as_str()))
                else {
                    break;
                };
                let value = activity
                    .entry(parent.id.clone())
                    .or_insert_with(|| summary.state.clone());
                if summary.state == "waiting_approval" {
                    *value = summary.state.clone();
                }
                current = parent;
            }
        }
        activity
    }

    fn project_locked(inner: &Inner, id: &str) -> Option<AgentSession> {
        let mut session = Self::project_summary(inner, id)?;
        let activity = Self::descendant_activity(inner);
        session.descendant_activity = activity.get(id).cloned().unwrap_or_default();
        let mut children: Vec<_> = inner
            .catalog
            .values()
            .filter(|d| {
                d.hidden
                    && d.parent_id.as_deref() == Some(id)
                    && d.provider.as_str() == session.provider
                    && d.id != id
            })
            .collect();
        children.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.id.cmp(&b.id)));
        session.subagents = children
            .into_iter()
            .take(200)
            .filter_map(|d| Self::project_summary(inner, &d.id))
            .collect();
        let mut subsessions: Vec<_> = inner
            .catalog
            .values()
            .filter(|d| {
                matches!(d.relation, "subsession" | "fork")
                    && !d.archived
                    && d.parent_id.as_deref() == Some(id)
                    && d.provider.as_str() == session.provider
                    && d.id != id
            })
            .collect();
        subsessions.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.id.cmp(&b.id)));
        session.subsessions = subsessions
            .into_iter()
            .take(200)
            .filter_map(|d| Self::project_summary(inner, &d.id))
            .collect();
        for child in session
            .subagents
            .iter_mut()
            .chain(session.subsessions.iter_mut())
        {
            child.descendant_activity = activity.get(&child.id).cloned().unwrap_or_default();
        }
        Some(session)
    }

    fn project_summary(inner: &Inner, id: &str) -> Option<AgentSession> {
        if desktop_chat::app_of(id).is_some() {
            return Self::project_chat(inner, id);
        }
        if cloud::is_cloud(id) {
            return Self::project_cloud(inner, id);
        }
        let d = inner.catalog.get(id)?;
        let open = inner.open.get(id);
        let controller = inner.controllers.get(id).filter(|_| !d.hidden);
        let runtime = inner
            .runtime
            .get(id)
            .filter(|r| r.observed.elapsed() < Duration::from_secs(10));
        let process_state = if let Some(runtime) = runtime {
            runtime.process_state
        } else {
            controller
                .filter(|_| d.provider != Provider::Claude)
                .map(|c| c.process_state())
                .unwrap_or_else(|| {
                    if inner.exited.contains(id) {
                        ProcessState::Ended
                    } else {
                        ProcessState::Unknown
                    }
                })
        };
        let state = match process_state {
            ProcessState::Running => {
                let ax = controller
                    .map(|c| c.state())
                    .unwrap_or(SessionState::Unknown);
                match runtime.map(|r| r.activity) {
                    Some(SessionState::Running) if ax == SessionState::WaitingApproval => ax,
                    Some(state) if state != SessionState::Unknown => state,
                    _ => ax,
                }
            }
            ProcessState::Ended => SessionState::Ended,
            ProcessState::Unknown => SessionState::Unknown,
        };
        let mut meta = open
            .map(|o| o.viewer.meta().clone())
            .unwrap_or_else(|| d.meta.clone());
        meta.title = Self::title_locked(inner, d).to_owned();
        if meta.cwd.as_os_str().is_empty() {
            // A project the app reported after the viewer opened.
            meta.cwd = d.meta.cwd.clone();
        }
        Some(AgentSession {
            id: id.to_string(),
            provider: d.provider.as_str().into(),
            title: meta.title,
            cwd: meta.cwd.to_string_lossy().into_owned(),
            state: state.as_str().into(),
            process_state: process_state.as_str().into(),
            updated_unix: d.meta.updated_unix,
            message_count: open.map(|o| o.messages.len() as u32).unwrap_or(0),
            controller: controller
                .map(|c| c.kind())
                .unwrap_or(ControllerKind::None)
                .as_str()
                .into(),
            archived: d.archived,
            device_id: String::new(),
            device_name: String::new(),
            mode: "code".into(),
            read_only: d.hidden,
            relationship: d.relation.into(),
            transcript_notice: open
                .map(|o| o.viewer.transcript_notice().to_owned())
                .unwrap_or_default(),
            // A suggestion follows a finished turn, never a running one.
            composer_suggestion: inner
                .predictions
                .get(id)
                .and_then(|p| p.text.clone())
                .filter(|_| !matches!(state, SessionState::Running | SessionState::WaitingApproval))
                .unwrap_or_default(),
            parent_session_id: d
                .parent_id
                .as_ref()
                .filter(|parent| {
                    inner
                        .catalog
                        .get(*parent)
                        .is_some_and(|p| p.provider == d.provider)
                })
                .cloned()
                .unwrap_or_default(),
            ..Default::default()
        })
    }

    /// Any well-formed chat session id projects, known or not: a watcher that
    /// reconnects after a daemon restart sees it as unknown until the desktop
    /// shows it again.
    fn project_chat(inner: &Inner, id: &str) -> Option<AgentSession> {
        let app = desktop_chat::app_of(id)?;
        let entry = inner.chats.get(id);
        let controller = inner.controllers.get(id);
        let process_state = controller.map(|c| c.process_state()).unwrap_or_else(|| {
            if inner.exited.contains(id) {
                ProcessState::Ended
            } else {
                ProcessState::Unknown
            }
        });
        let state = match process_state {
            ProcessState::Running => controller
                .map(|c| c.state())
                .unwrap_or(SessionState::Unknown),
            ProcessState::Ended => SessionState::Ended,
            ProcessState::Unknown => SessionState::Unknown,
        };
        let title = match entry {
            Some(e) if !e.title.is_empty() => e.title.clone(),
            _ if desktop_chat::is_new_session(id) => format!("New {} chat", app.name()),
            _ => format!("{} chat", app.name()),
        };
        Some(AgentSession {
            id: id.to_string(),
            provider: app.provider().as_str().into(),
            title,
            cwd: String::new(),
            state: state.as_str().into(),
            process_state: process_state.as_str().into(),
            updated_unix: entry.map(|e| e.updated_unix).unwrap_or(0),
            message_count: entry.map(|e| e.transcript.lines.len() as u32).unwrap_or(0),
            controller: controller
                .map(|c| c.kind())
                .unwrap_or(ControllerKind::None)
                .as_str()
                .into(),
            archived: false,
            device_id: String::new(),
            device_name: String::new(),
            mode: "chat".into(),
            ..Default::default()
        })
    }

    /// Reads only the desktop app's catalog database, never credentials or transcripts.
    pub fn local_chatgpt_catalog(&self) -> Result<muster_proto::v1::AgentChatCatalog, Error> {
        let home = self
            .config
            .codex_home
            .as_ref()
            .ok_or_else(|| Error::Unavailable("ChatGPT catalog home is unavailable".into()))?;
        self.chatgpt_catalog.lock().unwrap().read_home(home)
    }

    /// The session id of a conversation a desktop app lists or shows. A
    /// title-only conversation keeps the id it was first recorded under
    /// while its renames are followed.
    ///
    /// A sidebar row known only by its title resolves to the conversation
    /// already seen on screen under that title (the Claude app lists titles
    /// while its pages carry ids), when exactly one has it.
    pub fn chat_session_for(&self, app: ChatApp, key: &ChatKey) -> String {
        if let ChatKey::Title(title) = key {
            let inner = self.inner.lock().unwrap();
            let mut known: Vec<&String> = inner
                .chats
                .iter()
                .filter(|(_, e)| e.app == app && &e.key == key)
                .map(|(id, _)| id)
                .collect();
            known.sort();
            if let Some(id) = known.first() {
                return (*id).clone();
            }
            let titled: Vec<&String> = inner
                .chats
                .iter()
                .filter(|(_, e)| e.app == app && &e.title == title)
                .map(|(id, _)| id)
                .collect();
            if let [only] = titled.as_slice() {
                return (*only).clone();
            }
        }
        if app == ChatApp::ChatGpt {
            if let ChatKey::Title(title) = key {
                if let Some(id) = self.chatgpt_catalog.lock().unwrap().unique_title(title) {
                    return desktop_chat::session_id(app, &ChatKey::Id(id));
                }
            }
        }
        desktop_chat::session_id(app, key)
    }

    /// AX row matching accepts the source ID even when a newly created chat
    /// already has a temporary runtime identity. Never changes that identity
    /// during an active conversation.
    pub fn chat_row_matches(&self, app: ChatApp, key: &ChatKey, session: &str) -> bool {
        if app == ChatApp::ChatGpt
            && let ChatKey::Title(title) = key
        {
            let unique = self.chatgpt_catalog.lock().unwrap().unique_title(title);
            return unique.is_some_and(|id| {
                desktop_chat::session_id(app, &ChatKey::Id(id)) == session
                    || self.chat_session_for(app, key) == session
            });
        }
        self.chat_session_for(app, key) == session
    }

    /// How a known chat session is found again in its app: its key and title.
    pub fn chat_entry(&self, id: &str) -> Option<(ChatApp, ChatKey, String)> {
        let inner = self.inner.lock().unwrap();
        let e = inner.chats.get(id)?;
        Some((e.app, e.key.clone(), e.title.clone()))
    }

    /// Records what a desktop app shows now and pushes what changed.
    /// `bound` is the session this page resolved to last time: a title-only
    /// conversation renamed on screen (same opening message) keeps it.
    /// Returns the session the page resolved to; the empty new-chat page
    /// resolves to none.
    pub fn chat_observed(
        &self,
        app: ChatApp,
        bound: Option<&str>,
        key: ChatKey,
        title: &str,
        lines: Vec<ChatLine>,
    ) -> Option<String> {
        if key == ChatKey::New {
            return None;
        }
        let resolved = self.chat_session_for(app, &key);
        let now = model::now_unix();
        let mut events = Vec::new();
        let id = {
            let mut inner = self.inner.lock().unwrap();
            let mut id = inner
                .chats
                .iter()
                .filter(|(_, e)| e.app == app && e.key == key)
                .map(|(id, _)| id.clone())
                .min()
                .unwrap_or(resolved);
            if let (ChatKey::Title(_), Some(previous)) = (&key, bound)
                && previous != id
                && !inner.chats.contains_key(&id)
                && let Some(entry) = inner.chats.get_mut(previous)
                && entry.app == app
                && matches!(entry.key, ChatKey::Title(_))
                && !lines.is_empty()
                && entry.transcript.lines.first().map(|(l, _)| l) == lines.first()
            {
                entry.key = key.clone();
                id = previous.to_string();
            }
            let entry = inner.chats.entry(id.clone()).or_insert_with(|| ChatEntry {
                app,
                key: key.clone(),
                title: String::new(),
                transcript: ChatTranscript::default(),
                updated_unix: now,
            });
            let mut touched = false;
            if !title.is_empty() && entry.title != title {
                entry.title = title.to_string();
                touched = true;
            }
            let delta = entry.transcript.apply(lines, now);
            if delta.reset {
                events.push(Event::Reset {
                    session: id.clone(),
                });
            }
            for index in &delta.changed {
                let (line, seen) = &entry.transcript.lines[*index];
                events.push(Event::Message {
                    session: id.clone(),
                    message: AgentMessage {
                        id: entry.transcript.message_id(*index),
                        role: line.role.as_str().into(),
                        text: line.text.clone(),
                        ts_unix: *seen,
                        kind: model::MessageKind::Text.as_str().into(),
                        ..Default::default()
                    },
                });
            }
            if !delta.changed.is_empty() {
                entry.updated_unix = now;
                touched = true;
            }
            if touched && let Some(session) = Self::project_chat(&inner, &id) {
                events.push(Event::State(session));
            }
            id
        };
        for event in events {
            let _ = self.events.send(event);
        }
        Some(id)
    }

    fn chat_messages(
        &self,
        id: &str,
        tail: usize,
        before: Option<&str>,
    ) -> (Vec<AgentMessage>, bool) {
        let inner = self.inner.lock().unwrap();
        let Some(entry) = inner.chats.get(id) else {
            return (Vec::new(), false);
        };
        let t = &entry.transcript;
        let tail = if tail == 0 { 50 } else { tail.min(2000) };
        let end = before
            .and_then(|b| (0..t.lines.len()).find(|&i| t.message_id(i) == b))
            .unwrap_or(t.lines.len());
        let start = end.saturating_sub(tail);
        let messages = (start..end)
            .map(|i| {
                let (line, seen) = &t.lines[i];
                AgentMessage {
                    id: t.message_id(i),
                    role: line.role.as_str().into(),
                    text: line.text.clone(),
                    ts_unix: *seen,
                    kind: model::MessageKind::Text.as_str().into(),
                    ..Default::default()
                }
            })
            .collect();
        (messages, start > 0)
    }

    pub fn session(&self, id: &str) -> Option<AgentSession> {
        if orca::is_orca(id) {
            return self.orca.as_ref()?.session(id);
        }
        Self::project_locked(&self.inner.lock().unwrap(), id)
    }

    /// Subagent transcripts are observations, never independent control targets.
    fn require_writable(&self, id: &str) -> Result<(), Error> {
        if self
            .inner
            .lock()
            .unwrap()
            .catalog
            .get(id)
            .is_some_and(|d| d.hidden)
        {
            return Err(Error::Unavailable(
                "Subagent transcripts are read-only; use the parent session".into(),
            ));
        }
        Ok(())
    }

    /// Refuses before any unlock or Accessibility work when no desktop input
    /// channel exists. Claude records which client wrote a session.
    pub fn require_desktop_input(&self, id: &str) -> Result<(), Error> {
        if orca::is_orca(id) {
            return Err(Error::Unavailable(
                "Orca sessions use their independent runtime adapter".into(),
            ));
        }
        self.require_writable(id)?;
        if cloud::is_cloud(id) {
            return Err(Error::Unavailable(
                "a cloud session is not driven through a desktop app".into(),
            ));
        }
        let entry = {
            let inner = self.inner.lock().unwrap();
            inner
                .catalog
                .get(id)
                .map(|d| (d.provider, d.entrypoint.clone(), d.archived))
        };
        match entry {
            Some((Provider::Claude, _, true)) => Err(Error::Unavailable(format!(
                "Claude Code session {} is archived, so Claude.app no longer lists it; unarchive it there first. Nothing was sent.",
                &id[..8.min(id.len())]
            ))),
            Some((Provider::Claude, entrypoint, false)) => {
                claude::desktop_input(id, entrypoint.as_deref(), || {
                    claude::desktop_registry()
                        .is_some_and(|root| claude::desktop_registry_knows(&root, id))
                })
            }
            _ => Ok(()),
        }
    }

    /// Resolve a desktop title only when it identifies exactly one Codex
    /// session. Recency is not evidence of which same-named thread is visible.
    pub fn codex_session_by_title(&self, title: &str) -> Option<String> {
        self.native_session_by_title("codex", title)
    }

    pub fn native_session_by_title(&self, provider: &str, title: &str) -> Option<String> {
        if title.trim().is_empty() {
            return None;
        }
        let inner = self.inner.lock().unwrap();
        // Compare the titles sessions are shown with (an open viewer's is the
        // newest), the same one a caller got for its target.
        let mut matches = inner.catalog.values().filter(|d| {
            !d.hidden && d.provider.as_str() == provider && Self::title_locked(&inner, d) == title
        });
        let session = matches.next()?;
        matches.next().is_none().then(|| session.id.clone())
    }

    /// Navigation labels belong to the desktop catalog. Transcript titles can
    /// survive a rename (notably Claude side chats) and must not make a current
    /// unique desktop row ambiguous. This grants no write authority.
    pub fn native_navigation_title(&self, id: &str) -> Option<(String, bool)> {
        let target = self.session(id)?;
        if target.provider == "claude"
            && let Some(root) = &self.config.claude_desktop
            && let Some(title) = claude::desktop_navigation_title(root, id)
        {
            return Some(title);
        }
        let unique = self
            .native_session_by_title(&target.provider, &target.title)
            .as_deref()
            == Some(id);
        Some((target.title, unique))
    }

    /// Visible sessions, newest first.
    pub fn list(&self, provider: Option<Provider>, limit: usize) -> Vec<AgentSession> {
        let inner = self.inner.lock().unwrap();
        let mut activity = Self::descendant_activity(&inner);
        let mut sessions: Vec<AgentSession> = inner
            .catalog
            .iter()
            .filter(|(_, d)| !d.hidden && !d.archived)
            .filter(|(_, d)| provider.is_none_or(|p| d.provider == p))
            .filter_map(|(id, _)| {
                let mut session = Self::project_summary(&inner, id)?;
                session.descendant_activity = activity.remove(id).unwrap_or_default();
                Some(session)
            })
            .chain(
                inner
                    .cloud
                    .iter()
                    .filter(|(_, e)| !e.remote.archived)
                    .filter(|(_, e)| provider.is_none_or(|p| e.provider == p))
                    .filter_map(|(id, _)| Self::project_cloud(&inner, id)),
            )
            .collect();
        if provider.is_none() {
            if let Some(orca) = &self.orca {
                sessions.extend(orca.list());
            }
        }
        sessions.sort_by(|a, b| {
            b.updated_unix
                .cmp(&a.updated_unix)
                .then_with(|| a.id.cmp(&b.id))
        });
        let limit = if limit == 0 { 200 } else { limit };
        sessions.truncate(limit);
        sessions
    }

    /// Opens (parses) a session if needed.
    async fn ensure_open(&self, id: &str) -> Result<(), Error> {
        let discovered = {
            let mut inner = self.inner.lock().unwrap();
            if let Some(open) = inner.open.get_mut(id) {
                open.last_used = Instant::now();
                drop(inner);
                self.poll_transcripts(vec![id.to_owned()]);
                return Ok(());
            }
            match inner.catalog.get(id) {
                Some(d) => d.clone(),
                None => return Err(Error::NotFound(format!("session {id}"))),
            }
        };
        let parsed = tokio::task::spawn_blocking(move || {
            // A JSONL source is read by the engine over its grammar and
            // hooks; a store by its own viewer.
            let mut viewer: Box<dyn Viewer> = match discovered.provider.jsonl() {
                Some((records, hooks)) => {
                    Box::new(jsonl::JsonlViewer::open(&discovered, records, hooks))
                }
                None => Box::new(catdesk::CatdeskViewer::open(&discovered)),
            };
            let stat = viewer.stamp().map_err(|e| Error::Io(e.to_string()))?;
            let polled = viewer.poll().map_err(|e| Error::Io(e.to_string()))?;
            Ok::<_, Error>(Open {
                viewer,
                messages: polled.messages,
                last_used: Instant::now(),
                stat,
            })
        })
        .await
        .map_err(|e| Error::Io(e.to_string()))??;
        let mut inner = self.inner.lock().unwrap();
        let open = inner.open.entry(id.to_string()).or_insert(parsed);
        // The whole file was read: its newest title outlives the viewer.
        let (title, custom) = (open.viewer.meta().title.clone(), open.viewer.custom_title());
        if let Some(d) = inner.catalog.get_mut(id) {
            Self::refine_catalog_title(d, title, custom);
        }
        let session = Self::project_locked(&inner, id);
        drop(inner);
        // Initial watch state precedes parsing. Publish parsed availability too.
        if let Some(session) = session {
            let _ = self.events.send(Event::State(session));
        }
        Ok(())
    }

    pub async fn history_page(
        &self,
        request: &muster_proto::v1::AgentMessagesRequest,
    ) -> Result<muster_proto::v1::AgentMessagePage, Error> {
        if orca::is_orca(&request.session) {
            return self.orca_source()?.history(request).await;
        }
        let all = if cloud::is_cloud(&request.session) {
            self.ensure_cloud_open(&request.session).await?;
            self.inner
                .lock()
                .unwrap()
                .cloud
                .get(&request.session)
                .map(|entry| {
                    entry
                        .messages
                        .iter()
                        .map(Message::to_proto)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else if desktop_chat::app_of(&request.session).is_some() {
            let inner = self.inner.lock().unwrap();
            inner
                .chats
                .get(&request.session)
                .map(|entry| {
                    entry
                        .transcript
                        .lines
                        .iter()
                        .enumerate()
                        .map(|(i, (line, seen))| AgentMessage {
                            id: entry.transcript.message_id(i),
                            role: line.role.as_str().into(),
                            text: line.text.clone(),
                            ts_unix: *seen,
                            kind: model::MessageKind::Text.as_str().into(),
                            ..Default::default()
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else {
            self.ensure_open(&request.session).await?;
            self.inner
                .lock()
                .unwrap()
                .open
                .get(&request.session)
                .map(|open| {
                    open.messages
                        .iter()
                        .map(Message::to_proto)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        Ok(muster_proto::history::page(&all, request))
    }

    /// The last `tail` messages before `before` (a message id), oldest first.
    pub async fn messages(
        &self,
        id: &str,
        tail: usize,
        before: Option<&str>,
    ) -> Result<(Vec<AgentMessage>, bool), Error> {
        if desktop_chat::app_of(id).is_some() {
            return Ok(self.chat_messages(id, tail, before));
        }
        if cloud::is_cloud(id) {
            self.ensure_cloud_open(id).await?;
        } else {
            self.ensure_open(id).await?;
        }
        let inner = self.inner.lock().unwrap();
        let messages = match inner.open.get(id) {
            Some(open) => &open.messages,
            None => match inner.cloud.get(id) {
                Some(entry) => &entry.messages,
                None => return Ok((Vec::new(), false)),
            },
        };
        let tail = if tail == 0 { 50 } else { tail.min(2000) };
        let end = match before {
            Some(b) => messages
                .iter()
                .position(|m| m.id == b)
                .unwrap_or(messages.len()),
            None => messages.len(),
        };
        let start = end.saturating_sub(tail);
        Ok((
            messages[start..end].iter().map(Message::to_proto).collect(),
            start > 0,
        ))
    }

    pub fn pending(&self, id: &str) -> Vec<PendingInteraction> {
        if orca::is_orca(id) {
            return self
                .orca
                .as_ref()
                .map(|o| o.pending(id))
                .unwrap_or_default();
        }
        let (controller, discovered) = {
            let inner = self.inner.lock().unwrap();
            (
                inner.controllers.get(id).cloned(),
                inner.catalog.get(id).cloned(),
            )
        };
        let mut pending = controller.map(|c| c.pending()).unwrap_or_default();
        if let Some(d) = discovered {
            if let Some(mut snapshot) = questions::read_snapshot(&d.path, d.provider, id) {
                if d.provider == Provider::Claude {
                    claude_cards::desktop_tasks(&mut snapshot, id);
                }
                pending.retain(|q| {
                    !snapshot.retired.contains(&q.id)
                        && !snapshot.resolved.contains(&q.id)
                        && (!snapshot.complete
                            || q.kind != "question"
                            || snapshot.find(&q.id).is_some())
                });
                for q in snapshot.iter().cloned() {
                    if !pending.iter().any(|i| i.id == q.id) {
                        pending.push(q);
                    }
                }
            }
        }
        if let Ok(messages) = self.queued_messages(id) {
            pending.extend(messages.iter().map(|m| m.interaction(id)));
        }
        pending
    }

    /// Attaches a controller musterd built from a platform channel.
    pub fn attach(&self, controller: Arc<dyn Controller>) {
        let id = controller.session_id();
        {
            let mut inner = self.inner.lock().unwrap();
            inner.exited.remove(&id);
            inner.controllers.insert(id.clone(), controller);
        }
        if let Some(s) = self.session(&id) {
            let _ = self.events.send(Event::State(s));
        }
    }

    /// Attaches a desktop Accessibility controller unless one is already
    /// attached to a live app process for this session. A background scan
    /// and an operation can both find the session unattached; the first one
    /// wins, so an in-flight write and its unknown-outcome replay guard are
    /// never replaced by a fresh controller. Returns whether it attached.
    pub fn attach_native(&self, controller: Arc<dyn Controller>) -> bool {
        let id = controller.session_id();
        {
            let mut inner = self.inner.lock().unwrap();
            if inner
                .controllers
                .get(&id)
                .is_some_and(|existing| existing.process_state() != ProcessState::Ended)
            {
                return false;
            }
            inner.exited.remove(&id);
            inner.controllers.insert(id.clone(), controller);
        }
        if let Some(s) = self.session(&id) {
            let _ = self.events.send(Event::State(s));
        }
        true
    }

    /// Whether a controller is attached to this session id.
    pub fn has_controller(&self, id: &str) -> bool {
        self.inner.lock().unwrap().controllers.contains_key(id)
    }

    /// Keep live or temporarily unobservable controllers, including their
    /// unknown-delivery guards. Only a confirmed process exit permits reattach.
    pub fn retains_native_controller(&self, id: &str) -> bool {
        self.inner
            .lock()
            .unwrap()
            .controllers
            .get(id)
            .is_some_and(|c| {
                c.kind() == ControllerKind::NativeApp && c.process_state() != ProcessState::Ended
            })
    }

    /// Clone before OS calls: never hold the catalog mutex across Accessibility IPC.
    pub fn native_surface_ready(&self, id: &str) -> Result<bool, Error> {
        let controller = self.inner.lock().unwrap().controllers.get(id).cloned();
        match controller {
            Some(c) if c.kind() == ControllerKind::NativeApp => c.native_surface_ready(),
            _ => Ok(false),
        }
    }

    pub fn controller_events(&self) -> mpsc::Sender<(String, ControllerEvent)> {
        self.controller_events.clone()
    }

    /// Controllers outlive a single Locked Use transaction and its runtime.
    pub fn controller_runtime(&self) -> tokio::runtime::Handle {
        self.runtime.clone()
    }

    /// Only an attached desktop Accessibility controller may mutate a session.
    /// Transcript discovery never starts or resumes a provider CLI process.
    pub async fn complete(&self, id: &str) -> Result<String, Error> {
        self.controller(id)?.complete().await
    }

    pub fn question_diagnostic(&self, id: &str) -> Result<String, Error> {
        self.controller(id)?.question_diagnostic()
    }

    /// Choices belong to the selected native session, not to a cached transcript.
    pub async fn model_options(
        &self,
        id: &str,
        set: Option<(&str, &str)>,
    ) -> Result<muster_proto::v1::AgentChatOptions, Error> {
        self.controller(id)?.model_options(set).await
    }

    fn controller(&self, id: &str) -> Result<Arc<dyn Controller>, Error> {
        let inner = self.inner.lock().unwrap();
        if inner.catalog.get(id).is_some_and(|d| d.hidden) {
            return Err(Error::Unavailable(
                "Subagent transcripts are read-only; use the parent session".into(),
            ));
        }
        if let Some(controller) = inner.controllers.get(id) {
            return Ok(controller.clone());
        }
        if cloud::is_cloud(id) {
            return Err(Error::Unavailable(
                "a cloud session takes prompts only; answer it in the provider's web app".into(),
            ));
        }
        if desktop_chat::app_of(id).is_some() {
            return Err(Error::Unavailable(
                "no Accessibility controller: open this conversation from the desktop chat list"
                    .into(),
            ));
        }
        if !inner.catalog.contains_key(id) {
            return Err(Error::NotFound(format!("session {id}")));
        }
        Err(Error::Unavailable(
            "no Accessibility controller: open this session in its desktop app and grant Accessibility access".into(),
        ))
    }

    pub async fn send(&self, id: &str, text: &str) -> Result<(), Error> {
        self.send_to(id, text).await.map(|_| ())
    }

    /// Sends and returns the session the text landed in: `id`, or the
    /// conversation a new desktop chat became.
    /// See [`Controller::release_unconfirmed`].
    pub fn release_unconfirmed(&self, id: &str, operation: &str) -> Result<(), Error> {
        self.controller(id)?.release_unconfirmed(operation)
    }

    /// No controller creation or AX work: recovery also works after host restart.
    pub fn confirm_delivered(
        &self,
        id: &str,
        operation: &str,
        persist: &crate::control_guard::Confirmation,
    ) -> Result<(), Error> {
        let inner = self.inner.lock().unwrap();
        if let Some(controller) = inner.controllers.get(id) {
            controller.confirm_delivered(operation, persist)
        } else {
            persist()
        }
    }

    pub async fn send_to(&self, id: &str, text: &str) -> Result<String, Error> {
        if text.trim().is_empty() {
            return Err(Error::Invalid("empty message".into()));
        }
        if cloud::is_cloud(id) {
            let (source, remote) = self.cloud_source(id)?;
            if !self.inner.lock().unwrap().cloud.contains_key(id) {
                return Err(Error::NotFound(format!("session {id}")));
            }
            source.send(&remote, text).await?;
            // The prompt shows up in the session's events: read them soon.
            if let Some(entry) = self.inner.lock().unwrap().cloud.get_mut(id) {
                entry.last_used = Some(Instant::now());
            }
            self.cloud_wake.notify_one();
            return Ok(id.to_string());
        }
        Ok(entered(self.controller(id)?)
            .send_to(text)
            .await?
            .unwrap_or_else(|| id.to_string()))
    }

    /// Whether anyone follows agent events now (an open watch).
    pub fn watched(&self) -> bool {
        self.events.receiver_count() > 0
    }

    pub async fn respond(
        &self,
        interaction: &str,
        option: &str,
        answer: &str,
    ) -> Result<(), Error> {
        let session = self
            .inner
            .lock()
            .unwrap()
            .interactions
            .get(interaction)
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("interaction {interaction}")))?;
        self.require_writable(&session)?;
        let controller = self
            .inner
            .lock()
            .unwrap()
            .controllers
            .get(&session)
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("interaction {interaction}")))?;
        entered(controller)
            .respond(interaction, option, answer)
            .await
    }

    /// Resolve the interaction and controller under one lock, preserving the
    /// session that a signed remote operation named.
    pub async fn respond_in_session(
        &self,
        session: &str,
        interaction: &str,
        option: &str,
        answer: &str,
    ) -> Result<(), Error> {
        self.require_writable(session)?;
        if interaction.starts_with("queued:") {
            if option != queue::SEND_NOW || !answer.is_empty() {
                return Err(Error::Invalid("Invalid queued message action".into()));
            }
            queue::selected(&self.queued_messages(session)?, interaction)?;
            return entered(self.controller(session)?)
                .send_queued(interaction)
                .await;
        }
        let controller = {
            let inner = self.inner.lock().unwrap();
            if inner.interactions.get(interaction).map(String::as_str) != Some(session) {
                return Err(Error::NotFound(
                    "interaction is no longer in the authorized session".into(),
                ));
            }
            inner
                .controllers
                .get(session)
                .cloned()
                .ok_or_else(|| Error::NotFound(session.into()))?
        };
        entered(controller)
            .respond(interaction, option, answer)
            .await
    }

    pub async fn answer_questions(
        &self,
        session: &str,
        interaction: &str,
        answers: &[muster_proto::v1::QuestionAnswer],
    ) -> Result<(), Error> {
        let controller = self.controller(session)?;
        entered(controller)
            .answer_questions(interaction, answers)
            .await
    }

    pub async fn interrupt(&self, id: &str) -> Result<(), Error> {
        self.require_writable(id)?;
        if cloud::is_cloud(id) {
            let (source, _) = self.cloud_source(id)?;
            return Err(Error::Unavailable(format!(
                "{} does not offer interrupting a turn",
                source.cloud().label
            )));
        }
        let controller = self
            .inner
            .lock()
            .unwrap()
            .controllers
            .get(id)
            .cloned()
            .ok_or_else(|| Error::Conflict("no controller for this session".into()))?;
        entered(controller).interrupt().await
    }

    fn cwd_of(&self, id: &str) -> Result<PathBuf, Error> {
        if desktop_chat::app_of(id).is_some() {
            return Err(Error::Unavailable(
                "desktop chat conversations have no working directory".into(),
            ));
        }
        if cloud::is_cloud(id) {
            return Err(Error::Unavailable(
                "a cloud session's files are not browsable from here".into(),
            ));
        }
        let inner = self.inner.lock().unwrap();
        let cwd = inner
            .open
            .get(id)
            .map(|o| o.viewer.meta().cwd.clone())
            .filter(|cwd| !cwd.as_os_str().is_empty())
            .or_else(|| inner.catalog.get(id).map(|d| d.meta.cwd.clone()))
            .ok_or_else(|| Error::NotFound(format!("session {id}")))?;
        if cwd.as_os_str().is_empty() {
            return Err(Error::Unavailable(
                "the session has no working directory".into(),
            ));
        }
        Ok(cwd)
    }

    pub async fn files(&self, id: &str, path: &str) -> Result<muster_proto::v1::FileList, Error> {
        files::list(&self.cwd_of(id)?, path).await
    }

    pub async fn file(
        &self,
        id: &str,
        path: &str,
        max_bytes: u64,
    ) -> Result<muster_proto::v1::FileContent, Error> {
        if path.starts_with("muster-delivered:") {
            let record = self
                .inner
                .lock()
                .unwrap()
                .catalog
                .get(id)
                .cloned()
                .filter(|d| d.provider == Provider::Claude)
                .ok_or_else(|| Error::NotFound("Claude session not found".into()))?;
            let delivered = claude_cards::delivered_file(&record.path, path)?;
            let parent = delivered
                .parent()
                .ok_or_else(|| Error::Invalid("Invalid attachment path".into()))?;
            let name = delivered
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| Error::Invalid("Invalid attachment name".into()))?;
            return files::read(parent, name, max_bytes).await;
        }
        files::read(&self.cwd_of(id)?, path, max_bytes).await
    }

    pub async fn git(
        &self,
        id: &str,
        limit: u32,
        diff_commit: &str,
    ) -> Result<muster_proto::v1::GitLog, Error> {
        files::git(&self.cwd_of(id)?, limit, diff_commit).await
    }
}

pub mod control_guard;

/// Every mutating controller call passes here, so the ledger knows whether a
/// submission could have started (see control_guard::Delivery).
fn entered(controller: Arc<dyn Controller>) -> Arc<dyn Controller> {
    control_guard::delivery_enter(controller.reports_delivery());
    controller
}

pub mod native_runtime;
