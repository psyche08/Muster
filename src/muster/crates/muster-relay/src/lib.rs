//! muster-relay (ARCHITECTURE §3.5): registration proven by a signature
//! over a challenge, forwarding by device id, a mailbox of opaque signed
//! blobs keyed by 32 bytes, address reflection. Production exposes QUIC and
//! TCP/TLS with a public domain certificate. It holds no device keys and takes
//! no member authorization decision. Plain UDP is an explicit loopback test API.
//!
//! A listing names the newest 40 items of a key: a roster reader that was
//! away wants the latest entries and already holds the old ones.
//!
//! Presence (for carriers that sent `FEATURES` with `FEATURES_PRESENCE`): a
//! FWD to a device that has not been registered for `absent_grace` is
//! answered with `ABSENT`, so the sender stops dialing it; a device that
//! re-registers within the grace (a carrier change, a network change) is
//! never reported absent. `WATCH` asks for one `PRESENT` when the device
//! registers. The relay start counts as everyone's departure.

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use muster_core::DeviceId;
use muster_net::relay_proto::{FEATURES_PRESENCE, Frame, MAX_FRAME, verify_auth};
use muster_net::relay_tls::{self, RELAY_ALPN};
use sha2::{Digest, Sha256};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::{Semaphore, mpsc};

#[derive(Clone, Debug)]
pub struct RelayOptions {
    /// Items kept per mailbox key.
    pub mailbox_items: usize,
    /// Drop registrations idle this long.
    pub idle: Duration,
    /// Registered devices at most; later registrations are refused.
    pub max_clients: usize,
    /// Frames per second a registered device may send (burst twice that).
    pub client_frames_per_sec: u32,
    /// Forwarded bytes per second per registered device (burst twice that).
    pub client_bytes_per_sec: u64,
    /// Unauthenticated frames per second per source address.
    pub source_frames_per_sec: u32,
    /// Distinct mailbox keys at most.
    pub mailbox_keys: usize,
    /// Run this program with `<device id hex> <kind> <token hex>` when a
    /// member sends to a device that is offline but left a push token.
    pub push_command: Option<std::path::PathBuf>,
    /// At most one push per device in this long.
    pub push_interval: Duration,
    /// A device unregistered for less than this is re-registering, not
    /// absent: no ABSENT is sent for it.
    pub absent_grace: Duration,
}

/// At most one ABSENT per (sender, destination) in this long.
const ABSENT_INTERVAL: Duration = Duration::from_secs(2);
/// A WATCH lasts this long; clients renew theirs.
const WATCH_TTL: Duration = Duration::from_secs(600);
/// Devices one client may watch at once.
const MAX_WATCHES: usize = 64;

