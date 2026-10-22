//! Real TLS/QUIC through a fault-injecting dual-port proxy. No trust bypass.
use crate::{
    magicsock::{MagicOptions, RelayState},
    node::{Node, NodeConfig, RelayConfig},
    relay_tls,
    secure_relay::{Carrier, SecureRelay, Strategy},
};
use muster_core::{
    LocalPolicy, SharedSigner, SoftSigner,
    roster::{Author, DeviceKind, DeviceRecord, Roster},
};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    net::{TcpListener, UdpSocket},
    sync::watch,
    task::JoinSet,
};

struct Fixture {
    addr: SocketAddr,
    config: rustls::ClientConfig,
    udp: watch::Sender<bool>,
    tcp: watch::Sender<bool>,
    udp_limit: Arc<AtomicUsize>,
    tasks: JoinSet<()>,
}
impl Fixture {
    async fn new() -> Self {
        let cert = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
        let mut server = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.cert.der().clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()).into(),
        )
        .unwrap();
        server.alpn_protocols = vec![relay_tls::RELAY_ALPN.to_vec()];
        let mut roots = rustls::RootCertStore::empty();
        roots.add(cert.cert.der().clone()).unwrap();
        let config = relay_tls::client_config_with_roots(roots).unwrap();
        let relay = muster_relay::RelayServer::bind_secure(
            "127.0.0.1:0".parse().unwrap(),
            server,
            muster_relay::RelayOptions::default(),
        )
        .await
        .unwrap();
        let target = relay.udp_addr().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let udp_sock = Arc::new(UdpSocket::bind(addr).await.unwrap());
        let (udp, udp_enabled) = watch::channel(true);
        let udp_limit = Arc::new(AtomicUsize::new(65535));
        let (tcp, tcp_enabled) = watch::channel(true);
        let mut tasks = JoinSet::new();
        tasks.spawn(async move {
            relay.serve().await.unwrap();
        });
        tasks.spawn(async move {
            let mut connections=JoinSet::new();
            loop {
                tokio::select! {
                    accepted=listener.accept()=> {
                        let (mut client,_)=accepted.unwrap();
                        if !*tcp_enabled.borrow() {continue;}
                        let mut enabled=tcp_enabled.clone();
                        enabled.borrow_and_update();
                        connections.spawn(async move {
                            let Ok(mut remote)=tokio::net::TcpStream::connect(target).await else {return};
                            tokio::select! {
                                _=tokio::io::copy_bidirectional(&mut client,&mut remote)=> {},
                                _=enabled.changed()=> {},
                            }
                        });
                    }
                    _=connections.join_next(), if !connections.is_empty()=> {},
                }
            }
        });
        let limit = udp_limit.clone();
        tasks.spawn(async move {
            let mut upstreams = HashMap::<SocketAddr, Arc<UdpSocket>>::new();
            let mut readers = JoinSet::new();
            let mut buf = [0u8; 65535];
            loop {
                let (n, peer) = udp_sock.recv_from(&mut buf).await.unwrap();
                if !*udp_enabled.borrow() || n > limit.load(Ordering::Relaxed) {
                    continue;
                }
                let upstream = if let Some(s) = upstreams.get(&peer) {
                    s.clone()
                } else {
                    let s = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
                    s.connect(target).await.unwrap();
                    upstreams.insert(peer, s.clone());
                    let source = s.clone();
                    let downstream = udp_sock.clone();
                    let enabled = udp_enabled.clone();
                    let limit = limit.clone();
                    readers.spawn(async move {
                        let mut bytes = [0u8; 65535];
                        while let Ok(n) = source.recv(&mut bytes).await {
                            if *enabled.borrow() && n <= limit.load(Ordering::Relaxed) {
                                let _ = downstream.send_to(&bytes[..n], peer).await;
                            }
                        }
                    });
                    s
                };
                upstream.send(&buf[..n]).await.unwrap();
            }
        });
        Self {
            addr,
            config,
            udp,
            tcp,
            udp_limit,
            tasks,
        }
    }
    fn link(&self) -> SecureRelay {
        self.link_with(Strategy::Standby)
    }
    fn link_with(&self, strategy: Strategy) -> SecureRelay {
        SecureRelay::with_config(
            self.addr.to_string(),
            Arc::new(SoftSigner::generate()),
            self.config.clone(),
            strategy,
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.tasks.abort_all();
    }
}

