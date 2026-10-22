//! End to end on loopback: a relay, an admin node that creates a network,
//! and a joiner that pairs by invite, syncs the roster, opens a forward
//! stream to the admin's TCP echo, exercises a custom service, and is cut
//! off within two seconds of being removed.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use muster_core::roster::{Author, DeviceKind, DeviceRecord, Roster};
use muster_core::{DeviceId, HwSigner, Invite, LocalPolicy, Scope, SoftSigner};
use muster_net::Build;
use muster_net::magicsock::MagicOptions;
use muster_net::node::{Node, NodeConfig, RelayConfig, ServiceHandler};
use muster_net::pair::{Candidate, Verdict};
use muster_net::stream::{Hello, read_message, write_message};
use quinn::{RecvStream, SendStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// The relay, in-process.
async fn spawn_relay() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let server = muster_relay::RelayServer::bind(
        "127.0.0.1:0".parse().unwrap(),
        muster_relay::RelayOptions::default(),
    )
    .await
    .unwrap();
    let udp = server.udp_addr().unwrap();
    let task = tokio::spawn(async move {
        let _ = server.serve().await;
    });
    (udp, task)
}

fn config(relay: SocketAddr) -> NodeConfig {
    NodeConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        relay: RelayConfig::Udp(relay),
        options: MagicOptions {
            relay_only: false,
            mobile: false,
            allow_loopback: true,
            extra_candidates: Vec::new(),
            push_token: None,
        },
        datagrams: false,
        build: Default::default(),
    }
}

struct Echo;

impl ServiceHandler for Echo {
    fn handle(
        &self,
        _node: Arc<Node>,
        _peer: DeviceId,
        hello: Hello,
        mut send: SendStream,
        mut recv: RecvStream,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        Box::pin(async move {
            assert_eq!(hello.service, "echo");
            while let Ok(Some(message)) = read_message(&mut recv, 1024).await {
                let mut reply = message;
                reply.reverse();
                if write_message(&mut send, &reply).await.is_err() {
                    break;
                }
            }
            let _ = send.finish();
        })
    }
}

