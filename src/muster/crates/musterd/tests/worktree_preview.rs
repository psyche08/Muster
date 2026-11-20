//! Read session files and a commit through the actual local transport using
//! a disposable worktree, a synthetic transcript and an independent signer.
use std::{fs, process::Command, sync::Arc, time::Duration};

use muster_core::SoftSigner;
use muster_proto::v1::{self, request::Req, response::Resp};
use muster_rpc::Client;
use musterd::{Daemon, DaemonConfig, DefaultPlatform};
use serde_json::json;

#[tokio::test]
async fn session_tree_and_git_preview_over_local_rpc() {
    let dir = tempfile::tempdir().unwrap();
    let tree = dir.path().join("example repo");
    fs::create_dir_all(tree.join("src")).unwrap();
    fs::write(tree.join("src/example.txt"), b"sample text\n").unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["add", "src/example.txt"],
        vec!["commit", "-qm", "Example change"],
    ] {
        assert!(
            Command::new("git")
                .args([
                    "-c",
                    "user.name=Example",
                    "-c",
                    "user.email=example@example.invalid",
                    "-c",
                    "commit.gpgsign=false",
                    "-c",
                    "core.hooksPath=.unused-hooks"
                ])
                .args(args)
                .current_dir(&tree)
                .status()
                .unwrap()
                .success()
        );
    }
    let codex = dir.path().join("codex");
    let day = codex.join("sessions/2026/01/01");
    fs::create_dir_all(&day).unwrap();
    let id = "4055d1b1-b7b0-4537-9484-742bc60aa33a";
    let records = [
        json!({"type":"session_meta","payload":{"id":id,"cwd":tree}}),
        json!({"type":"turn_context","payload":{}}),
        json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Example"}]}}),
    ];
    fs::write(
        day.join(format!("rollout-2026-01-01T00-00-00-{id}.jsonl")),
        records
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
    )
    .unwrap();

    let mut config = DaemonConfig::new(dir.path().join("node"));
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.cloud = false;
    config.agent = Some(muster_agent::AgentConfig {
        codex_home: Some(codex),
        claude_home: None,
        claude_desktop: None,
        catdesk_home: None,
        deepseek_home: None,
        ..Default::default()
    });
    let socket = config.socket_path();
    let daemon = Daemon::start_with(
        config,
        Arc::new(DefaultPlatform),
        Some(Arc::new(SoftSigner::generate())),
    )
    .await
    .unwrap();
    let client = Client::new(muster_local::connect(&socket).await.unwrap());
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let Resp::AgentSessions(page) = client
                .call(Req::AgentList(v1::AgentListRequest::default()))
                .await
                .unwrap()
            else {
                panic!("expected catalog")
            };
            if page.sessions.iter().any(|session| session.id == id) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("synthetic session discovery");
    let Resp::FileList(list) = client
        .call(Req::AgentFiles(v1::AgentFilesRequest {
            session: id.into(),
            path: "src".into(),
        }))
        .await
        .unwrap()
    else {
        panic!("expected file list")
    };
    assert_eq!(list.entries[0].name, "example.txt");
    let Resp::FileContent(read) = client
        .call(Req::AgentFile(v1::AgentFileRequest {
            session: id.into(),
            path: "src/example.txt".into(),
            max_bytes: 6,
        }))
        .await
        .unwrap()
    else {
        panic!("expected file")
    };
    assert_eq!(read.content, b"sample");
    assert!(read.truncated);
    let Resp::GitLog(log) = client
        .call(Req::AgentGit(v1::AgentGitRequest {
            session: id.into(),
            limit: 1,
            ..Default::default()
        }))
        .await
        .unwrap()
    else {
        panic!("expected log")
    };
    assert_eq!(log.commits[0].subject, "Example change");
    let Resp::GitLog(diff) = client
        .call(Req::AgentGit(v1::AgentGitRequest {
            session: id.into(),
            diff_commit: log.commits[0].hash.clone(),
            ..Default::default()
        }))
        .await
        .unwrap()
    else {
        panic!("expected patch")
    };
    assert!(diff.diff.contains("+sample text"));
    daemon.shutdown().await;
}
