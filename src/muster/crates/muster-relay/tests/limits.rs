//! Production limits of the relay, driven by a raw UDP client: the per-client
//! frame budget, the registration cap, the mailbox key cap, and the push hook
//! that wakes an offline device.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use muster_core::{DeviceId, HwSigner, SoftSigner};
use muster_net::relay_proto::{Frame, auth_signature};
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

    /// HELLO → CHALLENGE → AUTH, returning the relay's answer to the AUTH.
    async fn register(&self) -> Frame {
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
        self.recv(Duration::from_secs(2))
            .await
            .expect("auth answer")
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

#[tokio::test]
async fn client_frame_budget_is_enforced() {
    let relay = start(RelayOptions {
        client_frames_per_sec: 10,
        ..RelayOptions::default()
    })
    .await;
    let a = Client::new(relay).await;
    assert!(matches!(a.register().await, Frame::Reflect { .. }));

    // The AUTH itself is not charged; a fresh bucket holds twice the rate.
    for _ in 0..100 {
        a.send(Frame::Fwd {
            dst: a.id,
            payload: vec![0x20, 1, 2, 3],
        })
        .await;
    }
    let mut delivered = 0;
    while let Some(frame) = a.recv(Duration::from_millis(300)).await {
        if matches!(frame, Frame::Deliver { .. }) {
            delivered += 1;
        }
    }
    assert!(
        delivered >= 15,
        "burst of 20 should mostly pass: {delivered}"
    );
    assert!(delivered <= 21, "rate limit did not bite: {delivered}");
}

#[tokio::test]
async fn registration_cap_refuses_the_next_client() {
    let relay = start(RelayOptions {
        max_clients: 1,
        ..RelayOptions::default()
    })
    .await;
    let a = Client::new(relay).await;
    assert!(matches!(a.register().await, Frame::Reflect { .. }));
    // Re-registering the same device is not a new client.
    assert!(matches!(a.register().await, Frame::Reflect { .. }));

    let b = Client::new(relay).await;
    match b.register().await {
        Frame::Error { text } => assert_eq!(text, "relay full"),
        other => panic!("second client registered: {other:?}"),
    }
}

#[tokio::test]
async fn mailbox_key_cap() {
    let relay = start(RelayOptions {
        mailbox_keys: 2,
        ..RelayOptions::default()
    })
    .await;
    let a = Client::new(relay).await;
    assert!(matches!(a.register().await, Frame::Reflect { .. }));
    for k in 1..=2u8 {
        a.send(Frame::MbPut {
            key: [k; 32],
            item: vec![k],
        })
        .await;
        assert!(matches!(
            a.recv(Duration::from_secs(2)).await,
            Some(Frame::MbAck { .. })
        ));
    }
    // A third key is refused, another item under a held key is not.
    a.send(Frame::MbPut {
        key: [3; 32],
        item: vec![3],
    })
    .await;
    match a.recv(Duration::from_secs(2)).await {
        Some(Frame::Error { text }) => assert_eq!(text, "mailbox full"),
        other => panic!("third key accepted: {other:?}"),
    }
    a.send(Frame::MbPut {
        key: [1; 32],
        item: vec![9],
    })
    .await;
    assert!(matches!(
        a.recv(Duration::from_secs(2)).await,
        Some(Frame::MbAck { .. })
    ));
}

#[tokio::test]
async fn unregistered_sender_cannot_put() {
    let relay = start(RelayOptions::default()).await;
    let a = Client::new(relay).await;
    a.send(Frame::MbPut {
        key: [1; 32],
        item: vec![1],
    })
    .await;
    match a.recv(Duration::from_secs(2)).await {
        Some(Frame::Error { text }) => assert_eq!(text, "not registered"),
        other => panic!("put accepted: {other:?}"),
    }
}

#[tokio::test]
async fn push_hook_wakes_an_offline_device() {
    let dir = std::env::temp_dir().join(format!("muster-relay-push-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("pushes");
    let script = dir.join("push.sh");
    std::fs::write(
        &script,
        format!("#!/bin/sh\necho \"$1 $2 $3\" >> {}\n", out.display()),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let relay = start(RelayOptions {
        idle: Duration::from_secs(1),
        push_command: Some(PathBuf::from(&script)),
        push_interval: Duration::from_secs(60),
        ..RelayOptions::default()
    })
    .await;

    // The phone registers, leaves a token, then goes quiet past `idle`.
    let phone = Client::new(relay).await;
    assert!(matches!(phone.register().await, Frame::Reflect { .. }));
    phone
        .send(Frame::PushReg {
            id: phone.id,
            kind: 1,
            token: vec![0xab, 0xcd],
        })
        .await;
    // A token for someone else's id is refused silently.
    let other = SoftSigner::generate().public_key().device_id();
    phone
        .send(Frame::PushReg {
            id: other,
            kind: 1,
            token: vec![1],
        })
        .await;
    tokio::time::sleep(Duration::from_millis(2500)).await;

    let desktop = Client::new(relay).await;
    assert!(matches!(desktop.register().await, Frame::Reflect { .. }));
    for dst in [phone.id, other] {
        desktop
            .send(Frame::Fwd {
                dst,
                payload: vec![0x20, 1],
            })
            .await;
    }
    // Pushes are rate limited per device: a second FWD runs nothing.
    desktop
        .send(Frame::Fwd {
            dst: phone.id,
            payload: vec![0x20, 2],
        })
        .await;

    let mut lines = Vec::new();
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Ok(text) = std::fs::read_to_string(&out) {
            lines = text.lines().map(str::to_string).collect();
            if !lines.is_empty() {
                break;
            }
        }
    }
    // Devices send their token again with every registration, on each
    // carrier; that must not restart the interval (a waking phone was
    // pushed twice within a second).
    assert!(matches!(phone.register().await, Frame::Reflect { .. }));
    phone
        .send(Frame::PushReg {
            id: phone.id,
            kind: 1,
            token: vec![0xab, 0xcd],
        })
        .await;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert!(matches!(desktop.register().await, Frame::Reflect { .. }));
    desktop
        .send(Frame::Fwd {
            dst: phone.id,
            payload: vec![0x20, 3],
        })
        .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    if let Ok(text) = std::fs::read_to_string(&out) {
        lines = text.lines().map(str::to_string).collect();
    }
    assert_eq!(lines, vec![format!("{} 1 abcd", phone.id.to_hex())]);
    let _ = std::fs::remove_dir_all(&dir);
}
