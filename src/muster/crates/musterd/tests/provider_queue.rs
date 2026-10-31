//! Independent RPC clients and sessions must share the target provider queue.
use muster_agent::{
    controller::{Controller, ProcessIdentity, ProcessState},
    model::{ControllerKind, PendingInteraction, SessionState},
};
use muster_proto::v1::{self, request::Req, response::Resp};
use musterd::{Daemon, DaemonConfig};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct NativeWork {
    session: String,
    events: tokio::sync::mpsc::UnboundedSender<String>,
    release: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}
#[async_trait::async_trait]
impl Controller for NativeWork {
    fn kind(&self) -> ControllerKind {
        ControllerKind::NativeApp
    }
    fn session_id(&self) -> String {
        self.session.clone()
    }
    fn state(&self) -> SessionState {
        SessionState::Idle
    }
    fn process_identity(&self) -> ProcessIdentity {
        ProcessIdentity { pid: 1, start: 1 }
    }
    fn process_state(&self) -> ProcessState {
        ProcessState::Running
    }
    fn pending(&self) -> Vec<PendingInteraction> {
        vec![]
    }
    async fn send(&self, text: &str) -> Result<(), muster_agent::Error> {
        let wait = self.release.lock().unwrap().take();
        let events = self.events.clone();
        let text = text.to_owned();
        muster_agent::control_guard::spawn_blocking(move || {
            events.send(format!("{text}:start")).unwrap();
            if let Some(wait) = wait {
                wait.recv_timeout(Duration::from_secs(10)).unwrap();
            }
            events.send(format!("{text}:finish")).unwrap();
        })
        .await
        .unwrap();
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

async fn independent_clients(cancel_first: bool) {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("codex");
    std::fs::create_dir_all(home.join("sessions")).unwrap();
    let sessions = [
        "01a0e659-2de9-7f63-b80b-3b53fc7754e0",
        "01a0e659-2de9-7f63-b80b-3b53fc7754e1",
    ];
    for (index, id) in sessions.iter().enumerate() {
        std::fs::write(
            home.join(format!("sessions/rollout-{index}.jsonl")),
            format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\"}}}}\n"),
        )
        .unwrap();
    }
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
    let (events, mut observed) = tokio::sync::mpsc::unbounded_channel();
    let (release, wait) = std::sync::mpsc::channel();
    for (session, wait) in sessions.iter().zip([Some(wait), None]) {
        agent.attach_native(Arc::new(NativeWork {
            session: session.to_string(),
            events: events.clone(),
            release: Mutex::new(wait),
        }));
    }
    let first = muster_rpc::Client::new(
        tokio::net::UnixStream::connect(daemon.socket_path())
            .await
            .unwrap(),
    );
    let second = muster_rpc::Client::new(
        tokio::net::UnixStream::connect(daemon.socket_path())
            .await
            .unwrap(),
    );
    let request = |index: usize| {
        Req::AgentSend(v1::AgentSendRequest {
            op_id: format!("client-{index}"),
            session: sessions[index].into(),
            text: index.to_string(),
            ..Default::default()
        })
    };
    let call = first.start(request(0)).await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), observed.recv())
            .await
            .unwrap()
            .as_deref(),
        Some("0:start")
    );
    let first_response = if cancel_first {
        call.cancel().await;
        assert!(
            tokio::time::timeout(Duration::from_secs(4), call.finish())
                .await
                .unwrap()
                .is_err()
        );
        None
    } else {
        Some(call)
    };
    let next = second.start(request(1)).await.unwrap();
    let overtook = tokio::time::timeout(Duration::from_millis(150), observed.recv()).await;
    release.send(()).unwrap();
    assert!(
        overtook.is_err(),
        "another client entered before the first OS operation finished: {overtook:?}"
    );
    for expected in ["0:finish", "1:start", "1:finish"] {
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), observed.recv())
                .await
                .unwrap()
                .as_deref(),
            Some(expected)
        );
    }
    if let Some(call) = first_response {
        assert!(
            matches!(call.finish().await.unwrap(), Resp::OpResult(r) if r.status == v1::op_result::Status::Applied as i32)
        );
    }
    assert!(
        matches!(next.finish().await.unwrap(), Resp::OpResult(r) if r.status == v1::op_result::Status::Applied as i32)
    );
    daemon.shutdown().await;
}

#[tokio::test]
async fn separate_clients_and_sessions_serialize_until_completion() {
    independent_clients(false).await;
}

#[tokio::test]
async fn cancelled_rpc_does_not_release_provider_while_os_work_runs() {
    independent_clients(true).await;
}
