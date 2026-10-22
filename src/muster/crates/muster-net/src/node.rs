//! The link owner: one QUIC endpoint on a MagicSock, one connection per
//! peer, service streams dispatched by name, roster gossip on every link,
//! membership enforced on every roster change.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::{Duration, Instant};

use muster_core::identity::{DeviceId, SharedSigner};
use muster_core::policy::LocalPolicy;
use muster_core::roster::{DeviceKind, DeviceRecord, Entry, EntryHash};
use quinn::{Connection, Endpoint, EndpointConfig, RecvStream, SendStream, TokioRuntime};
use tokio::sync::{broadcast, mpsc};

use crate::heartbeat::Build;
use crate::magicsock::{MagicOptions, MagicSock, PathInfo, RelayLink, RelayState};
use crate::pair::Pairing;
use crate::reconnect::{CONNECT_TIMEOUT, Retry};
use crate::relay_proto::{Frame, MAX_FRAME};
use crate::stream::{Hello, read_hello, write_hello};
use crate::tls::{SharedRoster, TlsIdentity, Trust, client_config, peer_device_id, server_config};
use crate::{Error, forward, gossip, heartbeat, pair};

pub type SharedPolicy = Arc<RwLock<LocalPolicy>>;

const ACK_OK: u8 = 0;
const ACK_REFUSED: u8 = 1;

/// How often a registered node reconciles its roster with the relay
/// mailbox; a (re)registration syncs at once.
const MAILBOX_SYNC_INTERVAL: Duration = Duration::from_secs(600);
/// A mailbox item must fit one relay frame with its header.
const MAILBOX_ITEM_MAX: usize = MAX_FRAME - 64;

#[derive(Clone, Debug)]
pub enum RelayConfig {
    /// Public-PKI QUIC/UDP + TLS/TCP on one authority, selected by health.
    Secure(String),
    #[cfg(test)]
    SecureTest(String, Arc<rustls::ClientConfig>),
    /// Plain UDP to a loopback test relay (`muster-relay --insecure-legacy`).
    Udp(SocketAddr),
    None,
}

#[derive(Clone, Debug)]
pub struct NodeConfig {
    pub bind: SocketAddr,
    pub relay: RelayConfig,
    pub options: MagicOptions,
    /// Accept vnet datagrams (musterd turns them into TUN writes).
    pub datagrams: bool,
    /// What this device runs, told to every member in the heartbeat.
    pub build: Build,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:0".parse().unwrap(),
            relay: RelayConfig::None,
            options: MagicOptions::default(),
            datagrams: false,
            build: Build::default(),
        }
    }
}

/// A service the application registers by name (muster-rpc lives here).
pub trait ServiceHandler: Send + Sync {
    fn handle(
        &self,
        node: Arc<Node>,
        peer: DeviceId,
        hello: Hello,
        send: SendStream,
        recv: RecvStream,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>>;
}

#[derive(Clone, Debug)]
pub struct PeerInfo {
    pub id: DeviceId,
    pub connected: bool,
    pub connection_status: String,
    pub path: Option<PathInfo>,
    pub rtt: Option<Duration>,
    /// The build the peer last reported in a heartbeat; kept after it drops.
    pub build: Option<Build>,
    /// Whether it is online, from evidence (presence), not from this instant's
    /// link: what member lists show and act on.
    pub presence: crate::presence::View,
}

#[derive(Clone)]
struct Transport {
    sock: Arc<MagicSock>,
    endpoint: Endpoint,
    generation: u64,
}

impl Transport {
    fn start(
        config: &NodeConfig,
        signer: &SharedSigner,
        identity: &TlsIdentity,
        trust: &Trust,
        presence: &Arc<crate::presence::Board>,
        generation: u64,
    ) -> Result<Self, Error> {
        let relay = match &config.relay {
            #[cfg(test)]
            RelayConfig::SecureTest(authority, tls) => RelayLink::Secure(
                crate::secure_relay::SecureRelay::with_config(
                    authority.clone(),
                    signer.clone(),
                    (**tls).clone(),
                    if config.options.mobile {
                        crate::secure_relay::Strategy::Fallback
                    } else {
                        crate::secure_relay::Strategy::Standby
                    },
                )
                .map_err(|error| Error::Tls(error.to_string()))?,
            ),
            RelayConfig::Secure(authority) => RelayLink::Secure(
                crate::secure_relay::SecureRelay::new(
                    authority.clone(),
                    signer.clone(),
                    if config.options.mobile {
                        crate::secure_relay::Strategy::Fallback
                    } else {
                        crate::secure_relay::Strategy::Standby
                    },
                )
                .map_err(|error| Error::Tls(error.to_string()))?,
            ),
            RelayConfig::Udp(addr) => RelayLink::Udp(*addr),
            RelayConfig::None => RelayLink::None,
        };
        let server = server_config(identity, trust.clone())?;
        let sock = MagicSock::bind(config.bind, relay, signer.clone(), config.options.clone())?;
        sock.set_presence(presence.clone());
        let mut endpoint_config = EndpointConfig::default();
        endpoint_config.grease_quic_bit(false);
        match Endpoint::new_with_abstract_socket(
            endpoint_config,
            Some(server),
            sock.clone(),
            Arc::new(TokioRuntime),
        ) {
            Ok(endpoint) => Ok(Self {
                sock,
                endpoint,
                generation,
            }),
            Err(error) => {
                sock.shutdown();
                Err(Error::Io(format!("endpoint: {error}")))
            }
        }
    }
}

pub struct Node {
    retries: Mutex<HashMap<DeviceId, Retry>>,
    presence: Arc<crate::presence::Board>,
    dial_locks: Mutex<HashMap<DeviceId, Arc<tokio::sync::Mutex<()>>>>,
    transport: RwLock<Option<Transport>>,
    config: Mutex<NodeConfig>,
    repair_wake: tokio::sync::Notify,
    reconfigure: tokio::sync::Mutex<()>,
    requested: std::sync::atomic::AtomicU64,
    completed: tokio::sync::watch::Sender<u64>,
    signer: SharedSigner,
    identity: TlsIdentity,
    trust: Trust,
    policy: SharedPolicy,
    pairing: Pairing,
    connections: Mutex<HashMap<DeviceId, Connection>>,
    /// When each current connection was adopted.
    adopted: Mutex<HashMap<DeviceId, Instant>>,
    builds: Mutex<HashMap<DeviceId, Build>>,
    handler: Mutex<Option<Arc<dyn ServiceHandler>>>,
    /// Entries newly accepted, for persistence and UI.
    entries_tx: broadcast::Sender<Entry>,
    /// Inbound vnet datagrams: (peer, raw IPv4 packet).
    datagram_tx: Option<mpsc::Sender<(DeviceId, bytes::Bytes)>>,
    datagram_rx: Mutex<Option<mpsc::Receiver<(DeviceId, bytes::Bytes)>>>,
    closed: AtomicBool,
}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Node({})", self.identity.device_id.short())
    }
}

impl Node {
    pub async fn start(
        config: NodeConfig,
        signer: SharedSigner,
        roster: SharedRoster,
        policy: SharedPolicy,
    ) -> Result<Arc<Self>, Error> {
        let identity = TlsIdentity::new(signer.clone())?;
        let trust = Trust::new(roster);
        let (presence, presence_changes) = crate::presence::Board::new(match config.relay {
            RelayConfig::None => crate::presence::Local::Up,
            _ => crate::presence::Local::Connecting,
        });
        let transport = Transport::start(&config, &signer, &identity, &trust, &presence, 1)?;
        let (entries_tx, _) = broadcast::channel(256);
        let (datagram_tx, datagram_rx) = if config.datagrams {
            let (tx, rx) = mpsc::channel(4096);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };
        let node = Arc::new(Self {
            retries: Mutex::default(),
            presence,
            dial_locks: Mutex::default(),
            transport: RwLock::new(Some(transport)),
            config: Mutex::new(config),
            repair_wake: tokio::sync::Notify::new(),
            reconfigure: tokio::sync::Mutex::new(()),
            requested: std::sync::atomic::AtomicU64::new(0),
            completed: tokio::sync::watch::channel(0).0,
            signer,
            identity,
            trust,
            policy,
            pairing: Pairing::default(),
            connections: Mutex::default(),
            adopted: Mutex::default(),
            builds: Mutex::default(),
            handler: Mutex::new(None),
            entries_tx,
            datagram_tx,
            datagram_rx: Mutex::new(datagram_rx),
            closed: AtomicBool::new(false),
        });
        node.sync_vnet_prefix();
        tokio::spawn(node.clone().accept_loop());
        tokio::spawn(mailbox_loop(Arc::downgrade(&node)));
        tokio::spawn(reconnect_loop(Arc::downgrade(&node)));
        tokio::spawn(presence_loop(Arc::downgrade(&node), presence_changes));
        Ok(node)
    }

