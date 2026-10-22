//! Authenticated, certificate-verified QUIC/UDP and TLS/TCP relay carriers.
//! Selection lives here, not in the platform UI. Carrier recovery never queues
//! or replays operations across generations; inner QUIC owns packet recovery.
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use muster_core::SharedSigner;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;

use crate::magicsock::{PlainUdp, RelayState};
use crate::reconnect::CONNECT_TIMEOUT;
use crate::relay_proto::{FEATURES_PRESENCE, Frame, MAX_FRAME, auth_signature};
use crate::relay_tls;

const CAPACITY: usize = 256;
const PROBE_INTERVAL: Duration = Duration::from_secs(5);
const PROBE_RETRY: Duration = Duration::from_secs(1);
const MIN_DWELL: Duration = Duration::from_secs(10);

/// How a device keeps its relay carriers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strategy {
    /// Desktops (D13): both carriers connected and probed every 5 s; the
    /// healthier one carries, the other stands by.
    Standby,
    /// Phones: one carrier at a time, QUIC first and TLS only while QUIC
    /// cannot connect; a lost or reset connection starts over with QUIC.
    /// No periodic probes (a carrier is live while its connection is),
    /// activation repeated only until the relay acknowledges it, sparse QUIC
    /// keepalives. Fewer packets for a phone's radio, slower failover. A
    /// network change verifies the live carrier with one probe instead of
    /// tearing it down: a phone's path updates are frequent and most change
    /// nothing a carrier depends on.
    Fallback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Carrier {
    Quic,
    Tls,
}
impl Carrier {
    fn index(self) -> usize {
        match self {
            Self::Quic => 0,
            Self::Tls => 1,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Quic => "quic/udp",
            Self::Tls => "tls/tcp",
        }
    }
}

#[derive(Clone, Debug, Default)]
struct Health {
    ready: bool,
    generation: u64,
    last_ack: Option<Instant>,
    rtt: Option<Duration>,
    successes: u32,
    failures: u32,
}
impl Health {
    /// Probed carriers need a recent probe answer; unprobed ones are live
    /// while their connection is.
    fn live(&self, now: Instant, probing: bool) -> bool {
        self.ready
            && (!probing
                || self.last_ack.is_some_and(|at| {
                    now.saturating_duration_since(at) < PROBE_INTERVAL + CONNECT_TIMEOUT
                }))
    }
    fn ack(&mut self, now: Instant, rtt: Duration) {
        self.ready = true;
        self.last_ack = Some(now);
        self.rtt = Some(self.rtt.map_or(rtt, |old| (old * 3 + rtt) / 4));
        self.successes = self.successes.saturating_add(1);
        self.failures = 0;
    }
    fn fail(&mut self) {
        self.ready = false;
        self.successes = 0;
        self.failures = self.failures.saturating_add(1);
    }
}

#[derive(Debug)]
struct Selection {
    paths: [Health; 2],
    active: Option<Carrier>,
    switched: Instant,
    started: Instant,
    probing: bool,
    /// The carrier and generation the relay acknowledged an Activate on.
    activated: Option<(Carrier, u64)>,
}
impl Selection {
    fn new(probing: bool) -> Self {
        let now = Instant::now();
        Self {
            paths: Default::default(),
            active: None,
            switched: now,
            started: now,
            probing,
            activated: None,
        }
    }
    fn live(&self, carrier: Carrier, now: Instant) -> bool {
        self.paths[carrier.index()].live(now, self.probing)
    }
    fn choose(&mut self, now: Instant) -> Option<Carrier> {
        let q = self.live(Carrier::Quic, now);
        let t = self.live(Carrier::Tls, now);
        let best = match (q, t) {
            (true, true) => {
                let qr = self.paths[0].rtt.unwrap_or(CONNECT_TIMEOUT);
                let tr = self.paths[1].rtt.unwrap_or(CONNECT_TIMEOUT);
                // Prefer datagrams unless TCP is materially faster.
                Some(if qr <= tr.mul_f64(1.25) {
                    Carrier::Quic
                } else {
                    Carrier::Tls
                })
            }
            (true, false) => Some(Carrier::Quic),
            (false, true) => Some(Carrier::Tls),
            _ => None,
        };
        let next = match (self.active, best) {
            (Some(current), Some(candidate)) if current != candidate && self.live(current, now) => {
                let stable = self.paths[candidate.index()].successes >= 3
                    && now.saturating_duration_since(self.switched) >= MIN_DWELL;
                if stable {
                    Some(candidate)
                } else {
                    Some(current)
                }
            }
            _ => best,
        };
        if self.active != next {
            log::info!("relay carrier {:?} -> {:?}", self.active, next);
            self.active = next;
            self.switched = now;
        }
        next
    }
    fn status(&mut self) -> RelayState {
        if self.choose(Instant::now()).is_some() {
            RelayState::Registered
        } else if self.started.elapsed() >= CONNECT_TIMEOUT {
            RelayState::Failed
        } else {
            RelayState::Connecting
        }
    }
}

