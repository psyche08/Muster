//! One UDP socket that presents every peer as a stable virtual address while
//! sending each datagram either straight to the peer or through the relay
//! (ARCHITECTURE §3.5). quinn never sees a path change. Direct paths are
//! learned by exchanging candidate addresses over the relay and punching
//! with PING/PONG; the first answering address becomes the send path and
//! stays until it times out. Proven in the P0 relay POC, hardened here:
//! control frames are told apart from QUIC by length, type byte and a known
//! peer id (QUIC bit greasing is off), the socket does no GRO/GSO, and every
//! address a peer is seen at maps inbound packets to it.

use std::collections::HashMap;
use std::fmt::Debug;
use std::io::{self, IoSliceMut};
use std::net::{Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use muster_core::identity::{DeviceId, SharedSigner};
use quinn::udp::{RecvMeta, Transmit};
use quinn::{AsyncUdpSocket, UdpPoller};
use tokio::sync::oneshot;

use crate::Error;
use crate::reconnect::{CONNECT_TIMEOUT, Retry};
use crate::relay_proto::{self as rp, Frame};

const PING: u8 = 0x10;
const PONG: u8 = 0x11;
const CONTROL_LEN: usize = 1 + 32 + 8;
// Probe the size QUIC starts with, not just a 41-byte control packet.
const PATH_PROBE_LEN: usize = 1350;

const PING_INTERVAL: Duration = Duration::from_millis(1500);
const PUNCH_INTERVAL: Duration = Duration::from_millis(400);
const DIRECT_TIMEOUT: Duration = Duration::from_secs(6);
const CANDIDATE_INTERVAL: Duration = Duration::from_secs(2);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);

// ------------------------------------------------------------ plain socket

pub(crate) struct PlainUdp {
    io: Mutex<Option<Arc<tokio::net::UdpSocket>>>,
    failure: tokio::sync::watch::Sender<Option<String>>,
    #[cfg(test)]
    fail_send: std::sync::atomic::AtomicI32,
    pollers: Mutex<Vec<Weak<PendingWrite>>>,
}

impl Debug for PlainUdp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlainUdp")
            .field("local_addr", &self.local_addr())
            .finish()
    }
}

impl PlainUdp {
    pub(crate) fn bind(addr: SocketAddr) -> io::Result<Arc<Self>> {
        let socket = std::net::UdpSocket::bind(addr)?;
        set_dont_fragment(&socket)?;
        socket.set_nonblocking(true)?;
        Ok(Arc::new(Self {
            io: Mutex::new(Some(Arc::new(tokio::net::UdpSocket::from_std(socket)?))),
            pollers: Mutex::new(Vec::new()),
            failure: tokio::sync::watch::channel(None).0,
            #[cfg(test)]
            fail_send: std::sync::atomic::AtomicI32::new(0),
        }))
    }

    fn fail(&self, reason: String) {
        self.failure.send_if_modified(|failure| {
            if failure.is_some() {
                return false;
            }
            *failure = Some(reason);
            true
        });
    }

    fn observe<T>(&self, operation: &str, result: io::Result<T>) -> io::Result<T> {
        if let Err(error) = &result
            && matches!(
                error.raw_os_error(),
                Some(libc::ENOTCONN | libc::EBADF | libc::ENOTSOCK)
            )
        {
            self.fail(format!("local UDP {operation}: {error}"));
        }
        result
    }

    pub(crate) fn retire(&self) {
        // Old Quinn connections may outlive the endpoint. Release both the
        // socket owner and any pending writable futures, so those handles
        // cannot keep a fixed UDP port bound after transport retirement.
        self.io.lock().unwrap().take();
        let pollers: Vec<_> = self
            .pollers
            .lock()
            .unwrap()
            .drain(..)
            .filter_map(|p| p.upgrade())
            .collect();
        for pending in pollers {
            pending.lock().unwrap().take();
        }
    }
}

#[cfg(target_os = "linux")]
fn set_dont_fragment(socket: &std::net::UdpSocket) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let (level, option, value) = if socket.local_addr()?.is_ipv6() {
        (
            libc::IPPROTO_IPV6,
            libc::IPV6_MTU_DISCOVER,
            libc::IPV6_PMTUDISC_PROBE,
        )
    } else {
        (
            libc::IPPROTO_IP,
            libc::IP_MTU_DISCOVER,
            libc::IP_PMTUDISC_PROBE,
        )
    };
    let rc = unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            level,
            option,
            (&raw const value).cast(),
            std::mem::size_of_val(&value) as libc::socklen_t,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn set_dont_fragment(socket: &std::net::UdpSocket) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    const IP_DONTFRAG: libc::c_int = 28;
    let (level, option) = if socket.local_addr()?.is_ipv6() {
        (libc::IPPROTO_IPV6, libc::IPV6_DONTFRAG)
    } else {
        (libc::IPPROTO_IP, IP_DONTFRAG)
    };
    let value: libc::c_int = 1;
    let rc = unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            level,
            option,
            (&raw const value).cast(),
            std::mem::size_of_val(&value) as libc::socklen_t,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "ios")))]
fn set_dont_fragment(_socket: &std::net::UdpSocket) -> io::Result<()> {
    Ok(())
}

type WritableFuture = Pin<Box<dyn std::future::Future<Output = io::Result<()>> + Send>>;
type PendingWrite = Mutex<Option<WritableFuture>>;

struct WritablePoller {
    socket: Arc<PlainUdp>,
    pending: Arc<PendingWrite>,
}

impl Debug for WritablePoller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WritablePoller")
    }
}

impl UdpPoller for WritablePoller {
    fn poll_writable(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        let mut pending = self.pending.lock().unwrap();
        if pending.is_none() {
            let Some(socket) = self.socket.io.lock().unwrap().clone() else {
                return Poll::Ready(Err(io::ErrorKind::NotConnected.into()));
            };
            *pending = Some(Box::pin(async move { socket.writable().await }));
        }
        let result = pending.as_mut().unwrap().as_mut().poll(cx);
        if result.is_ready() {
            *pending = None;
        }
        result.map(|result| self.socket.observe("writable", result))
    }
}