    /// Whether `peer` is online, from evidence (see `presence`).
    pub fn presence(&self, peer: &DeviceId) -> crate::presence::View {
        self.presence.view(peer)
    }

    /// The user asked to refresh: offline members get one more round of
    /// bounded attempts.
    pub fn refresh_presence(&self) {
        self.presence.refresh();
    }

    /// A phone went to the background: its failing dials say nothing about
    /// anyone until it is back.
    pub fn suspend(&self) {
        self.presence.suspend();
    }

    pub fn resume(&self) {
        self.presence.resume();
    }

    pub fn suspended(&self) -> bool {
        self.presence.suspended()
    }

    pub fn set_handler(&self, handler: Arc<dyn ServiceHandler>) {
        *self.handler.lock().unwrap() = Some(handler);
    }

    pub fn device_id(&self) -> DeviceId {
        self.identity.device_id
    }
    /// What this device tells its peers it runs.
    pub fn build(&self) -> Build {
        self.config.lock().unwrap().build.clone()
    }
    pub(crate) fn record_build(&self, peer: DeviceId, build: Build) {
        self.builds.lock().unwrap().insert(peer, build);
    }
    /// The link is still the one in use for `peer`.
    pub(crate) fn is_current(&self, peer: &DeviceId, connection: &Connection) -> bool {
        !self.closed.load(Ordering::Relaxed)
            && connection.close_reason().is_none()
            && self
                .connections
                .lock()
                .unwrap()
                .get(peer)
                .is_some_and(|c| c.stable_id() == connection.stable_id())
    }
    pub fn signer(&self) -> &SharedSigner {
        &self.signer
    }
    pub fn roster(&self) -> &SharedRoster {
        &self.trust.roster
    }
    pub fn policy(&self) -> &SharedPolicy {
        &self.policy
    }
    pub fn trust(&self) -> &Trust {
        &self.trust
    }
    pub fn pairing(&self) -> &Pairing {
        &self.pairing
    }
    pub fn local_addr(&self) -> Result<SocketAddr, Error> {
        self.transport()?
            .endpoint
            .local_addr()
            .map_err(|e| Error::Io(e.to_string()))
    }
    pub fn relay_state(&self) -> RelayState {
        self.sock()
            .map_or(RelayState::Failed, |sock| sock.relay_state())
    }
    pub fn reflected(&self) -> Option<SocketAddr> {
        self.sock().and_then(|sock| sock.reflected())
    }
    pub fn subscribe_entries(&self) -> broadcast::Receiver<Entry> {
        self.entries_tx.subscribe()
    }
    /// The inbound vnet datagram stream; taken once by the vnet.
    pub fn take_datagrams(&self) -> Option<mpsc::Receiver<(DeviceId, bytes::Bytes)>> {
        self.datagram_rx.lock().unwrap().take()
    }
    /// See [`MagicSock::set_push_token`].
    pub fn set_push_token(&self, token: Option<(u8, Vec<u8>)>) {
        let mut config = self.config.lock().unwrap();
        config.options.push_token = token.clone();
        if let Some(sock) = self.sock() {
            sock.set_push_token(token);
        }
    }

    /// Hands the roster's vnet prefix to the socket, so vips are never
    /// tried as direct paths. Called whenever the roster changes.
    fn sync_vnet_prefix(&self) {
        let prefix = self.trust.roster.read().unwrap().state().vnet_cidr;
        if let Some(sock) = self.sock() {
            sock.set_vnet_prefix(prefix);
        }
    }

    /// Replace only the transport, retaining the Node and its single Vnet
    /// datagram receiver. Completion means constructed, not relay/peer ready.
    pub async fn reconfigure_relay(&self, relay: RelayConfig) -> Result<(), Error> {
        if let RelayConfig::Secure(authority) = &relay {
            crate::secure_relay::SecureRelay::validate_authority(authority)
                .map_err(|e| Error::Protocol(e.to_string()))?;
        }
        let _serial = self.reconfigure.lock().await;
        let mut completed = self.completed.subscribe();
        let revision = {
            let mut config = self.config.lock().unwrap();
            if self.closed.load(Ordering::Relaxed) {
                return Err(Error::Io("node is shut down".into()));
            }
            config.relay = relay;
            self.requested.fetch_add(1, Ordering::Relaxed) + 1
        };
        self.repair_wake.notify_one();
        loop {
            if self.closed.load(Ordering::Relaxed) {
                return Err(Error::Io(
                    "node shut down during transport replacement".into(),
                ));
            }
            if *completed.borrow_and_update() >= revision {
                return Ok(());
            }
            completed
                .changed()
                .await
                .map_err(|e| Error::Io(e.to_string()))?;
        }
    }

    /// See [`MagicSock::network_changed`].
    pub fn network_changed(&self) {
        self.repair_wake.notify_one();
        if let Some(sock) = self.sock() {
            sock.network_changed();
        }
        for retry in self.retries.lock().unwrap().values_mut() {
            retry.reset();
        }
        self.presence.retry_unreachable();
        // QUIC survives a path change. Keep live streams (including an
        // in-progress presence-authorized action); the idle deadline and
        // reconnect loop replace only links that actually stop responding.
    }

    pub fn sock(&self) -> Option<Arc<MagicSock>> {
        self.transport
            .read()
            .unwrap()
            .as_ref()
            .map(|t| t.sock.clone())
    }

    fn transport(&self) -> Result<Transport, Error> {
        self.transport
            .read()
            .unwrap()
            .clone()
            .ok_or_else(|| Error::Io("network transport recovering".into()))
    }

    /// Opens an invite: from now until it is used or expires, unknown
    /// clients may connect to reach the pair service.
    pub fn open_invite(&self, invite: muster_core::Invite) {
        self.pairing.open(invite);
        self.trust
            .accept_unknown_clients
            .store(true, Ordering::Relaxed);
    }

    pub fn peers(&self) -> Vec<PeerInfo> {
        let sock = self.sock();
        let connections = self.connections.lock().unwrap();
        let roster = self.trust.roster.read().unwrap();
        let builds = self.builds.lock().unwrap();
        roster
            .state()
            .active_members()
            .filter(|m| m.record.device_id != self.identity.device_id)
            .map(|m| {
                let id = m.record.device_id;
                let connection = connections.get(&id).filter(|c| c.close_reason().is_none());
                PeerInfo {
                    id,
                    presence: self.presence.view(&id),
                    connected: connection.is_some(),
                    connection_status: if connection.is_some() {
                        "connected".into()
                    } else {
                        self.retries
                            .lock()
                            .unwrap()
                            .get(&id)
                            .map(|r| r.status(Instant::now()))
                            .unwrap_or("not connected")
                            .into()
                    },
                    path: sock.as_ref().and_then(|s| s.path(&id)),
                    rtt: connection.map(Connection::rtt),
                    build: builds.get(&id).cloned(),
                }
            })
            .collect()
    }

    pub fn connection_to(&self, peer: &DeviceId) -> Option<Connection> {
        self.connections
            .lock()
            .unwrap()
            .get(peer)
            .filter(|c| c.close_reason().is_none())
            .cloned()
    }

    /// Connection direction comes from the signed roster, not platform flags.
    fn is_service_host(&self, peer: &DeviceId) -> bool {
        self.trust
            .roster
            .read()
            .unwrap()
            .state()
            .active(peer)
            .is_some_and(|member| member.record.kind != DeviceKind::Phone)
    }

    /// Connects to a desktop/server member. Phones initiate links; desktops
    /// reply on those links without dialing back. Roster and heartbeat streams
    /// use the adopted connection directly.
    pub async fn connect(self: &Arc<Self>, peer: DeviceId) -> Result<Connection, Error> {
        if !self.trust.is_member(&self.identity.device_id) {
            return Err(Error::Unauthorized("this device is not a member".into()));
        }
        if !self.is_service_host(&peer) {
            return Err(Error::Unauthorized(
                "outbound member connections require a desktop or server".into(),
            ));
        }
        if let Some(existing) = self.connection_to(&peer) {
            return Ok(existing);
        }
        let config = client_config(&self.identity, self.trust.clone(), None)?;
        self.dial(peer, config).await
    }

