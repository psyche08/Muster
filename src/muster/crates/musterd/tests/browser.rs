//! Real RPC authorization + operation ledger; Chrome is a bounded socket fixture.
use muster_core::{
    HwSigner, Scope, SoftSigner,
    roster::{Author, DeviceKind, DeviceRecord},
};
use muster_proto::v1::{self, op_result::Status, request::Req, response::Resp};
use muster_rpc::{Client, Principal};
use musterd::{Daemon, DaemonConfig};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

struct Console(AtomicBool);
impl muster_platform::Platform for Console {
    fn locked(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

async fn fixture() -> (
    tempfile::TempDir,
    Arc<Daemon>,
    Client,
    SoftSigner,
    Arc<Console>,
    Arc<AtomicUsize>,
) {
    let dir = tempfile::tempdir().unwrap();
    let mut config = DaemonConfig::new(dir.path());
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.agent = None;
    config.terminal = None;
    let console = Arc::new(Console(AtomicBool::new(false)));
    let daemon = Daemon::start_with(config, console.clone(), None)
        .await
        .unwrap();
    let peer = SoftSigner::generate();
    let presence = SoftSigner::generate();
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
        let mut record = DeviceRecord::new(peer.public_key(), "phone", DeviceKind::Phone);
        record.presence_key = Some(presence.public_key());
        record.scopes = [
            Scope::AgentView,
            Scope::AgentControl,
            Scope::BrowserView,
            Scope::BrowserControl,
            Scope::LockedControl,
        ]
        .into_iter()
        .collect();
        let entry = Author {
            roster: &roster,
            signer: &**daemon.signer(),
        }
        .add(record)
        .unwrap();
        roster.insert(entry).unwrap();
    }
    let (a, b) = tokio::io::duplex(1024 * 1024);
    tokio::spawn(muster_rpc::serve(
        b,
        Principal::Remote(peer.public_key().device_id()),
        Arc::new(musterd::handler::MusterHandler::new(Arc::downgrade(
            &daemon,
        ))),
    ));
    std::fs::create_dir(dir.path().join("browser")).unwrap();
    let listener = tokio::net::UnixListener::bind(dir.path().join("browser/bridge.sock")).unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let observed = count.clone();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            muster_browser::write_frame(&mut stream, &json!({"ready":true}), 128)
                .await
                .unwrap();
            let request = muster_browser::read_frame(&mut stream, muster_browser::MAX_REQUEST)
                .await
                .unwrap();
            observed.fetch_add(1, Ordering::SeqCst);
            muster_browser::write_frame(
                &mut stream,
                &json!({"result":{"op":request["op"]}}),
                muster_browser::MAX_RESPONSE,
            )
            .await
            .unwrap();
        }
    });
    (dir, daemon, Client::new(a), presence, console, count)
}
fn request(id: &str) -> Req {
    Req::BrowserCall(v1::BrowserCallRequest {
        op_id: id.into(),
        json: r#"{"op":"type","tabId":1,"document":2,"ref":3,"text":"hello"}"#.into(),
    })
}
fn status(resp: Resp) -> Status {
    let Resp::OpResult(r) = resp else {
        panic!("unexpected response")
    };
    Status::try_from(r.status).unwrap()
}

#[tokio::test]
async fn locked_browser_requires_policy_presence_and_binding_without_unlocking() {
    let (_dir, daemon, client, presence, console, count) = fixture().await;
    console.0.store(true, Ordering::SeqCst);
    assert!(client.call(request("denied-policy")).await.is_err());
    daemon.policy().write().unwrap().allow_locked_control = true;
    assert!(client.call(request("denied-presence")).await.is_err());
    let id = client.next_id();
    let signature = presence
        .sign(&muster_proto::presence_message(id, &request("original")))
        .unwrap();
    assert!(
        client
            .start_presigned(id, request("changed"), signature)
            .await
            .unwrap()
            .finish()
            .await
            .is_err()
    );
    assert_eq!(count.load(Ordering::SeqCst), 0);
    for expected in [Status::Applied, Status::Duplicate] {
        let call = client
            .start_signed(request("once"), &|m| presence.sign(m).unwrap())
            .await
            .unwrap();
        assert_eq!(status(call.finish().await.unwrap()), expected);
    }
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert!(console.locked());
    assert!(
        client
            .call(Req::BrowserSetup(v1::BrowserSetupRequest { install: true }))
            .await
            .is_err()
    );
    daemon.shutdown().await;
}

#[tokio::test]
async fn browser_scopes_are_independent_and_revocation_precedes_cached_result() {
    let (_dir, daemon, client, _presence, _console, count) = fixture().await;
    assert_eq!(
        status(client.call(request("once")).await.unwrap()),
        Status::Applied
    );
    let peer_record = {
        let roster = daemon.roster().read().unwrap();
        roster
            .state()
            .members
            .values()
            .find(|m| m.record.name == "phone")
            .unwrap()
            .record
            .clone()
    };
    // Preserve agent permissions while removing browser permissions.
    let mut record = peer_record;
    record.scopes = [Scope::AgentView, Scope::AgentControl, Scope::LockedControl]
        .into_iter()
        .collect();
    {
        let mut roster = daemon.roster().write().unwrap();
        let entry = Author {
            roster: &roster,
            signer: &**daemon.signer(),
        }
        .update(
            record.device_id,
            muster_core::roster::DeviceUpdate {
                scopes: Some(record.scopes),
                ..Default::default()
            },
        )
        .unwrap();
        roster.insert(entry).unwrap();
    }
    assert!(client.call(request("once")).await.is_err());
    assert!(
        client
            .call(Req::BrowserCall(v1::BrowserCallRequest {
                op_id: "read".into(),
                json: r#"{"op":"tabs"}"#.into()
            }))
            .await
            .is_err()
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    daemon.shutdown().await;
}
use muster_platform::Platform;