impl Default for RelayOptions {
    fn default() -> Self {
        Self {
            mailbox_items: 512,
            idle: Duration::from_secs(90),
            max_clients: 10_000,
            client_frames_per_sec: 3000,
            client_bytes_per_sec: 30_000_000,
            source_frames_per_sec: 30,
            mailbox_keys: 10_000,
            push_command: None,
            push_interval: Duration::from_secs(600),
            absent_grace: Duration::from_secs(10),
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum RouteKey {
    Udp(SocketAddr),
    Connection(u64),
}

#[derive(Clone)]
enum Route {
    Udp(SocketAddr),
    Connection { id: u64, tx: mpsc::Sender<Vec<u8>> },
}

impl Route {
    fn key(&self) -> RouteKey {
        match self {
            Self::Udp(addr) => RouteKey::Udp(*addr),
            Self::Connection { id, .. } => RouteKey::Connection(*id),
        }
    }

    fn same(&self, other: &Route) -> bool {
        self.key() == other.key()
    }

    fn channel() -> (Self, mpsc::Receiver<Vec<u8>>) {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let (tx, rx) = mpsc::channel(256);
        (
            Self::Connection {
                id: NEXT.fetch_add(1, Ordering::Relaxed),
                tx,
            },
            rx,
        )
    }
}

struct Registration {
    route: Route,
    seen: Instant,
    /// Only business traffic or Activate changes the preferred return route; standby
    /// keepalives must not oscillate the device's inbound data path.
    active: Option<Instant>,
}

/// A token bucket: `rate` per second, holding at most `burst`.
struct Bucket {
    tokens: f64,
    last: Instant,
}

impl Bucket {
    fn new(burst: f64) -> Self {
        Self {
            tokens: burst,
            last: Instant::now(),
        }
    }

    fn take(&mut self, rate: f64, burst: f64, cost: f64) -> bool {
        let now = Instant::now();
        self.tokens = (self.tokens + now.duration_since(self.last).as_secs_f64() * rate).min(burst);
        self.last = now;
        if self.tokens >= cost {
            self.tokens -= cost;
            true
        } else {
            false
        }
    }
}

struct PushToken {
    kind: u8,
    token: Vec<u8>,
    last_push: Option<Instant>,
}

#[derive(Default)]
struct Stats {
    forwarded: u64,
    forwarded_bytes: u64,
    dropped: u64,
    auth_failures: u64,
    rate_limited: u64,
    pushes: u64,
}

/// Mailbox items under one key: (sha256, bytes), oldest first.
type Items = VecDeque<([u8; 32], Vec<u8>)>;
type Challenges = HashMap<(DeviceId, RouteKey), ([u8; 32], Instant)>;

pub struct Relay {
    registrations: Mutex<HashMap<DeviceId, Vec<Registration>>>,
    /// Route → id, so a FWD sender is named without a scan.
    by_route: Mutex<HashMap<RouteKey, DeviceId>>,
    challenges: Mutex<Challenges>,
    mailbox: Mutex<HashMap<[u8; 32], Items>>,
    stats: Mutex<Stats>,
    udp: Option<UdpSocket>,
    mailbox_items: usize,
    idle: Duration,
    options: RelayOptions,
    /// Per registered device: frames, then bytes.
    client_limits: Mutex<HashMap<DeviceId, (Bucket, Bucket)>>,
    /// Per source address, for frames sent before authentication.
    source_limits: Mutex<HashMap<SocketAddr, Bucket>>,
    push_tokens: Mutex<HashMap<DeviceId, PushToken>>,
    started: Instant,
    /// When a device's last carrier went away.
    departed: Mutex<HashMap<DeviceId, Instant>>,
    /// Carriers that asked for presence frames (`FEATURES`).
    features: Mutex<HashMap<RouteKey, u32>>,
    /// The last ABSENT per (sender, destination).
    absent_sent: Mutex<HashMap<(DeviceId, DeviceId), Instant>>,
    /// Watched device → watcher → expiry.
    watches: Mutex<HashMap<DeviceId, HashMap<DeviceId, Instant>>>,
}

impl Relay {
    fn presence_capable(&self, route: &Route) -> bool {
        self.features
            .lock()
            .unwrap()
            .get(&route.key())
            .is_some_and(|bits| bits & FEATURES_PRESENCE != 0)
    }

    /// A fresh route of `id` that asked for presence frames.
    fn presence_route(&self, id: DeviceId) -> Option<Route> {
        let registrations = self.registrations.lock().unwrap();
        let features = self.features.lock().unwrap();
        registrations
            .get(&id)?
            .iter()
            .filter(|r| r.seen.elapsed() < self.idle)
            .filter(|r| {
                features
                    .get(&r.route.key())
                    .is_some_and(|bits| bits & FEATURES_PRESENCE != 0)
            })
            .max_by_key(|r| r.active)
            .map(|r| r.route.clone())
    }

    fn registered(&self, id: DeviceId) -> bool {
        self.registrations
            .lock()
            .unwrap()
            .get(&id)
            .is_some_and(|routes| routes.iter().any(|r| r.seen.elapsed() < self.idle))
    }

    /// Since when `id` has had no fresh carrier (None while it has one).
    fn absent_since(&self, id: DeviceId) -> Option<Instant> {
        if let Some(routes) = self.registrations.lock().unwrap().get(&id) {
            if routes.iter().any(|r| r.seen.elapsed() < self.idle) {
                return None;
            }
            if let Some(seen) = routes.iter().map(|r| r.seen).max() {
                return Some(seen);
            }
        }
        Some(
            self.departed
                .lock()
                .unwrap()
                .get(&id)
                .copied()
                .unwrap_or(self.started),
        )
    }

    /// Whether `src` (on `from`) gets an ABSENT for `dst` now.
    fn absent_due(&self, src: DeviceId, dst: DeviceId, from: &Route) -> bool {
        if !self.presence_capable(from) {
            return false;
        }
        let Some(since) = self.absent_since(dst) else {
            return false;
        };
        if since.elapsed() < self.options.absent_grace {
            return false;
        }
        let mut sent = self.absent_sent.lock().unwrap();
        if sent
            .get(&(src, dst))
            .is_some_and(|at| at.elapsed() < ABSENT_INTERVAL)
        {
            return false;
        }
        if sent.len() >= 100_000 {
            sent.clear();
        }
        sent.insert((src, dst), Instant::now());
        true
    }

    /// `id` registered: tell its watchers once.
    async fn announce(&self, id: DeviceId) {
        self.departed.lock().unwrap().remove(&id);
        let watchers = self.watches.lock().unwrap().remove(&id).unwrap_or_default();
        let now = Instant::now();
        for (watcher, expires) in watchers {
            if expires <= now {
                continue;
            }
            if let Some(route) = self.presence_route(watcher) {
                self.deliver(&route, Frame::Present { id }).await;
            }
        }
    }
    async fn deliver(&self, route: &Route, frame: Frame) {
        let bytes = frame.encode();
        match route {
            Route::Udp(addr) => {
                if let Some(udp) = &self.udp {
                    let _ = udp.send_to(&bytes, addr).await;
                }
            }
            Route::Connection { tx, .. } => {
                // A slow connection cannot grow memory or stall the relay loop.
                let _ = tx.try_send(bytes);
            }
        }
    }

    fn sender_id(&self, route: &Route) -> Option<DeviceId> {
        self.by_route.lock().unwrap().get(&route.key()).copied()
    }

    fn touch(&self, id: DeviceId, route: &Route, business: bool) -> bool {
        let mut registrations = self.registrations.lock().unwrap();
        let Some(routes) = registrations.get_mut(&id) else {
            return false;
        };
        let Some(r) = routes.iter_mut().find(|r| r.route.same(route)) else {
            return false;
        };
        r.seen = Instant::now();
        if business {
            r.active = Some(r.seen);
        }
        true
    }

    fn disconnect(&self, route: &Route) {
        let mut registrations = self.registrations.lock().unwrap();
        let mut by_route = self.by_route.lock().unwrap();
        self.features.lock().unwrap().remove(&route.key());
        if let Some(id) = by_route.remove(&route.key()) {
            if let Some(routes) = registrations.get_mut(&id) {
                routes.retain(|r| !r.route.same(route));
                if routes.is_empty() {
                    registrations.remove(&id);
                    self.client_limits.lock().unwrap().remove(&id);
                    self.departed.lock().unwrap().insert(id, Instant::now());
                }
            }
        }
        self.challenges
            .lock()
            .unwrap()
            .retain(|(_, key), _| *key != route.key());
    }

    /// Whether `id` may send one more frame carrying `bytes` payload bytes.
    fn client_allowed(&self, id: DeviceId, bytes: usize) -> bool {
        let rate = self.options.client_frames_per_sec as f64;
        let bps = self.options.client_bytes_per_sec as f64;
        let mut limits = self.client_limits.lock().unwrap();
        let (frames, volume) = limits
            .entry(id)
            .or_insert_with(|| (Bucket::new(rate * 2.0), Bucket::new(bps * 2.0)));
        let ok = frames.take(rate, rate * 2.0, 1.0) && volume.take(bps, bps * 2.0, bytes as f64);
        if !ok {
            self.stats.lock().unwrap().rate_limited += 1;
        }
        ok
    }

    /// Whether an unauthenticated source may send one more frame.
    fn source_allowed(&self, addr: SocketAddr) -> bool {
        let rate = self.options.source_frames_per_sec as f64;
        let mut limits = self.source_limits.lock().unwrap();
        if limits.len() > 100_000 {
            limits.clear();
        }
        let ok = limits
            .entry(addr)
            .or_insert_with(|| Bucket::new(rate * 2.0))
            .take(rate, rate * 2.0, 1.0);
        if !ok {
            self.stats.lock().unwrap().rate_limited += 1;
        }
        ok
    }

    /// Wakes an offline device through the operator's push hook.
    fn push(&self, dst: DeviceId) {
        let Some(command) = self.options.push_command.clone() else {
            return;
        };
        let job = {
            let mut tokens = self.push_tokens.lock().unwrap();
            let Some(token) = tokens.get_mut(&dst) else {
                return;
            };
            if token
                .last_push
                .is_some_and(|t| t.elapsed() < self.options.push_interval)
            {
                return;
            }
            token.last_push = Some(Instant::now());
            (token.kind, token.token.clone())
        };
        self.stats.lock().unwrap().pushes += 1;
        let id = dst.to_hex();
        tokio::spawn(async move {
            let status = tokio::process::Command::new(&command)
                .arg(&id)
                .arg(job.0.to_string())
                .arg(hex::encode(job.1))
                .status()
                .await;
            match status {
                Ok(s) if s.success() => log::info!("push {} ok", dst.short()),
                Ok(s) => log::warn!("push {}: {s}", dst.short()),
                Err(e) => log::warn!("push {}: {e}", dst.short()),
            }
        });
    }

    /// Multiple authenticated carriers per device coexist, with bounded state.
    fn register(&self, id: DeviceId, route: Route) -> bool {
        let mut registrations = self.registrations.lock().unwrap();
        if !registrations.contains_key(&id) && registrations.len() >= self.options.max_clients {
            return false;
        }
        let mut by_route = self.by_route.lock().unwrap();
        // A connection/source address cannot impersonate two registered ids.
        if by_route
            .get(&route.key())
            .is_some_and(|existing| *existing != id)
        {
            return false;
        }
        let routes = registrations.entry(id).or_default();
        if let Some(r) = routes.iter_mut().find(|r| r.route.same(&route)) {
            r.seen = Instant::now();
            return true;
        }
        if routes.len() >= 4 {
            return false;
        }
        by_route.insert(route.key(), id);
        let now = Instant::now();
        routes.push(Registration {
            route,
            seen: now,
            active: None,
        });
        log::info!("registered {} carriers={}", id.short(), routes.len());
        true
    }

    async fn handle(&self, bytes: &[u8], from: Route, seen_as: SocketAddr) {
        if bytes.len() > MAX_FRAME {
            self.stats.lock().unwrap().dropped += 1;
            return;
        }
        let frame = match Frame::decode(bytes) {
            Ok(frame) => frame,
            Err(_) => {
                self.stats.lock().unwrap().dropped += 1;
                return;
            }
        };
        match frame {
            Frame::Hello { id } => {
                if !self.source_allowed(seen_as) {
                    return;
                }
                let mut nonce = [0u8; 32];
                rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut nonce);
                {
                    let mut challenges = self.challenges.lock().unwrap();
                    let key = (id, from.key());
                    if !challenges.contains_key(&key)
                        && challenges.len() >= self.options.max_clients.saturating_mul(4)
                    {
                        return;
                    }
                    challenges.insert(key, (nonce, Instant::now()));
                }
                self.deliver(&from, Frame::Challenge { nonce }).await;
            }
            Frame::Auth {
                id,
                key,
                nonce,
                signature,
            } => {
                if !self.source_allowed(seen_as) {
                    return;
                }
                let expected = self.challenges.lock().unwrap().remove(&(id, from.key()));
                let valid = expected
                    .is_some_and(|(n, at)| n == nonce && at.elapsed() < Duration::from_secs(30))
                    && verify_auth(&id, &key, &nonce, &signature).is_ok();
                if !valid {
                    self.stats.lock().unwrap().auth_failures += 1;
                    self.deliver(
                        &from,
                        Frame::Error {
                            text: "authentication failed".into(),
                        },
                    )
                    .await;
                    return;
                }
                let returning = !self.registered(id);
                if !self.register(id, from.clone()) {
                    self.stats.lock().unwrap().dropped += 1;
                    self.deliver(
                        &from,
                        Frame::Error {
                            text: "relay full".into(),
                        },
                    )
                    .await;
                    return;
                }
                self.deliver(&from, Frame::Reflect { addr: seen_as }).await;
                if returning {
                    self.announce(id).await;
                }
            }
            Frame::Features { bits } => {
                let Some(src) = self.sender_id(&from) else {
                    self.stats.lock().unwrap().dropped += 1;
                    return;
                };
                if !self.client_allowed(src, 0) {
                    return;
                }
                self.touch(src, &from, false);
                self.features.lock().unwrap().insert(from.key(), bits);
            }
            Frame::Watch { id: watched } => {
                let Some(src) = self.sender_id(&from) else {
                    self.stats.lock().unwrap().dropped += 1;
                    return;
                };
                if !self.client_allowed(src, 0) || !self.presence_capable(&from) {
                    return;
                }
                self.touch(src, &from, false);
                if self.registered(watched) {
                    // Registered already (or again before the WATCH arrived).
                    self.deliver(&from, Frame::Present { id: watched }).await;
                    return;
                }
                let mut watches = self.watches.lock().unwrap();
                let held = watches
                    .values()
                    .filter(|watchers| watchers.contains_key(&src))
                    .count();
                let renewing = watches
                    .get(&watched)
                    .is_some_and(|watchers| watchers.contains_key(&src));
                if renewing || held < MAX_WATCHES {
                    watches
                        .entry(watched)
                        .or_default()
                        .insert(src, Instant::now() + WATCH_TTL);
                } else {
                    self.stats.lock().unwrap().dropped += 1;
                }
            }
            Frame::PushReg { id, kind, token } => {
                if self.sender_id(&from) != Some(id)
                    || token.len() > 512
                    || !self.client_allowed(id, 0)
                {
                    self.stats.lock().unwrap().dropped += 1;
                    return;
                }
                // Devices send their token again with every registration and
                // on each carrier; the push interval is per device and
                // survives that.
                let mut tokens = self.push_tokens.lock().unwrap();
                let last_push = tokens.get(&id).and_then(|t| t.last_push);
                tokens.insert(
                    id,
                    PushToken {
                        kind,
                        token,
                        last_push,
                    },
                );
            }
            Frame::Probe { id, nonce } => {
                if self.sender_id(&from) != Some(id) || !self.client_allowed(id, 0) {
                    self.stats.lock().unwrap().dropped += 1;
                    return;
                }
                if self.touch(id, &from, false) {
                    self.deliver(&from, Frame::ProbeAck { nonce }).await;
                }
            }
            Frame::Activate { id } => {
                if self.sender_id(&from) != Some(id) || !self.client_allowed(id, 0) {
                    self.stats.lock().unwrap().dropped += 1;
                    return;
                }
                if self.touch(id, &from, true) {
                    self.deliver(&from, Frame::Reflect { addr: seen_as }).await;
                }
            }
            Frame::Keepalive { id } => {
                if self.sender_id(&from) != Some(id) {
                    self.deliver(
                        &from,
                        Frame::Error {
                            text: "not registered".into(),
                        },
                    )
                    .await;
                    return;
                }
                if !self.client_allowed(id, 0) {
                    return;
                }
                let known = self.touch(id, &from, false);
                if known {
                    self.deliver(&from, Frame::Reflect { addr: seen_as }).await;
                } else {
                    self.deliver(
                        &from,
                        Frame::Error {
                            text: "not registered".into(),
                        },
                    )
                    .await;
                }
            }
            Frame::Fwd { dst, payload } => {
                let Some(src) = self.sender_id(&from) else {
                    self.stats.lock().unwrap().dropped += 1;
                    return;
                };
                if !self.client_allowed(src, payload.len()) {
                    return;
                }
                self.touch(src, &from, true);
                let target = self
                    .registrations
                    .lock()
                    .unwrap()
                    .get(&dst)
                    .and_then(|routes| {
                        routes
                            .iter()
                            .filter(|r| r.seen.elapsed() < self.idle)
                            .max_by_key(|r| r.active)
                    })
                    .map(|r| r.route.clone());
                match target {
                    Some(route) => {
                        {
                            let mut stats = self.stats.lock().unwrap();
                            stats.forwarded += 1;
                            stats.forwarded_bytes += payload.len() as u64;
                        }
                        self.deliver(&route, Frame::Deliver { src, payload }).await;
                    }
                    None => {
                        self.stats.lock().unwrap().dropped += 1;
                        self.push(dst);
                        if self.absent_due(src, dst, &from) {
                            self.deliver(&from, Frame::Absent { id: dst }).await;
                        }
                    }
                }
            }
            Frame::MbPut { key, item } => {
                let Some(src) = self.sender_id(&from) else {
                    self.deliver(
                        &from,
                        Frame::Error {
                            text: "not registered".into(),
                        },
                    )
                    .await;
                    return;
                };
                if !self.client_allowed(src, item.len()) {
                    return;
                }
                self.touch(src, &from, true);
                let full = {
                    let mailbox = self.mailbox.lock().unwrap();
                    !mailbox.contains_key(&key) && mailbox.len() >= self.options.mailbox_keys
                };
                if full {
                    self.deliver(
                        &from,
                        Frame::Error {
                            text: "mailbox full".into(),
                        },
                    )
                    .await;
                    return;
                }
                let hash: [u8; 32] = Sha256::digest(&item).into();
                {
                    let mut mailbox = self.mailbox.lock().unwrap();
                    let items = mailbox.entry(key).or_default();
                    if !items.iter().any(|(h, _)| *h == hash) {
                        items.push_back((hash, item));
                        while items.len() > self.mailbox_items {
                            items.pop_front();
                        }
                    }
                }
                self.deliver(&from, Frame::MbAck { key, hash }).await;
            }
            Frame::MbList { key } => {
                let Some(id) = self.sender_id(&from) else {
                    return;
                };
                if !self.client_allowed(id, 0) {
                    return;
                }
                self.touch(id, &from, true);
                let hashes: Vec<[u8; 32]> = self
                    .mailbox
                    .lock()
                    .unwrap()
                    .get(&key)
                    .map(|items| items.iter().rev().map(|(h, _)| *h).take(40).collect())
                    .unwrap_or_default();
                self.deliver(&from, Frame::MbListing { key, hashes }).await;
            }
            Frame::MbGet { key, hash } => {
                let Some(id) = self.sender_id(&from) else {
                    return;
                };
                if !self.client_allowed(id, 0) {
                    return;
                }
                self.touch(id, &from, true);
                let item = self.mailbox.lock().unwrap().get(&key).and_then(|items| {
                    items
                        .iter()
                        .find(|(h, _)| *h == hash)
                        .map(|(_, i)| i.clone())
                });
                match item {
                    Some(item) => self.deliver(&from, Frame::MbItem { key, hash, item }).await,
                    None => {
                        self.deliver(
                            &from,
                            Frame::Error {
                                text: "no such item".into(),
                            },
                        )
                        .await
                    }
                }
            }
            _ => self.stats.lock().unwrap().dropped += 1,
        }
    }

    fn expire(&self) {
        let now = Instant::now();
        let mut registrations = self.registrations.lock().unwrap();
        let mut by_route = self.by_route.lock().unwrap();
        let mut limits = self.client_limits.lock().unwrap();
        let mut features = self.features.lock().unwrap();
        let mut departed = self.departed.lock().unwrap();
        registrations.retain(|id, routes| {
            routes.retain(|r| {
                let keep = now.duration_since(r.seen) < self.idle;
                if !keep {
                    by_route.remove(&r.route.key());
                    features.remove(&r.route.key());
                }
                keep
            });
            if routes.is_empty() {
                log::info!("expired {}", id.short());
                limits.remove(id);
                departed.insert(*id, now);
                false
            } else {
                true
            }
        });
        // Older departures add nothing over the relay start.
        departed.retain(|_, at| now.duration_since(*at) < Duration::from_secs(3600));
        drop((limits, features, departed));
        self.absent_sent
            .lock()
            .unwrap()
            .retain(|_, at| now.duration_since(*at) < ABSENT_INTERVAL);
        self.watches.lock().unwrap().retain(|_, watchers| {
            watchers.retain(|_, expires| *expires > now);
            !watchers.is_empty()
        });
        self.source_limits
            .lock()
            .unwrap()
            .retain(|_, b| now.duration_since(b.last) < Duration::from_secs(60));
        self.challenges
            .lock()
            .unwrap()
            .retain(|_, (_, at)| now.duration_since(*at) < Duration::from_secs(30));
    }
}

/// A bound relay. Production binds both TLS carriers to the same port;
/// `bind` remains an explicit plaintext UDP API for local tests.
pub struct RelayServer {
    relay: Arc<Relay>,
    secure: Option<(TcpListener, quinn::Endpoint, rustls::ServerConfig)>,
}

impl RelayServer {
    fn relay(udp: Option<UdpSocket>, options: RelayOptions) -> Arc<Relay> {
        Arc::new(Relay {
            registrations: Mutex::default(),
            by_route: Mutex::default(),
            challenges: Mutex::default(),
            mailbox: Mutex::default(),
            stats: Mutex::default(),
            udp,
            mailbox_items: options.mailbox_items,
            idle: options.idle,
            options,
            client_limits: Mutex::default(),
            source_limits: Mutex::default(),
            push_tokens: Mutex::default(),
            started: Instant::now(),
            departed: Mutex::default(),
            features: Mutex::default(),
            absent_sent: Mutex::default(),
            watches: Mutex::default(),
        })
    }

    pub async fn bind(udp: SocketAddr, options: RelayOptions) -> Result<Self> {
        let relay = Self::relay(Some(UdpSocket::bind(udp).await?), options);
        Ok(Self {
            relay,
            secure: None,
        })
    }

    pub async fn bind_secure(
        addr: SocketAddr,
        config: rustls::ServerConfig,
        options: RelayOptions,
    ) -> Result<Self> {
        anyhow::ensure!(
            config.alpn_protocols == [RELAY_ALPN.to_vec()],
            "relay ALPN required"
        );
        let tcp = TcpListener::bind(addr).await?;
        // Port zero in tests chooses one available port for both transports.
        let quic = quinn::Endpoint::server(
            relay_tls::quic_server_config(config.clone())?,
            tcp.local_addr()?,
        )?;
        Ok(Self {
            relay: Self::relay(None, options),
            secure: Some((tcp, quic, config)),
        })
    }

    pub fn udp_addr(&self) -> Result<SocketAddr> {
        match &self.secure {
            Some((_, endpoint, _)) => Ok(endpoint.local_addr()?),
            None => Ok(self
                .relay
                .udp
                .as_ref()
                .expect("legacy socket")
                .local_addr()?),
        }
    }

    pub fn tcp_addr(&self) -> Option<SocketAddr> {
        self.secure
            .as_ref()
            .and_then(|(listener, _, _)| listener.local_addr().ok())
    }

    pub async fn serve(self) -> Result<()> {
        let relay = self.relay;
        let mut tasks = tokio::task::JoinSet::<Result<()>>::new();
        let permits = Arc::new(Semaphore::new(
            relay.options.max_clients.saturating_mul(4).max(1),
        ));

        if let Some((listener, endpoint, config)) = self.secure {
            log::info!("muster-relay TCP/TLS + UDP/QUIC {}", listener.local_addr()?);
            let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
            let tcp_relay = relay.clone();
            let tcp_permits = permits.clone();
            tasks.spawn(async move {
                let mut clients = tokio::task::JoinSet::new();
                loop {
                    tokio::select! {
                        accepted = listener.accept() => {
                            let (stream, peer) = accepted?;
                            let Ok(permit) = tcp_permits.clone().try_acquire_owned() else { continue; };
                            let relay = tcp_relay.clone();
                            let acceptor = acceptor.clone();
                            clients.spawn(async move {
                                let _permit = permit;
                                let Ok(Ok(stream)) = tokio::time::timeout(Duration::from_secs(3), acceptor.accept(stream)).await else { return; };
                                if stream.get_ref().1.alpn_protocol() != Some(RELAY_ALPN) { return; }
                                let (mut reader, mut writer) = tokio::io::split(stream);
                                let (route, mut rx) = Route::channel();
                                // Keep frame reads in their own future: cancelling read_exact
                                // on every outbound frame would corrupt stream framing.
                                let incoming = async {
                                    loop {
                                        let timeout = if relay.sender_id(&route).is_some() { relay.idle } else { Duration::from_secs(3) };
                                        let Ok(Ok(frame)) = tokio::time::timeout(timeout, relay_tls::read_frame(&mut reader)).await else { break; };
                                        relay.handle(&frame, route.clone(), peer).await;
                                    }
                                };
                                let outgoing = async {
                                    while let Some(frame) = rx.recv().await {
                                        if !matches!(tokio::time::timeout(Duration::from_secs(3), relay_tls::write_frame(&mut writer, &frame)).await, Ok(Ok(()))) { break; }
                                    }
                                };
                                tokio::select! { _ = incoming => {}, _ = outgoing => {} }
                                relay.disconnect(&route);
                            });
                        }
                        _ = clients.join_next(), if !clients.is_empty() => {}
                    }
                }
            });
            let quic_relay = relay.clone();
            tasks.spawn(async move {
                let mut clients = tokio::task::JoinSet::new();
                loop {
                    tokio::select! {
                        incoming = endpoint.accept() => {
                            let Some(incoming) = incoming else { return Ok(()); };
                            let Ok(permit) = permits.clone().try_acquire_owned() else { incoming.refuse(); continue; };
                            let relay = quic_relay.clone();
                            clients.spawn(async move {
                                let _permit = permit;
                                let Ok(Ok(connection)) = tokio::time::timeout(Duration::from_secs(3), incoming).await else { return; };
                                let (route, mut rx) = Route::channel();
                                loop {
                                    let idle = if relay.sender_id(&route).is_some() { relay.idle } else { Duration::from_secs(3) };
                                    tokio::select! {
                                        incoming = tokio::time::timeout(idle, connection.read_datagram()) => {
                                            let Ok(Ok(frame)) = incoming else { break; };
                                            relay.handle(&frame, route.clone(), connection.remote_address()).await;
                                        }
                                        outgoing = rx.recv() => {
                                            let Some(frame) = outgoing else { break; };
                                            // If PMTU cannot carry an encapsulated PeerLink frame,
                                            // close this carrier so the client chooses TCP.
                                            if connection.send_datagram(frame.into()).is_err() { break; }
                                        }
                                    }
                                }
                                connection.close(0u8.into(), b"relay carrier closed");
                                relay.disconnect(&route);
                            });
                        }
                        _ = clients.join_next(), if !clients.is_empty() => {}
                    }
                }
            });
        }

        let stats_relay = relay.clone();
        tasks.spawn(async move {
            let period = stats_relay.idle.clamp(Duration::from_millis(1), Duration::from_secs(30));
            let mut tick = tokio::time::interval(period);
            loop {
                tick.tick().await;
                stats_relay.expire();
                let s = std::mem::take(&mut *stats_relay.stats.lock().unwrap());
                let clients = stats_relay.registrations.lock().unwrap().len();
                let keys = stats_relay.mailbox.lock().unwrap().len();
                log::info!("clients={clients} mailbox_keys={keys} fwd={} fwd_bytes={} dropped={} auth_failures={} rate_limited={} pushes={}",
                    s.forwarded, s.forwarded_bytes, s.dropped, s.auth_failures, s.rate_limited, s.pushes);
            }
        });
        if relay.udp.is_some() {
            tasks.spawn(async move {
                let udp = relay.udp.as_ref().expect("legacy socket");
                log::warn!("explicit plaintext UDP relay {}", udp.local_addr()?);
                let mut buf = vec![0u8; 65536];
                loop {
                    let (n, from) = udp.recv_from(&mut buf).await?;
                    relay.handle(&buf[..n], Route::Udp(from), from).await;
                }
            });
        }
        // Cancelling serve aborts all listeners and their connection tasks.
        match tasks.join_next().await {
            Some(result) => result?,
            None => Ok(()),
        }
    }
}
