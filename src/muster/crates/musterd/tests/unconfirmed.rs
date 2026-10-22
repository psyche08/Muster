//! A send refused behind an unconfirmed earlier write names that write in
//! its result, and a later send releases exactly that write before sending.
use muster_agent::{
    controller::Controller,
    model::{ControllerKind, PendingInteraction, SessionState},
};
use muster_proto::v1::{self, request::Req, response::Resp};
use musterd::{Daemon, DaemonConfig};
use std::sync::{Arc, Mutex};

/// A controller latched behind the write `app-1-4`, as the native one is
/// after a press without a provider receipt.
struct Latched {
    pending: Mutex<Option<String>>,
    sent: Mutex<Vec<String>>,
}

impl Latched {
    fn refused(&self, token: &str) -> muster_agent::Error {
        muster_agent::Error::Unconfirmed {
            operation: token.into(),
            message: format!("Not sent: the earlier send {token} has no provider receipt yet."),
        }
    }
}

#[async_trait::async_trait]
impl Controller for Latched {
    fn kind(&self) -> ControllerKind {
        ControllerKind::NativeApp
    }
    fn session_id(&self) -> String {
        "latched".into()
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
    fn release_unconfirmed(&self, operation: &str) -> Result<(), muster_agent::Error> {
        let mut pending = self.pending.lock().unwrap();
        match pending.as_deref() {
            Some(token) if token == operation => {
                *pending = None;
                Ok(())
            }
            Some(token) => Err(self.refused(token)),
            None => Ok(()),
        }
    }
    async fn send(&self, text: &str) -> Result<(), muster_agent::Error> {
        if let Some(token) = self.pending.lock().unwrap().as_deref() {
            return Err(self.refused(token));
        }
        self.sent.lock().unwrap().push(text.into());
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
async fn a_refusal_names_the_unconfirmed_write_and_only_that_one_is_released() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = DaemonConfig::new(dir.path());
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.agent = Some(muster_agent::AgentConfig {
        codex_home: None,
        claude_home: None,
        catdesk_home: None,
        claude_desktop: None,
        ..Default::default()
    });
    config.terminal = None;
    let daemon = Daemon::start_with(config, Arc::new(musterd::DefaultPlatform), None)
        .await
        .unwrap();
    let latched = Arc::new(Latched {
        pending: Mutex::new(Some("app-1-4".into())),
        sent: Mutex::new(Vec::new()),
    });
    daemon.agent().unwrap().attach(latched.clone());
    let client = muster_rpc::Client::new(
        tokio::net::UnixStream::connect(daemon.socket_path())
            .await
            .unwrap(),
    );
    let send = |op: &str, release: &str| {
        Req::AgentSend(v1::AgentSendRequest {
            op_id: op.into(),
            session: "latched".into(),
            text: "hello".into(),
            attachments: Vec::new(),
            release_unconfirmed: release.into(),
        })
    };
    for (op, release) in [("app-2-1", ""), ("app-2-2", "app-1-3")] {
        let Resp::OpResult(result) = client.call(send(op, release)).await.unwrap() else {
            panic!()
        };
        assert_eq!(result.status, v1::op_result::Status::Rejected as i32);
        assert_eq!(result.unconfirmed, "app-1-4");
        assert!(
            result.message.starts_with("conflict: Not sent"),
            "{}",
            result.message
        );
    }
    assert!(latched.sent.lock().unwrap().is_empty());
    let Resp::OpResult(result) = client.call(send("app-2-3", "app-1-4")).await.unwrap() else {
        panic!()
    };
    assert_eq!(
        result.status,
        v1::op_result::Status::Applied as i32,
        "{}",
        result.message
    );
    assert!(result.unconfirmed.is_empty());
    assert_eq!(latched.sent.lock().unwrap().as_slice(), ["hello"]);
    daemon.shutdown().await;
}

/// Exercises the real RPC ledger and delivery scope with a native-style guard.
struct DelayedReceipt {
    guard: muster_agent::unconfirmed::WriteGuard,
    transcript: std::path::PathBuf,
    sends: std::sync::atomic::AtomicUsize,
}

#[async_trait::async_trait]
impl Controller for DelayedReceipt {
    fn kind(&self) -> ControllerKind {
        ControllerKind::NativeApp
    }
    fn session_id(&self) -> String {
        "late-receipt-session".into()
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
    fn reports_delivery(&self) -> bool {
        true
    }
    async fn send(&self, text: &str) -> Result<(), muster_agent::Error> {
        self.guard.admit()?;
        let receipt = muster_agent::receipt::Receipt::new(
            &self.transcript,
            muster_agent::model::Provider::Codex,
            text,
        )
        .unwrap();
        self.guard.arm(
            muster_agent::unconfirmed::Origin::now("send"),
            Some(receipt),
            None,
        )?;
        muster_agent::control_guard::delivery_attempt();
        self.sends.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if text == "immediate prompt" {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&self.transcript)
                .unwrap();
            writeln!(
                file,
                "{}",
                serde_json::json!({"type":"response_item","payload":{
                "type":"message","role":"user","content":[{"type":"input_text","text":text}]}})
            )
            .unwrap();
            assert!(self.guard.reconcile().cleared());
            return Ok(());
        }
        Err(muster_agent::Error::UnknownOutcome(
            "receipt not yet available".into(),
        ))
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
async fn late_native_receipt_updates_original_rpc_ledger_without_resending() {
    use std::io::Write;
    use std::sync::atomic::Ordering;
    let dir = tempfile::tempdir().unwrap();
    let mut config = DaemonConfig::new(dir.path());
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.agent = Some(muster_agent::AgentConfig {
        codex_home: None,
        claude_home: None,
        catdesk_home: None,
        claude_desktop: None,
        ..Default::default()
    });
    config.terminal = None;
    let daemon = Daemon::start_with(config, Arc::new(musterd::DefaultPlatform), None)
        .await
        .unwrap();
    let mut transcript = tempfile::NamedTempFile::new().unwrap();
    let controller = Arc::new(DelayedReceipt {
        guard: Default::default(),
        transcript: transcript.path().into(),
        sends: Default::default(),
    });
    daemon.agent().unwrap().attach(controller.clone());
    let client = muster_rpc::Client::new(
        tokio::net::UnixStream::connect(daemon.socket_path())
            .await
            .unwrap(),
    );
    let request = || {
        Req::AgentSend(v1::AgentSendRequest {
            op_id: "late-op".into(),
            session: controller.session_id(),
            text: "late prompt".into(),
            ..Default::default()
        })
    };
    let Resp::OpResult(result) = client.call(request()).await.unwrap() else {
        panic!()
    };
    assert_eq!(result.status, v1::op_result::Status::Unknown as i32);
    assert_eq!(
        daemon
            .store()
            .ledger()
            .status("late-op")
            .unwrap()
            .as_deref(),
        Some("pending")
    );
    writeln!(
        transcript,
        "{}",
        serde_json::json!({"type":"response_item","payload":{
        "type":"message","role":"user","content":[{"type":"input_text","text":"late prompt"}]}})
    )
    .unwrap();
    assert!(controller.guard.reconcile().cleared());
    assert_eq!(
        daemon
            .store()
            .ledger()
            .status("late-op")
            .unwrap()
            .as_deref(),
        Some("applied")
    );
    let Resp::OpResult(result) = client.call(request()).await.unwrap() else {
        panic!()
    };
    assert_eq!(result.status, v1::op_result::Status::Duplicate as i32);
    assert_eq!(result.result_id, controller.session_id());
    assert_eq!(controller.sends.load(Ordering::SeqCst), 1);
    // Confirmation may persist before the synchronous handler completes too.
    let Resp::OpResult(result) = client
        .call(Req::AgentSend(v1::AgentSendRequest {
            op_id: "immediate-op".into(),
            session: controller.session_id(),
            text: "immediate prompt".into(),
            ..Default::default()
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(result.status, v1::op_result::Status::Applied as i32);
    assert_eq!(result.result_id, controller.session_id());
    assert_eq!(controller.sends.load(Ordering::SeqCst), 2);
    daemon.shutdown().await;
}