struct Outbound {
    generation: u64,
    bytes: Vec<u8>,
}

pub struct SecureRelay {
    tx: [mpsc::Sender<Outbound>; 2],
    rx: Mutex<mpsc::Receiver<Vec<u8>>>,
    state: Arc<Mutex<Selection>>,
    reset: watch::Sender<u64>,
    /// Probe the live carrier now (a phone's network change).
    verify: watch::Sender<u64>,
    strategy: Strategy,
    push_token: Arc<Mutex<Option<Vec<u8>>>>,
    task: tokio::task::AbortHandle,
}
impl std::fmt::Debug for SecureRelay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecureRelay")
            .field("active", &self.active())
            .finish()
    }
}
impl Drop for SecureRelay {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl SecureRelay {
    pub(crate) fn validate_authority(authority: &str) -> anyhow::Result<(String, u16)> {
        let (host, port) = authority
            .rsplit_once(':')
            .ok_or_else(|| anyhow::anyhow!("relay requires host:port"))?;
        let host = host.trim_matches(['[', ']']).to_owned();
        let port: u16 = port.parse()?;
        anyhow::ensure!(!host.is_empty() && port != 0, "invalid relay authority");
        Ok((host, port))
    }

    pub fn new(
        authority: String,
        signer: SharedSigner,
        strategy: Strategy,
    ) -> anyhow::Result<Self> {
        Self::with_config(authority, signer, relay_tls::client_config()?, strategy)
    }
    /// Explicit trust configuration for integration tests / private PKI. The
    /// production entry point above always uses public WebPKI roots.
    pub fn with_config(
        authority: String,
        signer: SharedSigner,
        tls: rustls::ClientConfig,
        strategy: Strategy,
    ) -> anyhow::Result<Self> {
        let (host, port) = Self::validate_authority(&authority)?;
        let state = Arc::new(Mutex::new(Selection::new(strategy == Strategy::Standby)));
        let push_token = Arc::new(Mutex::new(None));
        let (qtx, qrx) = mpsc::channel(CAPACITY);
        let (ttx, trx) = mpsc::channel(CAPACITY);
        let (incoming, rx) = mpsc::channel(CAPACITY);
        let (reset, _) = watch::channel(0);
        let (verify, _) = watch::channel(0);
        let mut tasks = JoinSet::new();
        let [quic, tls] =
            [(Carrier::Quic, qrx), (Carrier::Tls, trx)].map(|(carrier, outgoing)| Worker {
                carrier,
                strategy,
                host: host.clone(),
                port,
                signer: signer.clone(),
                tls: Arc::new(tls.clone()),
                state: state.clone(),
                push_token: push_token.clone(),
                incoming: incoming.clone(),
                outgoing,
                reset: reset.subscribe(),
                verify: verify.subscribe(),
            });
        match strategy {
            Strategy::Standby => {
                tasks.spawn(worker(quic));
                tasks.spawn(worker(tls));
            }
            Strategy::Fallback => {
                tasks.spawn(fallback(quic, tls));
            }
        }
        // Explicit, idempotent return-path activation. Repeat periodically:
        // a QUIC control datagram may be lost, including its first activation.
        // A phone stops repeating once the relay acknowledged it.
        let selection = state.clone();
        let activation_tx = [qtx.clone(), ttx.clone()];
        let id = signer.public_key().device_id();
        let period = match strategy {
            Strategy::Standby => Duration::from_millis(100),
            Strategy::Fallback => Duration::from_millis(500),
        };
        tasks.spawn(async move {
            let mut last = None;
            let mut sent = Instant::now();
            let mut tick = tokio::time::interval(period);
            loop {
                tick.tick().await;
                let (choice, acknowledged) = {
                    let mut state = selection.lock().unwrap();
                    let choice = state
                        .choose(Instant::now())
                        .map(|carrier| (carrier, state.paths[carrier.index()].generation));
                    let acknowledged = strategy == Strategy::Fallback
                        && choice.is_some()
                        && state.activated == choice;
                    (choice, acknowledged)
                };
                if let Some((carrier, generation)) = choice {
                    if !acknowledged
                        && (last != choice || sent.elapsed() >= Duration::from_secs(1))
                        && activation_tx[carrier.index()]
                            .try_send(Outbound {
                                generation,
                                bytes: Frame::Activate { id }.encode(),
                            })
                            .is_ok()
                    {
                        last = choice;
                        sent = Instant::now();
                    }
                } else {
                    last = None;
                }
            }
        });
        let task = tokio::spawn(async move { while tasks.join_next().await.is_some() {} });
        Ok(Self {
            tx: [qtx, ttx],
            rx: Mutex::new(rx),
            state,
            reset,
            verify,
            strategy,
            push_token,
            task: task.abort_handle(),
        })
    }
    pub fn carrier_ready(&self, carrier: Carrier) -> bool {
        self.state.lock().unwrap().live(carrier, Instant::now())
    }
    pub fn carrier_rtt(&self, carrier: Carrier) -> Option<Duration> {
        let state = self.state.lock().unwrap();
        state
            .live(carrier, Instant::now())
            .then_some(state.paths[carrier.index()].rtt)
            .flatten()
    }
    pub fn active(&self) -> Option<Carrier> {
        self.state.lock().unwrap().choose(Instant::now())
    }
    pub fn status(&self) -> RelayState {
        self.state.lock().unwrap().status()
    }
    pub fn shutdown(&self) {
        self.task.abort();
    }
    pub fn set_push_token(&self, token: Option<(u8, Vec<u8>)>, id: muster_core::DeviceId) {
        let bytes = token.map(|(kind, token)| Frame::PushReg { id, kind, token }.encode());
        *self.push_token.lock().unwrap() = bytes.clone();
        if let Some(bytes) = bytes {
            let state = self.state.lock().unwrap();
            for carrier in [Carrier::Quic, Carrier::Tls] {
                if state.live(carrier, Instant::now()) {
                    let _ = self.tx[carrier.index()].try_send(Outbound {
                        generation: state.paths[carrier.index()].generation,
                        bytes: bytes.clone(),
                    });
                }
            }
        }
    }
    pub fn network_changed(&self) {
        let mut state = self.state.lock().unwrap();
        // A phone with a live carrier verifies it (one probe, 3 s) rather
        // than dropping it: the old reset left no route for a second or more
        // on every path update, foreground and push.
        // On TLS (QUIC could not connect earlier) it still starts over with
        // QUIC, the carrier it prefers.
        if self.strategy == Strategy::Fallback
            && state.choose(Instant::now()) == Some(Carrier::Quic)
        {
            drop(state);
            self.verify.send_modify(|epoch| *epoch += 1);
            return;
        }
        for p in &mut state.paths {
            p.fail();
            p.generation += 1;
        }
        state.started = Instant::now();
        state.choose(Instant::now());
        self.reset.send_modify(|epoch| *epoch += 1);
    }
    pub fn send(&self, bytes: Vec<u8>) -> io::Result<()> {
        let mut state = self.state.lock().unwrap();
        let Some(active) = state.choose(Instant::now()) else {
            // A missing carrier is packet loss, not fatal to the peer endpoint.
            return Ok(());
        };
        let generation = state.paths[active.index()].generation;
        match self.tx[active.index()].try_send(Outbound { generation, bytes }) {
            Ok(()) => Ok(()),
            // Bounded datagram queue: drop rather than retaining stale packets
            // across a transport reconnect. Inner QUIC detects and recovers loss.
            Err(mpsc::error::TrySendError::Full(_)) => Ok(()),
            Err(mpsc::error::TrySendError::Closed(_)) => {
                state.paths[active.index()].fail();
                state.choose(Instant::now());
                Ok(())
            }
        }
    }
    pub fn poll_recv(&self, cx: &mut Context<'_>) -> Poll<Option<Vec<u8>>> {
        self.rx.lock().unwrap().poll_recv(cx)
    }
}

// Quinn's default Tokio adapter retries *every* recv error in a synchronous
// loop. A socket invalidated by iOS can therefore monopolize a runtime worker
// even after its connection attempt was cancelled. Share our error-propagating
// socket, and retire it on every exit (including handshake timeout/cancellation).
struct RelayEndpoint {
    endpoint: quinn::Endpoint,
    socket: Arc<PlainUdp>,
}
impl RelayEndpoint {
    fn bind(addr: SocketAddr) -> io::Result<Self> {
        let socket = PlainUdp::bind(addr)?;
        let endpoint = quinn::Endpoint::new_with_abstract_socket(
            quinn::EndpointConfig::default(),
            None,
            socket.clone(),
            Arc::new(quinn::TokioRuntime),
        )?;
        Ok(Self { endpoint, socket })
    }
}
impl Drop for RelayEndpoint {
    fn drop(&mut self) {
        self.endpoint.close(0u32.into(), b"carrier retired");
        self.socket.retire();
    }
}

struct Wire {
    tx: mpsc::Sender<Vec<u8>>,
    rx: mpsc::Receiver<Vec<u8>>,
    _tasks: JoinSet<()>,
    quic: Option<(RelayEndpoint, quinn::Connection)>,
}
impl Drop for Wire {
    fn drop(&mut self) {
        if let Some((endpoint, conn)) = &self.quic {
            conn.close(0u32.into(), b"carrier retired");
            endpoint.endpoint.close(0u32.into(), b"carrier retired");
        }
    }
}
impl Wire {
    async fn send(&self, bytes: Vec<u8>) -> anyhow::Result<()> {
        anyhow::ensure!(
            !bytes.is_empty() && bytes.len() <= MAX_FRAME,
            "invalid relay frame size"
        );
        self.tx
            .send(bytes)
            .await
            .map_err(|_| anyhow::anyhow!("relay writer closed"))
    }
    async fn recv(&mut self) -> anyhow::Result<Vec<u8>> {
        self.rx
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("relay reader closed"))
    }
}

