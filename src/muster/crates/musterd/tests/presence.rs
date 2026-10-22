//! Two real daemons: registration, explicit approval, signed proxy use and revocation.
use muster_core::{HwSigner, SoftSigner};
use muster_proto::v1::{self, request::Req, response::Resp};
use muster_rpc::Client;
use musterd::{Daemon, DaemonConfig};
use std::sync::Arc;
use std::time::Duration;

async fn local(d: &Daemon) -> Client {
    Client::new(
        tokio::net::UnixStream::connect(d.socket_path())
            .await
            .unwrap(),
    )
}
async fn cover(client: &Client, presence: &SoftSigner) -> Result<Resp, muster_rpc::Error> {
    client
        .start_signed(
            Req::DeviceSetCover(v1::DeviceSetCoverRequest { on: true }),
            &|message| presence.sign(message).unwrap(),
        )
        .await?
        .finish()
        .await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn registration_approval_signed_proxy_persistence_and_revocation() {
    let relay = muster_relay::RelayServer::bind("127.0.0.1:0".parse().unwrap(), Default::default())
        .await
        .unwrap();
    let address = relay.udp_addr().unwrap().to_string();
    let relay_task = tokio::spawn(async move { relay.serve().await });
    let da = tempfile::tempdir().unwrap();
    let db = tempfile::tempdir().unwrap();
    let config = |dir: &std::path::Path, name: &str| {
        let mut c = DaemonConfig::new(dir);
        c.name = name.into();
        c.bind = "127.0.0.1:0".parse().unwrap();
        c.relay = Some(address.clone());
        c.allow_loopback = true;
        c.agent = None;
        c.terminal = None;
        c
    };
    let a = Daemon::start_with(
        config(da.path(), "admin"),
        Arc::new(musterd::DefaultPlatform),
        None,
    )
    .await
    .unwrap();
    let b = Daemon::start_with(
        config(db.path(), "desktop"),
        Arc::new(musterd::DefaultPlatform),
        None,
    )
    .await
    .unwrap();
    a.create_network("presence", "192.168.13.0/24")
        .await
        .unwrap();
    let invite = a.make_invite(60).unwrap().encode();
    let joiner = b.clone();
    let join =
        tokio::spawn(async move { joiner.join(&invite, "desktop", "desktop", &|_, _| {}).await });
    tokio::time::timeout(Duration::from_secs(10), async {
        while a.pending_joins().joins.is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    a.decide(&b.device_id().to_hex(), true, &["agent.view".into()], "")
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), join)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let ca = local(&a).await;
    let cb = local(&b).await;
    let before = a.network_status();
    let presence = SoftSigner::generate();
    cb.call(Req::DevicePresence(v1::DevicePresenceRequest {
        public_key: presence.public_key().raw().to_vec(),
    }))
    .await
    .unwrap();
    assert!(
        b.roster()
            .read()
            .unwrap()
            .state()
            .active(&b.device_id())
            .unwrap()
            .record
            .presence_key
            .is_none()
    );
    let remote = cb.with_target(&a.device_id().to_hex());
    assert!(
        remote
            .call(Req::DevicePresence(v1::DevicePresenceRequest {
                public_key: presence.public_key().raw().to_vec()
            }))
            .await
            .is_err()
    );
    let enroll = |grant, revoke| {
        Req::NetworkPresence(v1::NetworkPresenceRequest {
            device_id: b.device_id().to_hex(),
            grant_locked_control: grant,
            revoke,
        })
    };
    assert!(cb.call(enroll(false, false)).await.is_err());
    ca.call(enroll(false, false)).await.unwrap();
    a.policy().write().unwrap().allow_locked_control = true;
    assert!(
        cover(&remote, &presence).await.is_err(),
        "registration must not grant locked control"
    );
    ca.call(enroll(true, false)).await.unwrap();
    let accepted = a.roster().read().unwrap().entries().count();
    ca.call(enroll(true, false)).await.unwrap();
    assert_eq!(
        a.roster().read().unwrap().entries().count(),
        accepted,
        "repeat delivery must not advance the epoch"
    );
    assert!(
        cover(&remote, &presence).await.is_ok(),
        "desktop signature must survive local proxying"
    );
    assert!(cover(&remote, &SoftSigner::generate()).await.is_err());
    for daemon in [&a, &b] {
        let stored = daemon.store().load_roster().unwrap();
        assert_eq!(
            stored
                .state()
                .active(&b.device_id())
                .unwrap()
                .record
                .presence_key,
            Some(presence.public_key())
        );
        assert_eq!(stored.network_id().unwrap().to_hex(), before.network_id);
        assert_eq!(
            stored
                .state()
                .active(&b.device_id())
                .unwrap()
                .record
                .vip
                .map(|ip| ip.to_string())
                .unwrap(),
            before
                .members
                .iter()
                .find(|m| m.device_id == b.device_id().to_hex())
                .unwrap()
                .vip
        );
    }
    ca.call(enroll(false, true)).await.unwrap();
    assert!(
        cover(&remote, &presence).await.is_err(),
        "revoked binding cannot resurrect itself from a stale envelope"
    );
    assert!(
        a.store()
            .load_roster()
            .unwrap()
            .state()
            .active(&b.device_id())
            .unwrap()
            .record
            .presence_key
            .is_none()
    );
    a.shutdown().await;
    b.shutdown().await;
    relay_task.abort();
}
