//! Actual RPC -> authorization/ledger -> private file -> controller boundary.
use muster_agent::{
    controller::Controller,
    model::{ControllerKind, PendingInteraction, SessionState},
};
use muster_proto::v1::{self, request::Req, response::Resp};
use musterd::{Daemon, DaemonConfig};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Capture(Mutex<Vec<String>>);
#[async_trait::async_trait]
impl Controller for Capture {
    fn kind(&self) -> ControllerKind {
        ControllerKind::NativeApp
    }
    fn session_id(&self) -> String {
        "01a0e659-2de9-7f63-b80b-3b53fc7754e0".into()
    }
    fn state(&self) -> SessionState {
        SessionState::Idle
    }
    fn process_identity(&self) -> muster_agent::controller::ProcessIdentity {
        muster_agent::controller::ProcessIdentity { pid: 1, start: 1 }
    }
    fn process_state(&self) -> muster_agent::controller::ProcessState {
        muster_agent::controller::ProcessState::Running
    }
    fn pending(&self) -> Vec<PendingInteraction> {
        vec![]
    }
    async fn model_options(
        &self,
        set: Option<(&str, &str)>,
    ) -> Result<v1::AgentChatOptions, muster_agent::Error> {
        let (model, effort) = set.unwrap_or(("Fixture model", "Low"));
        Ok(v1::AgentChatOptions {
            session: self.session_id(),
            models: vec![v1::AgentChatOption {
                name: model.into(),
                selected: true,
                ..Default::default()
            }],
            effort: effort.into(),
            ..Default::default()
        })
    }
    async fn send(&self, text: &str) -> Result<(), muster_agent::Error> {
        self.0.lock().unwrap().push(text.to_string());
        Ok(())
    }
    async fn respond(&self, _: &str, _: &str, _: &str) -> Result<(), muster_agent::Error> {
        unreachable!()
    }
    async fn interrupt(&self) -> Result<(), muster_agent::Error> {
        unreachable!()
    }
    async fn stop(&self) {}
}

#[tokio::test]
async fn attachment_rpc_preserves_bytes_and_duplicate_does_not_upload_or_send_again() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = DaemonConfig::new(dir.path());
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.agent = Some(muster_agent::AgentConfig {
        codex_home: None,
        claude_home: None,
        catdesk_home: None,
        deepseek_home: None,
        claude_desktop: None,
        ..Default::default()
    });
    config.terminal = None;
    let daemon = Daemon::start_with(config, Arc::new(musterd::DefaultPlatform), None)
        .await
        .unwrap();
    let capture = Arc::new(Capture::default());
    daemon.agent().unwrap().attach(capture.clone());
    let client = muster_rpc::Client::new(
        tokio::net::UnixStream::connect(daemon.socket_path())
            .await
            .unwrap(),
    );
    let mut request = v1::AgentSendRequest {
        op_id: "upload-once".into(),
        session: "01a0e659-2de9-7f63-b80b-3b53fc7754e0".into(),
        text: "Inspect the attachment".into(),
        attachments: vec![v1::AgentAttachment {
            name: "sample.bin".into(),
            media_type: "application/octet-stream".into(),
            data: (0..512 * 1024).map(|i| (i % 251) as u8).collect(),
        }],
        release_unconfirmed: String::new(),
    };
    for expected in [
        v1::op_result::Status::Applied,
        v1::op_result::Status::Duplicate,
    ] {
        let Resp::OpResult(result) = client.call(Req::AgentSend(request.clone())).await.unwrap()
        else {
            panic!()
        };
        assert_eq!(result.status, expected as i32, "{}", result.message);
    }
    assert_eq!(capture.0.lock().unwrap().len(), 1);
    let directories: Vec<_> = std::fs::read_dir(dir.path().join("attachments"))
        .unwrap()
        .map(|p| p.unwrap().path())
        .collect();
    assert_eq!(directories.len(), 1);
    let path = directories[0].join("0-sample.bin");
    assert_eq!(std::fs::read(&path).unwrap(), request.attachments[0].data);
    assert!(capture.0.lock().unwrap()[0].contains(path.to_str().unwrap()));
    request.attachments[0].data[0] ^= 1;
    let Resp::OpResult(result) = client.call(Req::AgentSend(request.clone())).await.unwrap() else {
        panic!()
    };
    assert_eq!(result.status, v1::op_result::Status::Rejected as i32);
    assert_eq!(capture.0.lock().unwrap().len(), 1);
    request.op_id = "invalid-path".into();
    request.attachments[0].name = "../outside".into();
    assert!(client.call(Req::AgentSend(request)).await.is_err());
    assert!(!dir.path().join("outside").exists());
    daemon.shutdown().await;
}

struct MissingAccessibility;
impl musterd::Platform for MissingAccessibility {
    fn accessibility(&self) -> Option<bool> {
        Some(false)
    }
}