impl AsyncUdpSocket for PlainUdp {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        let pending = Arc::new(Mutex::new(None));
        {
            let mut pollers = self.pollers.lock().unwrap();
            pollers.retain(|p| p.strong_count() > 0);
            pollers.push(Arc::downgrade(&pending));
        }
        Box::pin(WritablePoller {
            socket: self,
            pending,
        })
    }
    fn try_send(&self, transmit: &Transmit) -> io::Result<()> {
        #[cfg(test)]
        {
            let error = self.fail_send.swap(0, Ordering::Relaxed);
            if error != 0 {
                return self.observe("send", Err(io::Error::from_raw_os_error(error)));
            }
        }
        let result = self
            .io
            .lock()
            .unwrap()
            .as_ref()
            .ok_or_else(|| io::Error::from_raw_os_error(libc::ENOTCONN))
            .and_then(|socket| socket.try_send_to(transmit.contents, transmit.destination))
            .map(|_| ());
        self.observe("send", result)
    }
    fn poll_recv(
        &self,
        cx: &mut Context,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        let socket = self.io.lock().unwrap();
        let Some(socket) = socket.as_ref() else {
            return Poll::Ready(Err(io::ErrorKind::NotConnected.into()));
        };
        loop {
            match socket.poll_recv_ready(cx) {
                Poll::Ready(Ok(())) => {}
                Poll::Ready(Err(error)) => {
                    return Poll::Ready(self.observe("receive readiness", Err(error)));
                }
                Poll::Pending => return Poll::Pending,
            }
            match socket.try_recv_from(&mut bufs[0]) {
                Ok((len, addr)) => {
                    meta[0] = RecvMeta {
                        addr,
                        len,
                        stride: len,
                        ecn: None,
                        dst_ip: None,
                    };
                    return Poll::Ready(Ok(1));
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
                Err(error) => return Poll::Ready(self.observe("receive", Err(error))),
            }
        }
    }
    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io
            .lock()
            .unwrap()
            .as_ref()
            .ok_or(io::ErrorKind::NotConnected)?
            .local_addr()
    }
    fn may_fragment(&self) -> bool {
        false
    }
}

// -------------------------------------------------------------- relay link

/// How the relay is reached: the certificate-verified secure relay, or plain
/// UDP to a loopback test relay.
pub enum RelayLink {
    Secure(crate::secure_relay::SecureRelay),
    Udp(SocketAddr),
    None,
}

impl Debug for RelayLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Secure(link) => write!(f, "secure {link:?}"),
            Self::Udp(addr) => write!(f, "udp {addr}"),
            Self::None => f.write_str("none"),
        }
    }
}

// -------------------------------------------------------------- magicsock

#[derive(Clone, Copy, Debug, Default)]
pub struct PathCounters {
    pub tx_direct: u64,
    pub tx_relay: u64,
    pub rx_direct: u64,
    pub rx_relay: u64,
}

#[derive(Debug)]
struct PeerState {
    virtual_addr: SocketAddr,
    candidates: Vec<SocketAddr>,
    direct: Option<SocketAddr>,
    direct_since: Option<Instant>,
    direct_retry_at: Option<Instant>,
    last_pong: Option<Instant>,
    last_ping: Option<Instant>,
    last_candidates_sent: Option<Instant>,
    counters: PathCounters,
    /// When traffic from the peer was last reported to presence.
    reported: Option<Instant>,
}

/// Inbound traffic is reported to presence at most this often per peer.
const ACTIVITY_REPORT: Duration = Duration::from_millis(500);

#[derive(Clone, Debug)]
pub struct PathInfo {
    pub virtual_addr: SocketAddr,
    pub direct: Option<SocketAddr>,
    pub direct_for: Option<Duration>,
    pub candidates: Vec<SocketAddr>,
    pub counters: PathCounters,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayState {
    Unconfigured,
    Connecting,
    Failed,
    Registered,
}

#[derive(Default)]
struct State {
    peers: HashMap<DeviceId, PeerState>,
    by_virtual: HashMap<SocketAddr, DeviceId>,
    by_direct: HashMap<SocketAddr, DeviceId>,
    reflected: Option<SocketAddr>,
    registered: bool,
    registration: Retry,
    keepalive_pending: Option<Instant>,
    last_keepalive: Option<Instant>,
    /// Pending mailbox replies, keyed by (kind, key).
    mailbox_waiters: Vec<(u8, [u8; 32], oneshot::Sender<Frame>)>,
    /// (kind, token) sent to the relay on each registration.
    push_token: Option<(u8, Vec<u8>)>,
    /// The vnet's prefix: addresses in it are the tunnel itself and never a
    /// direct path (a PING that reaches a vip travels inside the link).
    vnet_prefix: Option<(Ipv4Addr, u8)>,
}

#[derive(Clone, Debug, Default)]
pub struct MagicOptions {
    pub relay_only: bool,
    /// A phone: one relay carrier at a time, QUIC first and TLS only as a
    /// fallback, without periodic health probes and with sparse QUIC
    /// keepalives (`secure_relay::Strategy::Fallback`).
    pub mobile: bool,
    pub allow_loopback: bool,
    pub extra_candidates: Vec<SocketAddr>,
    /// (kind, token) the relay may use to wake this device when it is
    /// offline: kind 1 = APNs, 2 = FCM, 3 = HarmonyOS push.
    pub push_token: Option<(u8, Vec<u8>)>,
}

pub struct MagicSock {
    #[cfg(test)]
    fail_receive: AtomicBool,
    #[cfg(test)]
    stall_receive: AtomicBool,
    // A local-only challenge proves this endpoint's receive pump still runs.
    // It never uses a peer or relay response as global health evidence.
    local_probe: Mutex<Option<(Vec<u8>, Instant)>>,
    closed: AtomicBool,
    inner: Arc<PlainUdp>,
    relay: RelayLink,
    signer: SharedSigner,
    my_id: DeviceId,
    options: MagicOptions,
    started: Instant,
    state: Mutex<State>,
    presence: std::sync::OnceLock<Arc<crate::presence::Board>>,
}

impl Debug for MagicSock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MagicSock({} via {:?})", self.my_id.short(), self.relay)
    }
}

