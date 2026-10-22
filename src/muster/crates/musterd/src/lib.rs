//! musterd (ARCHITECTURE §3.6): the device daemon. It owns the identity, the
//! roster copy, the link (muster-net Node), the local Unix socket that the
//! desktop app and muster-cli talk to, and the "muster" service that members
//! reach over the link. Every request goes through one handler with one
//! authorization step; the platform layer never decides.

pub mod agent_bridge;
pub mod authorized_keys;
pub mod browser;
pub mod build_info;
pub mod cloud_http;
#[cfg(target_os = "macos")]
pub mod ghostty;
pub mod handler;
pub mod locked_use;
pub mod network;
pub mod web;
mod presence;

mod diagnostics;
mod inspection;
#[cfg(target_os = "macos")]
mod plugin_install;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::Instant;

use muster_agent::{AgentConfig, AgentService};
use muster_core::roster::{DeviceKind, Entry};
use muster_core::{DeviceId, Invite, SharedSigner};
use muster_net::magicsock::MagicOptions;
use muster_net::node::{Node, NodeConfig, RelayConfig, ServiceHandler};
use muster_net::pair::{Candidate, Verdict};
use muster_net::stream::Hello;
use muster_net::vnet::{Vnet, VnetConfig};
use muster_net::{Build, SharedPolicy, SharedRoster};
use muster_rpc::Principal;
use muster_store::Store;
use muster_terminal::{Terminal, TerminalKind};
use quinn::{RecvStream, SendStream};
use tokio::sync::{broadcast, oneshot};
use tokio::task::JoinHandle;

pub use handler::MusterHandler;
pub use muster_platform::{DefaultPlatform, Platform};

pub const SERVICE: &str = "muster";
pub const SERVICE_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no network: create one or join by invite")]
    NoNetwork,
    #[error("invalid: {0}")]
    Invalid(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("unavailable: {0}")]
    Unavailable(String),
    #[error("outcome unknown: {0}")]
    UnknownOutcome(String),
    #[error("timeout: {0}")]
    Timeout(String),
    #[error("conflict: {0}")]
    Conflict(String),
    /// Not sent: an earlier write to the session has no provider receipt;
    /// `operation` names it for a release.
    #[error("conflict: {message}")]
    Unconfirmed { operation: String, message: String },
    /// A desktop app is held by another operation; nothing was done.
    #[error("busy: {0}")]
    Busy(String),
    #[error(transparent)]
    Net(#[from] muster_net::Error),
    #[error(transparent)]
    Core(#[from] muster_core::Error),
    #[error(transparent)]
    Store(#[from] muster_store::Error),
}

/// The source commit this daemon was built from, findable with `strings`
/// in an installed binary (see deploy/desktop/verify-release.sh).
pub const BUILD_STAMP: &str = concat!("MUSTER-BUILD-COMMIT:", env!("MUSTER_BUILD_COMMIT"));

#[derive(Clone)]
pub struct DaemonConfig {
    /// State directory: the store and the socket live here.
    pub dir: PathBuf,
    pub socket: Option<PathBuf>,
    pub bind: SocketAddr,
    /// "host:port" (UDP) or "ws(s)://…"; remembered in the store once given.
    pub relay: Option<String>,
    /// Empty keeps the stored name (or the hostname on first start).
    pub name: String,
    pub kind: DeviceKind,
    pub allow_loopback: bool,
    pub agent: Option<AgentConfig>,
    /// Whether the agent service also lists the providers' cloud sessions
    /// (`muster_agent::cloud`) with the credentials their CLIs keep here.
    pub cloud: bool,
    pub terminal: Option<TerminalKind>,
    pub vnet: bool,
    /// Serve the Unix socket (off when the app core embeds the daemon).
    pub listen: bool,
    /// This device's presence key (raw P-256 point), recorded in its roster
    /// entry so targets can verify presence proofs.
    pub presence_key: Option<muster_core::PublicKey>,
    /// Keep this file in step with the roster: one OpenSSH public key line
    /// per member that may reach tcp/22 here (sshd `AuthorizedKeysFile`).
    pub authorized_keys: Option<PathBuf>,
    /// How the identity is obtained; `None` uses muster-platform's choice for
    /// this machine (Secure Enclave on macOS, a stored software key elsewhere).
    pub signer: Option<SignerFactory>,
    /// The release version this device reports (settings, heartbeat). Empty
    /// = the installed package's, else the crate's; an app embedding the
    /// daemon passes its own.
    pub version: String,
}

pub type SignerFactory = Arc<dyn Fn(&Store) -> anyhow::Result<SharedSigner> + Send + Sync>;

impl std::fmt::Debug for DaemonConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DaemonConfig")
            .field("dir", &self.dir)
            .field("bind", &self.bind)
            .field("relay", &self.relay)
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("vnet", &self.vnet)
            .field("listen", &self.listen)
            .field("cloud", &self.cloud)
            .field("version", &self.version)
            .finish()
    }
}

