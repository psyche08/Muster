//! Receipt reads remain authorized while locked and cannot start native work.
use muster_core::{Author, DeviceKind, DeviceRecord, HwSigner, Scope, SoftSigner};
use muster_proto::{
    Message,
    v1::{self, op_result::Status, request::Req, response::Resp},
};
use muster_rpc::{Client, Principal};
use musterd::{Daemon, DaemonConfig};
use std::sync::Arc;
struct Locked;
impl muster_platform::Platform for Locked {
    fn locked(&self) -> bool {
        true
    }
}
fn connect(daemon: &Arc<Daemon>, peer: muster_core::DeviceId) -> Client {
    let (a, b) = tokio::io::duplex(1024 * 1024);
    tokio::spawn(muster_rpc::serve(
        b,
        Principal::Remote(peer),
        Arc::new(musterd::MusterHandler::new(Arc::downgrade(daemon))),
    ));
    Client::new(a)
}
#[tokio::test]
async fn locked_receipt_lookup_requires_current_grants_and_exact_original_body() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = DaemonConfig::new(dir.path());
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.listen = false;
    config.terminal = None;
    config.agent = Some(muster_agent::AgentConfig {
        codex_home: None,
        claude_home: None,
        catdesk_home: None,
        claude_desktop: None,
        ..Default::default()
    });
    let daemon = Daemon::start_with(config, Arc::new(Locked), None)
        .await
        .unwrap();
    use muster_agent::desktop_chat::{ChatApp, ChatKey};
    let session = daemon
        .agent()
        .unwrap()
        .chat_observed(
            ChatApp::ChatGpt,
            None,
            ChatKey::Id("receipt-fixture".into()),
            "receipt",
            Vec::new(),
        )
        .unwrap();
    let provider = daemon.agent().unwrap().session(&session).unwrap().provider;
    let allowed = SoftSigner::generate();
    let denied = SoftSigner::generate();
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
        for (signer, grant) in [(&allowed, true), (&denied, false)] {
            let mut record = DeviceRecord::new(signer.public_key(), "phone", DeviceKind::Phone);
            if grant {
                record
                    .scopes
                    .extend([Scope::Agents, Scope::AgentProvider(provider.clone())]);
            }
            let entry = Author {
                roster: &roster,
                signer: &**daemon.signer(),
            }
            .add(record)
            .unwrap();
            roster.insert(entry).unwrap();
        }
    }
    let request = v1::AgentSendRequest {
        op_id: "original".into(),
        session,
        text: "exact body".into(),
        ..Default::default()
    };
    let query = || Req::AgentSendStatus(request.clone());
    let client = connect(&daemon, allowed.public_key().device_id());
    let Resp::OpResult(r) = client.call(query()).await.unwrap() else {
        panic!()
    };
    assert_eq!(r.status, Status::Unknown as i32);
    assert!(
        daemon
            .store()
            .ledger()
            .status(&request.op_id)
            .unwrap()
            .is_none()
    );
    daemon
        .store()
        .ledger()
        .begin(&request.op_id, "agent.send", &request.encode_to_vec())
        .unwrap();
    daemon
        .store()
        .ledger()
        .complete(&request.op_id, request.session.as_bytes())
        .unwrap();
    drop(client);
    let client = connect(&daemon, allowed.public_key().device_id());
    let Resp::OpResult(r) = client.call(query()).await.unwrap() else {
        panic!()
    };
    assert_eq!(r.status, Status::Applied as i32);
    assert_eq!(r.result_id, request.session);
    let mut changed = request.clone();
    changed.text.push('x');
    assert!(client.call(Req::AgentSendStatus(changed)).await.is_err());
    assert!(
        connect(&daemon, denied.public_key().device_id())
            .call(query())
            .await
            .is_err()
    );
    let mut absent = request.clone();
    absent.session = "does-not-exist".into();
    assert!(matches!(
        connect(&daemon, denied.public_key().device_id())
            .call(Req::AgentSendStatus(absent))
            .await,
        Err(muster_rpc::Error::Remote {
            code: v1::error::Code::Unauthorized,
            ..
        })
    ));
    let mut manual = request.clone();
    manual.op_id = "manual".into();
    let confirm = || Req::AgentConfirmDelivered(manual.clone());
    assert!(
        client.call(confirm()).await.is_err(),
        "missing operations cannot be invented"
    );
    daemon
        .store()
        .ledger()
        .begin(&manual.op_id, "agent.send", &manual.encode_to_vec())
        .unwrap();
    let mut mismatch = manual.clone();
    mismatch.text.push('x');
    assert!(
        client
            .call(Req::AgentConfirmDelivered(mismatch))
            .await
            .is_err()
    );
    assert!(
        connect(&daemon, denied.public_key().device_id())
            .call(confirm())
            .await
            .is_err()
    );
    for _ in 0..2 {
        let Resp::OpResult(r) = client.call(confirm()).await.unwrap() else {
            panic!()
        };
        assert_eq!(r.status, Status::UserConfirmed as i32);
    }
    let Resp::OpResult(r) = client
        .call(Req::AgentSendStatus(manual.clone()))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(r.status, Status::UserConfirmed as i32);
    assert_eq!(r.result_id, manual.session);
    daemon.policy().write().unwrap().offer_agent = false;
    assert!(client.call(confirm()).await.is_err());
    assert!(client.call(query()).await.is_err());
    daemon.shutdown().await;
}