impl MagicSock {
    pub fn bind(
        bind: SocketAddr,
        relay: RelayLink,
        signer: SharedSigner,
        options: MagicOptions,
    ) -> Result<Arc<Self>, Error> {
        let inner = PlainUdp::bind(bind).map_err(|e| Error::Io(format!("bind {bind}: {e}")))?;
        let my_id = signer.public_key().device_id();
        if let RelayLink::Secure(link) = &relay {
            link.set_push_token(options.push_token.clone(), my_id);
        }
        let state = State {
            push_token: options.push_token.clone(),
            ..State::default()
        };
        let sock = Arc::new(Self {
            #[cfg(test)]
            fail_receive: AtomicBool::new(false),
            #[cfg(test)]
            stall_receive: AtomicBool::new(false),
            local_probe: Mutex::new(None),
            closed: AtomicBool::new(false),
            inner,
            relay,
            signer,
            my_id,
            options,
            started: Instant::now(),
            state: Mutex::new(state),
            presence: std::sync::OnceLock::new(),
        });
        tokio::spawn(sock.clone().driver());
        Ok(sock)
    }

    /// Where relay presence reports and inbound traffic go (the node's).
    pub fn set_presence(&self, board: Arc<crate::presence::Board>) {
        let _ = self.presence.set(board);
    }

    /// Asks the relay for one PRESENT when `id` registers.
    pub fn watch(&self, id: DeviceId) {
        let _ = self.send_to_relay(&Frame::Watch { id });
    }

