//! A CLI-started session that the provider's desktop app does not list is
//! archived in Muster's own catalog (ADR 0212): no native window and no
//! provider write, so a locked Mac needs no Locked Use operation for it,
//! while a desktop-listed session still does.
use muster_core::{Author, DeviceKind, DeviceRecord, HwSigner, Scope, SoftSigner};
use muster_proto::v1::{
    self, agent_organize_request::Action, error::Code, request::Req, response::Resp,
};
use muster_rpc::{Client, Principal};
use musterd::{Daemon, DaemonConfig};
use std::sync::Arc;
use std::time::{Duration, Instant};

const DESKTOP: &str = "11111111-1111-4111-8111-111111111111";
const CLI: &str = "22222222-2222-4222-8222-222222222222";

struct Locked;
impl muster_platform::Platform for Locked {
    fn locked(&self) -> bool {
        true
    }
}

fn archive(session: &str, op: &str) -> Req {
    Req::AgentOrganize(v1::AgentOrganizeRequest {
        op_id: op.into(),
        session: session.into(),
        action: Action::Archive as i32,
        ..Default::default()
    })
}

/// The daemon future is sized for the 8 MiB runtime thread in `main.rs`.
#[test]
fn cli_archive_skips_locked_use_on_a_locked_mac() {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .thread_stack_size(8 << 20)
        .enable_all()
        .build()
        .unwrap()
        .block_on(Box::pin(locked_mac_archive()));
}

async fn locked_mac_archive() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("claude/projects/repo");
    let user = dir.path().join("desktop/org/user");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&user).unwrap();
    for (id, entrypoint) in [(DESKTOP, "claude-desktop"), (CLI, "cli")] {
        std::fs::write(
            project.join(format!("{id}.jsonl")),
            format!(
                "{{\"type\":\"user\",\"uuid\":\"u-{id}\",\"sessionId\":\"{id}\",\"cwd\":\"/repo\",\"entrypoint\":\"{entrypoint}\",\"message\":{{\"role\":\"user\",\"content\":\"Example task\"}}}}\n"
            ),
        )
        .unwrap();
    }
    std::fs::write(
        user.join(format!("local_{DESKTOP}.json")),
        serde_json::json!({"cliSessionId": DESKTOP, "title": "Example", "isArchived": false})
            .to_string(),
    )
    .unwrap();
    let mut config = DaemonConfig::new(dir.path().join("muster"));
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.listen = false;
    config.terminal = None;
    config.agent = Some(muster_agent::AgentConfig {
        codex_home: None,
        claude_home: Some(dir.path().join("claude")),
        catdesk_home: None,
        deepseek_home: None,
        claude_desktop: Some(dir.path().join("desktop")),
        rescan: Duration::from_millis(100),
        ..Default::default()
    });
    let daemon = Daemon::start_with(config, Arc::new(Locked), None)
        .await
        .unwrap();
    let agent = daemon.agent().unwrap().clone();
    let deadline = Instant::now() + Duration::from_secs(5);
    while agent.session(CLI).is_none() || agent.session(DESKTOP).is_none() {
        assert!(Instant::now() < deadline, "the transcripts were not listed");
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
            .extend([Scope::Agents, Scope::AgentProvider("claude".into())]);
        let entry = Author {
            roster: &roster,
            signer: &**daemon.signer(),
        }
        .add(record)
        .unwrap();
        roster.insert(entry).unwrap();
    }
    // The Mac's own policy still decides whether a locked Mac takes control.
    daemon.policy().write().unwrap().allow_locked_control = true;
    let (a, b) = tokio::io::duplex(1024 * 1024);
    tokio::spawn(muster_rpc::serve(
        b,
        Principal::Remote(phone.public_key().device_id()),
        Arc::new(musterd::MusterHandler::new(Arc::downgrade(&daemon))),
    ));
    let client = Client::new(a);

    match client.call(archive(DESKTOP, "op-desktop")).await {
        Err(muster_rpc::Error::Remote { code, message }) => {
            assert_eq!(code, Code::LockedUseRequired, "{message}")
        }
        other => panic!("a desktop-listed session still needs Locked Use: {other:?}"),
    }
    match client.call(archive(CLI, "op-cli")).await {
        Ok(Resp::OpResult(r)) => assert_eq!(
            v1::op_result::Status::try_from(r.status),
            Ok(v1::op_result::Status::Applied),
            "{r:?}"
        ),
        other => panic!("a CLI session archives in Muster: {other:?}"),
    }
    assert!(
        !agent.list(None, 0).iter().any(|s| s.id == CLI),
        "the archived CLI session leaves the catalog"
    );
    assert!(agent.list(None, 0).iter().any(|s| s.id == DESKTOP));
    assert!(dir.path().join("muster/session-archive.json").exists());
}