impl DaemonConfig {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            socket: None,
            bind: "0.0.0.0:0".parse().unwrap(),
            relay: None,
            name: String::new(),
            kind: DeviceKind::Desktop,
            allow_loopback: false,
            agent: Some(AgentConfig::default()),
            cloud: true,
            terminal: None,
            vnet: false,
            listen: true,
            presence_key: None,
            signer: None,
            authorized_keys: None,
            version: String::new(),
        }
    }

    pub fn socket_path(&self) -> PathBuf {
        self.socket
            .clone()
            .unwrap_or_else(|| self.dir.join("musterd.sock"))
    }
}

/// A joiner waiting for this admin's answer.
pub struct PendingJoin {
    pub candidate: Candidate,
    pub since: Instant,
    reply: Option<oneshot::Sender<Verdict>>,
}

#[cfg(all(test, target_os = "macos"))]
type PendingVnetOpen = JoinHandle<
    Result<
        (
            String,
            muster_platform::macos::nethelper::NetHelper,
            std::os::fd::OwnedFd,
        ),
        String,
    >,
>;

struct VnetStartup {
    task: JoinHandle<()>,
    done: tokio::sync::watch::Receiver<bool>,
}

pub struct Daemon {
    /// Display-only root-helper installation observation: 0 unavailable, 1 checking, 2 verified, 3 pending.
    plugin_setup: std::sync::atomic::AtomicU8,
    pub(crate) locked_use: Arc<locked_use::Owner>,
    config: DaemonConfig,
    store: Arc<Store>,
    signer: SharedSigner,
    roster: SharedRoster,
    policy: SharedPolicy,
    node: Arc<Node>,
    network_config: tokio::sync::Mutex<()>,
    vnet_start: tokio::sync::Mutex<()>,
    vnet_startup: Mutex<Option<VnetStartup>>,
    #[cfg(all(test, target_os = "macos"))]
    injected_vnet_open: Mutex<Option<PendingVnetOpen>>,
    closing: std::sync::atomic::AtomicBool,
    relay_text: RwLock<Option<String>>,
    name: RwLock<String>,
    kind: RwLock<DeviceKind>,
    pending: Mutex<HashMap<DeviceId, PendingJoin>>,
    invite: Mutex<Option<Invite>>,
    agent: Option<Arc<AgentService>>,
    terminal: Option<Arc<Terminal>>,
    #[cfg(unix)]
    tmux: Arc<Terminal>,
    platform: Arc<dyn Platform>,
    vnet: Mutex<Option<Arc<Vnet>>>,
    /// The web window the policy asks for, if any (ADR 0026). Dropping it
    /// closes the port, so moving the window is: take it, then open the new
    /// one.
    web: tokio::sync::Mutex<Option<muster_web::Window>>,
    #[cfg(target_os = "macos")]
    vnet_helper: Mutex<Option<muster_platform::macos::nethelper::NetHelper>>,
    /// Fires whenever roster, peers-of-interest or pending joins change.
    changed: broadcast::Sender<()>,
    started: Instant,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    /// The privacy cover (the app draws it; the daemon is the record).
    covered: std::sync::atomic::AtomicBool,
    /// One lock per desktop app, shared by every daemon of this user, so
    /// agent and chat operations never drive one app at the same time.
    /// `None` on a platform that drives no desktop apps.
    pub(crate) desktop_locks: Option<muster_agent::desktop_lock::DesktopLocks>,
    /// FIFO per provider on this host; different providers are independent.
    pub(crate) agent_operations: crate::agent_bridge::ProviderQueues,
}

impl Daemon {
    pub async fn start(config: DaemonConfig) -> anyhow::Result<Arc<Self>> {
        Self::start_with(config, muster_platform::platform(), None).await
    }