    /// Traffic from `id` arrived: report it, at most every ACTIVITY_REPORT.
    fn saw(&self, id: DeviceId, peer: &mut PeerState) {
        let now = Instant::now();
        if peer
            .reported
            .is_none_or(|at| now.saturating_duration_since(at) >= ACTIVITY_REPORT)
        {
            peer.reported = Some(now);
            if let Some(board) = self.presence.get() {
                board.activity(id);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn fail_next_receive_for_test(&self) {
        self.fail_receive.store(true, Ordering::Relaxed);
    }

    pub(crate) async fn failed(&self) -> String {
        let mut failure = self.inner.failure.subscribe();
        failure
            .wait_for(|value| value.is_some())
            .await
            .expect("socket owns failure sender")
            .clone()
            .unwrap()
    }

    #[cfg(test)]
    pub(crate) fn stall_receive_for_test(&self) {
        self.stall_receive.store(true, Ordering::Relaxed);
    }

    #[cfg(test)]
    pub(crate) fn fail_next_send_for_test(&self, errno: i32) {
        self.inner.fail_send.store(errno, Ordering::Relaxed);
    }

    pub fn shutdown(&self) {
        self.closed.store(true, Ordering::Relaxed);
        if let RelayLink::Secure(link) = &self.relay {
            link.shutdown();
        }
        self.inner.retire();
    }

    /// A tiny PONG does not prove that a path can carry QUIC. Cool down a
    /// failed direct path so the next attempt can actually use the relay.
    pub fn defer_direct(&self, id: &DeviceId) {
        let mut state = self.state.lock().unwrap();
        if let Some(peer) = state.peers.get_mut(id)
            && peer.direct.take().is_some()
        {
            peer.direct_since = None;
            peer.last_pong = None;
            peer.direct_retry_at = Some(Instant::now() + Duration::from_secs(30));
            log::info!(
                "peer {} direct path failed; using relay for 30 seconds",
                id.short()
            );
        }
    }

    fn relay_addr(&self) -> Option<SocketAddr> {
        match &self.relay {
            RelayLink::Udp(addr) => Some(*addr),
            _ => None,
        }
    }

    pub fn my_id(&self) -> DeviceId {
        self.my_id
    }

    /// Tells the socket which addresses belong to the vnet, so neither its
    /// own TUN address nor a peer's vip is ever tried as a direct path: a
    /// packet to a vip enters the tunnel, which runs over this very link.
    pub fn set_vnet_prefix(&self, prefix: Option<(Ipv4Addr, u8)>) {
        let mut state = self.state.lock().unwrap();
        if state.vnet_prefix == prefix {
            return;
        }
        state.vnet_prefix = prefix;
        for peer in state.peers.values_mut() {
            peer.candidates.retain(|c| !in_prefix(c, prefix));
            if peer.direct.is_some_and(|d| in_prefix(&d, prefix)) {
                peer.direct = None;
                peer.direct_since = None;
                peer.last_pong = None;
            }
        }
        state.by_direct.retain(|addr, _| !in_prefix(addr, prefix));
    }

    /// Replaces the push token the relay may wake this device with; it goes
    /// out now when registered, and again on every later registration.
    pub fn set_push_token(&self, token: Option<(u8, Vec<u8>)>) {
        if let RelayLink::Secure(link) = &self.relay {
            link.set_push_token(token.clone(), self.my_id);
        }
        let registered = {
            let mut state = self.state.lock().unwrap();
            state.push_token = token.clone();
            state.registered
        };
        if let (true, Some((kind, token))) = (registered, token) {
            let _ = self.send_to_relay(&Frame::PushReg {
                id: self.my_id,
                kind,
                token,
            });
        }
    }

    /// The host's addresses changed (interface up/down, roaming): register
    /// with the relay again for a fresh reflected address and rediscover
    /// every direct path. Known direct sources stay valid so a path that
    /// survived keeps delivering while it is re-probed.
    pub fn network_changed(&self) {
        crate::interfaces::invalidate();
        crate::doh::invalidate();
        if let RelayLink::Secure(link) = &self.relay {
            link.network_changed();
        }
        let mut state = self.state.lock().unwrap();
        state.registered = false;
        state.registration.reset();
        state.keepalive_pending = None;
        state.reflected = None;
        for peer in state.peers.values_mut() {
            peer.direct = None;
            peer.direct_since = None;
            peer.direct_retry_at = None;
            peer.last_pong = None;
            peer.last_ping = None;
            peer.last_candidates_sent = None;
        }
        log::info!("network changed: re-registering and re-probing paths");
    }

    /// Stable, unroutable, unique enough: 10.x.y.z:port from the id.
    pub fn virtual_addr(id: &DeviceId) -> SocketAddr {
        let ip = Ipv4Addr::new(10, id.0[0], id.0[1], id.0[2]);
        let port = u16::from_be_bytes([id.0[3], id.0[4]]) | 0x0400;
        SocketAddr::new(ip.into(), port)
    }

    pub fn add_peer(&self, id: DeviceId) -> SocketAddr {
        let mut state = self.state.lock().unwrap();
        self.add_peer_locked(&mut state, id)
    }

    fn add_peer_locked(&self, state: &mut State, id: DeviceId) -> SocketAddr {
        if let Some(peer) = state.peers.get(&id) {
            return peer.virtual_addr;
        }
        let virtual_addr = Self::virtual_addr(&id);
        state.by_virtual.insert(virtual_addr, id);
        state.peers.insert(
            id,
            PeerState {
                virtual_addr,
                candidates: Vec::new(),
                direct: None,
                direct_since: None,
                direct_retry_at: None,
                last_pong: None,
                last_ping: None,
                last_candidates_sent: None,
                counters: PathCounters::default(),
                reported: None,
            },
        );
        virtual_addr
    }

    pub fn forget_peer(&self, id: &DeviceId) {
        let mut state = self.state.lock().unwrap();
        if let Some(peer) = state.peers.remove(id) {
            state.by_virtual.remove(&peer.virtual_addr);
            state.by_direct.retain(|_, v| v != id);
        }
    }

    pub fn path(&self, id: &DeviceId) -> Option<PathInfo> {
        let state = self.state.lock().unwrap();
        state.peers.get(id).map(|p| PathInfo {
            virtual_addr: p.virtual_addr,
            direct: p.direct,
            direct_for: p.direct_since.map(|t| t.elapsed()),
            candidates: p.candidates.clone(),
            counters: p.counters,
        })
    }

    pub fn reflected(&self) -> Option<SocketAddr> {
        self.state.lock().unwrap().reflected
    }

    pub fn relay_state(&self) -> RelayState {
        match &self.relay {
            RelayLink::Secure(link) => link.status(),
            RelayLink::None => RelayState::Unconfigured,
            _ => {
                let state = self.state.lock().unwrap();
                if state.registered {
                    RelayState::Registered
                } else if state.registration.status(Instant::now()) == "failed" {
                    RelayState::Failed
                } else {
                    RelayState::Connecting
                }
            }
        }
    }

    fn is_known_peer(&self, id_bytes: &[u8]) -> bool {
        id_bytes
            .try_into()
            .ok()
            .map(DeviceId)
            .is_some_and(|id| self.state.lock().unwrap().peers.contains_key(&id))
    }

    pub(crate) fn local_candidates(&self) -> Vec<SocketAddr> {
        let port = self.inner.local_addr().map(|a| a.port()).unwrap_or(0);
        let (reflected, prefix) = {
            let state = self.state.lock().unwrap();
            (state.reflected, state.vnet_prefix)
        };
        let mut out = Vec::new();
        if let Ok(interfaces) = crate::interfaces::snapshot() {
            for interface in interfaces.iter() {
                let ip = interface.ip();
                if ip.is_ipv6() || (ip.is_loopback() && !self.options.allow_loopback) {
                    continue;
                }
                if matches!(ip, std::net::IpAddr::V4(v4) if v4.is_link_local()) {
                    continue;
                }
                if !is_physical_interface(&interface.name) {
                    // A VPN, a proxy's utun or another overlay (the previous
                    // Muster's vnet among them): reachable, but with its own
                    // MTU and its own idea of who is on the other end.
                    log::debug!("candidate {ip} on {} skipped: tunnel", interface.name);
                    continue;
                }
                out.push(SocketAddr::new(ip, port));
            }
        }
        out.extend(self.options.extra_candidates.iter().copied());
        if let Some(reflected) = reflected
            && !out.contains(&reflected)
        {
            out.push(reflected);
        }
        out.retain(|c| !in_prefix(c, prefix));
        out
    }

    /// One datagram to `destination`. A path whose MTU is below the
    /// datagram (EMSGSIZE) drops it like any lossy path would: quinn's MTU
    /// probes then settle on a size that fits instead of the connection
    /// dying on the error.
    fn send_raw(&self, destination: SocketAddr, contents: &[u8]) -> io::Result<()> {
        match self.send_raw_inner(destination, contents) {
            Err(error) if error.raw_os_error() == Some(libc::EMSGSIZE) => {
                log::debug!(
                    "datagram of {} bytes to {destination} exceeds the path MTU; dropped",
                    contents.len()
                );
                Ok(())
            }
            other => other,
        }
    }

    fn send_raw_inner(&self, destination: SocketAddr, contents: &[u8]) -> io::Result<()> {
        self.inner.try_send(&Transmit {
            destination,
            ecn: None,
            contents,
            segment_size: None,
            src_ip: None,
        })
    }

    fn send_to_relay(&self, frame: &Frame) -> io::Result<()> {
        let bytes = frame.encode();
        match &self.relay {
            RelayLink::Secure(link) => link.send(bytes),
            RelayLink::Udp(addr) => self.send_raw(*addr, &bytes),
            RelayLink::None => Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "no relay configured",
            )),
        }
    }

    fn send_candidates(&self, id: &DeviceId) {
        let candidates = self.local_candidates();
        let _ = self.send_to_relay(&Frame::Fwd {
            dst: *id,
            payload: rp::encode_candidates(&candidates),
        });
    }

    fn control_frame(kind: u8, id: &DeviceId, nonce: u64) -> Vec<u8> {
        let mut out = Vec::with_capacity(CONTROL_LEN);
        out.push(kind);
        out.extend_from_slice(&id.0);
        out.extend_from_slice(&nonce.to_be_bytes());
        out
    }

    /// Mailbox round trip through the relay: put an item, list a key, get one item.
    pub async fn mailbox(&self, request: Frame) -> Result<Frame, Error> {
        let (kind, key) = match &request {
            Frame::MbPut { key, .. } => (rp::MB_ACK, *key),
            Frame::MbList { key } => (rp::MB_LISTING, *key),
            Frame::MbGet { key, .. } => (rp::MB_ITEM, *key),
            _ => return Err(Error::Protocol("not a mailbox request".into())),
        };
        let (tx, rx) = oneshot::channel();
        self.state
            .lock()
            .unwrap()
            .mailbox_waiters
            .push((kind, key, tx));
        self.send_to_relay(&request)
            .map_err(|e| Error::Io(e.to_string()))?;
        tokio::time::timeout(Duration::from_secs(5), rx)
            .await
            .map_err(|_| Error::Timeout("relay mailbox"))?
            .map_err(|_| Error::Io("mailbox waiter dropped".into()))
    }

    async fn driver(self: Arc<Self>) {
        let mut next_probe = Instant::now() + Duration::from_secs(5);
        let mut missed_probes = 0;
        let mut tick = tokio::time::interval(Duration::from_millis(200));
        loop {
            tick.tick().await;
            if self.closed.load(Ordering::Relaxed) {
                return;
            }
            let now = Instant::now();
            if now >= next_probe {
                let pending = self.local_probe.lock().unwrap().take();
                if let Some((_, sent)) = pending {
                    if now.duration_since(sent) >= Duration::from_secs(3) {
                        missed_probes += 1;
                        log::warn!(
                            "local UDP receive probe missed ({missed_probes}/2); peer/relay status is independent"
                        );
                    }
                } else {
                    missed_probes = 0;
                }
                if missed_probes >= 2 {
                    self.inner
                        .fail("local UDP receive pump failed two loopback probes".into());
                    return;
                }
                let mut probe = b"MUSTER-local-socket-probe:".to_vec();
                probe.extend_from_slice(&rand::random::<[u8; 32]>());
                if let Ok(mut address) = self.inner.local_addr() {
                    if address.ip().is_unspecified() {
                        address.set_ip(if address.is_ipv4() {
                            std::net::Ipv4Addr::LOCALHOST.into()
                        } else {
                            std::net::Ipv6Addr::LOCALHOST.into()
                        });
                    }
                    *self.local_probe.lock().unwrap() = Some((probe.clone(), now));
                    if let Err(error) = self.send_raw_inner(address, &probe) {
                        log::warn!("local UDP probe send: {error}");
                    }
                } else {
                    self.inner.fail("local UDP address unavailable".into());
                    return;
                }
                next_probe = now + Duration::from_secs(5);
            }
            let mut candidates_to = Vec::new();
            let mut pings = Vec::new();
            let mut relay_frames = Vec::new();
            {
                let mut state = self.state.lock().unwrap();
                if !matches!(self.relay, RelayLink::None | RelayLink::Secure(_)) {
                    if state
                        .keepalive_pending
                        .is_some_and(|at| now.duration_since(at) >= CONNECT_TIMEOUT)
                    {
                        state.registered = false;
                        state.keepalive_pending = None;
                        state.registration.fail(now);
                        log::warn!("relay keepalive deadline expired");
                    }
                    if !state.registered {
                        if state.registration.expired(now) {
                            state.registration.fail(now);
                        }
                        if state.registration.due(now) {
                            relay_frames.push(Frame::Hello { id: self.my_id });
                            log::debug!("relay registration attempt");
                            state.registration.begin(now);
                        }
                    } else if state.keepalive_pending.is_none()
                        && state
                            .last_keepalive
                            .is_none_or(|t| now.duration_since(t) >= KEEPALIVE_INTERVAL)
                    {
                        relay_frames.push(Frame::Keepalive { id: self.my_id });
                        state.last_keepalive = Some(now);
                        state.keepalive_pending = Some(now);
                    }
                }
                let direct_allowed = !self.options.relay_only;
                for (id, peer) in state.peers.iter_mut() {
                    if !direct_allowed || peer.direct_retry_at.is_some_and(|at| now < at) {
                        continue;
                    }
                    if peer.direct.is_none()
                        && peer
                            .last_candidates_sent
                            .is_none_or(|t| now.duration_since(t) >= CANDIDATE_INTERVAL)
                    {
                        candidates_to.push(*id);
                        peer.last_candidates_sent = Some(now);
                    }
                    let nonce = now.duration_since(self.started).as_micros() as u64;
                    match peer.direct {
                        Some(direct) => {
                            if peer
                                .last_pong
                                .is_some_and(|t| now.duration_since(t) > DIRECT_TIMEOUT)
                            {
                                log::info!(
                                    "peer {} direct path {direct} timed out; relay",
                                    id.short()
                                );
                                peer.direct = None;
                                peer.direct_since = None;
                            } else if peer
                                .last_ping
                                .is_none_or(|t| now.duration_since(t) >= PING_INTERVAL)
                            {
                                pings.push((direct, nonce));
                                peer.last_ping = Some(now);
                            }
                        }
                        None => {
                            if !peer.candidates.is_empty()
                                && peer
                                    .last_ping
                                    .is_none_or(|t| now.duration_since(t) >= PUNCH_INTERVAL)
                            {
                                pings.extend(peer.candidates.iter().map(|c| (*c, nonce)));
                                peer.last_ping = Some(now);
                            }
                        }
                    }
                }
            }
            for frame in relay_frames {
                if let Err(error) = self.send_to_relay(&frame) {
                    log::warn!("relay control send failed: {error}");
                }
            }
            for id in candidates_to {
                self.send_candidates(&id);
            }
            for (addr, nonce) in pings {
                let mut probe = Self::control_frame(PING, &self.my_id, nonce);
                probe.resize(PATH_PROBE_LEN, 0);
                let _ = self.send_raw(addr, &probe);
            }
        }
    }

    /// Handles a relay frame; returns (payload offset, virtual source) for QUIC data.
    fn handle_relay_frame(&self, bytes: &[u8]) -> Option<(usize, SocketAddr)> {
        if self.closed.load(Ordering::Relaxed) {
            return None;
        }
        let frame = match Frame::decode(bytes) {
            Ok(frame) => frame,
            Err(error) => {
                log::debug!("bad relay frame: {error}");
                return None;
            }
        };
        match frame {
            Frame::Challenge { nonce } => {
                let signature = match rp::auth_signature(self.signer.as_ref(), &nonce) {
                    Ok(s) => s,
                    Err(error) => {
                        log::warn!("cannot sign relay challenge: {error}");
                        return None;
                    }
                };
                let _ = self.send_to_relay(&Frame::Auth {
                    id: self.my_id,
                    key: self.signer.public_key(),
                    nonce,
                    signature,
                });
                None
            }
            Frame::Reflect { addr } => {
                let mut state = self.state.lock().unwrap();
                if !state.registered {
                    log::info!("registered with relay; seen as {addr}");
                    if let Some((kind, token)) = &state.push_token {
                        let _ = self.send_to_relay(&Frame::PushReg {
                            id: self.my_id,
                            kind: *kind,
                            token: token.clone(),
                        });
                    }
                    // The secure carriers ask for presence frames themselves.
                    if !matches!(self.relay, RelayLink::Secure(_)) {
                        let _ = self.send_to_relay(&Frame::Features {
                            bits: rp::FEATURES_PRESENCE,
                        });
                    }
                }
                state.registered = true;
                state.registration.reset();
                state.keepalive_pending = None;
                state.last_keepalive = Some(Instant::now());
                // The secure carrier has a separate socket. Its reflected
                // address is NOT the inner peer endpoint's NAT candidate.
                if !matches!(self.relay, RelayLink::Secure(_)) && state.reflected != Some(addr) {
                    state.reflected = Some(addr);
                }
                None
            }
            Frame::Error { text } => {
                log::warn!("relay: {text}");
                let mut state = self.state.lock().unwrap();
                state.registered = false;
                state.registration.fail(Instant::now());
                state.keepalive_pending = None;
                None
            }
            Frame::Deliver { src, payload } => {
                let mut state = self.state.lock().unwrap();
                let virtual_addr = self.add_peer_locked(&mut state, src);
                match *payload.first()? {
                    rp::PAYLOAD_QUIC => {
                        let peer = state.peers.get_mut(&src)?;
                        peer.counters.rx_relay += 1;
                        self.saw(src, peer);
                        Some((34, virtual_addr))
                    }
                    rp::PAYLOAD_CANDIDATES => {
                        let prefix = state.vnet_prefix;
                        let mut candidates = rp::decode_candidates(&payload).unwrap_or_default();
                        candidates.retain(|c| !in_prefix(c, prefix));
                        let peer = state.peers.get_mut(&src)?;
                        if peer.candidates != candidates {
                            log::info!("peer {} candidates {:?}", src.short(), candidates);
                            // A restarted/roaming peer has new endpoints. Do not
                            // keep sending to its old process's still-open socket.
                            if peer.direct.is_some_and(|addr| !candidates.contains(&addr)) {
                                peer.direct = None;
                                peer.direct_since = None;
                                peer.last_pong = None;
                            }
                            peer.candidates = candidates;
                            peer.last_ping = None;
                        }
                        let reply = peer
                            .last_candidates_sent
                            .is_none_or(|t| t.elapsed() >= CANDIDATE_INTERVAL);
                        if reply {
                            peer.last_candidates_sent = Some(Instant::now());
                            drop(state);
                            if !self.options.relay_only {
                                self.send_candidates(&src);
                            }
                        }
                        None
                    }
                    _ => None,
                }
            }
            // The relay's word on presence (sent only after FEATURES).
            Frame::Absent { id } => {
                log::info!("relay: {} is not registered", id.short());
                if let Some(board) = self.presence.get() {
                    board.absent(id);
                }
                None
            }
            Frame::Present { id } => {
                log::info!("relay: {} registered", id.short());
                if let Some(board) = self.presence.get() {
                    board.present(id);
                }
                None
            }
            reply @ (Frame::MbAck { .. } | Frame::MbListing { .. } | Frame::MbItem { .. }) => {
                let (kind, key) = match &reply {
                    Frame::MbAck { key, .. } => (rp::MB_ACK, *key),
                    Frame::MbListing { key, .. } => (rp::MB_LISTING, *key),
                    Frame::MbItem { key, .. } => (rp::MB_ITEM, *key),
                    _ => unreachable!(),
                };
                let mut state = self.state.lock().unwrap();
                if let Some(index) = state
                    .mailbox_waiters
                    .iter()
                    .position(|(k, ky, _)| *k == kind && *ky == key)
                {
                    let (_, _, tx) = state.mailbox_waiters.remove(index);
                    let _ = tx.send(reply);
                }
                None
            }
            _ => None,
        }
    }

    fn handle_direct_control(&self, from: SocketAddr, bytes: &[u8]) {
        if self.closed.load(Ordering::Relaxed) {
            return;
        }
        let kind = bytes[0];
        let id = DeviceId(bytes[1..33].try_into().unwrap());
        let nonce = u64::from_be_bytes(bytes[33..41].try_into().unwrap());
        let mut state = self.state.lock().unwrap();
        if !state.peers.contains_key(&id) || in_prefix(&from, state.vnet_prefix) {
            return;
        }
        state.by_direct.insert(from, id);
        match kind {
            PING => {
                drop(state);
                let mut pong = Self::control_frame(PONG, &self.my_id, nonce);
                pong.resize(bytes.len(), 0);
                let _ = self.send_raw(from, &pong);
            }
            PONG => {
                let peer = state.peers.get_mut(&id).unwrap();
                if peer.direct_retry_at.is_some_and(|at| Instant::now() < at) {
                    return;
                }
                match peer.direct {
                    Some(current) if current == from => peer.last_pong = Some(Instant::now()),
                    Some(_) => {}
                    None => {
                        log::info!("peer {} direct via {from}", id.short());
                        peer.direct = Some(from);
                        peer.direct_since = Some(Instant::now());
                        peer.last_pong = Some(Instant::now());
                    }
                }
            }
            _ => {}
        }
    }
}

impl AsyncUdpSocket for MagicSock {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        self.inner.clone().create_io_poller()
    }