async fn connect(
    carrier: Carrier,
    host: &str,
    port: u16,
    tls: Arc<rustls::ClientConfig>,
    liveness: relay_tls::Liveness,
) -> anyhow::Result<Wire> {
    let addresses = crate::doh::resolve(host, port).await?;
    let mut last = anyhow::anyhow!("no relay address");
    for addr in addresses {
        match tokio::time::timeout(
            CONNECT_TIMEOUT / 2,
            connect_address(carrier, host, addr, tls.clone(), liveness),
        )
        .await
        {
            Ok(Ok(wire)) => return Ok(wire),
            Ok(Err(error)) => last = error,
            Err(_) => last = anyhow::anyhow!("relay address handshake timeout"),
        }
    }
    Err(last)
}
async fn connect_address(
    carrier: Carrier,
    host: &str,
    addr: SocketAddr,
    tls: Arc<rustls::ClientConfig>,
    liveness: relay_tls::Liveness,
) -> anyhow::Result<Wire> {
    let (tx, mut outgoing) = mpsc::channel::<Vec<u8>>(CAPACITY);
    let (incoming, rx) = mpsc::channel(CAPACITY);
    let mut tasks = JoinSet::new();
    let quic = match carrier {
        Carrier::Tls => {
            let stream = tokio::net::TcpStream::connect(addr).await?;
            stream.set_nodelay(true)?;
            let name = rustls::pki_types::ServerName::try_from(host.to_owned())?;
            let stream = tokio_rustls::TlsConnector::from(tls)
                .connect(name, stream)
                .await?;
            anyhow::ensure!(
                stream.get_ref().1.alpn_protocol() == Some(relay_tls::RELAY_ALPN),
                "relay ALPN mismatch"
            );
            let (mut reader, mut writer) = tokio::io::split(stream);
            tasks.spawn(async move {
                while let Ok(bytes) = relay_tls::read_frame(&mut reader).await {
                    if incoming.send(bytes).await.is_err() {
                        break;
                    }
                }
            });
            tasks.spawn(async move {
                while let Some(bytes) = outgoing.recv().await {
                    if !matches!(
                        tokio::time::timeout(
                            CONNECT_TIMEOUT,
                            relay_tls::write_frame(&mut writer, &bytes)
                        )
                        .await,
                        Ok(Ok(()))
                    ) {
                        break;
                    }
                }
            });
            None
        }
        Carrier::Quic => {
            let bind = if addr.is_ipv4() {
                "0.0.0.0:0"
            } else {
                "[::]:0"
            };
            let mut endpoint = RelayEndpoint::bind(bind.parse()?)?;
            endpoint
                .endpoint
                .set_default_client_config(relay_tls::quic_client_config(&tls, liveness)?);
            let conn = endpoint.endpoint.connect(addr, host)?.await?;
            anyhow::ensure!(
                conn.max_datagram_size().is_some_and(|max| max >= MAX_FRAME),
                "relay QUIC MTU cannot carry bounded frames; use TCP"
            );
            let recv_conn = conn.clone();
            tasks.spawn(async move {
                while let Ok(bytes) = recv_conn.read_datagram().await {
                    if bytes.len() > MAX_FRAME || incoming.send(bytes.to_vec()).await.is_err() {
                        break;
                    }
                }
            });
            let send_conn = conn.clone();
            tasks.spawn(async move {
                while let Some(bytes) = outgoing.recv().await {
                    if !matches!(
                        tokio::time::timeout(
                            CONNECT_TIMEOUT,
                            send_conn.send_datagram_wait(bytes.into())
                        )
                        .await,
                        Ok(Ok(()))
                    ) {
                        send_conn.close(0u32.into(), b"datagram send failed");
                        break;
                    }
                }
            });
            Some((endpoint, conn))
        }
    };
    Ok(Wire {
        tx,
        rx,
        _tasks: tasks,
        quic,
    })
}