    /// The joiner's connection to the admin named in an invite.
    pub async fn connect_pinned(self: &Arc<Self>, peer: DeviceId) -> Result<Connection, Error> {
        // Enrollment pinning must not bypass member connection direction.
        if self.trust.is_member(&self.device_id()) && self.trust.is_member(&peer) {
            return self.connect(peer).await;
        }
        if let Some(existing) = self.connection_to(&peer) {
            return Ok(existing);
        }
        let config = client_config(&self.identity, self.trust.clone(), Some(peer))?;
        self.dial(peer, config).await
    }

    async fn dial(
        self: &Arc<Self>,
        peer: DeviceId,
        config: quinn::ClientConfig,
    ) -> Result<Connection, Error> {
        let lock = self
            .dial_locks
            .lock()
            .unwrap()
            .entry(peer)
            .or_default()
            .clone();
        let deadline = tokio::time::Instant::now() + CONNECT_TIMEOUT;
        let _dial = tokio::time::timeout_at(deadline, lock.lock())
            .await
            .map_err(|_| Error::Timeout("connect (3 seconds)"))?;
        if let Some(existing) = self.connection_to(&peer) {
            return Ok(existing);
        }
        // Concurrent callers share one verdict: once a member is offline,
        // nobody dials it again until new evidence or a refresh.
        let member = self.trust.is_member(&peer);
        if member && !self.presence.may_dial(&peer) {
            return Err(Error::Offline(format!(
                "{} is {}",
                peer.short(),
                self.presence.view(&peer).as_str()
            )));
        }
        let transport = self.transport()?;
        self.retries
            .lock()
            .unwrap()
            .entry(peer)
            .or_default()
            .begin(Instant::now());
        let addr = transport.sock.add_peer(peer);
        let attempt = tokio::time::timeout_at(deadline, async {
            let connecting =
                transport
                    .endpoint
                    .connect_with(config, addr, crate::tls::SERVER_NAME)?;
            connecting.await.map_err(Error::from)
        });
        // The relay may declare the member absent while this dial runs: stop
        // sending to it at once rather than at the deadline.
        let declared_absent = async {
            loop {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if member && !self.presence.may_dial(&peer) {
                    return;
                }
            }
        };
        let result = tokio::select! {
            result = attempt => result
                .map_err(|_| Error::Timeout("connect (3 seconds)"))
                .and_then(|r| r),
            () = declared_absent => {
                self.retries.lock().unwrap().entry(peer).or_default().reset();
                return Err(Error::Offline(format!("{} is offline", peer.short())));
            }
        };
        let connection = match result {
            Ok(c) => c,
            Err(e) => {
                transport.sock.defer_direct(&peer);
                self.retries
                    .lock()
                    .unwrap()
                    .entry(peer)
                    .or_default()
                    .fail(Instant::now());
                if member {
                    self.presence.dial_failed(peer);
                }
                return Err(e);
            }
        };
        self.adopt(peer, connection.clone(), transport.generation);
        Ok(self.connection_to(&peer).unwrap_or(connection))
    }

    /// Registers a member connection and starts serving and syncing it.
    pub(crate) fn adopt(self: &Arc<Self>, peer: DeviceId, connection: Connection, generation: u64) {
        let transport = self.transport.read().unwrap();
        if self.closed.load(Ordering::Relaxed)
            || connection.close_reason().is_some()
            || !transport
                .as_ref()
                .is_some_and(|t| t.generation == generation)
        {
            connection.close(0u32.into(), b"retired transport");
            return;
        }
        let mut connections = self.connections.lock().unwrap();
        let mut adopted = self.adopted.lock().unwrap();
        // Both ends may reconnect simultaneously. Both choose the same
        // connection: the one initiated by the lower device id.
        let preferred = if self.device_id() < peer {
            quinn::Side::Client
        } else {
            quinn::Side::Server
        };
        if let Some(existing) = connections
            .get(&peer)
            .filter(|c| c.close_reason().is_none())
        {
            if existing.stable_id() == connection.stable_id() {
                return;
            }
            // A peer dials only while it has no link. A new connection after
            // our current one is settled means it lost that one (a restart,
            // a rebuilt socket, its idle timeout): ours is dead and only
            // waits for its own timeout. Replace it, or a relaunched phone
            // is refused as a duplicate until then. Within the connect
            // window two connections are a simultaneous dial.
            let settled = adopted
                .get(&peer)
                .is_some_and(|at| at.elapsed() >= CONNECT_TIMEOUT);
            if !settled && (existing.side() == preferred || connection.side() != preferred) {
                connection.close(0u32.into(), b"duplicate");
                return;
            }
        }
        let previous = connections.insert(peer, connection.clone());
        adopted.insert(peer, Instant::now());
        drop((connections, adopted));
        drop(transport);
        self.retries
            .lock()
            .unwrap()
            .entry(peer)
            .or_default()
            .reset();
        if let Some(previous) = previous
            && previous.stable_id() != connection.stable_id()
        {
            previous.close(0u32.into(), b"replaced");
        }
        tokio::spawn(
            self.clone()
                .serve_connection(peer, connection.clone(), generation),
        );
        if self.trust.is_member(&peer) {
            self.presence.link_up(peer);
            tokio::spawn(heartbeat::run(
                Arc::downgrade(self),
                peer,
                connection.clone(),
            ));
            let node = self.clone();
            tokio::spawn(async move {
                match node
                    .open_on(&connection, gossip::SERVICE, gossip::VERSION)
                    .await
                {
                    Ok((mut send, mut recv)) => {
                        match gossip::exchange(&node, &mut send, &mut recv).await {
                            Ok(n) if n > 0 => {
                                log::info!("roster sync with {} accepted {n} entries", peer.short())
                            }
                            Ok(_) => {}
                            Err(error) => log::debug!("roster sync with {}: {error}", peer.short()),
                        }
                    }
                    Err(error) => {
                        log::debug!("cannot open roster stream to {}: {error}", peer.short())
                    }
                }
            });
        }
    }

    /// Opens a service stream on a member connection.
    pub async fn open(
        self: &Arc<Self>,
        peer: DeviceId,
        service: &str,
        version: u32,
    ) -> Result<(SendStream, RecvStream), Error> {
        tokio::time::timeout(CONNECT_TIMEOUT, async {
            let connection = self.connect(peer).await?;
            self.open_on(&connection, service, version).await
        })
        .await
        .map_err(|_| Error::Timeout("connect/service (3 seconds)"))?
    }

    /// Opens a stream, says which service, and waits for the peer to accept
    /// it, so a refusal (not a member, unknown service) is an error here
    /// rather than a silently dead stream.
    pub async fn open_on(
        &self,
        connection: &Connection,
        service: &str,
        version: u32,
    ) -> Result<(SendStream, RecvStream), Error> {
        let deadline = tokio::time::Instant::now() + CONNECT_TIMEOUT;
        let (mut send, mut recv) = tokio::time::timeout_at(deadline, connection.open_bi())
            .await
            .map_err(|_| Error::Timeout("service stream (3 seconds)"))??;
        tokio::time::timeout_at(
            deadline,
            write_hello(&mut send, &Hello::new(service, version)),
        )
        .await
        .map_err(|_| Error::Timeout("service hello (3 seconds)"))??;
        let mut ack = [0u8; 1];
        tokio::time::timeout_at(deadline, recv.read_exact(&mut ack))
            .await
            .map_err(|_| Error::Timeout("service accept"))?
            .map_err(|e| Error::Io(format!("service accept: {e}")))?;
        match ack[0] {
            ACK_OK => Ok((send, recv)),
            ACK_REFUSED => Err(Error::Unauthorized(format!("{service} refused by peer"))),
            other => Err(Error::Protocol(format!("bad service ack {other}"))),
        }
    }

    /// A forward stream to `port` on `peer`.
    pub async fn forward(
        self: &Arc<Self>,
        peer: DeviceId,
        port: u16,
    ) -> Result<(SendStream, RecvStream), Error> {
        tokio::time::timeout(CONNECT_TIMEOUT, forward::open(self, peer, port))
            .await
            .map_err(|_| Error::Timeout("forward connect (3 seconds)"))?
    }

