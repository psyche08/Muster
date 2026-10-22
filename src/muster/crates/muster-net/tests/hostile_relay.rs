//! The relay is untrusted (R1): with a hostile one in the path, tampering
//! and withholding cost availability only, replays change nothing, and
//! recovery is automatic once the relay behaves. Also R2: the creator
//! going away leaves the other members talking.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use muster_core::roster::{Author, DeviceKind, DeviceRecord, Roster};
use muster_core::{DeviceId, HwSigner, Invite, LocalPolicy, Scope, SoftSigner};
use muster_net::magicsock::MagicOptions;
use muster_net::node::{Node, NodeConfig, RelayConfig, ServiceHandler};
use muster_net::pair::{Candidate, Verdict};
use muster_net::relay_proto::DELIVER;
use muster_net::stream::{Hello, read_message, write_message};
use muster_net::{SharedPolicy, SharedRoster};
use quinn::{RecvStream, SendStream};
use tokio::net::UdpSocket;

const PASS: u8 = 0;
const TAMPER: u8 = 1;
const DROP: u8 = 2;

/// A datagram the proxy forwarded: the socket it left on, the explicit
/// destination for downstream sends, and the bytes.
type Recorded = (Arc<UdpSocket>, Option<SocketAddr>, Vec<u8>);

/// A UDP man in the middle between the nodes and the real relay.
struct Proxy {
    addr: SocketAddr,
    mode: Arc<AtomicU8>,
    /// Every datagram forwarded, with where it went, for replay.
    recorded: Arc<Mutex<Vec<Recorded>>>,
}

/// Corrupts a delivered QUIC packet where it must matter: the byte after
/// the first header byte (version or connection id) and a byte inside the
/// authentication tag. Only deliveries are touched: both peers sit behind
/// this proxy, so a forwarded packet passes it twice and an XOR applied on
/// both legs would cancel out.
fn mangle(mode: u8, bytes: &[u8]) -> Option<Vec<u8>> {
    match mode {
        DROP => None,
        TAMPER if bytes.len() > 60 && bytes[0] == DELIVER => {
            let mut out = bytes.to_vec();
            out[35] ^= 0xff;
            let last = out.len() - 8;
            out[last] ^= 0xff;
            Some(out)
        }
        _ => Some(bytes.to_vec()),
    }
}

impl Proxy {
    async fn start(relay: SocketAddr) -> Proxy {
        let front = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let addr = front.local_addr().unwrap();
        let mode = Arc::new(AtomicU8::new(PASS));
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let (m, r, f) = (mode.clone(), recorded.clone(), front.clone());
        tokio::spawn(async move {
            let mut upstreams: HashMap<SocketAddr, Arc<UdpSocket>> = HashMap::new();
            let mut buf = vec![0u8; 2048];
            loop {
                let Ok((n, client)) = f.recv_from(&mut buf).await else {
                    return;
                };
                let upstream = match upstreams.get(&client) {
                    Some(u) => u.clone(),
                    None => {
                        let u = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
                        u.connect(relay).await.unwrap();
                        upstreams.insert(client, u.clone());
                        let (u2, f2, m2, r2) = (u.clone(), f.clone(), m.clone(), r.clone());
                        tokio::spawn(async move {
                            let mut buf = vec![0u8; 2048];
                            loop {
                                let Ok(n) = u2.recv(&mut buf).await else {
                                    return;
                                };
                                if let Some(bytes) = mangle(m2.load(Ordering::Relaxed), &buf[..n]) {
                                    r2.lock().unwrap().push((
                                        f2.clone(),
                                        Some(client),
                                        bytes.clone(),
                                    ));
                                    let _ = f2.send_to(&bytes, client).await;
                                }
                            }
                        });
                        u
                    }
                };
                if let Some(bytes) = mangle(m.load(Ordering::Relaxed), &buf[..n]) {
                    r.lock()
                        .unwrap()
                        .push((upstream.clone(), None, bytes.clone()));
                    let _ = upstream.send(&bytes).await;
                }
            }
        });
        Proxy {
            addr,
            mode,
            recorded,
        }
    }

    fn set(&self, mode: u8) {
        self.mode.store(mode, Ordering::Relaxed);
    }

    /// Sends every recorded datagram again, in order.
    async fn replay(&self) -> usize {
        let items: Vec<_> = self.recorded.lock().unwrap().clone();
        for (sock, to, bytes) in &items {
            match to {
                Some(addr) => {
                    let _ = sock.send_to(bytes, addr).await;
                }
                None => {
                    let _ = sock.send(bytes).await;
                }
            }
        }
        items.len()
    }
}