    /// `signer` overrides the stored software identity (hardware keys on
    /// the desktops come in through here).
    pub async fn start_with(
        config: DaemonConfig,
        platform: Arc<dyn Platform>,
        signer: Option<SharedSigner>,
    ) -> anyhow::Result<Arc<Self>> {
        let mut config = config;
        if config.version.trim().is_empty() {
            config.version = build_info::installed_version();
        }
        std::fs::create_dir_all(&config.dir)?;
        let store = Arc::new(Store::open(&config.dir.join("musterd.db"))?);
        let signer = match (signer, &config.signer) {
            (Some(signer), _) => signer,
            (None, Some(factory)) => factory(&store)?,
            (None, None) => muster_platform::signer::device_signer(&store)?,
        };
        let name = if config.name.is_empty() {
            match store.meta_string("device.name")? {
                Some(name) => name,
                None => hostname(),
            }
        } else {
            store.set_meta("device.name", config.name.as_bytes())?;
            config.name.clone()
        };
        let kind = match store.meta_string("device.kind")? {
            Some(kind) if config.name.is_empty() => parse_kind(&kind).unwrap_or(config.kind),
            _ => {
                store.set_meta("device.kind", kind_str(config.kind).as_bytes())?;
                config.kind
            }
        };
        let relay_text = match &config.relay {
            Some(relay) => {
                store.set_meta("relay", relay.as_bytes())?;
                Some(relay.clone())
            }
            None => store.meta_string("relay")?,
        };
        let relay = parse_relay(relay_text.as_deref()).await;
        let roster: SharedRoster = Arc::new(RwLock::new(store.load_roster()?));
        let policy: SharedPolicy = Arc::new(RwLock::new(store.policy()?));
        let node = Node::start(
            node_config(&config, relay, stored_push_token(&store)),
            signer.clone(),
            roster.clone(),
            policy.clone(),
        )
        .await?;
        let (changed, _) = broadcast::channel(64);
        let agent = config.agent.clone().map(|mut agent| {
            if config.cloud && agent.cloud_http.is_none() {
                match cloud_http::Client::connect() {
                    Ok(http) => agent.cloud_http = Some(http),
                    Err(error) => log::warn!("cloud sessions off: no HTTPS client: {error}"),
                }
            }
            if agent.orca_rpc.is_none() {
                match muster_agent::orca::runtime::NativeRuntime::open(&config.dir.join("orca")) {
                    Ok(runtime) => agent.orca_rpc = Some(runtime),
                    Err(_) => log::warn!("Rust CLI session store unavailable"),
                }
            }

            AgentService::start(agent)
        });
        let terminal = config.terminal.clone().map(Terminal::new);
        #[cfg(unix)]
        let tmux = Arc::new(Terminal::Tmux(muster_terminal::tmux::TmuxService::new(
            config.dir.join("tmux.sock"),
        )));
        let locked_use = Arc::new(locked_use::Owner::new(platform.locked_use(&config.dir)));
        let desktop_locks = platform.accessibility().is_some().then(|| {
            muster_agent::desktop_lock::DesktopLocks::new(
                muster_paths::desktop_lock_dir()
                    .unwrap_or_else(|| config.dir.join("desktop-locks")),
            )
        });
        let daemon = Arc::new(Self {
            locked_use,
            config,
            store,
            signer,
            roster,
            policy,
            node,
            network_config: tokio::sync::Mutex::new(()),
            vnet_start: tokio::sync::Mutex::new(()),
            vnet_startup: Mutex::new(None),
            #[cfg(all(test, target_os = "macos"))]
            injected_vnet_open: Mutex::new(None),
            closing: std::sync::atomic::AtomicBool::new(false),
            relay_text: RwLock::new(relay_text),
            name: RwLock::new(name),
            kind: RwLock::new(kind),
            pending: Mutex::new(HashMap::new()),
            invite: Mutex::new(None),
            agent,
            terminal,
            #[cfg(unix)]
            tmux,
            platform,
            vnet: Mutex::new(None),
            web: tokio::sync::Mutex::new(None),
            #[cfg(target_os = "macos")]
            vnet_helper: Mutex::new(None),
            changed,
            started: Instant::now(),
            tasks: Mutex::new(Vec::new()),
            covered: std::sync::atomic::AtomicBool::new(false),
            plugin_setup: std::sync::atomic::AtomicU8::new(0),
            desktop_locks,
            agent_operations: crate::agent_bridge::ProviderQueues::default(),
        });
        let node = daemon.node();
        daemon.wire_node(&node);
        if daemon.config.listen {
            daemon.listen().await?;
            daemon
                .locked_use
                .listen(daemon.config.dir.join("locked-use.sock"))
                .await?;
        }
        #[cfg(target_os = "macos")]
        if daemon.config.listen {
            if let (Ok(executable), Some(home)) =
                (std::env::current_exe(), std::env::var_os("HOME"))
            {
                if plugin_install::eligible(
                    &executable,
                    &daemon.config.dir,
                    std::path::Path::new(&home),
                    std::env::var_os("MUSTER_DIR").is_some(),
                ) {
                    daemon
                        .plugin_setup
                        .store(1, std::sync::atomic::Ordering::Relaxed);
                    let task = tokio::spawn(plugin_install::run(Arc::downgrade(&daemon)));
                    daemon.tasks.lock().unwrap().push(task);
                }
            }
        }
        if daemon.config.vnet {
            daemon.maybe_start_vnet().await;
        }
        // A window the owner opened earlier is open again after a restart; a
        // port it can no longer bind is logged, not fatal.
        if let Err(error) = daemon.sync_web_window().await {
            log::warn!("web window: {error}");
        }
        if let Some(path) = daemon.config.authorized_keys.clone() {
            let task = tokio::spawn(authorized_keys::run(Arc::downgrade(&daemon), path));
            daemon.tasks.lock().unwrap().push(task);
        }
        // Only a platform that drives desktop apps discovers their sessions.
        #[cfg(target_os = "macos")]
        if let Some(agent) = &daemon.agent
            && daemon.platform.accessibility().is_some()
        {
            let bridge = tokio::spawn(agent_bridge::native_apps(
                agent.clone(),
                daemon.agent_operations.clone(),
            ));
            daemon.tasks.lock().unwrap().push(bridge);
        }
        log::info!(
            "musterd {} ({}) as {} [{}], socket {}; {BUILD_STAMP}",
            daemon.device_id().short(),
            daemon.signer.backend(),
            daemon.name(),
            kind_str(daemon.kind()),
            daemon.config.socket_path().display()
        );
        Ok(daemon)
    }