async fn wait_for<F: Fn() -> bool>(what: &str, timeout: Duration, check: F) {
    let started = Instant::now();
    while !check() {
        assert!(started.elapsed() < timeout, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pair_sync_forward_and_revoke() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();
    let (relay, _relay_process) = spawn_relay().await;

    // Admin creates the network.
    let admin_signer: Arc<dyn HwSigner> = Arc::new(SoftSigner::generate());
    let admin_id = admin_signer.public_key().device_id();
    let admin_roster = Arc::new(RwLock::new(Roster::new()));
    {
        let mut roster = admin_roster.write().unwrap();
        let record = DeviceRecord::new(admin_signer.public_key(), "mac-a", DeviceKind::Desktop);
        let genesis = Author {
            roster: &roster,
            signer: admin_signer.as_ref(),
        }
        .genesis("home", (Ipv4Addr::new(192, 168, 14, 0), 24), record)
        .unwrap();
        roster.insert(genesis).unwrap();
    }
    let mut policy = LocalPolicy::default();
    let echo_port = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let echo_addr = echo_port.local_addr().unwrap();
    policy.open_ports = vec![muster_core::PortRule {
        proto: muster_core::Proto::Tcp,
        port: Some(echo_addr.port()),
    }];
    let admin_build = Build {
        version: "1.0.20".into(),
        commit: "15fc1359".into(),
    };
    let admin = Node::start(
        NodeConfig {
            build: admin_build.clone(),
            ..config(relay)
        },
        admin_signer.clone(),
        admin_roster.clone(),
        Arc::new(RwLock::new(policy)),
    )
    .await
    .unwrap();
    admin.set_handler(Arc::new(Echo));

    // A TCP echo on the admin machine, the thing a forward stream reaches.
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = echo_port.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                while let Ok(n) = socket.read(&mut buf).await {
                    if n == 0 || socket.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            });
        }
    });

    // The admin approves anyone presenting a valid invite proof, granting agent.view and ssh.
    let approvals = Arc::new(Mutex::new(Vec::<Candidate>::new()));
    let seen = approvals.clone();
    admin
        .pairing()
        .set_approver(Arc::new(move |candidate: Candidate| {
            seen.lock().unwrap().push(candidate.clone());
            let (tx, rx) = tokio::sync::oneshot::channel();
            let mut record = candidate.record;
            record.scopes.insert(Scope::AgentView);
            record.scopes.insert("net:tcp/*".parse().unwrap());
            tx.send(Verdict::Approve(record)).ok();
            rx
        }));
    let network_id = admin_roster.read().unwrap().network_id().unwrap();
    let invite = Invite::new(
        network_id,
        "home",
        admin_signer.public_key(),
        &relay.to_string(),
        600,
    );
    admin.open_invite(invite.clone());
    wait_for("admin relay registration", Duration::from_secs(5), || {
        admin.relay_state() == muster_net::magicsock::RelayState::Registered
    })
    .await;

    // Joiner reads the QR and pairs.
    let joiner_signer: Arc<dyn HwSigner> = Arc::new(SoftSigner::generate());
    let joiner_id = joiner_signer.public_key().device_id();
    let joiner_roster = Arc::new(RwLock::new(Roster::new()));
    let joiner_build = Build {
        version: "1.0 (17)".into(),
        commit: "8763d4ae".into(),
    };
    let joiner = Node::start(
        NodeConfig {
            build: joiner_build.clone(),
            ..config(relay)
        },
        joiner_signer.clone(),
        joiner_roster.clone(),
        Arc::new(RwLock::new(LocalPolicy::default())),
    )
    .await
    .unwrap();
    wait_for("joiner relay registration", Duration::from_secs(5), || {
        joiner.relay_state() == muster_net::magicsock::RelayState::Registered
    })
    .await;
    let decoded = Invite::decode(&invite.encode()).unwrap();
    let record = DeviceRecord::new(joiner_signer.public_key(), "iphone", DeviceKind::Phone);
    tokio::time::timeout(Duration::from_secs(20), joiner.join(&decoded, record))
        .await
        .expect("pairing finishes")
        .expect("pairing succeeds");
    assert!(
        joiner.presence(&admin_id).online(),
        "successful pairing must promote the admin's existing link to online"
    );

    assert_eq!(approvals.lock().unwrap().len(), 1);
    assert_eq!(joiner_roster.read().unwrap().network_id(), Some(network_id));

    // The same member scanning another invite is handed the roster again
    // without anyone approving and without a second Add entry.
    let again = Invite::new(
        network_id,
        "home",
        admin_signer.public_key(),
        &relay.to_string(),
        600,
    );
    admin.open_invite(again.clone());
    let record = DeviceRecord::new(joiner_signer.public_key(), "iphone", DeviceKind::Phone);
    tokio::time::timeout(
        Duration::from_secs(20),
        joiner.join(&Invite::decode(&again.encode()).unwrap(), record),
    )
    .await
    .expect("re-pairing finishes")
    .expect("re-pairing succeeds");
    assert_eq!(approvals.lock().unwrap().len(), 1, "no second approval");
    assert_eq!(
        joiner_roster.read().unwrap().state(),
        admin_roster.read().unwrap().state()
    );
    assert_eq!(
        joiner_roster.read().unwrap().state(),
        admin_roster.read().unwrap().state()
    );
    assert!(
        joiner_roster
            .read()
            .unwrap()
            .state()
            .active(&joiner_id)
            .unwrap()
            .record
            .scopes
            .contains(&Scope::AgentView)
    );
    assert!(!admin.pairing().any_open(), "invite consumed");

    // The joiner is now a member: open a custom service stream to the admin.
    let (mut send, mut recv) =
        tokio::time::timeout(Duration::from_secs(10), joiner.open(admin_id, "echo", 1))
            .await
            .unwrap()
            .unwrap();
    write_message(&mut send, b"hello").await.unwrap();
    let reply = read_message(&mut recv, 1024).await.unwrap().unwrap();
    assert_eq!(reply, b"olleh");
    send.finish().unwrap();

    // Forward stream to the admin's TCP echo through the same link.
    let (mut fsend, mut frecv) = joiner
        .forward(admin_id, echo_addr.port())
        .await
        .expect("forward allowed");
    fsend.write_all(b"ping over forward").await.unwrap();
    let mut buf = vec![0u8; 17];
    frecv.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"ping over forward");
    // A port the policy does not open is refused.
    let closed = joiner.forward(admin_id, echo_addr.port() + 1).await;
    assert!(
        matches!(closed, Err(muster_net::Error::Unauthorized(_))),
        "{closed:?}"
    );
    drop((fsend, frecv));

    // Each member link's heartbeat tells both ends what the other runs.
    wait_for("heartbeat builds", Duration::from_secs(5), || {
        joiner
            .peers()
            .iter()
            .any(|p| p.id == admin_id && p.build.as_ref() == Some(&admin_build))
            && admin
                .peers()
                .iter()
                .any(|p| p.id == joiner_id && p.build.as_ref() == Some(&joiner_build))
    })
    .await;

    // Direct path should come up on loopback within a few seconds.
    wait_for("direct path", Duration::from_secs(10), || {
        joiner
            .peers()
            .iter()
            .any(|p| p.id == admin_id && p.path.as_ref().is_some_and(|path| path.direct.is_some()))
    })
    .await;

    // Learn the direct path MTU on a fresh connection, then keep that same
    // connection while moving a large stream through the relay's frame cap.
    admin
        .connection_to(&joiner_id)
        .unwrap()
        .close(0u32.into(), b"fresh direct path");
    joiner
        .connection_to(&admin_id)
        .unwrap()
        .close(0u32.into(), b"fresh direct path");
    wait_for("fresh direct connection", Duration::from_secs(8), || {
        admin.connection_to(&joiner_id).is_some() && joiner.connection_to(&admin_id).is_some()
    })
    .await;
    let connection = joiner.connection_to(&admin_id).unwrap();
    let connection_id = connection.stable_id();
    let (mut large_send, mut large_recv) =
        joiner.forward(admin_id, echo_addr.port()).await.unwrap();
    let large = vec![0x5a; 256 * 1024];
    for via_relay in [false, true] {
        if via_relay {
            admin.sock().unwrap().defer_direct(&joiner_id);
            joiner.sock().unwrap().defer_direct(&admin_id);
        }
        tokio::time::timeout(Duration::from_secs(3), async {
            let mut reply = vec![0; large.len()];
            let (written, received) = tokio::join!(
                large_send.write_all(&large),
                large_recv.read_exact(&mut reply)
            );
            written.unwrap();
            received.unwrap();
            assert_eq!(reply, large);
        })
        .await
        .expect("large stream survives path change without reconnecting");
        if !via_relay {
            tokio::time::sleep(Duration::from_secs(1)).await;
            assert!(
                connection.stats().path.current_mtu <= muster_net::relay_proto::MAX_QUIC_DATAGRAM,
                "direct MTU must also fit relay encapsulation: {:?}",
                connection.stats().path
            );
        }
        assert_eq!(
            joiner.connection_to(&admin_id).unwrap().stable_id(),
            connection_id
        );
    }
    assert!(
        joiner
            .sock()
            .unwrap()
            .path(&admin_id)
            .unwrap()
            .counters
            .rx_relay
            > 0
    );
    drop((large_send, large_recv));

    // Network changes on both ends cause simultaneous reconnect attempts.
    // Neither a UI click nor a new RPC is needed to restore the link.
    admin
        .connection_to(&joiner_id)
        .unwrap()
        .close(0u32.into(), b"test lost link");
    joiner
        .connection_to(&admin_id)
        .unwrap()
        .close(0u32.into(), b"test lost link");
    admin.network_changed();
    joiner.network_changed();
    wait_for("automatic reconnect", Duration::from_secs(8), || {
        admin.connection_to(&joiner_id).is_some() && joiner.connection_to(&admin_id).is_some()
    })
    .await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let stable = joiner.connection_to(&admin_id).unwrap().stable_id();
    for _ in 0..5 {
        let (mut send, mut recv) = joiner.open(admin_id, "echo", 1).await.unwrap();
        write_message(&mut send, b"reconnected").await.unwrap();
        assert_eq!(
            read_message(&mut recv, 1024).await.unwrap().unwrap(),
            b"detcennocer"
        );
        assert_eq!(joiner.connection_to(&admin_id).unwrap().stable_id(), stable);
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    // A peer that goes away must fail within three seconds. Restarting it
    // with the same identity recovers automatically without re-pairing.
    joiner.shutdown().await;
    wait_for("disconnected peer", Duration::from_secs(4), || {
        admin.connection_to(&joiner_id).is_none()
    })
    .await;
    let started = Instant::now();
    assert!(admin.open(joiner_id, "echo", 1).await.is_err());
    assert!(started.elapsed() < Duration::from_millis(3300));
    let joiner = Node::start(
        config(relay),
        joiner_signer.clone(),
        joiner_roster.clone(),
        Arc::new(RwLock::new(LocalPolicy::default())),
    )
    .await
    .unwrap();
    wait_for("peer restart recovery", Duration::from_secs(12), || {
        admin.connection_to(&joiner_id).is_some() && joiner.connection_to(&admin_id).is_some()
    })
    .await;

    // Admin removes the joiner: the link is cut within two seconds and the
    // joiner cannot reconnect.
    let remove = {
        let roster = admin_roster.read().unwrap();
        Author {
            roster: &roster,
            signer: admin_signer.as_ref(),
        }
        .remove(joiner_id)
        .unwrap()
    };
    let removed_at = Instant::now();
    admin.publish(remove).unwrap();
    wait_for("link closed", Duration::from_secs(2), || {
        !admin
            .peers()
            .iter()
            .any(|p| p.id == joiner_id && p.connected)
            && admin.connection_to(&joiner_id).is_none()
    })
    .await;
    assert!(removed_at.elapsed() < Duration::from_secs(2));
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        joiner.open(admin_id, "echo", 1).await.is_err(),
        "removed device must not open streams"
    );
    assert!(
        joiner_roster
            .read()
            .unwrap()
            .state()
            .active(&joiner_id)
            .is_none(),
        "joiner learned its own removal from the push"
    );

    admin.shutdown().await;
    joiner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unavailable_relay_fails_then_recovers_without_user_action() {
    use muster_net::magicsock::RelayState;
    let silent = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = silent.local_addr().unwrap();
    let start = Instant::now();
    let node = Node::start(
        config(addr),
        Arc::new(SoftSigner::generate()),
        Arc::new(RwLock::new(Roster::new())),
        Arc::new(RwLock::new(LocalPolicy::default())),
    )
    .await
    .unwrap();
    wait_for("failed registration", Duration::from_millis(3500), || {
        node.relay_state() == RelayState::Failed
    })
    .await;
    assert!(start.elapsed() >= Duration::from_secs(3));
    drop(silent);
    let relay = muster_relay::RelayServer::bind(addr, muster_relay::RelayOptions::default())
        .await
        .unwrap();
    let server = tokio::spawn(async move { relay.serve().await });
    wait_for("automatic relay recovery", Duration::from_secs(5), || {
        node.relay_state() == RelayState::Registered
    })
    .await;
    node.shutdown().await;
    server.abort();
}