struct Worker {
    carrier: Carrier,
    strategy: Strategy,
    host: String,
    port: u16,
    signer: SharedSigner,
    tls: Arc<rustls::ClientConfig>,
    state: Arc<Mutex<Selection>>,
    push_token: Arc<Mutex<Option<Vec<u8>>>>,
    incoming: mpsc::Sender<Vec<u8>>,
    outgoing: mpsc::Receiver<Outbound>,
    reset: watch::Receiver<u64>,
    verify: watch::Receiver<u64>,
}
impl Worker {
    async fn open(&self) -> anyhow::Result<(Wire, Vec<u8>)> {
        let liveness = match self.strategy {
            Strategy::Standby => relay_tls::Liveness::Fast,
            Strategy::Fallback => relay_tls::Liveness::Sparse,
        };
        let mut wire = connect(
            self.carrier,
            &self.host,
            self.port,
            self.tls.clone(),
            liveness,
        )
        .await?;
        let id = self.signer.public_key().device_id();
        wire.send(Frame::Hello { id }.encode()).await?;
        let Frame::Challenge { nonce } = Frame::decode(&wire.recv().await?)? else {
            anyhow::bail!("expected relay challenge")
        };
        let signer = self.signer.clone();
        let signature =
            tokio::task::spawn_blocking(move || auth_signature(signer.as_ref(), &nonce)).await??;
        wire.send(
            Frame::Auth {
                id,
                key: self.signer.public_key(),
                nonce,
                signature,
            }
            .encode(),
        )
        .await?;
        let bytes = wire.recv().await?;
        anyhow::ensure!(
            matches!(Frame::decode(&bytes)?, Frame::Reflect { .. }),
            "relay registration refused"
        );
        // A tiny authentication response does not prove usable UDP MTU.
        // Probe and ACK are padded to MAX_FRAME and must both cross the path.
        let nonce = rand::random::<u64>();
        wire.send(Frame::Probe { id, nonce }.encode()).await?;
        loop {
            match Frame::decode(&wire.recv().await?)? {
                Frame::ProbeAck { nonce: received } if received == nonce => break,
                Frame::Error { text } => anyhow::bail!("relay probe rejected: {text}"),
                // Forwarded traffic can arrive after AUTH; the inner peer
                // protocol recovers these datagrams if not yet ready.
                _ => {}
            }
        }
        let push_token = self.push_token.lock().unwrap().clone();
        if let Some(bytes) = push_token {
            wire.send(bytes).await?;
        }
        // ABSENT / PRESENT on this carrier. An older relay counts the frame
        // as dropped and carries on.
        wire.send(
            Frame::Features {
                bits: FEATURES_PRESENCE,
            }
            .encode(),
        )
        .await?;
        Ok((wire, bytes))
    }
    fn ack(&self, generation: u64, rtt: Duration) {
        let mut state = self.state.lock().unwrap();
        let p = &mut state.paths[self.carrier.index()];
        if p.generation == generation {
            p.ack(Instant::now(), rtt);
        }
        state.choose(Instant::now());
    }
    /// Carries frames until the connection fails (an error) or the network
    /// changes (`Ok`).
    async fn run(&mut self, wire: &mut Wire, generation: u64) -> anyhow::Result<()> {
        let probing = self.strategy == Strategy::Standby;
        let mut next_probe = Instant::now() + PROBE_INTERVAL;
        let mut pending = None::<(u64, Instant)>;
        let mut retry_at = next_probe;
        self.verify.borrow_and_update();
        loop {
            let deadline =
                pending.map_or(next_probe, |(_, at)| (at + CONNECT_TIMEOUT).min(retry_at));
            let id = self.signer.public_key().device_id();
            tokio::select! {
                changed = self.reset.changed() => { let _ = changed; return Ok(()) }
                // A network change: is this carrier still there?
                changed = self.verify.changed(), if pending.is_none() => {
                    let _ = changed;
                    let nonce = rand::random::<u64>();
                    wire.tx.try_send(Frame::Probe { id, nonce }.encode())
                        .map_err(|_| anyhow::anyhow!("relay control queue unavailable"))?;
                    let sent = Instant::now();
                    pending = Some((nonce, sent));
                    retry_at = sent + PROBE_RETRY;
                }
                _ = tokio::time::sleep_until(deadline.into()), if probing || pending.is_some() => {
                    let now = Instant::now();
                    let (nonce, sent) = pending.unwrap_or_else(|| (rand::random::<u64>(), now));
                    // QUIC DATAGRAM does not retransmit a lost probe/ACK. Retry
                    // only the same nonce inside the original three-second
                    // window; unrelated traffic and stale ACKs cannot extend it.
                    anyhow::ensure!(now < sent + CONNECT_TIMEOUT, "relay keepalive exceeded 3s");
                    wire.tx.try_send(Frame::Probe { id, nonce }.encode())
                        .map_err(|_| anyhow::anyhow!("relay control queue unavailable"))?;
                    pending = Some((nonce, sent));
                    retry_at = now + PROBE_RETRY;
                }
                incoming = wire.recv() => {
                    let bytes = incoming?;
                    // A newer relay's frame this client does not know is
                    // not a broken carrier.
                    let Ok(frame) = Frame::decode(&bytes) else { continue };
                    match frame {
                        Frame::ProbeAck { nonce } => {
                            if pending.is_some_and(|(expected,_)| nonce == expected) {
                                let (_,at) = pending.take().expect("checked");
                                anyhow::ensure!(at.elapsed() < CONNECT_TIMEOUT, "relay keepalive exceeded 3s");
                                self.ack(generation,at.elapsed());
                                next_probe = Instant::now() + PROBE_INTERVAL;
                            }
                        }
                        // Activation ACK, never a probe RTT sample.
                        Frame::Reflect { .. } => {
                            self.state.lock().unwrap().activated = Some((self.carrier, generation));
                        }
                        Frame::Error { text } => anyhow::bail!("relay rejected carrier: {text}"),
                        Frame::Challenge { .. } => anyhow::bail!("unexpected relay challenge"),
                        _ => {
                            // Do not block keepalive behind a stalled consumer.
                            let _ = self.incoming.try_send(bytes);
                        }
                    }
                }
                outbound = self.outgoing.recv() => {
                    let Some(outbound) = outbound else { anyhow::bail!("relay stopped") };
                    if outbound.generation == generation {
                        match wire.tx.try_send(outbound.bytes) {
                            Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => {},
                            Err(mpsc::error::TrySendError::Closed(_)) => anyhow::bail!("relay writer closed"),
                        }
                    }
                }
            }
        }
    }
}
/// How one connection attempt ended.
enum Ended {
    /// Authenticated, then lost.
    Lost(anyhow::Error),
    /// Never authenticated.
    Failed(anyhow::Error),
    /// The network changed: start again at once.
    Reset,
}