    fn wire_node(self: &Arc<Self>, node: &Arc<Node>) {
        node.set_handler(Arc::new(RemoteService {
            daemon: Arc::downgrade(self),
        }));
        let weak = Arc::downgrade(self);
        node.pairing()
            .set_approver(Arc::new(move |candidate: Candidate| {
                let (tx, rx) = oneshot::channel();
                if let Some(daemon) = weak.upgrade() {
                    let id = candidate.record.device_id;
                    daemon.pending.lock().unwrap().insert(
                        id,
                        PendingJoin {
                            candidate,
                            since: Instant::now(),
                            reply: Some(tx),
                        },
                    );
                    let _ = daemon.changed.send(());
                    log::info!("join request from {} awaits approval", id.short());
                }
                rx
            }));
        let weak = Arc::downgrade(self);
        let mut entries = node.subscribe_entries();
        let persist = tokio::spawn(async move {
            loop {
                match entries.recv().await {
                    Ok(entry) => {
                        let Some(daemon) = weak.upgrade() else { return };
                        daemon.persist(&[entry]);
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // Missed some: write the whole roster (idempotent).
                        let Some(daemon) = weak.upgrade() else { return };
                        let all: Vec<Entry> =
                            daemon.roster.read().unwrap().entries().cloned().collect();
                        daemon.persist(&all);
                    }
                    Err(_) => return,
                }
            }
        });
        self.tasks.lock().unwrap().push(persist);
    }

    pub(crate) fn persist(&self, entries: &[Entry]) {
        if let Err(error) = self.store.append_entries(entries) {
            log::error!("persist roster: {error}");
        }
        let _ = self.changed.send(());
        self.stop_stale_vnet();
    }

