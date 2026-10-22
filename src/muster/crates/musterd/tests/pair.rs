//! Two daemons on one host: A creates a network and invites, B joins and
//! waits for approval, A approves with scopes, both see the roster, B calls
//! A over the "muster" service with its scopes enforced, and A removes B.

use std::sync::Arc;
use std::time::{Duration, Instant};

use muster_core::roster::DeviceKind;
use muster_proto::v1::{self, request::Req, response::Resp};
use muster_rpc::Client;
use musterd::{Daemon, DaemonConfig};

/// A controller for session "x" that refuses every write, so an authorized
/// call reaches the agent service through the ledger and is rejected there.
/// (Test daemons run on `DefaultPlatform`, which selects nothing through
/// Accessibility; only an attached controller can act.)
struct Refusing;

#[async_trait::async_trait]
impl muster_agent::controller::Controller for Refusing {
    fn kind(&self) -> muster_agent::model::ControllerKind {
        muster_agent::model::ControllerKind::NativeApp
    }
    fn session_id(&self) -> String {
        "x".into()
    }
    fn state(&self) -> muster_agent::model::SessionState {
        muster_agent::model::SessionState::Idle
    }
    fn process_identity(&self) -> muster_agent::controller::ProcessIdentity {
        muster_agent::controller::ProcessIdentity { pid: 1, start: 1 }
    }
    fn process_state(&self) -> muster_agent::controller::ProcessState {
        muster_agent::controller::ProcessState::Running
    }
    fn pending(&self) -> Vec<muster_agent::PendingInteraction> {
        Vec::new()
    }
    async fn send(&self, _: &str) -> Result<(), muster_agent::Error> {
        Err(muster_agent::Error::Unavailable("fixture refuses".into()))
    }
    async fn respond(&self, _: &str, _: &str, _: &str) -> Result<(), muster_agent::Error> {
        Err(muster_agent::Error::Unavailable("fixture refuses".into()))
    }
    async fn interrupt(&self) -> Result<(), muster_agent::Error> {
        Err(muster_agent::Error::Unavailable("fixture refuses".into()))
    }
    async fn stop(&self) {}
}

async fn relay() -> String {
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
    addr.to_string()
}

fn config(dir: &std::path::Path, relay: Option<String>, name: &str) -> DaemonConfig {
    let mut config = DaemonConfig::new(dir);
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.relay = relay;
    config.name = name.into();
    config.kind = DeviceKind::Desktop;
    config.allow_loopback = true;
    config.agent = Some(muster_agent::AgentConfig {
        codex_home: Some(dir.join("codex")),
        claude_home: Some(dir.join("claude")),
        catdesk_home: None,
        claude_desktop: None,
        ..Default::default()
    });
    // A PTY terminal on A, so the terminal RPCs run end to end on Linux.
    config.terminal = if name == "alpha" {
        Some(muster_terminal::TerminalKind::Pty(
            muster_terminal::pty::PtyConfig {
                shell: "/bin/sh".into(),
                cols: 40,
                rows: 8,
                ..Default::default()
            },
        ))
    } else {
        None
    };
    config
}