struct Echo;

impl ServiceHandler for Echo {
    fn handle(
        &self,
        _node: Arc<Node>,
        _peer: DeviceId,
        _hello: Hello,
        mut send: SendStream,
        mut recv: RecvStream,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        Box::pin(async move {
            while let Ok(Some(message)) = read_message(&mut recv, 1024).await {
                if write_message(&mut send, &message).await.is_err() {
                    break;
                }
            }
            let _ = send.finish();
        })
    }
}

async fn relay() -> SocketAddr {
    let server = muster_relay::RelayServer::bind(
        "127.0.0.1:0".parse().unwrap(),
        muster_relay::RelayOptions::default(),
    )
    .await
    .unwrap();
    let addr = server.udp_addr().unwrap();
    tokio::spawn(async move {
        let _ = server.serve().await;
    });
    addr
}

fn config(relay: SocketAddr, relay_only: bool) -> NodeConfig {
    NodeConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        relay: RelayConfig::Udp(relay),
        options: MagicOptions {
            relay_only,
            mobile: false,
            allow_loopback: true,
            extra_candidates: Vec::new(),
            push_token: None,
        },
        datagrams: false,
        build: Default::default(),
    }
}

struct Member {
    node: Arc<Node>,
    id: DeviceId,
    signer: Arc<dyn HwSigner>,
    roster: SharedRoster,
}

async fn admin(relay: SocketAddr, relay_only: bool) -> (Member, Invite) {
    let signer: Arc<dyn HwSigner> = Arc::new(SoftSigner::generate());
    let roster: SharedRoster = Arc::new(RwLock::new(Roster::new()));
    {
        let mut r = roster.write().unwrap();
        let genesis = Author {
            roster: &r,
            signer: signer.as_ref(),
        }
        .genesis(
            "hostile",
            (Ipv4Addr::new(10, 66, 0, 0), 24),
            DeviceRecord::new(signer.public_key(), "admin", DeviceKind::Desktop),
        )
        .unwrap();
        r.insert(genesis).unwrap();
    }
    let policy: SharedPolicy = Arc::new(RwLock::new(LocalPolicy::default()));
    let node = Node::start(
        config(relay, relay_only),
        signer.clone(),
        roster.clone(),
        policy,
    )
    .await
    .unwrap();
    node.set_handler(Arc::new(Echo));
    node.pairing()
        .set_approver(Arc::new(|candidate: Candidate| {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let mut record = candidate.record;
            record.scopes.insert(Scope::AgentView);
            let _ = tx.send(Verdict::Approve(record));
            rx
        }));
    let network_id = roster.read().unwrap().network_id().unwrap();
    let invite = Invite::new(
        network_id,
        "hostile",
        signer.public_key(),
        &relay.to_string(),
        3600,
    );
    node.open_invite(invite.clone());
    let id = signer.public_key().device_id();
    (
        Member {
            node,
            id,
            signer,
            roster,
        },
        invite,
    )
}

async fn joiner(relay: SocketAddr, relay_only: bool, name: &str) -> Member {
    let signer: Arc<dyn HwSigner> = Arc::new(SoftSigner::generate());
    let roster: SharedRoster = Arc::new(RwLock::new(Roster::new()));
    let node = Node::start(
        config(relay, relay_only),
        signer.clone(),
        roster.clone(),
        Arc::new(RwLock::new(LocalPolicy::default())),
    )
    .await
    .unwrap();
    node.set_handler(Arc::new(Echo));
    let id = signer.public_key().device_id();
    let _ = name;
    Member {
        node,
        id,
        signer,
        roster,
    }
}