    async fn listen(self: &Arc<Self>) -> anyhow::Result<()> {
        let path = self.config.socket_path();
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        let listener = tokio::net::UnixListener::bind(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        let handler: Arc<dyn muster_rpc::Handler> =
            Arc::new(MusterHandler::new(Arc::downgrade(self)));
        let accept = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        tokio::spawn(muster_rpc::serve(stream, Principal::Local, handler.clone()));
                    }
                    Err(error) => {
                        log::warn!("socket accept: {error}");
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                }
            }
        });
        self.tasks.lock().unwrap().push(accept);
        Ok(())
    }

    /// Replaces the transport on `relay` (a joiner learns it from the invite).
    /// Node, Vnet, service subscriptions, roster, policy and identity stay.
    pub async fn restart_node(self: &Arc<Self>, relay_text: &str) -> Result<(), Error> {
        let relay = parse_relay(Some(relay_text)).await;
        let _serial = self.network_config.lock().await;
        // Stable Node owns Vnet's one datagram receiver and the service handler.
        // Only its transport generation changes; no TUN/helper or tasks are duplicated.
        self.node().reconfigure_relay(relay).await?;
        *self.relay_text.write().unwrap() = Some(relay_text.to_string());
        self.store.set_meta("relay", relay_text.as_bytes())?;
        log::info!("relay configuration applied; existing Vnet/TUN binding retained");
        Ok(())
    }

    /// Remembers the platform's push token (kind 1 APNs, 2 FCM, 3 HarmonyOS)
    /// and hands it to the relay, so a member's traffic can wake this device
    /// while the app is in the background. An empty token clears it.
    pub fn set_push_token(&self, kind: u8, token: &[u8]) -> Result<(), Error> {
        if token.len() > 512 {
            return Err(Error::Invalid("push token too long".into()));
        }
        let mut stored = Vec::with_capacity(token.len() + 1);
        stored.push(kind);
        stored.extend_from_slice(token);
        self.store
            .set_meta(PUSH_TOKEN_KEY, if token.is_empty() { &[] } else { &stored })?;
        self.node().set_push_token(if token.is_empty() {
            None
        } else {
            Some((kind, token.to_vec()))
        });
        Ok(())
    }

    /// The platform reports a network change (interfaces, addresses,
    /// coming back from the background): re-register with the relay and
    /// re-probe every path.
    pub fn network_changed(&self) {
        self.node().network_changed();
    }

    /// The user asked to refresh: offline members get one more round of
    /// bounded attempts (muster_net::presence).
    pub fn refresh_presence(&self) {
        self.node().refresh_presence();
    }

    /// A phone's app went to the background or came back: failed dials
    /// while suspended say nothing about anyone.
    pub fn suspend(&self) {
        self.node().suspend();
    }

    pub fn resume(&self) {
        self.node().resume();
    }

    /// Brings up the TUN when this device holds a vip (needs the OS side:
    /// root on Linux, the NetHelper's fd on macOS).
    pub async fn maybe_start_vnet(self: &Arc<Self>) {
        let mut done = {
            let mut startup = self.vnet_startup.lock().unwrap();
            if self.closing.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            if startup.as_ref().is_none_or(|job| job.task.is_finished()) {
                let (completed, done) = tokio::sync::watch::channel(false);
                let daemon = self.clone();
                let task = tokio::spawn(async move {
                    daemon.start_vnet().await;
                    let _ = completed.send(true);
                });
                *startup = Some(VnetStartup { task, done });
            }
            startup.as_ref().unwrap().done.clone()
        };
        if tokio::time::timeout(
            std::time::Duration::from_secs(3),
            done.wait_for(|done| *done),
        )
        .await
        .is_err()
        {
            log::warn!(
                "vnet helper open pending after 3s; single background task will complete startup"
            );
        }
    }

    async fn start_vnet(&self) {
        let _start = self.vnet_start.lock().await;
        if self.closing.load(std::sync::atomic::Ordering::Relaxed)
            || self.vnet.lock().unwrap().is_some()
        {
            return;
        }
        let has_vip = self
            .roster
            .read()
            .unwrap()
            .state()
            .active(&self.device_id())
            .is_some_and(|m| m.record.vip.is_some());
        if !has_vip {
            return;
        }
        #[allow(unused_mut)]
        let mut config = VnetConfig::default();
        #[cfg(target_os = "macos")]
        let (helper, helper_address) = {
            use std::os::fd::{FromRawFd, IntoRawFd, OwnedFd};
            let address = self.device_state().vip;
            let mtu = config.mtu;
            // The worker owns this one open beyond the caller's observation
            // deadline. Detached blocking results own their fd even on shutdown.
            let open = || {
                tokio::task::spawn_blocking(move || {
                    let mut helper = muster_platform::macos::nethelper::NetHelper::connect()?;
                    let fd = helper.open(&address, i64::from(mtu))?;
                    // SAFETY: open() transfers the duplicated descriptor to us.
                    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
                    Ok::<_, String>((address, helper, fd))
                })
            };
            #[cfg(test)]
            let opened = self
                .injected_vnet_open
                .lock()
                .unwrap()
                .take()
                .unwrap_or_else(open);
            #[cfg(not(test))]
            let opened = open();
            let since = Instant::now();
            match opened.await {
                Ok(Ok((address, helper, fd))) => {
                    log::info!(
                        "vnet helper open completed after {:?} for {address}",
                        since.elapsed()
                    );
                    if self.closing.load(std::sync::atomic::Ordering::Relaxed) {
                        return;
                    }
                    config.existing_fd = Some(fd.into_raw_fd());
                    (helper, address)
                }
                result => {
                    log::warn!(
                        "vnet helper open failed after {:?}: {}",
                        since.elapsed(),
                        match result {
                            Ok(Err(error)) => error,
                            Err(error) => error.to_string(),
                            _ => unreachable!(),
                        }
                    );
                    return;
                }
            }
        };
        match Vnet::start(self.node(), config).await {
            Ok(vnet) => {
                let roster = self.roster.read().unwrap();
                let mut current = self.vnet.lock().unwrap();
                #[cfg(target_os = "macos")]
                let helper_matches = helper_address == vnet.address().to_string();
                #[cfg(not(target_os = "macos"))]
                let helper_matches = true;
                if self.closing.load(std::sync::atomic::Ordering::Relaxed)
                    || !helper_matches
                    || roster
                        .state()
                        .active(&self.device_id())
                        .and_then(|m| m.record.vip)
                        != Some(vnet.address())
                    || roster.state().vnet_cidr != Some(vnet.cidr())
                {
                    vnet.shutdown();
                    return;
                }
                #[cfg(target_os = "macos")]
                {
                    *self.vnet_helper.lock().unwrap() = Some(helper);
                }
                log::info!("vnet up at {}", vnet.address());
                *current = Some(vnet);
                let _ = self.changed.send(());
            }
            Err(error) => log::warn!("vnet: {error}"),
        }
    }

    fn stop_vnet(&self) {
        let mut current = self.vnet.lock().unwrap();
        if let Some(vnet) = current.take() {
            vnet.shutdown();
        }
        #[cfg(target_os = "macos")]
        {
            self.vnet_helper.lock().unwrap().take();
        }
    }

    /// Do not keep serving a stale address after signed renumbering/removal.
    /// The maintenance operation explicitly requires a daemon restart.
    fn stop_stale_vnet(&self) {
        let stale = {
            let roster = self.roster.read().unwrap();
            let state = roster.state();
            self.vnet.lock().unwrap().as_ref().is_some_and(|vnet| {
                state.active(&self.device_id()).and_then(|m| m.record.vip) != Some(vnet.address())
                    || state.vnet_cidr != Some(vnet.cidr())
            })
        };
        if stale {
            self.stop_vnet();
            log::warn!("vnet address plan changed; restart daemon to apply the new interface");
        }
    }

    /// What this device runs: its release version and source commit.
    pub fn build(&self) -> Build {
        Build {
            version: self.config.version.clone(),
            commit: build_info::COMMIT.into(),
        }
    }

    pub fn node(&self) -> Arc<Node> {
        self.node.clone()
    }
    pub fn device_id(&self) -> DeviceId {
        self.signer.public_key().device_id()
    }
    pub fn signer(&self) -> &SharedSigner {
        &self.signer
    }
    pub fn store(&self) -> &Store {
        &self.store
    }
    pub fn roster(&self) -> &SharedRoster {
        &self.roster
    }
    pub fn policy(&self) -> &SharedPolicy {
        &self.policy
    }
    pub fn name(&self) -> String {
        self.name.read().unwrap().clone()
    }
    pub fn kind(&self) -> DeviceKind {
        *self.kind.read().unwrap()
    }
    pub fn covered(&self) -> bool {
        self.covered.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn set_cover(&self, on: bool) {
        self.covered.store(on, std::sync::atomic::Ordering::Relaxed);
        log::info!("privacy cover {}", if on { "up" } else { "down" });
        let _ = self.changed.send(());
    }

    pub fn relay_text(&self) -> Option<String> {
        self.relay_text.read().unwrap().clone()
    }
    pub fn agent(&self) -> Option<&Arc<AgentService>> {
        self.agent.as_ref()
    }
    pub fn terminal(&self) -> Option<&Arc<Terminal>> {
        self.terminal.as_ref()
    }
    pub fn platform(&self) -> &Arc<dyn Platform> {
        &self.platform
    }
    pub fn vnet(&self) -> Option<Arc<Vnet>> {
        self.vnet.lock().unwrap().clone()
    }
    pub fn subscribe_changes(&self) -> broadcast::Receiver<()> {
        self.changed.subscribe()
    }
    pub fn uptime_secs(&self) -> u64 {
        self.started.elapsed().as_secs()
    }
    pub fn socket_path(&self) -> PathBuf {
        self.config.socket_path()
    }
    /// This device's roster record as it should be added.
    pub fn own_record(&self) -> muster_core::roster::DeviceRecord {
        let mut record = muster_core::roster::DeviceRecord::new(
            self.signer.public_key(),
            self.name(),
            self.kind(),
        );
        record.presence_key = self.presence_candidate().ok().flatten();
        record
    }
    pub(crate) fn set_identity_labels(&self, name: &str, kind: DeviceKind) {
        if !name.is_empty() {
            *self.name.write().unwrap() = name.to_string();
            let _ = self.store.set_meta("device.name", name.as_bytes());
        }
        *self.kind.write().unwrap() = kind;
        let _ = self
            .store
            .set_meta("device.kind", kind_str(kind).as_bytes());
    }

    pub async fn shutdown(&self) {
        self.closing
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let startup = self.vnet_startup.lock().unwrap().take();
        if let Some(startup) = startup {
            startup.task.abort();
            let _ = startup.task.await;
            log::info!("vnet startup observer stopped; late helper results cannot publish");
        }
        let _start = self.vnet_start.lock().await;
        let vnet = self.vnet.lock().unwrap().take();
        if let Some(vnet) = vnet {
            vnet.shutdown_and_wait().await;
        }
        #[cfg(target_os = "macos")]
        {
            self.vnet_helper.lock().unwrap().take();
            // Detached blocking results own/drop their fd and helper when XPC
            // completes; they can no longer publish into this daemon.
            #[cfg(test)]
            self.injected_vnet_open.lock().unwrap().take();
        }
        if let Some(agent) = &self.agent {
            if let Ok(orca) = agent.orca_source() {
                orca.shutdown();
            }
        }
        self.locked_use.shutdown();
        for task in self.tasks.lock().unwrap().drain(..) {
            task.abort();
        }
        self.node().shutdown().await;
        let _ = std::fs::remove_file(self.config.socket_path());
    }
}