    fn try_send(&self, transmit: &Transmit) -> io::Result<()> {
        let (peer_id, direct) = {
            let mut state = self.state.lock().unwrap();
            let Some(id) = state.by_virtual.get(&transmit.destination).copied() else {
                return Ok(());
            };
            let peer = state.peers.get_mut(&id).unwrap();
            match peer.direct {
                Some(addr) => {
                    peer.counters.tx_direct += 1;
                    (id, Some(addr))
                }
                None => {
                    peer.counters.tx_relay += 1;
                    (id, None)
                }
            }
        };
        if let Some(addr) = direct {
            match self.send_raw(addr, transmit.contents) {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Err(error),
                Err(error) => {
                    log::debug!("direct send to {addr}: {error}; falling back to relay");
                    self.defer_direct(&peer_id);
                    if let Some(peer) = self.state.lock().unwrap().peers.get_mut(&peer_id) {
                        peer.counters.tx_relay += 1;
                    }
                }
            }
        }
        let mut payload = Vec::with_capacity(1 + transmit.contents.len());
        payload.push(rp::PAYLOAD_QUIC);
        payload.extend_from_slice(transmit.contents);
        self.send_to_relay(&Frame::Fwd {
            dst: peer_id,
            payload,
        })
    }

    fn poll_recv(
        &self,
        cx: &mut Context,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        #[cfg(test)]
        if self.stall_receive.load(Ordering::Relaxed) {
            return Poll::Pending;
        }
        #[cfg(test)]
        if self.fail_receive.swap(false, Ordering::Relaxed) {
            return Poll::Ready(Err(io::Error::from_raw_os_error(libc::ENOTCONN)));
        }
        for _ in 0..64 {
            let relay_frame = match &self.relay {
                RelayLink::Secure(link) => Some(link.poll_recv(cx)),
                _ => None,
            };
            if let Some(Poll::Ready(Some(frame))) = relay_frame {
                if let Some((offset, source)) = self.handle_relay_frame(&frame) {
                    let payload = &frame[offset..];
                    let n = payload.len().min(bufs[0].len());
                    bufs[0][..n].copy_from_slice(&payload[..n]);
                    meta[0] = RecvMeta {
                        addr: source,
                        len: n,
                        stride: n,
                        ecn: None,
                        dst_ip: None,
                    };
                    return Poll::Ready(Ok(1));
                }
                continue;
            }

            match self.inner.poll_recv(cx, &mut bufs[..1], &mut meta[..1]) {
                Poll::Ready(Ok(n)) if n >= 1 => {
                    let len = meta[0].len;
                    let from = meta[0].addr;
                    {
                        let mut probe = self.local_probe.lock().unwrap();
                        if probe
                            .as_ref()
                            .is_some_and(|(bytes, _)| bytes.as_slice() == &bufs[0][..len])
                        {
                            probe.take();
                            continue;
                        }
                    }
                    if self.relay_addr() == Some(from) {
                        let handled = self.handle_relay_frame(&bufs[0][..len]);
                        let Some((offset, source)) = handled else {
                            continue;
                        };
                        bufs[0].copy_within(offset..len, 0);
                        meta[0].addr = source;
                        meta[0].len = len - offset;
                        meta[0].stride = len - offset;
                        return Poll::Ready(Ok(1));
                    }
                    if matches!(len, CONTROL_LEN | PATH_PROBE_LEN)
                        && matches!(bufs[0][0], PING | PONG)
                        && self.is_known_peer(&bufs[0][1..33])
                    {
                        let bytes = bufs[0][..len].to_vec();
                        self.handle_direct_control(from, &bytes);
                        continue;
                    }
                    let source = {
                        let mut state = self.state.lock().unwrap();
                        state.by_direct.get(&from).copied().map(|id| {
                            let peer = state.peers.get_mut(&id).unwrap();
                            peer.counters.rx_direct += 1;
                            self.saw(id, peer);
                            peer.virtual_addr
                        })
                    };
                    match source {
                        Some(source) => {
                            meta[0].addr = source;
                            return Poll::Ready(Ok(1));
                        }
                        None => continue,
                    }
                }
                Poll::Ready(Ok(_)) => continue,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Pending => return Poll::Pending,
            }
        }
        cx.waker().wake_by_ref();
        Poll::Pending
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }
    fn max_transmit_segments(&self) -> usize {
        1
    }
    fn max_receive_segments(&self) -> usize {
        1
    }
    fn may_fragment(&self) -> bool {
        false
    }
}

