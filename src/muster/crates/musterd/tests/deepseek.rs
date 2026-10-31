//! DeepSeek Harness sessions are driven through Muster's plugin inside the
//! app (ADR 0074), yet stay under the same rules as Accessibility-driven
//! sessions: a locked Mac demands a signed Locked Use operation, and nothing
//! is sent while the plugin is not running.
use muster_core::{Author, DeviceKind, DeviceRecord, HwSigner, Scope, SoftSigner};
use muster_proto::v1::{self, error::Code, request::Req};
use muster_rpc::{Client, Principal};
use musterd::{Daemon, DaemonConfig};
use std::sync::Arc;
use std::time::{Duration, Instant};

const SESSION: &str = "session-4fa08e96-7b5c-4d2e-9f6a-0c1d2e3f4a5b";

struct Locked;
impl muster_platform::Platform for Locked {
    fn locked(&self) -> bool {
        true
    }
}

struct Unlocked;
impl muster_platform::Platform for Unlocked {}

fn connect(daemon: &Arc<Daemon>, peer: muster_core::DeviceId) -> Client {
    let (a, b) = tokio::io::duplex(1024 * 1024);
    tokio::spawn(muster_rpc::serve(
        b,
        Principal::Remote(peer),
        Arc::new(musterd::MusterHandler::new(Arc::downgrade(daemon))),
    ));
    Client::new(a)
}

/// A daemon reading one DeepSeek Harness session (an uncompressed log), and
/// a phone allowed to drive DeepSeek Harness sessions.
async fn daemon(
    platform: Arc<dyn muster_platform::Platform>,
) -> (tempfile::TempDir, Arc<Daemon>, Client) {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("dsh");
    let log = home.join("sessions/--tmp-project--").join(SESSION);
    std::fs::create_dir_all(&log).unwrap();
    std::fs::write(
        log.join("session.v4.jsonl"),
        format!(
            "{}\n",
            serde_json::json!({"type":"session","version":4,"id":SESSION,"createdAt":1,
                "cwd":dir.path(),"isSeeded":false})
        ),
    )
    .unwrap();
    let mut config = DaemonConfig::new(dir.path().join("muster"));
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.listen = false;
    config.terminal = None;
    config.agent = Some(muster_agent::AgentConfig {
        codex_home: None,
        claude_home: None,
        catdesk_home: None,
        deepseek_home: Some(home),
        claude_desktop: None,
        rescan: Duration::from_millis(100),
        ..Default::default()
    });
    let daemon = Daemon::start_with(config, platform, None).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while daemon.agent().unwrap().session(SESSION).is_none() {
        assert!(Instant::now() < deadline, "the log was not listed");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let phone = SoftSigner::generate();
    {
        let mut roster = daemon.roster().write().unwrap();
        let entry = Author {
            roster: &roster,
            signer: &**daemon.signer(),
        }
        .genesis(
            "test",
            ("192.168.14.0".parse().unwrap(), 24),
            daemon.own_record(),
        )
        .unwrap();
        roster.insert(entry).unwrap();
        let mut record = DeviceRecord::new(phone.public_key(), "phone", DeviceKind::Phone);
        record
            .scopes
            .extend([Scope::Agents, Scope::AgentProvider("deepseek".into())]);
        let entry = Author {
            roster: &roster,
            signer: &**daemon.signer(),
        }
        .add(record)
        .unwrap();
        roster.insert(entry).unwrap();
    }
    let client = connect(&daemon, phone.public_key().device_id());
    (dir, daemon, client)
}

fn send() -> Req {
    Req::AgentSend(v1::AgentSendRequest {
        op_id: "op-1".into(),
        session: SESSION.into(),
        text: "run the tests".into(),
        ..Default::default()
    })
}

/// The gates every agent provider meets on a locked Mac: the Mac's own
/// policy must allow locked control, and then only a signed Locked Use
/// operation (presence and `locked.control`) runs.
#[tokio::test]
async fn a_locked_mac_demands_locked_use_as_for_a_native_app() {
    let (_dir, daemon, client) = daemon(Arc::new(Locked)).await;
    match client.call(send()).await {
        Err(muster_rpc::Error::Remote { code, message }) => {
            assert_eq!(code, Code::Unauthorized, "{message}");
            assert!(message.contains("refused by local policy"), "{message}");
        }
        other => panic!("a locked Mac must refuse an unsigned send: {other:?}"),
    }
    daemon.policy().write().unwrap().allow_locked_control = true;
    match client.call(send()).await {
        Err(muster_rpc::Error::Remote { code, message }) => {
            assert_eq!(code, Code::LockedUseRequired, "{message}")
        }
        other => panic!("a locked Mac must refuse an unsigned send: {other:?}"),
    }
    assert!(daemon.store().ledger().status("op-1").unwrap().is_none());
}

#[tokio::test]
async fn nothing_is_sent_while_the_plugin_is_not_running() {
    let (_dir, daemon, client) = daemon(Arc::new(Unlocked)).await;
    let error = client.call(send()).await.unwrap_err().to_string();
    assert!(error.contains("Muster's plugin"), "{error}");
    assert!(error.contains("Nothing was sent"), "{error}");
    assert!(daemon.store().ledger().status("op-1").unwrap().is_none());
    // A model choice stays with the app.
    let options = client
        .call(Req::AgentChatOptions(v1::AgentChatOptionsRequest {
            session: SESSION.into(),
            ..Default::default()
        }))
        .await
        .unwrap_err()
        .to_string();
    assert!(options.contains("model or effort"), "{options}");
}