/// Members reach the same handler over a "muster" service stream.
struct RemoteService {
    daemon: Weak<Daemon>,
}

impl ServiceHandler for RemoteService {
    fn handle(
        &self,
        _node: Arc<Node>,
        peer: DeviceId,
        hello: Hello,
        send: SendStream,
        recv: RecvStream,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        let daemon = self.daemon.clone();
        Box::pin(async move {
            if hello.service != SERVICE || hello.version != SERVICE_VERSION {
                log::info!(
                    "{} asked for unknown service {}/{}",
                    peer.short(),
                    hello.service,
                    hello.version
                );
                return;
            }
            let handler: Arc<dyn muster_rpc::Handler> = Arc::new(MusterHandler::new(daemon));
            muster_rpc::serve(
                tokio::io::join(recv, send),
                Principal::Remote(peer),
                handler,
            )
            .await;
        })
    }
}

const PUSH_TOKEN_KEY: &str = "push.token";

/// `[kind][token…]` as stored by `Daemon::set_push_token`.
fn stored_push_token(store: &Store) -> Option<(u8, Vec<u8>)> {
    let bytes = store.meta(PUSH_TOKEN_KEY).ok().flatten()?;
    let (kind, token) = bytes.split_first()?;
    (!token.is_empty()).then(|| (*kind, token.to_vec()))
}