    /// Pairs into `invite`'s network; the returned roster is installed as ours.
    pub async fn join(
        self: &Arc<Self>,
        invite: &muster_core::Invite,
        record: DeviceRecord,
    ) -> Result<(), Error> {
        let roster = pair::join(self, invite, record).await?;
        let entries: Vec<Entry> = roster.entries().cloned().collect();
        *self.trust.roster.write().unwrap() = roster;
        self.sync_vnet_prefix();
        // The pinned pairing connection was adopted before we had a roster,
        // so adopt could not mark its peer online. The verified response and
        // still-live current link now provide member liveness evidence.
        if self.trust.is_member(&invite.admin_id) && self.connection_to(&invite.admin_id).is_some()
        {
            self.presence.link_up(invite.admin_id);
        }
        for entry in entries {
            let _ = self.entries_tx.send(entry);
        }
        Ok(())
    }

    /// Serialize a small synchronous commit with transport retirement. The
    /// operation must not re-enter transport accessors or perform async I/O.
    pub(crate) fn with_generation<T>(
        &self,
        generation: u64,
        operation: impl FnOnce() -> Result<T, Error>,
    ) -> Result<T, Error> {
        let current = self.transport.read().unwrap();
        if self.closed.load(Ordering::Relaxed)
            || !current.as_ref().is_some_and(|t| t.generation == generation)
        {
            return Err(Error::Io("retired network transport".into()));
        }
        operation()
    }

    /// Signs an Add on behalf of pairing; returns the whole roster for the joiner.
    pub(crate) fn author_add(
        self: &Arc<Self>,
        record: DeviceRecord,
        generation: u64,
    ) -> Result<Vec<Entry>, Error> {
        let entry = self.with_generation(generation, || {
            let mut roster = self.trust.roster.write().unwrap();
            pair::author_add(&mut roster, self.signer.as_ref(), record)
        })?;
        self.announce(vec![entry]);
        Ok(self
            .trust
            .roster
            .read()
            .unwrap()
            .entries()
            .cloned()
            .collect())
    }

    /// Inserts an entry this device authored and pushes it to peers.
    pub fn publish(self: &Arc<Self>, entry: Entry) -> Result<(), Error> {
        self.trust.roster.write().unwrap().insert(entry.clone())?;
        self.sync_vnet_prefix();
        self.deposit(vec![entry.clone()]);
        self.announce(vec![entry]);
        Ok(())
    }

    /// The relay mailbox key of this network: its id.
    fn mailbox_key(&self) -> Option<[u8; 32]> {
        self.trust
            .roster
            .read()
            .unwrap()
            .network_id()
            .map(|id| id.0)
    }

    /// Leaves entries in the relay mailbox under the network id, so a device
    /// that comes back when no peer is online still learns of them (§3.5:
    /// the mailbox holds opaque signed blobs; the relay verifies nothing and
    /// every reader validates them as roster entries).
    fn deposit(self: &Arc<Self>, entries: Vec<Entry>) {
        if !matches!(
            self.sock()
                .map_or(RelayState::Failed, |sock| sock.relay_state()),
            RelayState::Registered
        ) {
            return;
        }
        let Some(key) = self.mailbox_key() else {
            return;
        };
        let node = self.clone();
        tokio::spawn(async move {
            for entry in entries {
                let item = entry.encode();
                if item.len() > MAILBOX_ITEM_MAX {
                    log::warn!(
                        "roster entry {} too large for the mailbox",
                        entry.hash().short()
                    );
                    continue;
                }
                let result = match node.transport() {
                    Ok(transport) => transport.sock.mailbox(Frame::MbPut { key, item }).await,
                    Err(error) => Err(error),
                };
                if let Err(error) = result {
                    log::debug!("mailbox put: {error}");
                    break;
                }
            }
        });
    }

    /// Reconciles the roster with the relay mailbox both ways: absorbs the
    /// entries listed there that this device lacks, then deposits the
    /// accepted entries the relay lacks. Returns how many were absorbed.
    pub async fn sync_mailbox(self: &Arc<Self>) -> Result<usize, Error> {
        let Some(key) = self.mailbox_key() else {
            return Ok(0);
        };
        if !matches!(
            self.sock()
                .map_or(RelayState::Failed, |sock| sock.relay_state()),
            RelayState::Registered
        ) {
            return Err(Error::Io("relay not registered".into()));
        }
        let listed = match self
            .transport()?
            .sock
            .mailbox(Frame::MbList { key })
            .await?
        {
            Frame::MbListing { hashes, .. } => hashes,
            other => {
                return Err(Error::Protocol(format!(
                    "unexpected mailbox reply {other:?}"
                )));
            }
        };
        // The relay names items by the hash of their bytes (encoded entry
        // with its signature), not by the entry hash.
        let held: HashSet<[u8; 32]> = self
            .trust
            .roster
            .read()
            .unwrap()
            .entries()
            .map(|entry| mailbox_item_hash(&entry.encode()))
            .collect();
        let mut fetched = Vec::new();
        for hash in listed.iter().filter(|hash| !held.contains(*hash)) {
            let hash = *hash;
            if let Frame::MbItem { item, .. } = self
                .transport()?
                .sock
                .mailbox(Frame::MbGet { key, hash })
                .await?
            {
                if mailbox_item_hash(&item) != hash {
                    log::info!(
                        "mailbox item {} does not hash to its name",
                        EntryHash(hash).short()
                    );
                    continue;
                }
                match Entry::decode(&item) {
                    Ok(entry) => fetched.push(entry),
                    Err(error) => log::info!("mailbox item {}: {error}", EntryHash(hash).short()),
                }
            }
        }
        let absorbed = self.absorb_entries(fetched);
        let listed: HashSet<[u8; 32]> = listed.into_iter().collect();
        let ours: Vec<Vec<u8>> = self
            .trust
            .roster
            .read()
            .unwrap()
            .entries_after(&[])
            .iter()
            .map(Entry::encode)
            .filter(|item| {
                item.len() <= MAILBOX_ITEM_MAX && !listed.contains(&mailbox_item_hash(item))
            })
            .collect();
        for item in ours {
            self.transport()?
                .sock
                .mailbox(Frame::MbPut { key, item })
                .await?;
        }
        Ok(absorbed)
    }

    /// Inserts entries learned from a peer; returns how many were new.
    pub fn absorb_entries(self: &Arc<Self>, entries: Vec<Entry>) -> usize {
        let mut accepted = Vec::new();
        let mut pending = entries;
        let mut progress = true;
        while progress && !pending.is_empty() {
            progress = false;
            let mut rest = Vec::new();
            for entry in pending {
                let outcome = self.trust.roster.write().unwrap().insert(entry.clone());
                match outcome {
                    Ok(muster_core::roster::Insert::Accepted) => {
                        accepted.push(entry);
                        progress = true;
                    }
                    Ok(muster_core::roster::Insert::Duplicate) => {}
                    Err(muster_core::Error::MissingParents(_)) => rest.push(entry),
                    Err(error) => log::info!("rejected roster entry: {error}"),
                }
            }
            pending = rest;
        }
        let count = accepted.len();
        if count > 0 {
            self.sync_vnet_prefix();
            self.announce(accepted);
        }
        count
    }

