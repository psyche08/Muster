use std::future::{Future, pending};
use std::io::{self, Write};
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use muster_core::identity::{PublicKey, SharedSigner};
use muster_core::roster::{Author, DeviceKind, DeviceRecord, Entry, Roster};
use muster_core::{DeviceId, HwSigner, LocalPolicy, SoftSigner};
use muster_net::magicsock::{MagicOptions, RelayState};
use muster_net::remote_desktop::{Handler, Session};
use muster_net::{Error, Node, NodeConfig, RelayConfig};

struct Fixture {
    viewer: DeviceId,
    allowed: AtomicBool,
}

impl Handler for Fixture {
    fn allows(&self, peer: DeviceId) -> bool {
        peer == self.viewer && self.allowed.load(Ordering::Acquire)
    }

    fn serve<'session>(
        &'session self,
        _: DeviceId,
        session: &'session mut Session,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'session>> {
        Box::pin(async move {
            assert_eq!(session.recv_control().await.unwrap().unwrap(), b"control");
            session.send_control(b"confirmed").await.unwrap();
            let media = session.recv_media().await.unwrap();
            session.send_media(&media).unwrap();
            assert_eq!(session.recv_control().await.unwrap().unwrap(), b"revoke");
            self.allowed.store(false, Ordering::Release);
            pending().await
        })
    }
}

fn signed_roster(host: &dyn HwSigner, viewer: PublicKey) -> Vec<Entry> {
    let mut roster = Roster::new();
    let genesis = Author {
        roster: &roster,
        signer: host,
    }
    .genesis(
        "transport-test",
        ("10.44.0.0".parse().unwrap(), 24),
        DeviceRecord::new(host.public_key(), "host", DeviceKind::Desktop),
    )
    .unwrap();
    roster.insert(genesis.clone()).unwrap();
    let add_viewer = Author {
        roster: &roster,
        signer: host,
    }
    .add(DeviceRecord::new(viewer, "viewer", DeviceKind::Desktop))
    .unwrap();
    vec![genesis, add_viewer]
}

async fn node(
    signer: SharedSigner,
    entries: &[Entry],
    bind: IpAddr,
    relay: RelayConfig,
) -> Arc<Node> {
    let mut roster = Roster::new();
    for entry in entries {
        roster.insert(entry.clone()).unwrap();
    }
    let node = Node::start(
        NodeConfig {
            bind: (bind, 0).into(),
            relay,
            datagrams: true,
            options: MagicOptions {
                relay_only: true,
                ..Default::default()
            },
            ..Default::default()
        },
        signer,
        Arc::new(RwLock::new(roster)),
        Arc::new(RwLock::new(LocalPolicy::default())),
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while node.relay_state() != RelayState::Registered {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("test relay registration");
    node
}

async fn viewer_run(viewer: &Arc<Node>, host: DeviceId) {
    let mut vnet = viewer.take_datagrams().unwrap();
    let mut session = viewer.open_remote_desktop(host).await.unwrap();
    session.send_control(b"control").await.unwrap();
    assert_eq!(session.recv_control().await.unwrap().unwrap(), b"confirmed");
    assert!(session.send_control(&[0; 16 * 1024 + 1]).await.is_err());
    assert!(session.send_media(&[0; 1150]).is_err());
    let media = [0x7b; 1100];
    session.send_media(&media).unwrap();
    assert_eq!(session.recv_media().await.unwrap(), &media[..]);
    assert!(vnet.try_recv().is_err(), "media does not reach vnet");
    session.send_control(b"revoke").await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(2), session.recv_media())
            .await
            .expect("revocation ends media receive without closing PeerLink")
            .is_err()
    );
    drop(session);
    assert!(viewer.connection_to(&host).is_some());
    assert!(matches!(
        viewer.open_remote_desktop(host).await,
        Err(Error::Unauthorized(_))
    ));
    println!("TRANSPORT_OK control+media+isolation+revocation+denial");
}

async fn relay(bind: IpAddr) -> muster_relay::RelayServer {
    muster_relay::RelayServer::bind((bind, 0).into(), Default::default())
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn signed_peers_exchange_and_revoke_a_transport_session() {
    let bind = "127.0.0.1".parse().unwrap();
    let relay = relay(bind).await;
    let relay_addr = relay.udp_addr().unwrap();
    let relay = tokio::spawn(relay.serve());
    let host_key: SharedSigner = Arc::new(SoftSigner::generate());
    let viewer_key: SharedSigner = Arc::new(SoftSigner::generate());
    let entries = signed_roster(host_key.as_ref(), viewer_key.public_key());
    let host = node(host_key, &entries, bind, RelayConfig::Udp(relay_addr)).await;
    let viewer = node(viewer_key, &entries, bind, RelayConfig::Udp(relay_addr)).await;
    assert!(matches!(
        viewer.open_remote_desktop(host.device_id()).await,
        Err(Error::Unauthorized(_))
    ));
    host.set_remote_desktop_handler(Some(Arc::new(Fixture {
        viewer: viewer.device_id(),
        allowed: AtomicBool::new(true),
    })));
    tokio::time::timeout(
        Duration::from_secs(10),
        viewer_run(&viewer, host.device_id()),
    )
    .await
    .unwrap();
    viewer.shutdown().await;
    host.shutdown().await;
    relay.abort();
}

fn emit(label: &str, bytes: &[u8]) {
    print!("{label} ");
    for byte in bytes {
        print!("{byte:02x}");
    }
    println!();
    io::stdout().flush().unwrap();
}

fn input() -> String {
    let mut line = String::new();
    assert_ne!(
        io::stdin().read_line(&mut line).unwrap(),
        0,
        "missing fixture input"
    );
    line.trim().to_string()
}

fn decode(line: &str) -> Vec<u8> {
    assert!(line.is_ascii() && line.len().is_multiple_of(2));
    (0..line.len())
        .step_by(2)
        .map(|offset| u8::from_str_radix(&line[offset..offset + 2], 16).unwrap())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "manual two-host test, exchanging only ephemeral public keys and signed entries over SSH"]
async fn transport_between_hosts() {
    let bind = "0.0.0.0".parse().unwrap();
    let relay = RelayConfig::Secure(std::env::var("MUSTER_FIXTURE_RELAY").unwrap());
    let signer: SharedSigner = Arc::new(SoftSigner::generate());
    if std::env::var_os("MUSTER_FIXTURE_HOST").is_some() {
        let viewer = PublicKey::from_raw(&decode(&input())).unwrap();
        let entries = signed_roster(signer.as_ref(), viewer.clone());
        let host = node(signer, &entries, bind, relay).await;
        host.set_remote_desktop_handler(Some(Arc::new(Fixture {
            viewer: viewer.device_id(),
            allowed: AtomicBool::new(true),
        })));
        println!("HOST {}", host.device_id());
        for entry in entries {
            emit("ENTRY", &entry.encode());
        }
        assert_eq!(input(), "done");
        host.shutdown().await;
    } else {
        emit("VIEWER", signer.public_key().raw());
        let entries = [
            Entry::decode(&decode(&input())).unwrap(),
            Entry::decode(&decode(&input())).unwrap(),
        ];
        let host = DeviceId::from_hex(&input()).unwrap();
        let viewer = node(signer, &entries, bind, relay).await;
        tokio::time::timeout(Duration::from_secs(10), viewer_run(&viewer, host))
            .await
            .unwrap();
        viewer.shutdown().await;
    }
}