impl Worker {
    /// One connection: opens it (bounded by CONNECT_TIMEOUT), carries frames
    /// until it ends, then marks the carrier failed.
    async fn session(&mut self) -> Ended {
        while self.outgoing.try_recv().is_ok() {}
        // A change seen before this attempt is what it starts from.
        self.reset.borrow_and_update();
        let generation = {
            let mut state = self.state.lock().unwrap();
            let p = &mut state.paths[self.carrier.index()];
            p.generation += 1;
            p.ready = false;
            p.generation
        };
        let started = Instant::now();
        let mut opening_reset = self.reset.clone();
        let opened = tokio::select! {
            biased;
            _ = opening_reset.changed() => {
                self.reset.borrow_and_update();
                return Ended::Reset;
            }
            result = tokio::time::timeout(CONNECT_TIMEOUT, self.open()) => result,
        };
        let ended = match opened {
            Ok(Ok((mut wire, reflect))) => {
                log::info!(
                    "relay {} authenticated {}:{}",
                    self.carrier.label(),
                    self.host,
                    self.port
                );
                self.ack(generation, started.elapsed());
                // REFLECT belongs to this outer socket, not inner PeerLink.
                let _ = reflect;
                match self.run(&mut wire, generation).await {
                    Ok(()) => Ended::Reset,
                    Err(error) => Ended::Lost(error),
                }
            }
            Ok(Err(error)) => Ended::Failed(error),
            Err(_) => Ended::Failed(anyhow::anyhow!("relay connect/register exceeded 3s")),
        };
        {
            let mut state = self.state.lock().unwrap();
            let p = &mut state.paths[self.carrier.index()];
            if p.generation == generation {
                p.fail();
            }
            state.choose(Instant::now());
        }
        if let Ended::Lost(error) | Ended::Failed(error) = &ended {
            log::warn!("relay {} unavailable: {error}", self.carrier.label());
        }
        ended
    }