/// Phone peers are clients, not automatic dial targets, including when their
/// runtime options differ from their signed device kind.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn member_connection_direction_and_phone_service_boundary() {
    let (relay, relay_task) = spawn_relay().await;
    let signers: Vec<Arc<dyn HwSigner>> = (0..4)
        .map(|_| Arc::new(SoftSigner::generate()) as Arc<dyn HwSigner>)
        .collect();
    let ids: Vec<_> = signers.iter().map(|s| s.public_key().device_id()).collect();
    let mut roster = Roster::new();
    let genesis = Author {
        roster: &roster,
        signer: signers[0].as_ref(),
    }
    .genesis(
        "direction",
        (Ipv4Addr::new(192, 168, 14, 0), 24),
        DeviceRecord::new(signers[0].public_key(), "desktop", DeviceKind::Desktop),
    )
    .unwrap();
    roster.insert(genesis).unwrap();
    for (i, kind) in [
        (1, DeviceKind::Server),
        (2, DeviceKind::Phone),
        (3, DeviceKind::Phone),
    ] {
        let entry = Author {
            roster: &roster,
            signer: signers[0].as_ref(),
        }
        .add(DeviceRecord::new(
            signers[i].public_key(),
            format!("peer-{i}"),
            kind,
        ))
        .unwrap();
        roster.insert(entry).unwrap();
    }
    let mut nodes = Vec::new();
    for signer in &signers[..2] {
        let node = Node::start(
            config(relay),
            signer.clone(),
            Arc::new(RwLock::new(roster.clone())),
            Arc::new(RwLock::new(LocalPolicy::default())),
        )
        .await
        .unwrap();
        node.set_handler(Arc::new(Echo));
        nodes.push(node);
    }
    wait_for(
        "desktop/server auto connection",
        Duration::from_secs(5),
        || nodes[0].connection_to(&ids[1]).is_some(),
    )
    .await;
    // Wait across reconnect ticks, then check that offline phones have never
    // been added to the socket or the retry state by desktop/server nodes.
    tokio::time::sleep(Duration::from_millis(400)).await;
    for node in &nodes {
        for phone in &ids[2..] {
            assert!(node.sock().unwrap().path(phone).is_none());
            assert!(matches!(
                node.connect(*phone).await,
                Err(muster_net::Error::Unauthorized(_))
            ));
            assert!(matches!(
                node.connect_pinned(*phone).await,
                Err(muster_net::Error::Unauthorized(_))
            ));
            assert_eq!(
                node.peers()
                    .into_iter()
                    .find(|p| p.id == *phone)
                    .unwrap()
                    .connection_status,
                "not connected"
            );
        }
    }
    for signer in &signers[2..] {
        let node = Node::start(
            config(relay),
            signer.clone(),
            Arc::new(RwLock::new(roster.clone())),
            Arc::new(RwLock::new(LocalPolicy::default())),
        )
        .await
        .unwrap();
        // Even an accidentally installed handler must not expose phone services.
        node.set_handler(Arc::new(Echo));
        nodes.push(node);
    }
    wait_for("phone-initiated links", Duration::from_secs(5), || {
        nodes[..2]
            .iter()
            .all(|host| ids[2..].iter().all(|id| host.connection_to(id).is_some()))
    })
    .await;
    for (index, other) in [(2, 3), (3, 2)] {
        let phone = &nodes[index];
        assert!(matches!(
            phone.connect(ids[other]).await,
            Err(muster_net::Error::Unauthorized(_))
        ));
        assert!(phone.sock().unwrap().path(&ids[other]).is_none());
        for (host_index, host) in nodes[..2].iter().enumerate() {
            let incoming = host.connection_to(&ids[index]).unwrap();
            assert_eq!(incoming.side(), quinn::Side::Server);
            assert!(
                host.connect(ids[index]).await.is_err(),
                "no reverse service access even on an existing link"
            );
            assert!(
                host.open_on(&incoming, "echo", 1).await.is_err(),
                "phone refuses application services"
            );
            let (mut send, mut recv) = phone.open(ids[host_index], "echo", 1).await.unwrap();
            write_message(&mut send, b"phone request").await.unwrap();
            assert_eq!(
                read_message(&mut recv, 1024).await.unwrap().unwrap(),
                b"tseuqer enohp"
            );
        }
    }
    for node in nodes {
        node.shutdown().await;
    }
    relay_task.abort();
}
