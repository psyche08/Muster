//! Presence end to end on loopback: a relay, a phone and a desktop (or a
//! registered device that never answers). What the phone shows and when it
//! dials follow evidence, not one link's instant state.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use muster_core::roster::{Author, DeviceKind, DeviceRecord, Roster};
use muster_core::{DeviceId, HwSigner, LocalPolicy, SoftSigner};
use muster_net::magicsock::MagicOptions;
use muster_net::node::{Node, NodeConfig, RelayConfig};
use muster_net::presence::{MAX_RETRIES, View};
use muster_net::relay_proto::{Frame, auth_signature};

async fn spawn_relay(absent_grace: Duration) -> SocketAddr {
    let server = muster_relay::RelayServer::bind(
        "127.0.0.1:0".parse().unwrap(),
        muster_relay::RelayOptions {
            absent_grace,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let udp = server.udp_addr().unwrap();
    tokio::spawn(async move {
        let _ = server.serve().await;
    });
    udp
}

struct Network {
    relay: SocketAddr,
    roster: Roster,
    phone: Arc<dyn HwSigner>,
    desktop: Arc<dyn HwSigner>,
}

impl Network {
    async fn new(absent_grace: Duration) -> Self {
        let relay = spawn_relay(absent_grace).await;
        let desktop: Arc<dyn HwSigner> = Arc::new(SoftSigner::generate());
        let phone: Arc<dyn HwSigner> = Arc::new(SoftSigner::generate());
        let mut roster = Roster::new();
        let genesis = Author {
            roster: &roster,
            signer: desktop.as_ref(),
        }
        .genesis(
            "presence",
            (Ipv4Addr::new(192, 168, 15, 0), 24),
            DeviceRecord::new(desktop.public_key(), "mac", DeviceKind::Desktop),
        )
        .unwrap();
        roster.insert(genesis).unwrap();
        let add = Author {
            roster: &roster,
            signer: desktop.as_ref(),
        }
        .add(DeviceRecord::new(
            phone.public_key(),
            "phone",
            DeviceKind::Phone,
        ))
        .unwrap();
        roster.insert(add).unwrap();
        Self {
            relay,
            roster,
            phone,
            desktop,
        }
    }

    async fn start(&self, signer: &Arc<dyn HwSigner>, phone: bool) -> Arc<Node> {
        Node::start(
            NodeConfig {
                bind: "127.0.0.1:0".parse().unwrap(),
                relay: RelayConfig::Udp(self.relay),
                options: MagicOptions {
                    relay_only: phone,
                    mobile: phone,
                    allow_loopback: true,
                    ..Default::default()
                },
                datagrams: false,
                build: Default::default(),
            },
            signer.clone(),
            Arc::new(RwLock::new(self.roster.clone())),
            Arc::new(RwLock::new(LocalPolicy::default())),
        )
        .await
        .unwrap()
    }

    fn mac(&self) -> DeviceId {
        self.desktop.public_key().device_id()
    }
}

async fn wait_for(what: &str, timeout: Duration, check: impl Fn() -> bool) {
    let started = Instant::now();
    while !check() {
        assert!(started.elapsed() < timeout, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn sent(node: &Node, peer: &DeviceId) -> u64 {
    node.sock()
        .and_then(|s| s.path(peer))
        .map_or(0, |p| p.counters.tx_relay)
}

/// The relay's absence report stops dialing at once, many callers share it
/// without dialing, and the relay's PRESENT brings the member back.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_absent_mac_is_not_dialed_until_the_relay_says_it_registered() {
    let net = Network::new(Duration::from_millis(300)).await;
    let mac = net.mac();
    let phone = net.start(&net.phone, true).await;
    wait_for(
        "the relay's absence report",
        Duration::from_secs(10),
        || phone.presence(&mac) == View::Offline,
    )
    .await;
    // The absence verdict cancels an in-flight Quinn handshake. Its final
    // close/retransmit can flush after the presence event, so first require
    // a bounded quiet window, then assert no further dialing traffic.
    let settle = Instant::now();
    let mut before = sent(&phone, &mac);
    loop {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let current = sent(&phone, &mac);
        if current == before {
            break;
        }
        assert!(
            settle.elapsed() < Duration::from_secs(2),
            "cancelled handshake did not drain"
        );
        before = current;
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(sent(&phone, &mac), before, "an absent member is not dialed");
    // Concurrent requests share the verdict, none of them dials.
    let attempts = (0..8).map(|_| {
        let phone = phone.clone();
        tokio::spawn(async move { phone.connect(mac).await })
    });
    for attempt in attempts {
        assert!(matches!(
            attempt.await.unwrap(),
            Err(muster_net::Error::Offline(_))
        ));
    }
    assert_eq!(sent(&phone, &mac), before);
    // The Mac starts: the relay tells the phone, which connects.
    let desktop = net.start(&net.desktop, false).await;
    wait_for("PRESENT and a link", Duration::from_secs(10), || {
        phone.presence(&mac) == View::Online && phone.connection_to(&mac).is_some()
    })
    .await;
    desktop.shutdown().await;
    phone.shutdown().await;
}

/// A lost link that comes back within its retries is never shown offline.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_link_lost_and_regained_is_never_shown_offline() {
    let net = Network::new(Duration::from_secs(10)).await;
    let mac = net.mac();
    let desktop = net.start(&net.desktop, false).await;
    let phone = net.start(&net.phone, true).await;
    wait_for("the first link", Duration::from_secs(10), || {
        phone.presence(&mac) == View::Online
    })
    .await;
    let first = phone.connection_to(&mac).unwrap();
    first.close(0u32.into(), b"test: missed heartbeat");
    let started = Instant::now();
    let mut seen = Vec::new();
    while started.elapsed() < Duration::from_secs(5) {
        let view = phone.presence(&mac);
        if seen.last() != Some(&view) {
            seen.push(view);
        }
        if view == View::Online
            && phone
                .connection_to(&mac)
                .is_some_and(|c| c.stable_id() != first.stable_id())
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        seen.iter().all(|v| v.online()),
        "shown offline while reconnecting: {seen:?}"
    );
    assert_eq!(seen.last(), Some(&View::Online));
    desktop.shutdown().await;
    phone.shutdown().await;
}

/// Registered with the relay but never answering: no relay authority, so the
/// bounded reconnects decide; then it is left alone until its probe time.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_member_that_never_answers_is_unreachable_after_its_retries() {
    let net = Network::new(Duration::from_millis(300)).await;
    let mac = net.mac();
    // The Mac's identity holds a relay registration but runs no node.
    let silent = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let register = |frame: Frame| frame.encode();
    silent
        .send_to(&register(Frame::Hello { id: mac }), net.relay)
        .await
        .unwrap();
    let mut buf = [0u8; 2048];
    let n = silent.recv(&mut buf).await.unwrap();
    let Ok(Frame::Challenge { nonce }) = Frame::decode(&buf[..n]) else {
        panic!("no challenge")
    };
    let auth = Frame::Auth {
        id: mac,
        key: net.desktop.public_key(),
        nonce,
        signature: auth_signature(net.desktop.as_ref(), &nonce).unwrap(),
    };
    silent.send_to(&auth.encode(), net.relay).await.unwrap();
    let phone = net.start(&net.phone, true).await;
    // The first dial and MAX_RETRIES more, each 3 s, with 1 and 2 s backoff.
    let budget = Duration::from_secs(3 * (1 + u64::from(MAX_RETRIES)) + 1 + 2 + 4);
    let started = Instant::now();
    let mut views = Vec::new();
    while phone.presence(&mac) != View::Unreachable {
        assert!(
            started.elapsed() < budget,
            "still {:?}",
            phone.presence(&mac)
        );
        let view = phone.presence(&mac);
        if views.last() != Some(&view) {
            views.push(view);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(views, [View::Connecting], "never shown offline before");
    assert!(
        started.elapsed() >= Duration::from_secs(9),
        "three attempts first"
    );
    let before = sent(&phone, &mac);
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(sent(&phone, &mac), before, "left alone until its probe");
    // A refresh tries it again.
    phone.refresh_presence();
    assert_eq!(phone.presence(&mac), View::Connecting);
    phone.shutdown().await;
}