    /// Waits `delay` or until the network changes; true when it changed.
    async fn pause(&mut self, delay: Duration) -> bool {
        tokio::select! {
            _ = self.reset.changed() => true,
            _ = tokio::time::sleep(delay) => false,
        }
    }
}

/// A desktop's carrier: reconnects on its own, backing off while it fails.
async fn worker(mut w: Worker) {
    let mut failures = 0u32;
    loop {
        match w.session().await {
            Ended::Reset => continue,
            Ended::Lost(_) => failures = 0,
            Ended::Failed(_) => {}
        }
        let delay = retry_delay(failures);
        failures = failures.saturating_add(1);
        if w.pause(delay).await {
            failures = 0;
        }
    }
}

/// A phone's carriers: QUIC first, TLS only while QUIC cannot connect. A
/// lost connection or a network change starts over with QUIC; when both
/// fail, the attempts back off.
async fn fallback(mut quic: Worker, mut tls: Worker) {
    let mut failures = 0u32;
    loop {
        match quic.session().await {
            Ended::Reset => continue,
            Ended::Lost(_) => {
                failures = 0;
                quic.pause(retry_delay(0)).await;
                continue;
            }
            Ended::Failed(_) => {}
        }
        match tls.session().await {
            Ended::Reset => continue,
            Ended::Lost(_) => {
                failures = 0;
                quic.pause(retry_delay(0)).await;
                continue;
            }
            Ended::Failed(_) => {}
        }
        let delay = retry_delay(failures);
        failures = failures.saturating_add(1);
        if quic.pause(delay).await {
            failures = 0;
        }
    }
}