#[tokio::test]
async fn missing_accessibility_is_actionable_and_does_not_stage_attachments() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = DaemonConfig::new(dir.path());
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.name = "Mac-C".into();
    config.agent = Some(muster_agent::AgentConfig {
        codex_home: None,
        claude_home: None,
        catdesk_home: None,
        deepseek_home: None,
        claude_desktop: None,
        ..Default::default()
    });
    config.terminal = None;
    let daemon = Daemon::start_with(config, Arc::new(MissingAccessibility), None)
        .await
        .unwrap();
    let client = muster_rpc::Client::new(
        tokio::net::UnixStream::connect(daemon.socket_path())
            .await
            .unwrap(),
    );
    let error = client
        .call(Req::AgentSend(v1::AgentSendRequest {
            op_id: "permission-denied".into(),
            session: "target-session".into(),
            text: "fixture".into(),
            attachments: vec![],
            release_unconfirmed: String::new(),
        }))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("Mac-C"), "{error}");
    assert!(
        error.contains("Accessibility") && error.contains("System Settings"),
        "{error}"
    );
    assert!(!dir.path().join("attachments").exists());
}

/// A daemon whose platform drives no desktop apps (Linux, or a test with
/// `DefaultPlatform` on a Mac) selects nothing through Accessibility: without
/// an attached controller a send fails clearly before the ledger or any
/// attachment write, and desktop chats are unavailable.
#[tokio::test]
async fn platform_without_desktop_control_never_selects_through_accessibility() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = DaemonConfig::new(dir.path());
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.agent = Some(muster_agent::AgentConfig {
        codex_home: None,
        claude_home: None,
        catdesk_home: None,
        deepseek_home: None,
        claude_desktop: None,
        ..Default::default()
    });
    config.terminal = None;
    let daemon = Daemon::start_with(config, Arc::new(musterd::DefaultPlatform), None)
        .await
        .unwrap();
    let client = muster_rpc::Client::new(
        tokio::net::UnixStream::connect(daemon.socket_path())
            .await
            .unwrap(),
    );
    let error = client
        .call(Req::AgentSend(v1::AgentSendRequest {
            op_id: "no-controller".into(),
            session: "unattached-session".into(),
            text: "fixture".into(),
            attachments: vec![v1::AgentAttachment {
                name: "note.txt".into(),
                media_type: "text/plain".into(),
                data: b"x".to_vec(),
            }],
            release_unconfirmed: String::new(),
        }))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("no desktop Accessibility control"),
        "{error}"
    );
    assert!(!dir.path().join("attachments").exists());
    let error = client
        .call(Req::AgentChats(v1::AgentChatsRequest {
            provider: "claude".into(),
            ..Default::default()
        }))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("no desktop Accessibility control"),
        "{error}"
    );
    let error = client
        .call(Req::AgentChatOptions(v1::AgentChatOptionsRequest {
            session: "claude-chat-new".into(),
            ..Default::default()
        }))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("no desktop Accessibility control"),
        "{error}"
    );
    daemon.shutdown().await;
}

#[tokio::test]
async fn coding_options_rpc_uses_the_attached_session_controller() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("codex");
    std::fs::create_dir_all(home.join("sessions")).unwrap();
    std::fs::write(
        home.join("sessions/rollout-options.jsonl"),
        "{\"type\":\"session_meta\",\"payload\":{\"id\":\"01a0e659-2de9-7f63-b80b-3b53fc7754e0\"}}\n",
    )
    .unwrap();
    let mut config = DaemonConfig::new(dir.path().join("daemon"));
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.agent = Some(muster_agent::AgentConfig {
        codex_home: Some(home),
        claude_home: None,
        claude_desktop: None,
        catdesk_home: None,
        deepseek_home: None,
        ..Default::default()
    });
    config.terminal = None;
    let daemon = Daemon::start_with(config, Arc::new(musterd::DefaultPlatform), None)
        .await
        .unwrap();
    let agent = daemon.agent().unwrap();
    agent.rescan().await;
    let capture = Arc::new(Capture::default());
    agent.attach_native(capture.clone());
    let client = muster_rpc::Client::new(
        tokio::net::UnixStream::connect(daemon.socket_path())
            .await
            .unwrap(),
    );
    let Resp::AgentChatOptions(options) = client
        .call(Req::AgentChatOptions(v1::AgentChatOptionsRequest {
            session: "01a0e659-2de9-7f63-b80b-3b53fc7754e0".into(),
            ..Default::default()
        }))
        .await
        .unwrap()
    else {
        panic!("options response")
    };
    assert_eq!(options.models[0].name, "Fixture model");
    let Resp::AgentChatOptions(options) = client
        .call(Req::AgentChatSetOptions(v1::AgentChatSetOptionsRequest {
            session: "01a0e659-2de9-7f63-b80b-3b53fc7754e0".into(),
            model: "Another model".into(),
            effort: "High".into(),
            ..Default::default()
        }))
        .await
        .unwrap()
    else {
        panic!("options response")
    };
    assert_eq!(options.models[0].name, "Another model");
    assert_eq!(options.effort, "High");
    assert!(
        capture.0.lock().unwrap().is_empty(),
        "option changes must never send a prompt"
    );
    assert!(agent.retains_native_controller("01a0e659-2de9-7f63-b80b-3b53fc7754e0"));
    assert!(
        client
            .call(Req::AgentChatSetOptions(v1::AgentChatSetOptionsRequest {
                session: "01a0e659-2de9-7f63-b80b-3b53fc7754e0".into(),
                ..Default::default()
            }))
            .await
            .is_err()
    );
    assert!(
        client
            .call(Req::AgentChatOptions(v1::AgentChatOptionsRequest {
                session: "missing-session".into(),
                ..Default::default()
            }))
            .await
            .is_err()
    );
    daemon.shutdown().await;
}