async fn wait(what: &str, timeout: Duration, mut check: impl FnMut() -> bool) {
    tokio::time::timeout(timeout, async {
        while !check() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out: {what}"));
}
async fn mailbox(link: &SecureRelay, value: u8) {
    use crate::relay_proto::Frame;
    let key = [value; 32];
    link.send(
        Frame::MbPut {
            key,
            item: vec![value; 1000],
        }
        .encode(),
    )
    .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let bytes = std::future::poll_fn(|cx| link.poll_recv(cx)).await.unwrap();
            if let Frame::MbAck { key: got, .. } = Frame::decode(&bytes).unwrap() {
                assert_eq!(got, key);
                break;
            }
        }
    })
    .await
    .expect("mailbox on selected secure carrier");
}

/// A phone keeps one carrier: QUIC while it connects, TLS only when it
/// cannot, QUIC again after a network change. No standby connection and
/// no periodic probes; the relay acknowledges activation once.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn phone_uses_quic_first_and_tls_only_as_a_fallback() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();
    let f = Fixture::new().await;
    let link = f.link_with(Strategy::Fallback);
    wait("QUIC carries", Duration::from_secs(4), || {
        link.active() == Some(Carrier::Quic)
    })
    .await;
    mailbox(&link, 1).await;
    // Well past a standby probe period: still one carrier, still live
    // without probe answers, and activation was acknowledged.
    tokio::time::sleep(Duration::from_secs(6)).await;
    {
        let state = link.state.lock().unwrap();
        assert!(state.paths[0].ready && !state.paths[1].ready);
        assert_eq!(state.paths[1].generation, 0, "TLS was never tried");
        assert_eq!(state.activated.map(|(c, _)| c), Some(Carrier::Quic));
    }
    assert_eq!(link.active(), Some(Carrier::Quic));
    mailbox(&link, 2).await;
    link.shutdown();

    // UDP blocked from the start: QUIC cannot connect, TLS carries.
    f.udp.send_replace(false);
    let link = f.link_with(Strategy::Fallback);
    wait("TLS carries without UDP", Duration::from_secs(9), || {
        link.active() == Some(Carrier::Tls)
    })
    .await;
    mailbox(&link, 3).await;
    // UDP back and the network changes: QUIC first again.
    f.udp.send_replace(true);
    link.network_changed();
    wait(
        "QUIC again after a network change",
        Duration::from_secs(5),
        || link.active() == Some(Carrier::Quic),
    )
    .await;
    mailbox(&link, 4).await;
    assert!(!link.state.lock().unwrap().paths[1].ready);
    link.shutdown();
    tokio::time::sleep(Duration::from_millis(50)).await;
}