    /// Publishes new entries: to listeners, then to every connected peer —
    /// including one the entries remove, so it learns why its link drops —
    /// and only then enforces membership. The whole sequence stays inside
    /// the two-second revocation budget.
    fn announce(self: &Arc<Self>, entries: Vec<Entry>) {
        self.sync_vnet_prefix();
        for entry in &entries {
            let _ = self.entries_tx.send(entry.clone());
        }
        let peers: Vec<(DeviceId, Connection)> = self
            .connections
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, c)| c.close_reason().is_none())
            .map(|(id, c)| (*id, c.clone()))
            .collect();
        let node = self.clone();
        tokio::spawn(async move {
            let mut pushes = Vec::new();
            for (peer, connection) in peers {
                let heads = node.trust.roster.read().unwrap().heads();
                let entries = entries.clone();
                pushes.push(tokio::spawn(async move {
                    if let Err(error) = push_entries(&connection, &heads, &entries).await {
                        log::debug!("push to {}: {error}", peer.short());
                    }
                }));
            }
            let _ = tokio::time::timeout(
                Duration::from_millis(700),
                futures_util::future::join_all(pushes),
            )
            .await;
            node.enforce_membership();
        });
    }

    /// Closes every connection whose peer is no longer an active member.
    pub fn enforce_membership(&self) {
        let sock = self.sock();
        let mut connections = self.connections.lock().unwrap();
        let roster = self.trust.roster.read().unwrap();
        connections.retain(|id, connection| {
            if roster.state().active(id).is_some() {
                true
            } else {
                log::info!("closing link to {}: no longer a member", id.short());
                connection.close(1u32.into(), b"not a member");
                if let Some(sock) = &sock {
                    sock.forget_peer(id);
                }
                false
            }
        });
    }

    async fn accept_loop(self: Arc<Self>) {
        let mut backoff = Duration::from_secs(1);
        let mut last_rebuild = Instant::now() - Duration::from_secs(60);
        loop {
            let Ok(transport) = self.transport() else {
                return;
            };
            let generation = transport.generation;
            let reason = loop {
                if self.closed.load(Ordering::Relaxed) {
                    return;
                }
                if self.requested.load(Ordering::Relaxed) > *self.completed.borrow() {
                    break "relay configuration changed; preserving Node/Vnet binding".to_string();
                }
                let incoming = tokio::select! {
                    biased;
                    failure = transport.sock.failed() => break failure,
                    _ = self.repair_wake.notified() => continue,
                    incoming = transport.endpoint.accept() => match incoming {
                        Some(incoming) => incoming,
                        None => break "QUIC endpoint receive driver stopped".to_string(),
                    },
                };
                let node = self.clone();
                tokio::spawn(async move {
                    let connection = match incoming.await {
                        Ok(connection) => connection,
                        Err(error) => {
                            log::debug!("incoming handshake failed: {error}");
                            return;
                        }
                    };
                    let peer = match peer_device_id(&connection) {
                        Ok(peer) => peer,
                        Err(error) => {
                            log::warn!("accepted connection without identity: {error}");
                            connection.close(1u32.into(), b"no identity");
                            return;
                        }
                    };
                    let member = node.trust.is_member(&peer);
                    if !member && !node.pairing.any_open() {
                        connection.close(1u32.into(), b"not a member");
                        return;
                    }
                    log::info!(
                        "accepted {} ({})",
                        peer.short(),
                        if member { "member" } else { "pairing" }
                    );
                    let current_guard = node.transport.read().unwrap();
                    let Some(current) = current_guard.as_ref().filter(|t| {
                        t.generation == generation && !node.closed.load(Ordering::Relaxed)
                    }) else {
                        connection.close(0u32.into(), b"retired transport");
                        return;
                    };
                    current.sock.add_peer(peer);
                    drop(current_guard);
                    if member {
                        node.adopt(peer, connection, generation);
                    } else {
                        tokio::spawn(node.clone().serve_connection(peer, connection, generation));
                    }
                });
            };
            if self.closed.load(Ordering::Relaxed) {
                return;
            }
            if last_rebuild.elapsed() >= Duration::from_secs(60) {
                backoff = Duration::from_secs(1);
            }
            log::warn!(
                "transport generation {generation} failed/requested: {reason}; rebuild in {backoff:?}"
            );
            // accept_loop is the sole repair owner. Retire the whole pair before
            // rebinding, so fixed ports can be reclaimed and late handshakes fail.
            let retired = self.transport.write().unwrap().take();
            transport.sock.shutdown();
            transport.endpoint.close(0u32.into(), b"transport failed");
            self.connections.lock().unwrap().clear();
            drop(retired);
            drop(transport);
            loop {
                // Network-change storms and repeated configuration requests do
                // not bypass this deadline. Shutdown alone ends the wait early.
                let deadline = tokio::time::sleep(backoff);
                tokio::pin!(deadline);
                loop {
                    if self.closed.load(Ordering::Relaxed) {
                        return;
                    }
                    tokio::select! {
                        _ = &mut deadline => break,
                        _ = self.repair_wake.notified() => {},
                    }
                }
                last_rebuild = Instant::now();
                backoff = (backoff * 2).min(Duration::from_secs(60));
                if self.closed.load(Ordering::Relaxed) {
                    return;
                }
                let result = self.rebuild_transport(generation + 1);
                match result {
                    Ok(true) => {
                        self.sync_vnet_prefix();
                        for retry in self.retries.lock().unwrap().values_mut() {
                            retry.reset();
                        }
                        log::info!(
                            "transport generation {} rebuilt; relay registration and peer reconnect pending; Vnet datagram binding retained",
                            generation + 1
                        );
                        break;
                    }
                    Ok(false) => return,
                    Err(error) => log::warn!(
                        "network transport rebuild failed: {error}; retry in {backoff:?}"
                    ),
                }
            }
        }
    }

    fn rebuild_transport(&self, generation: u64) -> Result<bool, Error> {
        // Serialize token updates with construction/publication. No await or
        // platform callbacks occur while these two ownership locks are held.
        let config = self.config.lock().unwrap();
        if self.closed.load(Ordering::Relaxed) {
            return Ok(false);
        }
        let next = Transport::start(
            &config,
            &self.signer,
            &self.identity,
            &self.trust,
            &self.presence,
            generation,
        )?;
        let mut current = self.transport.write().unwrap();
        if self.closed.load(Ordering::Relaxed) {
            next.sock.shutdown();
            next.endpoint.close(0u32.into(), b"shutdown");
            return Ok(false);
        }
        *current = Some(next);
        self.completed
            .send_replace(self.requested.load(Ordering::Relaxed));
        Ok(true)
    }

    async fn serve_connection(
        self: Arc<Self>,
        peer: DeviceId,
        connection: Connection,
        generation: u64,
    ) {
        if let Some(tx) = self.datagram_tx.clone() {
            let datagram_conn = connection.clone();
            tokio::spawn(async move {
                while let Ok(datagram) = datagram_conn.read_datagram().await {
                    if tx.send((peer, datagram)).await.is_err() {
                        break;
                    }
                }
            });
        }
        loop {
            let (send, mut recv) = match connection.accept_bi().await {
                Ok(stream) => stream,
                Err(error) => {
                    log::debug!("link with {} ended: {error}", peer.short());
                    break;
                }
            };
            let node = self.clone();
            let connection = connection.clone();
            tokio::spawn(async move {
                let hello = match read_hello(&mut recv).await {
                    Ok(hello) => hello,
                    Err(error) => {
                        log::debug!("stream from {} without hello: {error}", peer.short());
                        return;
                    }
                };
                if node.with_generation(generation, || Ok(())).is_err()
                    || connection.close_reason().is_some()
                {
                    return;
                }
                let member = node.trust.is_member(&peer);
                let handler = node.handler.lock().unwrap().clone();
                let allowed = match (hello.service.as_str(), member) {
                    (pair::SERVICE, _) => true,
                    (_, false) => false,
                    (gossip::SERVICE | heartbeat::SERVICE, true) => true,
                    (_, true) if !node.is_service_host(&node.device_id()) => false,
                    (forward::SERVICE, true) => true,
                    (_, true) => handler.is_some(),
                };
                let mut send = send;
                if !allowed {
                    log::info!(
                        "{} refused service {} for {}",
                        if member { "member" } else { "non-member" },
                        hello.service,
                        peer.short()
                    );
                    let _ = send.write_all(&[ACK_REFUSED]).await;
                    let _ = send.finish();
                    return;
                }
                if send.write_all(&[ACK_OK]).await.is_err() {
                    return;
                }
                match hello.service.as_str() {
                    pair::SERVICE => {
                        pair::serve(&node, peer, &connection, send, recv, generation).await
                    }
                    gossip::SERVICE => gossip::serve(&node, send, recv).await,
                    heartbeat::SERVICE => heartbeat::serve(&node, peer, send, recv).await,
                    forward::SERVICE => forward::serve(&node, peer, send, recv).await,
                    _ => {
                        handler
                            .expect("checked above")
                            .handle(node.clone(), peer, hello, send, recv)
                            .await
                    }
                }
            });
        }
        let mut connections = self.connections.lock().unwrap();
        if connections
            .get(&peer)
            .is_some_and(|c| c.stable_id() == connection.stable_id())
        {
            connections.remove(&peer);
            drop(connections);
            // Only the current link's end is news; a replaced one is not.
            if self.trust.is_member(&peer) {
                self.presence.link_lost(peer);
            }
        }
    }

    pub async fn shutdown(&self) {
        self.closed.store(true, Ordering::Relaxed);
        self.repair_wake.notify_one();
        self.completed.send_modify(|_| {});
        let transport = self.transport.write().unwrap().take();
        if let Some(transport) = transport {
            transport.endpoint.close(0u32.into(), b"shutdown");
            // Quinn's failed endpoint driver does not notify idle waiters.
            // Keep the socket alive for normal close packets, but never let
            // an already failed driver hold application shutdown indefinitely.
            let _ = tokio::time::timeout(CONNECT_TIMEOUT, transport.endpoint.wait_idle()).await;
            transport.sock.shutdown();
        }
    }
}