/// Whether `addr` is an IPv4 address inside `prefix`.
fn in_prefix(addr: &SocketAddr, prefix: Option<(Ipv4Addr, u8)>) -> bool {
    let (Some((base, bits)), SocketAddr::V4(v4)) = (prefix, addr) else {
        return false;
    };
    if bits == 0 {
        return true;
    }
    let mask = u32::MAX << (32 - u32::from(bits.min(32)));
    (u32::from(*v4.ip()) & mask) == (u32::from(base) & mask)
}

#[cfg(test)]
mod tests {
    use super::*;
    use muster_core::{HwSigner, SoftSigner};

    #[test]
    fn prefix_membership() {
        let prefix = Some((Ipv4Addr::new(192, 168, 14, 0), 24));
        assert!(in_prefix(&"192.168.14.7:1".parse().unwrap(), prefix));
        assert!(!in_prefix(&"192.168.15.7:1".parse().unwrap(), prefix));
        assert!(!in_prefix(&"10.0.0.1:1".parse().unwrap(), prefix));
        assert!(!in_prefix(&"[::1]:1".parse().unwrap(), prefix));
        assert!(!in_prefix(&"192.168.14.7:1".parse().unwrap(), None));
    }

    #[tokio::test]
    async fn path_probe_reply_preserves_quic_packet_size() {
        let sock = MagicSock::bind(
            "127.0.0.1:0".parse().unwrap(),
            RelayLink::None,
            Arc::new(SoftSigner::generate()),
            MagicOptions::default(),
        )
        .unwrap();
        let peer = SoftSigner::generate().public_key().device_id();
        let receiver = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        sock.add_peer(peer);
        let mut ping = MagicSock::control_frame(PING, &peer, 42);
        ping.resize(PATH_PROBE_LEN, 0);
        let mut writable = sock.inner.clone().create_io_poller();
        std::future::poll_fn(|cx| writable.as_mut().poll_writable(cx))
            .await
            .unwrap();
        sock.handle_direct_control(receiver.local_addr().unwrap(), &ping);
        let mut reply = [0; 1500];
        let (length, _) =
            tokio::time::timeout(Duration::from_secs(1), receiver.recv_from(&mut reply))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(length, PATH_PROBE_LEN);
        assert_eq!(reply[0], PONG);
        assert_eq!(&reply[33..41], &42u64.to_be_bytes());
        sock.shutdown();
    }