/// A single lost datagram must not destroy an authenticated carrier. The
/// existing blackhole test below still requires failover on a persistent loss.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lost_probe_is_retried_inside_original_deadline_without_reconnecting() {
    let f = Fixture::new().await;
    let link = f.link_with(Strategy::Fallback);
    wait("QUIC authenticated", Duration::from_secs(4), || {
        link.active() == Some(Carrier::Quic)
    })
    .await;
    let (generation, previous_ack) = {
        let state = link.state.lock().unwrap();
        (state.paths[0].generation, state.paths[0].last_ack)
    };
    f.udp.send_replace(false);
    // Uses the same probe path as a phone network-change verification.
    link.network_changed();
    tokio::time::sleep(Duration::from_millis(1200)).await;
    f.udp.send_replace(true);
    wait(
        "probe retransmission acknowledged",
        Duration::from_millis(1700),
        || link.state.lock().unwrap().paths[0].last_ack > previous_ack,
    )
    .await;
    {
        let state = link.state.lock().unwrap();
        assert_eq!(
            state.paths[0].generation, generation,
            "no replacement connection"
        );
        assert!(state.paths[0].ready);
        assert_eq!(state.paths[1].generation, 0, "no unnecessary TLS fallback");
    }
    mailbox(&link, 9).await;
    link.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dual_carrier_blackhole_failover_recovery_and_retirement() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();
    let f = Fixture::new().await;
    let link = f.link();
    wait("both authenticated", Duration::from_secs(4), || {
        link.state.lock().unwrap().paths.iter().all(|p| p.ready)
    })
    .await;
    mailbox(&link, 1).await;
    f.udp.send_replace(false);
    wait(
        "UDP failed and TCP selected",
        Duration::from_secs(9),
        || link.active() == Some(Carrier::Tls) && !link.state.lock().unwrap().paths[0].ready,
    )
    .await;
    mailbox(&link, 2).await;
    f.udp.send_replace(true);
    wait("UDP reauthenticated", Duration::from_secs(12), || {
        link.state.lock().unwrap().paths[0].ready
    })
    .await;
    f.tcp.send_replace(false);
    wait(
        "TCP failed and UDP selected",
        Duration::from_secs(9),
        || link.active() == Some(Carrier::Quic),
    )
    .await;
    mailbox(&link, 3).await;
    link.network_changed();
    wait("network change resumes UDP", Duration::from_secs(5), || {
        link.active() == Some(Carrier::Quic)
    })
    .await;
    mailbox(&link, 4).await;
    link.shutdown();
    tokio::time::sleep(Duration::from_millis(50)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn untrusted_cert_never_registers_or_downgrades() {
    let f = Fixture::new().await;
    let link = SecureRelay::new(
        f.addr.to_string(),
        Arc::new(SoftSigner::generate()),
        Strategy::Standby,
    )
    .unwrap();
    tokio::time::sleep(Duration::from_millis(3300)).await;
    assert_eq!(link.status(), RelayState::Failed);
    assert_eq!(link.active(), None);
}

// Exercise inner, hardware-identity-authenticated QUIC over each secure relay
// carrier. This catches nesting MTU and MagicSock wakeup/forwarding failures.
struct Echo;
impl crate::node::ServiceHandler for Echo {
    fn handle(
        &self,
        _node: Arc<Node>,
        _peer: muster_core::DeviceId,
        _hello: crate::stream::Hello,
        mut send: quinn::SendStream,
        mut recv: quinn::RecvStream,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        Box::pin(async move {
            let bytes = recv.read_to_end(1024 * 1024).await.unwrap();
            send.write_all(&bytes).await.unwrap();
            send.finish().unwrap();
        })
    }
}
async fn echo(a: &Arc<Node>, b: &Arc<Node>) {
    let bytes = vec![42u8; 256 * 1024];
    let (mut send, mut recv) = a.open(b.device_id(), "echo", 1).await.unwrap();
    send.write_all(&bytes).await.unwrap();
    send.finish().unwrap();
    assert_eq!(recv.read_to_end(bytes.len()).await.unwrap(), bytes);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn peer_quic_large_bidirectional_rpc_survives_carrier_changes() {
    let f = Fixture::new().await;
    let a: SharedSigner = Arc::new(SoftSigner::generate());
    let b: SharedSigner = Arc::new(SoftSigner::generate());
    let mut roster = Roster::new();
    let entry = Author {
        roster: &roster,
        signer: a.as_ref(),
    }
    .genesis(
        "secure",
        ("10.44.0.0".parse().unwrap(), 24),
        DeviceRecord::new(a.public_key(), "a", DeviceKind::Desktop),
    )
    .unwrap();
    roster.insert(entry).unwrap();
    let entry = Author {
        roster: &roster,
        signer: a.as_ref(),
    }
    .add(DeviceRecord::new(b.public_key(), "b", DeviceKind::Desktop))
    .unwrap();
    roster.insert(entry).unwrap();
    let mut nodes = Vec::new();
    for signer in [a, b] {
        let node = Node::start(
            NodeConfig {
                bind: "127.0.0.1:0".parse().unwrap(),
                relay: RelayConfig::SecureTest(f.addr.to_string(), Arc::new(f.config.clone())),
                options: MagicOptions {
                    relay_only: true,
                    mobile: false,
                    ..Default::default()
                },
                datagrams: false,
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
    let (a, b) = (&nodes[0], &nodes[1]);
    wait("peers ready", Duration::from_secs(8), || {
        a.relay_state() == RelayState::Registered && b.relay_state() == RelayState::Registered
    })
    .await;
    for (udp, tcp) in [(true, true), (false, true), (true, false)] {
        f.udp.send_replace(udp);
        f.tcp.send_if_modified(|current| {
            if *current == tcp {
                false
            } else {
                *current = tcp;
                true
            }
        });
        // Failed packets are recovered by peer QUIC; the application initiates
        // a fresh operation only here, never automatically replays a write.
        tokio::time::sleep(Duration::from_secs(9)).await;
        tokio::time::timeout(Duration::from_secs(12), async {
            tokio::join!(echo(a, b), echo(b, a));
        })
        .await
        .expect("both directions");
    }
    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn small_mtu_path_is_not_healthy_just_because_small_pings_work() {
    let f = Fixture::new().await;
    let link = f.link();
    wait("both ready", Duration::from_secs(4), || {
        link.carrier_ready(Carrier::Quic) && link.carrier_ready(Carrier::Tls)
    })
    .await;
    // QUIC transport pings/ACKs still cross; full-size data and probes do not.
    f.udp_limit.store(1300, Ordering::Relaxed);
    wait("MTU loss selects TLS", Duration::from_secs(10), || {
        !link.carrier_ready(Carrier::Quic) && link.active() == Some(Carrier::Tls)
    })
    .await;
    mailbox(&link, 5).await;
    f.udp_limit.store(65535, Ordering::Relaxed);
    wait("MTU recovery", Duration::from_secs(12), || {
        link.carrier_ready(Carrier::Quic)
    })
    .await;
}