/// Maintain member links with bounded attempts and backoff.
async fn reconnect_loop(node: Weak<Node>) {
    let mut jobs = tokio::task::JoinSet::new();
    let mut pending = HashSet::new();
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        while let Some(result) = jobs.try_join_next() {
            if let Ok(peer) = result {
                pending.remove(&peer);
            }
        }
        let Some(node) = node.upgrade() else { return };
        if node.closed.load(Ordering::Relaxed) {
            return;
        }
        if !node.trust.is_member(&node.device_id()) {
            continue;
        }
        // Without the relay nothing is known about anyone: failures then do
        // not count against members (see `presence`).
        if let Some(sock) = node.sock() {
            node.presence.set_local(match sock.relay_state() {
                RelayState::Registered | RelayState::Unconfigured => crate::presence::Local::Up,
                RelayState::Connecting => crate::presence::Local::Connecting,
                RelayState::Failed => crate::presence::Local::Down,
            });
        }
        // Never hold presence and the roster together: compute who we dial
        // first (peers() takes them one after the other).
        let members = node.peers();
        let hosts: HashSet<DeviceId> = members
            .iter()
            .map(|p| p.id)
            .filter(|id| node.is_service_host(id))
            .collect();
        node.presence.expire(|peer| hosts.contains(peer));
        let peers: Vec<_> = members
            .into_iter()
            .filter(|p| !p.connected && hosts.contains(&p.id) && node.presence.may_dial(&p.id))
            .map(|p| p.id)
            .collect();
        for peer in peers {
            if jobs.len() >= 4 {
                break;
            }
            if pending.contains(&peer) {
                continue;
            }
            let sock = node.sock();
            let due = {
                let mut retries = node.retries.lock().unwrap();
                let retry = retries.entry(peer).or_default();
                let now = Instant::now();
                // A caller may have cancelled a dial before its timeout handler ran.
                if retry.expired(now) {
                    if let Some(sock) = &sock {
                        sock.defer_direct(&peer);
                    }
                    retry.fail(now);
                }
                retry.due(now)
            };
            if !due {
                continue;
            }
            pending.insert(peer);
            let node = node.clone();
            jobs.spawn(async move {
                if let Err(e) = node.connect(peer).await {
                    log::debug!("reconnect {}: {e}", peer.short());
                }
                peer
            });
        }
    }
}

/// How often members the relay reported absent are watched again: a WATCH
/// lasts ten minutes and does not survive a relay restart.
const WATCH_RENEWAL: Duration = Duration::from_secs(60);

/// Acts on presence changes: logs them, and for a member the relay reports
/// absent closes a link that only looks alive and asks the relay to say
/// when it registers.
async fn presence_loop(
    node: Weak<Node>,
    mut changes: mpsc::UnboundedReceiver<crate::presence::Change>,
) {
    use crate::presence::{Reason, State};
    let mut renew = tokio::time::interval(WATCH_RENEWAL);
    renew.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            change = changes.recv() => {
                let Some(change) = change else { return };
                let Some(node) = node.upgrade() else { return };
                log::info!("presence {}: {:?} -> {:?}", change.peer.short(), change.from, change.to);
                if change.to == (State::Offline { reason: Reason::Absent }) {
                    if let Some(connection) = node.connection_to(&change.peer) {
                        connection.close(0u32.into(), b"peer absent");
                    }
                    if let Some(sock) = node.sock() {
                        sock.watch(change.peer);
                    }
                }
            }
            _ = renew.tick() => {
                let Some(node) = node.upgrade() else { return };
                if let Some(sock) = node.sock() {
                    for peer in node.presence.absent_members() {
                        sock.watch(peer);
                    }
                }
            }
        }
    }
}

/// What the relay calls a mailbox item: SHA-256 of its bytes.
fn mailbox_item_hash(item: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    sha2::Sha256::digest(item).into()
}

/// Syncs the roster with the relay mailbox whenever the relay registration
/// comes up, then every `MAILBOX_SYNC_INTERVAL`, until the node is gone.
async fn mailbox_loop(node: Weak<Node>) {
    let mut was_registered = false;
    let mut last_sync: Option<Instant> = None;
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        let Some(node) = node.upgrade() else { return };
        if node.closed.load(Ordering::Relaxed) {
            return;
        }
        let registered = matches!(node.relay_state(), RelayState::Registered);
        let due = registered
            && (!was_registered
                || last_sync.is_none_or(|at| at.elapsed() >= MAILBOX_SYNC_INTERVAL));
        was_registered = registered;
        if !due {
            continue;
        }
        last_sync = Some(Instant::now());
        match node.sync_mailbox().await {
            Ok(0) => {}
            Ok(count) => log::info!("mailbox: {count} roster entries absorbed"),
            Err(error) => log::debug!("mailbox sync: {error}"),
        }
    }
}