    #[tokio::test]
    async fn failed_direct_path_stays_on_relay_despite_pongs_until_network_changes() {
        let sock = MagicSock::bind(
            "127.0.0.1:0".parse().unwrap(),
            RelayLink::None,
            Arc::new(SoftSigner::generate()),
            MagicOptions::default(),
        )
        .unwrap();
        let peer = SoftSigner::generate().public_key().device_id();
        let addr = "127.0.0.1:34567".parse().unwrap();
        sock.add_peer(peer);
        let pong = MagicSock::control_frame(PONG, &peer, 1);
        sock.handle_direct_control(addr, &pong);
        assert_eq!(sock.path(&peer).unwrap().direct, Some(addr));
        sock.defer_direct(&peer);
        sock.handle_direct_control(addr, &pong);
        assert_eq!(sock.path(&peer).unwrap().direct, None);
        sock.network_changed();
        sock.handle_direct_control(addr, &pong);
        assert_eq!(sock.path(&peer).unwrap().direct, Some(addr));
        sock.shutdown();
    }

    #[tokio::test]
    async fn vnet_addresses_are_never_direct_candidates() {
        let signer: SharedSigner = Arc::new(SoftSigner::generate());
        let sock = MagicSock::bind(
            "127.0.0.1:0".parse().unwrap(),
            RelayLink::None,
            signer.clone(),
            MagicOptions {
                relay_only: false,
                mobile: false,
                allow_loopback: true,
                extra_candidates: vec![
                    "10.9.0.5:4000".parse().unwrap(),
                    "192.168.14.7:4000".parse().unwrap(),
                ],
                push_token: None,
            },
        )
        .unwrap();
        // Before the prefix is known every extra candidate is offered.
        assert!(
            sock.local_candidates()
                .contains(&"192.168.14.7:4000".parse().unwrap())
        );

        sock.set_vnet_prefix(Some((Ipv4Addr::new(192, 168, 14, 0), 24)));
        let offered = sock.local_candidates();
        assert!(offered.contains(&"10.9.0.5:4000".parse().unwrap()));
        assert!(
            !offered
                .iter()
                .any(|c| in_prefix(c, Some((Ipv4Addr::new(192, 168, 14, 0), 24))))
        );

        // A peer's list loses its vip entry, and a PING from a vip is ignored.
        let peer = SoftSigner::generate().public_key().device_id();
        sock.add_peer(peer);
        let payload = rp::encode_candidates(&[
            "10.9.0.6:4000".parse().unwrap(),
            "192.168.14.2:4000".parse().unwrap(),
        ]);
        sock.handle_relay_frame(&Frame::Deliver { src: peer, payload }.encode());
        let path = sock.path(&peer).unwrap();
        assert_eq!(
            path.candidates,
            vec!["10.9.0.6:4000".parse::<SocketAddr>().unwrap()]
        );
        sock.handle_direct_control(
            "192.168.14.2:4000".parse().unwrap(),
            &MagicSock::control_frame(PING, &peer, 1),
        );
        assert!(sock.state.lock().unwrap().by_direct.is_empty());
        sock.handle_direct_control(
            "10.9.0.6:4000".parse().unwrap(),
            &MagicSock::control_frame(PONG, &peer, 1),
        );
        assert_eq!(
            sock.path(&peer).unwrap().direct,
            Some("10.9.0.6:4000".parse().unwrap())
        );

        // Learning the prefix later drops a direct path that went through the tunnel.
        sock.set_vnet_prefix(None);
        sock.handle_direct_control(
            "192.168.14.2:4000".parse().unwrap(),
            &MagicSock::control_frame(PONG, &peer, 2),
        );
        sock.set_vnet_prefix(Some((Ipv4Addr::new(192, 168, 14, 0), 24)));
        assert_eq!(
            sock.path(&peer).unwrap().direct,
            Some("10.9.0.6:4000".parse().unwrap())
        );
    }
}

/// Whether an interface name is a physical (or bridged physical) network
/// interface rather than a tunnel or overlay. Direct candidates come only
/// from these; overlays have their own MTU and, for the previous Muster's
/// vnet, their own routing that must not carry this network's link.
pub fn is_physical_interface(name: &str) -> bool {
    const TUNNELS: [&str; 12] = [
        "utun", "ipsec", "ppp", "tun", "tap", "gif", "stf", "awdl", "llw", "vmnet", "feth", "wg",
    ];
    !TUNNELS.iter().any(|prefix| {
        name.starts_with(prefix) && name[prefix.len()..].chars().all(|c| c.is_ascii_digit())
    })
}

#[cfg(test)]
mod interface_tests {
    use super::is_physical_interface;

    #[test]
    fn tunnels_are_not_candidates() {
        for tunnel in [
            "utun0", "utun11", "ipsec0", "ppp0", "tun0", "wg0", "awdl0", "llw0",
        ] {
            assert!(!is_physical_interface(tunnel), "{tunnel}");
        }
        for physical in ["en0", "en5", "eth0", "wlan0", "bridge0", "lo0", "enp3s0"] {
            assert!(is_physical_interface(physical), "{physical}");
        }
    }
}