async fn client(daemon: &Daemon) -> Client {
    Client::new(
        tokio::net::UnixStream::connect(daemon.socket_path())
            .await
            .unwrap(),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn readdress_preserves_pairing_and_persists_on_both_peers() {
    let relay = relay().await;
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let mut cfg_a = config(dir_a.path(), Some(relay.clone()), "admin");
    let mut cfg_b = config(dir_b.path(), Some(relay), "member");
    cfg_a.agent = None;
    cfg_b.agent = None;
    let a = Daemon::start_with(cfg_a, Arc::new(musterd::DefaultPlatform), None)
        .await
        .unwrap();
    let b = Daemon::start_with(cfg_b, Arc::new(musterd::DefaultPlatform), None)
        .await
        .unwrap();
    a.create_network("migration", "192.168.14.0/24")
        .await
        .unwrap();
    let network_id = a.network_status().network_id;
    let invite = a.make_invite(60).unwrap().encode();
    let joiner = b.clone();
    let join =
        tokio::spawn(async move { joiner.join(&invite, "member", "desktop", &|_, _| {}).await });
    wait_for("join pending", Duration::from_secs(10), || {
        !a.pending_joins().joins.is_empty()
    })
    .await;
    a.decide(&b.device_id().to_hex(), true, &["agent.view".into()], "")
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), join)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let ca = client(&a).await;
    let plan = [
        (a.device_id().to_hex(), "192.168.13.11".into()),
        (b.device_id().to_hex(), "192.168.13.12".into()),
    ]
    .into();
    let request = v1::NetworkReaddressRequest {
        vnet_cidr: "192.168.13.0/24".into(),
        addresses: plan,
    };
    let (send, recv) = b
        .node()
        .open(a.device_id(), musterd::SERVICE, musterd::SERVICE_VERSION)
        .await
        .unwrap();
    let remote = Client::new(tokio::io::join(recv, send));
    let denied = remote
        .call(Req::NetworkReaddress(request.clone()))
        .await
        .unwrap_err();
    assert!(matches!(
        denied,
        muster_rpc::Error::Remote {
            code: v1::error::Code::Unauthorized,
            ..
        }
    ));
    assert!(matches!(
        ca.call(Req::NetworkReaddress(request)).await.unwrap(),
        Resp::NetworkStatus(_)
    ));
    wait_for(
        "signed address plan persisted at peer",
        Duration::from_secs(10),
        || {
            b.store().load_roster().unwrap().state().vnet_cidr
                == Some(("192.168.13.0".parse().unwrap(), 24))
        },
    )
    .await;
    for daemon in [&a, &b] {
        assert_eq!(daemon.network_status().network_id, network_id);
        let stored = daemon.store().load_roster().unwrap();
        assert_eq!(
            stored.state().active(&a.device_id()).unwrap().record.vip,
            Some("192.168.13.11".parse().unwrap())
        );
        assert_eq!(
            stored.state().active(&b.device_id()).unwrap().record.vip,
            Some("192.168.13.12".parse().unwrap())
        );
        assert_eq!(stored.state().active_members().count(), 2);
    }
    assert!(matches!(
        remote
            .call(Req::DeviceInfo(v1::DeviceInfoRequest {}))
            .await
            .unwrap(),
        Resp::DeviceInfo(_)
    ));
    a.shutdown().await;
    b.shutdown().await;
}

