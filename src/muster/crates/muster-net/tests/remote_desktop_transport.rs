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
use muster_net::remote_desktop::{Access, Handler, Session};
use muster_net::{Error, Node, NodeConfig, RelayConfig};

struct Fixture {
    mouse: bool,
    allowed: AtomicBool,
}

impl Handler for Fixture {
    fn console_ready(&self) -> bool {
        self.allowed.load(Ordering::Acquire)
    }

    fn serve<'session>(
        &'session self,
        _: DeviceId,
        session: &'session mut Session,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'session>> {
        Box::pin(async move {
            let mut applied = false;
            assert_eq!(session.with_mouse(|| applied = true).is_ok(), self.mouse);
            assert_eq!(applied, self.mouse);
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

async fn viewer_run(
    viewer: &Arc<Node>,
    host: DeviceId,
    vnet: &mut tokio::sync::mpsc::Receiver<(DeviceId, bytes::Bytes)>,
) {
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
    let mut vnet = viewer.take_datagrams().unwrap();
    let view_offer = host
        .share_remote_desktop(
            viewer.device_id(),
            Access::View,
            Arc::new(Fixture {
                mouse: false,
                allowed: AtomicBool::new(true),
            }),
        )
        .unwrap();
    tokio::time::timeout(
        Duration::from_secs(10),
        viewer_run(&viewer, host.device_id(), &mut vnet),
    )
    .await
    .unwrap();
    let replacement = host
        .share_remote_desktop(
            viewer.device_id(),
            Access::ViewAndMouse,
            Arc::new(Fixture {
                mouse: true,
                allowed: AtomicBool::new(true),
            }),
        )
        .unwrap();
    drop(view_offer);
    tokio::time::timeout(
        Duration::from_secs(10),
        viewer_run(&viewer, host.device_id(), &mut vnet),
    )
    .await
    .unwrap();
    drop(replacement);
    viewer.shutdown().await;
    host.shutdown().await;
    relay.abort();
}

struct WaitingHost;
impl Handler for WaitingHost {
    fn console_ready(&self) -> bool {
        true
    }
    fn serve<'s>(
        &'s self,
        _: DeviceId,
        session: &'s mut Session,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 's>> {
        Box::pin(async move {
            session.send_control(b"ready").await.unwrap();
            pending().await
        })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn replacing_a_live_offer_ends_its_stream_and_stale_handles_cannot_stop_the_new_offer() {
    let bind = "127.0.0.1".parse().unwrap();
    let server = relay(bind).await;
    let relay_address = server.udp_addr().unwrap();
    let relay_task = tokio::spawn(server.serve());
    let host_key: SharedSigner = Arc::new(SoftSigner::generate());
    let viewer_key: SharedSigner = Arc::new(SoftSigner::generate());
    let entries = signed_roster(host_key.as_ref(), viewer_key.public_key());
    let host = node(host_key, &entries, bind, RelayConfig::Udp(relay_address)).await;
    let viewer = node(viewer_key, &entries, bind, RelayConfig::Udp(relay_address)).await;
    let backend = Arc::new(WaitingHost);
    assert!(
        host.share_remote_desktop(
            SoftSigner::generate().public_key().device_id(),
            Access::View,
            backend.clone()
        )
        .is_err()
    );
    let first = host
        .share_remote_desktop(viewer.device_id(), Access::ViewAndMouse, backend.clone())
        .unwrap();
    let mut old = viewer.open_remote_desktop(host.device_id()).await.unwrap();
    assert_eq!(old.recv_control().await.unwrap().unwrap(), b"ready");
    assert!(
        old.with_mouse(|| panic!("viewer cannot apply input"))
            .is_err()
    );
    let second = host
        .share_remote_desktop(viewer.device_id(), Access::View, backend)
        .unwrap();
    drop(first);
    assert!(
        tokio::time::timeout(Duration::from_secs(2), old.recv_media())
            .await
            .unwrap()
            .is_err()
    );
    drop(old);
    let mut current = viewer.open_remote_desktop(host.device_id()).await.unwrap();
    assert_eq!(current.recv_control().await.unwrap().unwrap(), b"ready");
    drop(second);
    assert!(
        tokio::time::timeout(Duration::from_secs(2), current.recv_media())
            .await
            .unwrap()
            .is_err()
    );
    drop(current);
    assert!(matches!(
        viewer.open_remote_desktop(host.device_id()).await,
        Err(Error::Unauthorized(_))
    ));
    viewer.shutdown().await;
    host.shutdown().await;
    relay_task.abort();
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
        let _offer = host
            .share_remote_desktop(
                viewer.device_id(),
                Access::View,
                Arc::new(Fixture {
                    mouse: false,
                    allowed: AtomicBool::new(true),
                }),
            )
            .unwrap();
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
        tokio::time::timeout(
            Duration::from_secs(10),
            viewer_run(&viewer, host, &mut viewer.take_datagrams().unwrap()),
        )
        .await
        .unwrap();
        viewer.shutdown().await;
    }
}