// An extended outage must not keep both carriers doing TLS/DNS handshakes
// every eight seconds on a phone. A network-change reset still wakes either
// carrier immediately, and a healthy connection resets the failure count.
fn retry_delay(failures: u32) -> Duration {
    Duration::from_secs((1u64 << failures.min(6)).min(60))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancelled_relay_endpoint_retires_socket_despite_stale_quinn_handles() {
        use quinn::AsyncUdpSocket;
        for bind in ["127.0.0.1:0", "[::1]:0"] {
            let endpoint = RelayEndpoint::bind(bind.parse().unwrap()).unwrap();
            let socket = endpoint.socket.clone();
            let address = socket.local_addr().unwrap();
            let stale_endpoint = endpoint.endpoint.clone();
            let mut writable = socket.clone().create_io_poller();
            drop(endpoint); // Also the cancellation path while connect() is pending.
            let mut cx = Context::from_waker(std::task::Waker::noop());
            let mut bytes = [0; 1500];
            let mut bufs = [std::io::IoSliceMut::new(&mut bytes)];
            let mut meta = [quinn::udp::RecvMeta::default()];
            assert!(matches!(socket.poll_recv(&mut cx, &mut bufs, &mut meta),
            Poll::Ready(Err(error)) if error.kind() == io::ErrorKind::NotConnected));
            assert!(matches!(writable.as_mut().poll_writable(&mut cx),
            Poll::Ready(Err(error)) if error.kind() == io::ErrorKind::NotConnected));
            let rebound = std::net::UdpSocket::bind(address).unwrap();
            drop((rebound, stale_endpoint));
        }
    }

    #[test]
    fn prolonged_outage_bounds_handshakes_and_recovers_initial_retry_after_reset() {
        let minute: u64 = (0..7).map(|n| retry_delay(n).as_secs()).sum();
        assert_eq!(minute, 123);
        assert_eq!(retry_delay(u32::MAX), Duration::from_secs(60));
        assert_eq!(retry_delay(0), Duration::from_secs(1));
    }
    #[test]
    fn selection_failure_hysteresis_and_rtt() {
        let mut s = Selection::new(true);
        let t = s.started;
        s.paths[1].ack(t, Duration::from_millis(40));
        assert_eq!(s.choose(t), Some(Carrier::Tls));
        s.paths[0].ack(t, Duration::from_millis(10));
        assert_eq!(s.choose(t), Some(Carrier::Tls));
        for dt in [5, 10] {
            let now = t + Duration::from_secs(dt);
            s.paths[0].ack(now, Duration::from_millis(10));
            s.paths[1].ack(now, Duration::from_millis(40));
        }
        let now = t + MIN_DWELL;
        assert_eq!(s.choose(now), Some(Carrier::Quic));
        s.paths[0].fail();
        assert_eq!(s.choose(now), Some(Carrier::Tls));
        s.paths[1].fail();
        assert_eq!(s.choose(now), None);
    }
    #[test]
    fn an_unprobed_carrier_is_live_while_connected() {
        let mut s = Selection::new(false);
        let t = s.started;
        assert_eq!(s.choose(t), None);
        s.paths[0].ack(t, Duration::from_millis(30));
        let much_later = t + PROBE_INTERVAL * 10;
        assert_eq!(s.choose(much_later), Some(Carrier::Quic));
        s.paths[0].fail();
        assert_eq!(s.choose(much_later), None);
    }

    #[test]
    fn stale_health_is_not_success_and_tcp_can_win() {
        let mut s = Selection::new(true);
        let t = s.started;
        s.paths[0].ack(t, Duration::from_millis(150));
        s.paths[1].ack(t, Duration::from_millis(20));
        assert_eq!(s.choose(t), Some(Carrier::Tls));
        assert_eq!(s.choose(t + PROBE_INTERVAL + CONNECT_TIMEOUT), None);
    }
}

#[cfg(test)]
#[path = "secure_relay_tests.rs"]
mod integration;
