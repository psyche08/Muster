//! Presence frames, driven by raw UDP clients: ABSENT only to carriers that
//! asked for it, only after the absence grace (the relay start counts as a
//! departure) and rate limited; WATCH answered with PRESENT at once or when
//! the device registers.

use std::net::SocketAddr;
use std::time::Duration;

use muster_core::{DeviceId, HwSigner, SoftSigner};
use muster_net::relay_proto::{FEATURES_PRESENCE, Frame, auth_signature};
use muster_relay::{RelayOptions, RelayServer};
use tokio::net::UdpSocket;

struct Client {
    socket: UdpSocket,
    relay: SocketAddr,
    signer: SoftSigner,
    id: DeviceId,
}

impl Client {
    async fn new(relay: SocketAddr) -> Client {
        let signer = SoftSigner::generate();
        let id = signer.public_key().device_id();
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        Client {
            socket,
            relay,
            signer,
            id,
        }
    }

    async fn send(&self, frame: Frame) {
        self.socket
            .send_to(&frame.encode(), self.relay)
            .await
            .unwrap();
    }

    async fn recv(&self, wait: Duration) -> Option<Frame> {
        let mut buf = vec![0u8; 2048];
        let (n, _) = tokio::time::timeout(wait, self.socket.recv_from(&mut buf))
            .await
            .ok()?
            .ok()?;
        Frame::decode(&buf[..n]).ok()
    }

    async fn register(&self) {
        self.send(Frame::Hello { id: self.id }).await;
        let Some(Frame::Challenge { nonce }) = self.recv(Duration::from_secs(2)).await else {
            panic!("no challenge");
        };
        self.send(Frame::Auth {
            id: self.id,
            key: self.signer.public_key(),
            nonce,
            signature: auth_signature(&self.signer, &nonce).unwrap(),
        })
        .await;
        assert!(matches!(
            self.recv(Duration::from_secs(2)).await,
            Some(Frame::Reflect { .. })
        ));
    }

    async fn presence(&self) {
        self.send(Frame::Features {
            bits: FEATURES_PRESENCE,
        })
        .await;
    }

    async fn fwd(&self, dst: DeviceId) {
        self.send(Frame::Fwd {
            dst,
            payload: vec![0x20, 1, 2, 3],
        })
        .await;
    }

    /// The next presence frame within `wait`, skipping anything else.
    async fn next_presence(&self, wait: Duration) -> Option<Frame> {
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            if let frame @ (Frame::Absent { .. } | Frame::Present { .. }) = self.recv(left).await? {
                return Some(frame);
            }
        }
    }
}

async fn start(options: RelayOptions) -> SocketAddr {
    let server = RelayServer::bind("127.0.0.1:0".parse().unwrap(), options)
        .await
        .unwrap();
    let addr = server.udp_addr().unwrap();
    tokio::spawn(server.serve());
    addr
}

const SHORT: Duration = Duration::from_millis(250);

#[tokio::test]
async fn absent_goes_only_to_carriers_that_asked_and_only_past_the_grace() {
    let relay = start(RelayOptions {
        absent_grace: Duration::from_millis(400),
        ..RelayOptions::default()
    })
    .await;
    let a = Client::new(relay).await;
    let nobody = SoftSigner::generate().public_key().device_id();
    a.register().await;
    a.presence().await;
    // The relay just started: everyone may still be re-registering.
    a.fwd(nobody).await;
    assert!(a.next_presence(SHORT).await.is_none());
    tokio::time::sleep(Duration::from_millis(400)).await;
    a.fwd(nobody).await;
    assert_eq!(
        a.next_presence(SHORT).await,
        Some(Frame::Absent { id: nobody })
    );
    // At most one per destination every two seconds.
    a.fwd(nobody).await;
    assert!(a.next_presence(SHORT).await.is_none());
    // A carrier that never asked (an older client) gets nothing it cannot read.
    let old = Client::new(relay).await;
    old.register().await;
    old.fwd(nobody).await;
    assert!(old.next_presence(SHORT).await.is_none());
}

#[tokio::test]
async fn a_device_back_within_the_grace_is_never_reported_absent() {
    let relay = start(RelayOptions {
        absent_grace: Duration::from_millis(600),
        idle: Duration::from_millis(200),
        ..RelayOptions::default()
    })
    .await;
    let a = Client::new(relay).await;
    let b = Client::new(relay).await;
    tokio::time::sleep(Duration::from_millis(650)).await;
    a.register().await;
    a.presence().await;
    b.register().await;
    // B goes quiet: its carrier is stale after `idle`, but it left only now.
    // (A idles out too with this short `idle`: it registers again to send.)
    tokio::time::sleep(Duration::from_millis(300)).await;
    a.register().await;
    a.presence().await;
    a.fwd(b.id).await;
    assert!(a.next_presence(SHORT).await.is_none());
    // Away past the grace: absent.
    tokio::time::sleep(Duration::from_millis(500)).await;
    a.register().await;
    a.presence().await;
    a.fwd(b.id).await;
    assert_eq!(
        a.next_presence(SHORT).await,
        Some(Frame::Absent { id: b.id })
    );
}

#[tokio::test]
async fn watch_answers_present_when_the_device_registers_or_at_once() {
    let relay = start(RelayOptions::default()).await;
    let a = Client::new(relay).await;
    let b = Client::new(relay).await;
    a.register().await;
    a.presence().await;
    a.send(Frame::Watch { id: b.id }).await;
    assert!(a.next_presence(SHORT).await.is_none());
    b.register().await;
    assert_eq!(
        a.next_presence(Duration::from_secs(1)).await,
        Some(Frame::Present { id: b.id })
    );
    // One answer per WATCH; a WATCH for a registered device answers now.
    b.register().await;
    assert!(a.next_presence(SHORT).await.is_none());
    a.send(Frame::Watch { id: b.id }).await;
    assert_eq!(
        a.next_presence(Duration::from_secs(1)).await,
        Some(Frame::Present { id: b.id })
    );
    // Without FEATURES a WATCH is ignored.
    let old = Client::new(relay).await;
    let c = Client::new(relay).await;
    old.register().await;
    old.send(Frame::Watch { id: c.id }).await;
    c.register().await;
    assert!(old.next_presence(SHORT).await.is_none());
}