fn node_config(
    config: &DaemonConfig,
    relay: RelayConfig,
    push_token: Option<(u8, Vec<u8>)>,
) -> NodeConfig {
    NodeConfig {
        bind: config.bind,
        relay,
        options: MagicOptions {
            // A phone reaches its members through the relay only, on one
            // relay carrier at a time (no hole punching, no probes).
            relay_only: config.kind == DeviceKind::Phone,
            mobile: config.kind == DeviceKind::Phone,
            allow_loopback: config.allow_loopback,
            extra_candidates: Vec::new(),
            push_token,
        },
        datagrams: config.vnet,
        build: Build {
            version: config.version.clone(),
            commit: build_info::COMMIT.into(),
        },
    }
}

/// Public host:port always uses certificate-verified QUIC + TLS.
/// Explicit loopback transports remain available for local development tests.
pub async fn parse_relay(text: Option<&str>) -> RelayConfig {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else {
        return RelayConfig::None;
    };
    // Before 2026-09-28 a relay could be a `wss://host` WebSocket. That
    // carrier is gone; the same host serves the secure relay on 9443, the one
    // public relay port (ARCHITECTURE D13), so a remembered address keeps
    // working instead of failing to start the link.
    if let Some(host) = retired_websocket_host(text) {
        log::warn!("relay {text}: WebSocket relays are retired; using {host}:9443");
        return RelayConfig::Secure(format!("{host}:9443"));
    }
    if let Ok(addr) = text.parse::<SocketAddr>()
        && addr.ip().is_loopback()
    {
        return RelayConfig::Udp(addr);
    }
    // The socket owner resolves via DoH and refreshes after failures/TTL;
    // a failed lookup must remain a retryable configuration, not `None`.
    RelayConfig::Secure(text.to_string())
}

/// The host of a retired `wss://host[:port][/path]` relay address.
fn retired_websocket_host(text: &str) -> Option<&str> {
    let authority = text.strip_prefix("wss://")?.split(['/', '?', '#']).next()?;
    let host = match authority.rsplit_once(':') {
        Some((host, port)) if port.parse::<u16>().is_ok() => host,
        _ => authority,
    };
    (!host.is_empty()).then_some(host)
}

/// The machine's name for a device that was given none: gethostname(2)
/// without a `.local` suffix (macOS under launchd has neither
/// /etc/hostname nor $HOSTNAME), then the file and the variable.
fn hostname() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: buf is a valid, writable buffer of the stated length.
    let ok = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } == 0;
    let from_syscall = if ok {
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..end]).trim().to_string()
    } else {
        String::new()
    };
    Some(from_syscall)
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_end_matches(".local").to_string())
        .or_else(|| {
            std::fs::read_to_string("/etc/hostname")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .or_else(|| std::env::var("HOSTNAME").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "device".into())
}

pub fn parse_kind(text: &str) -> Option<DeviceKind> {
    match text.trim().to_ascii_lowercase().as_str() {
        "desktop" | "mac" | "pc" => Some(DeviceKind::Desktop),
        "phone" | "ios" | "android" | "harmony" => Some(DeviceKind::Phone),
        "server" | "linux" => Some(DeviceKind::Server),
        _ => None,
    }
}

pub fn kind_str(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Desktop => "desktop",
        DeviceKind::Phone => "phone",
        DeviceKind::Server => "server",
    }
}

#[cfg(test)]
mod relay_text_tests {
    use super::*;