async fn wait_registered(node: &Node) {
    let started = Instant::now();
    while node.relay_state() != muster_net::magicsock::RelayState::Registered {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "relay registration"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn echo_works(from: &Member, to: &Member) -> bool {
    let Ok(Ok((mut send, mut recv))) =
        tokio::time::timeout(Duration::from_secs(15), from.node.open(to.id, "echo", 1)).await
    else {
        return false;
    };
    if write_message(&mut send, b"ping").await.is_err() {
        return false;
    }
    matches!(tokio::time::timeout(Duration::from_secs(10), read_message(&mut recv, 1024)).await, Ok(Ok(Some(reply))) if reply == b"ping")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tamper_replay_and_withhold_cost_availability_only() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();
    let relay_addr = relay().await;
    let proxy = Proxy::start(relay_addr).await;
    let (admin, invite) = admin(proxy.addr, true).await;
    wait_registered(&admin.node).await;

    // Tampered QUIC packets: pairing cannot complete, nothing is recorded.
    proxy.set(TAMPER);
    let victim = joiner(proxy.addr, true, "victim").await;
    wait_registered(&victim.node).await;
    let record = DeviceRecord::new(victim.signer.public_key(), "victim", DeviceKind::Phone);
    let outcome =
        tokio::time::timeout(Duration::from_secs(20), victim.node.join(&invite, record)).await;
    assert!(
        !matches!(outcome, Ok(Ok(()))),
        "pairing succeeded through a tampering relay"
    );
    assert!(
        victim.roster.read().unwrap().is_empty(),
        "the joiner learned a roster through tampered packets"
    );
    assert_eq!(
        admin
            .roster
            .read()
            .unwrap()
            .state()
            .active_members()
            .count(),
        1
    );
    victim.node.shutdown().await;

    // The relay behaves again: a fresh joiner pairs and reaches the admin.
    proxy.set(PASS);
    let (admin, invite) = {
        // A new invite: the tampered attempt consumed the previous one's
        // pairing window on the admin side.
        let network_id = admin.roster.read().unwrap().network_id().unwrap();
        let invite = Invite::new(
            network_id,
            "hostile",
            admin.signer.public_key(),
            &proxy.addr.to_string(),
            3600,
        );
        admin.node.open_invite(invite.clone());
        (admin, invite)
    };
    let member = joiner(proxy.addr, true, "member").await;
    wait_registered(&member.node).await;
    let record = DeviceRecord::new(member.signer.public_key(), "member", DeviceKind::Phone);
    tokio::time::timeout(Duration::from_secs(30), member.node.join(&invite, record))
        .await
        .expect("pairing in time")
        .expect("pairing through a well-behaved relay");
    assert!(echo_works(&member, &admin).await, "echo over the relay");
    let entries_before = (
        admin.roster.read().unwrap().len(),
        member.roster.read().unwrap().len(),
    );

    // Replaying everything the relay ever carried changes nothing.
    let replayed = proxy.replay().await;
    assert!(replayed > 20, "recorded {replayed} datagrams");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(echo_works(&member, &admin).await, "link survives replay");
    assert_eq!(
        (
            admin.roster.read().unwrap().len(),
            member.roster.read().unwrap().len()
        ),
        entries_before
    );
    assert_eq!(
        member.roster.read().unwrap().state(),
        admin.roster.read().unwrap().state()
    );

    // Withholding: the members lose each other, and nothing else.
    proxy.set(DROP);
    let started = Instant::now();
    loop {
        let connected = member
            .node
            .peers()
            .iter()
            .any(|p| p.id == admin.id && p.connected);
        if !connected {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "link never noticed the relay withholding"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert!(
        !echo_works(&member, &admin).await,
        "echo through a withholding relay"
    );
    assert_eq!(
        member.roster.read().unwrap().state(),
        admin.roster.read().unwrap().state()
    );
    admin.node.shutdown().await;
    member.node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn members_outlive_the_creator() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();
    let relay_addr = relay().await;
    let (admin, invite) = admin(relay_addr, false).await;
    wait_registered(&admin.node).await;
    let mut members = Vec::new();
    for name in ["b", "c"] {
        let m = joiner(relay_addr, false, name).await;
        wait_registered(&m.node).await;
        let record = DeviceRecord::new(m.signer.public_key(), name, DeviceKind::Desktop);
        tokio::time::timeout(Duration::from_secs(30), m.node.join(&invite, record))
            .await
            .expect("pairing in time")
            .expect("pairing");
        // The invite is single use: open it again for the next joiner.
        admin.node.open_invite(invite.clone());
        members.push(m);
    }
    // Both members learn of each other through gossip before the admin goes.
    let started = Instant::now();
    loop {
        let known = members
            .iter()
            .all(|m| m.roster.read().unwrap().state().active_members().count() == 3);
        if known {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "gossip did not converge"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    admin.node.shutdown().await;
    assert!(
        echo_works(&members[0], &members[1]).await,
        "b reaches c without the creator"
    );
    assert!(
        echo_works(&members[1], &members[0]).await,
        "c reaches b without the creator"
    );
    for m in &members {
        m.node.shutdown().await;
    }
}