async fn wait_for<F: Fn() -> bool>(what: &str, timeout: Duration, check: F) {
    let started = Instant::now();
    while !check() {
        assert!(started.elapsed() < timeout, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn create_invite_join_approve_call_and_remove() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();
    let relay = relay().await;
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    // Pairing/RPC policy is tested against an explicit unlocked fixture,
    // independent of whether the developer's real Mac is currently locked.
    let a = Daemon::start_with(
        config(dir_a.path(), Some(relay.clone()), "alpha"),
        std::sync::Arc::new(musterd::DefaultPlatform),
        None,
    )
    .await
    .unwrap();
    // B starts without a relay: the invite tells it which one to use.
    let b = Daemon::start_with(
        config(dir_b.path(), None, "bravo"),
        std::sync::Arc::new(musterd::DefaultPlatform),
        None,
    )
    .await
    .unwrap();
    let ca = client(&a).await;
    let cb = client(&b).await;

    // Local device info and an empty network.
    let Resp::DeviceInfo(info) = ca
        .call(Req::DeviceInfo(v1::DeviceInfoRequest {}))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(info.name, "alpha");
    assert_eq!(info.device_id, a.device_id().to_hex());
    let Resp::NetworkStatus(status) = ca
        .call(Req::NetworkStatus(v1::NetworkStatusRequest {}))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert!(!status.has_network);

    // A creates the network and an invite.
    let Resp::NetworkStatus(status) = ca
        .call(Req::NetworkCreate(v1::NetworkCreateRequest {
            name: "home".into(),
            vnet_cidr: String::new(),
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert!(status.has_network);
    assert_eq!(status.members.len(), 1);
    assert_eq!(status.members[0].role, "admin");
    assert_eq!(status.vnet_cidr, "192.168.13.0/24");
    let Resp::InviteText(invite) = ca
        .call(Req::NetworkInvite(v1::NetworkInviteRequest {
            ttl_secs: 120,
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert!(invite.text.starts_with("muster1:"));

    // B joins; the call streams progress and blocks until A decides.
    let join_client = cb.clone();
    let invite_text = invite.text.clone();
    let join = tokio::spawn(async move {
        let mut call = join_client
            .start(Req::NetworkJoin(v1::NetworkJoinRequest {
                invite_text,
                device_name: "bravo".into(),
                device_kind: "phone".into(),
            }))
            .await
            .unwrap();
        let mut phases = Vec::new();
        while let Some(ev) = call.events.recv().await {
            if let v1::event::Ev::JoinProgress(p) = ev {
                phases.push(p.phase);
            }
        }
        (phases, call.finish().await)
    });

    // A sees the pending join and approves it with agent.view only.
    let pending = {
        let started = Instant::now();
        loop {
            let Resp::PendingJoins(p) = ca
                .call(Req::NetworkPending(v1::NetworkPendingRequest {}))
                .await
                .unwrap()
            else {
                panic!()
            };
            if !p.joins.is_empty() {
                break p;
            }
            assert!(
                started.elapsed() < Duration::from_secs(20),
                "join request never arrived"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };
    assert_eq!(pending.joins[0].name, "bravo");
    assert_eq!(pending.joins[0].kind, "phone");
    assert_eq!(pending.joins[0].device_id, b.device_id().to_hex());
    let Resp::Ack(_) = ca
        .call(Req::NetworkDecide(v1::NetworkDecideRequest {
            device_id: pending.joins[0].device_id.clone(),
            approve: true,
            scopes: vec![
                "agent.view".into(),
                "terminal.view".into(),
                "terminal.control".into(),
            ],
            reason: String::new(),
        }))
        .await
        .unwrap()
    else {
        panic!()
    };

    let (phases, outcome) = tokio::time::timeout(Duration::from_secs(30), join)
        .await
        .unwrap()
        .unwrap();
    let Resp::NetworkStatus(status_b) = outcome.unwrap() else {
        panic!()
    };
    assert_eq!(
        phases.last().copied(),
        Some(v1::join_progress::Phase::Joined as i32)
    );
    assert!(status_b.has_network);
    assert_eq!(status_b.members.len(), 2);
    let me_b = status_b.members.iter().find(|m| m.is_self).unwrap();
    assert_eq!(me_b.name, "bravo");
    assert_eq!(me_b.kind, "phone");
    assert_eq!(
        me_b.scopes,
        vec!["agent.view", "terminal.view", "terminal.control"]
    );
    assert!(me_b.vip.is_empty(), "phones get no vip (D9)");
    assert!(
        !status_b
            .members
            .iter()
            .find(|m| !m.is_self)
            .unwrap()
            .vip
            .is_empty()
    );

    // The roster survived to disk on both sides.
    assert_eq!(
        a.store()
            .load_roster()
            .unwrap()
            .state()
            .active_members()
            .count(),
        2
    );
    assert_eq!(
        b.store()
            .load_roster()
            .unwrap()
            .state()
            .active_members()
            .count(),
        2
    );

    // Relay edits must retain the Node/Vnet owner and its installed RPC handler.
    let original = b.node();
    let a_connection = a
        .node()
        .connection_to(&b.device_id())
        .map(|c| c.stable_id());
    b.restart_node(&relay).await.unwrap();
    assert!(Arc::ptr_eq(&original, &b.node()));
    assert_eq!(b.store().meta("relay").unwrap().unwrap(), relay.as_bytes());
    wait_for(
        "new transport adopted by remote",
        Duration::from_secs(15),
        || {
            a.node()
                .connection_to(&b.device_id())
                .is_some_and(|c| Some(c.stable_id()) != a_connection)
        },
    )
    .await;

    // B calls A over the link: the same handler, remote principal, scopes enforced.
    let (send, recv) = tokio::time::timeout(
        Duration::from_secs(15),
        b.node()
            .open(a.device_id(), musterd::SERVICE, musterd::SERVICE_VERSION),
    )
    .await
    .expect("open in time")
    .expect("muster service accepted");
    let remote = Client::new(tokio::io::join(recv, send));
    let Resp::DeviceInfo(info) = remote
        .call(Req::DeviceInfo(v1::DeviceInfoRequest {}))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(info.name, "alpha");
    let Resp::AgentSessions(page) = remote
        .call(Req::AgentList(v1::AgentListRequest::default()))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert!(page.sessions.is_empty());
    let denied = remote
        .call(Req::AgentSend(v1::AgentSendRequest {
            attachments: Vec::new(),
            op_id: "op-1".into(),
            session: "x".into(),
            text: "hi".into(),
            release_unconfirmed: String::new(),
        }))
        .await
        .unwrap_err();
    assert!(
        matches!(
            denied,
            muster_rpc::Error::Remote {
                code: v1::error::Code::Unauthorized,
                ..
            }
        ),
        "{denied:?}"
    );
    let denied = remote
        .call(Req::NetworkCreate(v1::NetworkCreateRequest::default()))
        .await
        .unwrap_err();
    assert!(matches!(
        denied,
        muster_rpc::Error::Remote {
            code: v1::error::Code::Unauthorized,
            ..
        }
    ));
    // network.admin from a non-admin member is refused before any presence check.
    let denied = remote
        .call(Req::NetworkRemove(v1::NetworkRemoveRequest {
            device_id: a.device_id().to_hex(),
        }))
        .await
        .unwrap_err();
    assert!(matches!(
        denied,
        muster_rpc::Error::Remote {
            code: v1::error::Code::Unauthorized,
            ..
        }
    ));

    // A phone cannot be a remote RPC target, even for its network admin.
    let phone_target = ca
        .with_target(&b.device_id().to_hex())
        .call(Req::DeviceInfo(v1::DeviceInfoRequest {}))
        .await
        .unwrap_err();
    assert!(
        phone_target
            .to_string()
            .contains("outbound member connections require a desktop or server")
    );
    // B's app addresses desktop A through its own daemon: the envelope is
    // proxied over the link, including streamed events.
    let via_b = cb.with_target(&a.device_id().to_hex());
    let Resp::DeviceInfo(info) = tokio::time::timeout(
        Duration::from_secs(10),
        via_b.call(Req::DeviceInfo(v1::DeviceInfoRequest {})),
    )
    .await
    .expect("proxied call in time")
    .unwrap() else {
        panic!()
    };
    assert_eq!(info.name, "alpha");
    let mut watch = via_b
        .start(Req::DeviceWatch(v1::DeviceWatchRequest {}))
        .await
        .unwrap();
    let first = tokio::time::timeout(Duration::from_secs(5), watch.events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        first,
        v1::event::Ev::NetworkStatus(_) | v1::event::Ev::DeviceState(_)
    ));
    watch.cancel().await;
    assert!(watch.finish().await.is_ok());
    // A member cannot bounce through another device's daemon.
    let denied = remote
        .with_target(&b.device_id().to_hex())
        .call(Req::DeviceInfo(v1::DeviceInfoRequest {}))
        .await
        .unwrap_err();
    assert!(
        matches!(
            denied,
            muster_rpc::Error::Remote {
                code: v1::error::Code::Unauthorized,
                ..
            }
        ),
        "{denied:?}"
    );
    // An unknown target fails cleanly.
    let missing = ca
        .with_target(&"00".repeat(32))
        .call(Req::DeviceInfo(v1::DeviceInfoRequest {}))
        .await
        .unwrap_err();
    assert!(
        matches!(
            missing,
            muster_rpc::Error::Remote {
                code: v1::error::Code::Unavailable,
                ..
            }
        ),
        "{missing:?}"
    );

    // B drives a shell on A over the link: create, attach, type, read back, close.
    let Resp::TerminalSession(term) = remote
        .call(Req::TerminalCreate(v1::TerminalCreateRequest::default()))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(term.source, "pty");
    let mut attach = remote
        .start(Req::TerminalAttach(v1::TerminalAttachRequest {
            session: term.id.clone(),
            since_rev: 0,
        }))
        .await
        .unwrap();
    let first = tokio::time::timeout(Duration::from_secs(5), attach.events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(first, v1::event::Ev::TerminalFrame(ref f) if f.full));
    let Resp::OpResult(r) = remote
        .call(Req::TerminalInput(v1::TerminalInputRequest {
            op_id: "term-1".into(),
            session: term.id.clone(),
            bytes: b"echo remote-$((20+22))\r".to_vec(),
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(r.status, v1::op_result::Status::Applied as i32);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let ev = tokio::time::timeout_at(deadline, attach.events.recv())
            .await
            .expect("shell output")
            .unwrap();
        if let v1::event::Ev::TerminalFrame(f) = ev
            && f.lines.iter().any(|l| l.text.contains("remote-42"))
        {
            break;
        }
    }
    let Resp::TerminalLines(lines) = remote
        .call(Req::TerminalHistory(v1::TerminalHistoryRequest {
            session: term.id.clone(),
            lines: 20,
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert!(lines.lines.iter().any(|l| l.text.contains("remote-42")));
    let Resp::TerminalSessions(list) = remote
        .call(Req::TerminalSessions(v1::TerminalSessionsRequest {
            watch: false,
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(list.sessions.len(), 1);
    attach.cancel().await;
    let _ = attach.finish().await;
    remote
        .call(Req::TerminalClose(v1::TerminalCloseRequest {
            session: term.id.clone(),
        }))
        .await
        .unwrap();
    // The authorized_keys view lists B (it holds net:tcp/22 via the widened scopes below? not yet).
    assert!(!musterd::authorized_keys::render(&a).contains("bravo"));

    // A widens B's scopes; B sees the update through gossip.
    let Resp::NetworkStatus(_) = ca
        .call(Req::NetworkSetScopes(v1::NetworkSetScopesRequest {
            device_id: b.device_id().to_hex(),
            scopes: vec![
                "agent.view".into(),
                "agent.control".into(),
                "net:tcp/22".into(),
            ],
            role: String::new(),
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    let b_roster = b.roster().clone();
    let b_id = b.device_id();
    wait_for("scope update at B", Duration::from_secs(5), || {
        b_roster
            .read()
            .unwrap()
            .state()
            .active(&b_id)
            .is_some_and(|m| m.record.scopes.contains(&muster_core::Scope::AgentControl))
    })
    .await;
    assert!(
        musterd::authorized_keys::render(&a).contains("ecdsa-sha2-nistp256"),
        "{}",
        musterd::authorized_keys::render(&a)
    );
    // Native sends also require the local Locked Use switch.
    a.policy().write().unwrap().allow_locked_control = true;
    // Now the same call reaches the agent service (and fails there, not at the door).
    a.agent().unwrap().attach(Arc::new(Refusing));
    let Resp::OpResult(result) = remote
        .call(Req::AgentSend(v1::AgentSendRequest {
            attachments: Vec::new(),
            op_id: "op-2".into(),
            session: "x".into(),
            text: "hi".into(),
            release_unconfirmed: String::new(),
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(result.status, v1::op_result::Status::Rejected as i32);
    assert!(
        result.message.contains("fixture refuses"),
        "{}",
        result.message
    );
    // A repeated op_id answers from the ledger without running again.
    let Resp::OpResult(again) = remote
        .call(Req::AgentSend(v1::AgentSendRequest {
            attachments: Vec::new(),
            op_id: "op-2".into(),
            session: "x".into(),
            text: "hi".into(),
            release_unconfirmed: String::new(),
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(again.status, v1::op_result::Status::Rejected as i32);
    let Resp::OpResult(conflict) = remote
        .call(Req::AgentSend(v1::AgentSendRequest {
            attachments: Vec::new(),
            op_id: "op-2".into(),
            session: "x".into(),
            text: "different".into(),
            release_unconfirmed: String::new(),
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(conflict.status, v1::op_result::Status::Rejected as i32);
    assert!(conflict.message.contains("different body"));

    // Policy is local state, editable over the socket.
    let Resp::Policy(mut policy) = ca
        .call(Req::PolicyGet(v1::PolicyGetRequest {}))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(policy.open_ports, vec!["tcp/22"]);
    policy.open_ports = vec!["tcp/22".into(), "tcp/5900".into()];
    policy.offer_agent = false;
    let Resp::Policy(policy) = ca
        .call(Req::PolicySet(v1::PolicySetRequest {
            policy: Some(policy),
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(policy.open_ports.len(), 2);
    assert!(!policy.offer_agent);
    // With the agent switched off locally, B's scope no longer gets through.
    let denied = remote
        .call(Req::AgentList(v1::AgentListRequest::default()))
        .await
        .unwrap_err();
    assert!(matches!(
        denied,
        muster_rpc::Error::Remote {
            code: v1::error::Code::Unauthorized,
            ..
        }
    ));

    // A removes B: B's roster shows it, and the link is cut.
    let Resp::NetworkStatus(status) = ca
        .call(Req::NetworkRemove(v1::NetworkRemoveRequest {
            device_id: b.device_id().to_hex(),
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(status.members.len(), 1);
    wait_for("removal at B", Duration::from_secs(5), || {
        b_roster.read().unwrap().state().active(&b_id).is_none()
    })
    .await;
    let a_node = a.node();
    wait_for("link closed", Duration::from_secs(3), || {
        !a_node.peers().iter().any(|p| p.id == b_id && p.connected)
    })
    .await;
    let Resp::NetworkStatus(status_b) = cb
        .call(Req::NetworkStatus(v1::NetworkStatusRequest {}))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert!(!status_b.members.iter().any(|m| m.is_self));

    a.shutdown().await;
    b.shutdown().await;
    drop(Arc::clone(&a));
}

// DeviceInfo must describe the daemon's identity signer, independent of the
// UI process's keys, build features, or previously persisted software identity.
#[tokio::test]
async fn device_info_reports_active_signer_backend_over_rpc() {
    use muster_core::{HwSigner, PublicKey, SoftSigner};
    #[derive(Debug)]
    struct TaggedSigner(SoftSigner);
    impl HwSigner for TaggedSigner {
        fn public_key(&self) -> PublicKey {
            self.0.public_key()
        }
        fn sign(&self, message: &[u8]) -> Result<Vec<u8>, muster_core::Error> {
            self.0.sign(message)
        }
        fn backend(&self) -> &'static str {
            "test-platform"
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let cfg = || {
        let mut c = config(dir.path(), None, "key-test");
        c.agent = None;
        c
    };
    let software = Daemon::start_with(cfg(), Arc::new(musterd::DefaultPlatform), None)
        .await
        .unwrap();
    assert_eq!(
        software.device_info().key_backend,
        software.signer().backend()
    );
    let old_id = software.device_id();
    software.shutdown().await;
    let signer: muster_core::SharedSigner = Arc::new(TaggedSigner(SoftSigner::generate()));
    let expected_id = signer.public_key().device_id().to_hex();
    let daemon = Daemon::start_with(cfg(), Arc::new(musterd::DefaultPlatform), Some(signer))
        .await
        .unwrap();
    let rpc = client(&daemon).await;
    let Resp::DeviceInfo(info) = rpc
        .call(Req::DeviceInfo(v1::DeviceInfoRequest {}))
        .await
        .unwrap()
    else {
        panic!("expected DeviceInfo")
    };
    assert_eq!(info.key_backend, "test-platform");
    assert_eq!(info.device_id, expected_id);
    assert_ne!(info.device_id, old_id.to_hex());
    daemon.shutdown().await;
}