    #[cfg(target_os = "macos")]
    async fn delayed_helper_daemon() -> (tempfile::TempDir, Arc<Daemon>) {
        let dir = tempfile::tempdir().unwrap();
        let mut config = DaemonConfig::new(dir.path());
        config.agent = None;
        config.cloud = false;
        config.listen = false;
        config.vnet = true;
        let daemon = Daemon::start_with(config, Arc::new(DefaultPlatform), None)
            .await
            .unwrap();
        // Populate the roster without create_network's automatic helper open.
        // The first actual start must use only the injected mock descriptor.
        let entry = muster_core::roster::Author {
            roster: &daemon.roster.read().unwrap(),
            signer: daemon.signer.as_ref(),
        }
        .genesis(
            "helper-lifecycle-test",
            ("10.92.0.0".parse().unwrap(), 24),
            daemon.own_record(),
        )
        .unwrap();
        daemon.node().publish(entry).unwrap();
        (dir, daemon)
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn delayed_helper_completion_is_consumed_without_another_start_request() {
        let (_dir, daemon) = delayed_helper_daemon().await;
        let (release, blocked) = tokio::sync::oneshot::channel::<()>();
        *daemon.injected_vnet_open.lock().unwrap() = Some(tokio::spawn(async move {
            let _ = blocked.await;
            Err("injected delayed helper failure".to_string())
        }));
        daemon.maybe_start_vnet().await;
        let original = daemon
            .vnet_startup
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .task
            .id();
        // Concurrent observations time out but cannot create a second XPC open.
        tokio::join!(daemon.maybe_start_vnet(), daemon.maybe_start_vnet());
        let mut done = {
            let startup = daemon.vnet_startup.lock().unwrap();
            let startup = startup.as_ref().unwrap();
            assert_eq!(startup.task.id(), original);
            assert!(!*startup.done.borrow());
            startup.done.clone()
        };
        release.send(()).unwrap();
        // No more maybe_start_vnet call: the existing worker must consume it.
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            done.wait_for(|done| *done),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(daemon.injected_vnet_open.lock().unwrap().is_none());
        assert!(daemon.vnet().is_none());
        assert!(daemon.vnet_helper.lock().unwrap().is_none());
        daemon.shutdown().await;
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn delayed_helper_success_publishes_vnet_and_shutdown_closes_descriptor() {
        use std::os::fd::OwnedFd;
        let _ = env_logger::builder().is_test(true).try_init();
        let (_dir, daemon) = delayed_helper_daemon().await;
        let address = daemon.device_state().vip;
        let (device, kernel) = std::os::unix::net::UnixDatagram::pair().unwrap();
        let fd: OwnedFd = device.into();
        // Connect allocates an XPC client only. No helper operation, root TUN,
        // system route or network policy is requested by this injected result.
        let helper = muster_platform::macos::nethelper::NetHelper::connect().unwrap();
        let (release, blocked) = tokio::sync::oneshot::channel::<()>();
        *daemon.injected_vnet_open.lock().unwrap() = Some(tokio::spawn(async move {
            let _ = blocked.await;
            Ok((address, helper, fd))
        }));
        daemon.maybe_start_vnet().await;
        assert!(daemon.vnet().is_none());
        let mut done = daemon
            .vnet_startup
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .done
            .clone();
        release.send(()).unwrap();
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            done.wait_for(|done| *done),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(daemon.vnet().is_some());
        assert!(daemon.vnet_helper.lock().unwrap().is_some());
        daemon.shutdown().await;
        assert!(daemon.vnet().is_none());
        assert!(daemon.vnet_helper.lock().unwrap().is_none());
        assert!(kernel.send(&[0]).is_err(), "mock TUN descriptor leaked");
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn shutdown_cancels_startup_without_waiting_for_blocking_helper() {
        let (_dir, daemon) = delayed_helper_daemon().await;
        let (release, blocked) = tokio::sync::oneshot::channel::<()>();
        let address = daemon.device_state().vip;
        let (device, kernel) = std::os::unix::net::UnixDatagram::pair().unwrap();
        let fd: std::os::fd::OwnedFd = device.into();
        let helper = muster_platform::macos::nethelper::NetHelper::connect().unwrap();
        let (finished, dropped) = tokio::sync::oneshot::channel();
        *daemon.injected_vnet_open.lock().unwrap() = Some(tokio::spawn(async move {
            let _ = blocked.await;
            let _ = finished.send(());
            Ok((address, helper, fd))
        }));
        daemon.maybe_start_vnet().await;
        tokio::time::timeout(std::time::Duration::from_secs(1), daemon.shutdown())
            .await
            .unwrap();
        assert!(daemon.vnet_startup.lock().unwrap().is_none());
        release.send(()).unwrap();
        dropped.await.unwrap();
        assert!(
            kernel.send(&[0]).is_err(),
            "late helper result leaked its descriptor"
        );
        daemon.maybe_start_vnet().await;
        assert!(daemon.vnet_startup.lock().unwrap().is_none());
        assert!(daemon.vnet().is_none());
        assert!(daemon.vnet_helper.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn a_remembered_websocket_relay_becomes_the_secure_relay_on_its_host() {
        for text in [
            "wss://muster.linsheng.dev",
            "wss://muster.linsheng.dev/",
            "wss://muster.linsheng.dev:443/relay",
        ] {
            assert!(
                matches!(parse_relay(Some(text)).await, RelayConfig::Secure(a) if a == "muster.linsheng.dev:9443"),
                "{text}"
            );
        }
        assert!(matches!(
            parse_relay(Some("muster.linsheng.dev:9443")).await,
            RelayConfig::Secure(a) if a == "muster.linsheng.dev:9443"
        ));
        assert!(matches!(
            parse_relay(Some("127.0.0.1:9443")).await,
            RelayConfig::Udp(_)
        ));
        assert!(matches!(parse_relay(Some("  ")).await, RelayConfig::None));
    }
}