/// One-way roster push: heads, their heads, our entries, their entries (dropped).
async fn push_entries(
    connection: &Connection,
    heads: &[muster_core::roster::EntryHash],
    entries: &[Entry],
) -> Result<(), Error> {
    let (mut send, mut recv) = connection.open_bi().await?;
    write_hello(&mut send, &Hello::new(gossip::SERVICE, gossip::VERSION)).await?;
    let mut ack = [0u8; 1];
    recv.read_exact(&mut ack)
        .await
        .map_err(|e| Error::Io(e.to_string()))?;
    if ack[0] != ACK_OK {
        return Err(Error::Unauthorized("roster push refused".into()));
    }
    let mut w = muster_core::canonical::Writer::new();
    w.u16(heads.len() as u16);
    for h in heads {
        w.fixed(&h.0);
    }
    crate::stream::write_message(&mut send, &w.finish()).await?;
    let _ = crate::stream::read_message(&mut recv, crate::stream::MAX_MESSAGE).await?;
    let mut w = muster_core::canonical::Writer::new();
    w.u16(entries.len() as u16);
    for e in entries {
        w.bytes(&e.encode());
    }
    crate::stream::write_message(&mut send, &w.finish()).await?;
    let _ = crate::stream::read_message(&mut recv, crate::stream::MAX_MESSAGE).await?;
    let _ = send.finish();
    Ok(())
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    use crate::stream::{read_message, write_message};
    use muster_core::SoftSigner;
    use muster_core::roster::{Author, DeviceKind, Roster};

    struct Echo;
    impl ServiceHandler for Echo {
        fn handle(
            &self,
            _: Arc<Node>,
            _: DeviceId,
            _: Hello,
            mut send: SendStream,
            mut recv: RecvStream,
        ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
            Box::pin(async move {
                if let Ok(Some(message)) = read_message(&mut recv, 1024).await {
                    let _ = write_message(&mut send, &message).await;
                }
                let _ = send.finish();
            })
        }
    }

    async fn wait(check: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while !check() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("network state converges");
    }

    /// One round trip over a freshly rebuilt transport.
    ///
    /// Both sides are converging while this runs: a connection that is present
    /// can still be the one being retired, so the first attempt may lose its
    /// stream to that retirement. The claim under test is that an RPC succeeds
    /// after recovery, not that the first attempt does, so a lost attempt is
    /// retried inside the same deadline. Anything other than a lost
    /// connection still fails the test immediately.
    async fn echo(node: &Arc<Node>, peer: DeviceId) {
        tokio::time::timeout(CONNECT_TIMEOUT, async {
            loop {
                match echo_once(node, peer).await {
                    Ok(()) => return,
                    Err(error) if retired(&error) => {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                    Err(error) => panic!("RPC after socket recovery: {error}"),
                }
            }
        })
        .await
        .expect("RPC after socket recovery");
    }

    async fn echo_once(node: &Arc<Node>, peer: DeviceId) -> Result<(), Error> {
        let (mut send, mut recv) = node.open(peer, "echo", 1).await?;
        write_message(&mut send, b"after socket recovery").await?;
        let reply = read_message(&mut recv, 1024)
            .await?
            .ok_or_else(|| Error::Io("echo closed without a reply".into()))?;
        assert_eq!(reply, b"after socket recovery");
        Ok(())
    }

    /// A stream lost because the connection carrying it was being replaced.
    fn retired(error: &Error) -> bool {
        let text = error.to_string();
        text.contains("connection lost") || text.contains("closed")
    }

    async fn fixture() -> (
        Arc<Node>,
        Arc<Node>,
        tokio::task::JoinHandle<anyhow::Result<()>>,
    ) {
        let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .is_test(true)
            .try_init();
        let server = muster_relay::RelayServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            muster_relay::RelayOptions::default(),
        )
        .await
        .unwrap();
        let relay = server.udp_addr().unwrap();
        let server = tokio::spawn(server.serve());
        let a: SharedSigner = Arc::new(SoftSigner::generate());
        let b: SharedSigner = Arc::new(SoftSigner::generate());
        let mut roster = Roster::new();
        let genesis = Author {
            roster: &roster,
            signer: a.as_ref(),
        }
        .genesis(
            "recovery",
            ("10.44.0.0".parse().unwrap(), 24),
            DeviceRecord::new(a.public_key(), "a", DeviceKind::Desktop),
        )
        .unwrap();
        roster.insert(genesis).unwrap();
        let add = Author {
            roster: &roster,
            signer: a.as_ref(),
        }
        // This fixture exercises application RPC in both directions.
        .add(DeviceRecord::new(b.public_key(), "b", DeviceKind::Desktop))
        .unwrap();
        roster.insert(add).unwrap();
        let mut nodes = Vec::new();
        for signer in [a, b] {
            let reservation = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
            let bind = reservation.local_addr().unwrap();
            drop(reservation);
            let node = Node::start(
                NodeConfig {
                    bind,
                    relay: RelayConfig::Udp(relay),
                    options: MagicOptions {
                        relay_only: true,
                        mobile: false,
                        ..Default::default()
                    },
                    datagrams: true,
                    build: Default::default(),
                },
                signer,
                Arc::new(RwLock::new(roster.clone())),
                Arc::new(RwLock::new(LocalPolicy::default())),
            )
            .await
            .unwrap();
            node.set_handler(Arc::new(Echo));
            nodes.push(node);
        }
        let b = nodes.pop().unwrap();
        let a = nodes.pop().unwrap();
        wait(|| {
            a.relay_state() == RelayState::Registered
                && b.relay_state() == RelayState::Registered
                && a.connection_to(&b.device_id()).is_some()
                && b.connection_to(&a.device_id()).is_some()
        })
        .await;
        (a, b, server)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn send_only_socket_failure_recovers_without_receive_error() {
        let (a, b, server) = fixture().await;
        let old = a.transport().unwrap();
        let connection = a.connection_to(&b.device_id()).unwrap();
        let remote = b.connection_to(&a.device_id()).unwrap().stable_id();
        old.sock.fail_next_send_for_test(libc::ENOTCONN);
        connection
            .send_datagram(bytes::Bytes::from_static(b"fault injection"))
            .unwrap();
        wait(|| a.transport().is_ok_and(|t| t.generation > old.generation)).await;
        wait(|| {
            a.connection_to(&b.device_id()).is_some()
                && b.connection_to(&a.device_id())
                    .is_some_and(|c| c.stable_id() != remote)
        })
        .await;
        assert_eq!(a.transport().unwrap().generation, old.generation + 1);
        assert!(
            old.endpoint.local_addr().is_err(),
            "old socket released despite retained handles"
        );
        echo(&a, b.device_id()).await;
        echo(&b, a.device_id()).await;
        a.shutdown().await;
        b.shutdown().await;
        server.abort();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn offline_peer_and_unreachable_path_do_not_rebuild_global_transport() {
        let (a, b, server) = fixture().await;
        let generation = a.transport().unwrap().generation;
        let socket = a.sock().unwrap();
        socket.fail_next_send_for_test(libc::EHOSTUNREACH);
        let _ = a
            .connection_to(&b.device_id())
            .unwrap()
            .send_datagram(bytes::Bytes::from_static(b"path failure"));
        b.shutdown().await;
        for _ in 0..20 {
            a.network_changed();
        }
        tokio::time::sleep(Duration::from_secs(16)).await;
        assert_eq!(a.transport().unwrap().generation, generation);
        assert!(
            tokio::time::timeout(Duration::from_millis(50), socket.failed())
                .await
                .is_err()
        );
        a.shutdown().await;
        server.abort();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn invalid_relay_preserves_transport_and_shutdown_cancels_pending_reconfiguration() {
        let (a, b, server) = fixture().await;
        let generation = a.transport().unwrap().generation;
        assert!(
            a.reconfigure_relay(RelayConfig::Secure("invalid".into()))
                .await
                .is_err()
        );
        assert_eq!(a.transport().unwrap().generation, generation);
        let blocker = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        a.config.lock().unwrap().bind = blocker.local_addr().unwrap();
        let node = a.clone();
        let relay = a.config.lock().unwrap().relay.clone();
        let pending = tokio::spawn(async move { node.reconfigure_relay(relay).await });
        wait(|| a.transport().is_err()).await;
        a.shutdown().await;
        assert!(
            tokio::time::timeout(Duration::from_secs(1), pending)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        drop(blocker);
        assert!(a.transport().is_err());
        b.shutdown().await;
        server.abort();
    }

    #[cfg(all(target_os = "macos", feature = "vnet"))]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn vnet_bidirectional_survives_repeated_transport_and_relay_replacement() {
        use crate::vnet::{Vnet, VnetConfig};
        use std::os::fd::{AsRawFd, IntoRawFd};
        async fn tun(node: Arc<Node>) -> (Arc<Vnet>, tokio::net::UnixDatagram, i32) {
            let (device, kernel) = std::os::unix::net::UnixDatagram::pair().unwrap();
            let fd = device.as_raw_fd();
            kernel.set_nonblocking(true).unwrap();
            let kernel = tokio::net::UnixDatagram::from_std(kernel).unwrap();
            let vnet = Vnet::start(
                node,
                VnetConfig {
                    existing_fd: Some(device.into_raw_fd()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            (vnet, kernel, fd)
        }
        async fn packet(
            from: &Vnet,
            source: &tokio::net::UnixDatagram,
            to: &Vnet,
            sink: &tokio::net::UnixDatagram,
        ) {
            let mut frame = (libc::AF_INET as u32).to_be_bytes().to_vec();
            let mut ip = vec![0u8; 40];
            ip[0] = 0x45;
            ip[9] = 1; // ICMP, still subject to source VIP validation.
            ip[12..16].copy_from_slice(&from.address().octets());
            ip[16..20].copy_from_slice(&to.address().octets());
            frame.extend_from_slice(&ip);
            source.send(&frame).await.unwrap();
            let mut actual = [0u8; 1500];
            let n = tokio::time::timeout(Duration::from_secs(3), sink.recv(&mut actual))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(&actual[..n], frame.as_slice());
        }
        let (a, b, server) = fixture().await;
        let replacement_relay = muster_relay::RelayServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            muster_relay::RelayOptions::default(),
        )
        .await
        .unwrap();
        let replacement_address = replacement_relay.udp_addr().unwrap();
        let replacement_server = tokio::spawn(replacement_relay.serve());
        let (va, ka, fda) = tun(a.clone()).await;
        let (vb, kb, fdb) = tun(b.clone()).await;
        let weak_a = Arc::downgrade(&va);
        packet(&va, &ka, &vb, &kb).await;
        packet(&vb, &kb, &va, &ka).await;
        let relay = a.config.lock().unwrap().relay.clone();
        let mut previous_rebuild = None;
        for round in 0..3 {
            let old = a.transport().unwrap();
            let remote = b.connection_to(&a.device_id()).unwrap().stable_id();
            if round == 0 {
                old.sock.fail_next_send_for_test(libc::ENOTCONN);
                a.connection_to(&b.device_id())
                    .unwrap()
                    .send_datagram(bytes::Bytes::from_static(b"fail"))
                    .unwrap();
            } else if round == 1 {
                a.reconfigure_relay(relay.clone()).await.unwrap();
            } else {
                a.reconfigure_relay(RelayConfig::Udp(replacement_address))
                    .await
                    .unwrap();
                b.reconfigure_relay(RelayConfig::Udp(replacement_address))
                    .await
                    .unwrap();
            }
            wait(|| a.transport().is_ok_and(|t| t.generation > old.generation)).await;
            if let Some(previous) = previous_rebuild {
                assert!(
                    Instant::now().duration_since(previous) >= Duration::from_secs(1),
                    "recovery must not spin"
                );
            }
            previous_rebuild = Some(Instant::now());
            wait(|| {
                a.connection_to(&b.device_id()).is_some()
                    && b.connection_to(&a.device_id())
                        .is_some_and(|c| c.stable_id() != remote && c.close_reason().is_none())
            })
            .await;
            echo(&a, b.device_id()).await;
            echo(&b, a.device_id()).await;
            packet(&va, &ka, &vb, &kb).await;
            packet(&vb, &kb, &va, &ka).await;
            assert!(
                a.take_datagrams().is_none(),
                "same Vnet owns the one receiver"
            );
            assert_eq!(a.transport().unwrap().generation, old.generation + 1);
            assert!(old.endpoint.local_addr().is_err());
        }
        // A failed second attachment must close its transferred fd and leave
        // the original packet loops usable, not steal/drop their receiver.
        let (extra, _kernel) = std::os::unix::net::UnixDatagram::pair().unwrap();
        let extra_fd = extra.into_raw_fd();
        assert!(
            Vnet::start(
                a.clone(),
                VnetConfig {
                    existing_fd: Some(extra_fd),
                    ..Default::default()
                }
            )
            .await
            .is_err()
        );
        assert_eq!(unsafe { libc::fcntl(extra_fd, libc::F_GETFD) }, -1);
        assert_eq!(va.stats().rx_packets, 4);
        assert_eq!(vb.stats().rx_packets, 4);
        va.shutdown();
        tokio::join!(va.shutdown_and_wait(), va.shutdown_and_wait());
        vb.shutdown_and_wait().await;
        assert_eq!(unsafe { libc::fcntl(fda, libc::F_GETFD) }, -1);
        assert_eq!(unsafe { libc::fcntl(fdb, libc::F_GETFD) }, -1);
        drop(va);
        assert!(weak_a.upgrade().is_none(), "packet tasks released Vnet");
        replacement_server.abort();
        a.shutdown().await;
        b.shutdown().await;
        server.abort();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn silent_receive_pump_stall_is_detected_without_peer_failure_heuristics() {
        let (a, b, server) = fixture().await;
        let old = a.transport().unwrap();
        let remote = b.connection_to(&a.device_id()).unwrap().stable_id();
        old.sock.stall_receive_for_test();
        tokio::time::timeout(Duration::from_secs(20), async {
            while !a.transport().is_ok_and(|t| t.generation > old.generation) {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("local receive watchdog repairs stalled pump");
        wait(|| {
            b.connection_to(&a.device_id())
                .is_some_and(|c| c.stable_id() != remote)
                && a.connection_to(&b.device_id()).is_some()
        })
        .await;
        echo(&a, b.device_id()).await;
        echo(&b, a.device_id()).await;
        assert_eq!(a.transport().unwrap().generation, old.generation + 1);
        a.shutdown().await;
        b.shutdown().await;
        server.abort();
    }

    async fn fail(node: &Arc<Node>) -> (Transport, tokio::net::UdpSocket) {
        let old = node.transport().unwrap();
        // Force a real bind failure so the recovery gap is observable and
        // shutdown/token updates can race with a pending repair.
        let blocker = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        node.config.lock().unwrap().bind = blocker.local_addr().unwrap();
        // Read the old endpoint's address before injecting the fault. The
        // receive loop can observe the injected failure and release that
        // socket straight away, and then asking it for its own address fails
        // with NotConnected — which is the recovery working, not a fault to
        // report. The address is only needed to deliver the wake packet.
        let address = old.endpoint.local_addr().unwrap();
        old.sock.fail_next_receive_for_test();
        let wake = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        wake.send_to(&[0], address).await.unwrap();
        wait(|| node.transport().is_err()).await;
        (old, blocker)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn fatal_socket_error_rebuilds_same_node_and_rejects_retired_connections() {
        let (a, b, server) = fixture().await;
        // Initial simultaneous dials may still be choosing their shared link.
        // This read-only echo is fixture readiness, before fault injection;
        // recovery echoes below must still succeed without replay.
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let ready = async {
                    let (mut send, mut recv) = a.open(b.device_id(), "echo", 1).await?;
                    write_message(&mut send, b"fixture ready").await?;
                    read_message(&mut recv, 1024).await
                };
                if let Ok(Ok(reply)) = tokio::time::timeout(CONNECT_TIMEOUT, ready).await {
                    assert_eq!(reply.as_deref(), Some(b"fixture ready".as_slice()));
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("fixture RPC ready before socket fault injection");
        let identity = a.device_id();
        let address = a.local_addr().unwrap();
        let old_connection = a.connection_to(&b.device_id()).unwrap();
        let old_remote = b.connection_to(&a.device_id()).unwrap();
        let (old, blocker) = fail(&a).await;
        let old_generation = old.generation;
        assert_eq!(a.relay_state(), RelayState::Failed);
        assert!(a.local_addr().is_err());
        a.set_push_token(Some((1, b"updated during repair".to_vec())));
        a.config.lock().unwrap().bind = address;
        drop(blocker);
        for _ in 0..3 {
            a.network_changed();
        }
        wait(|| {
            a.relay_state() == RelayState::Registered
                && a.connection_to(&b.device_id()).is_some()
                && b.connection_to(&a.device_id())
                    .is_some_and(|c| c.stable_id() != old_remote.stable_id())
        })
        .await;
        assert_eq!(a.transport().unwrap().generation, old_generation + 1);
        assert_eq!(a.device_id(), identity);
        assert_eq!(a.local_addr().unwrap(), address);
        // Even retained old connection/endpoint handles cannot keep the UDP
        // file descriptor alive after retirement.
        assert!(old.endpoint.local_addr().is_err());
        drop(old);
        echo(&a, b.device_id()).await;
        let replacement = a.connection_to(&b.device_id()).unwrap().stable_id();
        a.adopt(b.device_id(), old_connection, old_generation);
        assert_eq!(
            a.connection_to(&b.device_id()).unwrap().stable_id(),
            replacement
        );
        echo(&b, a.device_id()).await;
        a.shutdown().await;
        b.shutdown().await;
        server.abort();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn retired_pairing_cannot_apply_late_approval() {
        let (a, b, server) = fixture().await;
        let mut config = a.config.lock().unwrap().clone();
        config.bind = "127.0.0.1:0".parse().unwrap();
        let signer: SharedSigner = Arc::new(SoftSigner::generate());
        let record = DeviceRecord::new(signer.public_key(), "pending", DeviceKind::Phone);
        let joiner = Node::start(
            config,
            signer,
            Arc::new(RwLock::new(Roster::new())),
            Arc::new(RwLock::new(LocalPolicy::default())),
        )
        .await
        .unwrap();
        let invite = muster_core::Invite::new(
            a.roster().read().unwrap().network_id().unwrap(),
            "recovery",
            a.signer.public_key(),
            "127.0.0.1:1",
            600,
        );
        a.open_invite(invite.clone());
        let pending = Arc::new(Mutex::new(None));
        let captured = pending.clone();
        a.pairing().set_approver(Arc::new(move |candidate| {
            let (tx, rx) = tokio::sync::oneshot::channel();
            *captured.lock().unwrap() = Some((candidate.record, tx));
            rx
        }));
        wait(|| joiner.relay_state() == RelayState::Registered).await;
        let j = joiner.clone();
        let joining = tokio::spawn(async move { j.join(&invite, record).await });
        wait(|| pending.lock().unwrap().is_some()).await;
        let (old, blocker) = fail(&a).await;
        let (record, verdict) = pending.lock().unwrap().take().unwrap();
        let peer = record.device_id;
        // Local approval is cancelled immediately; the remote joiner also
        // needs its old QUIC idle timeout to observe the lost endpoint.
        wait(|| verdict.is_closed()).await;
        assert!(verdict.send(pair::Verdict::Approve(record)).is_err());
        // The joiner sees the retired endpoint only at its link idle timeout,
        // which its own first keepalive after the last packet restarts once.
        tokio::time::timeout(
            crate::tls::LINK_IDLE_TIMEOUT + crate::tls::LINK_KEEPALIVE + Duration::from_secs(5),
            async {
                while !joining.is_finished() {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            },
        )
        .await
        .expect("retired pairing is cancelled");
        assert!(joining.await.unwrap().is_err());
        assert!(!a.trust.is_member(&peer));
        assert!(a.pairing().any_open());
        a.shutdown().await;
        drop((old, blocker));
        joiner.shutdown().await;
        b.shutdown().await;
        server.abort();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn shutdown_during_rebind_never_resurrects_transport() {
        let (a, b, server) = fixture().await;
        let (old, blocker) = fail(&a).await;
        a.shutdown().await;
        drop((old, blocker));
        a.network_changed();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(a.transport().is_err());
        assert!(a.sock().is_none());
        assert!(a.connection_to(&b.device_id()).is_none());
        b.shutdown().await;
        server.abort();
    }
}
