//! Public TLS identity is separate from signed device registration. Both
//! carriers on one port must coexist without stealing challenges or routes.
use std::{net::SocketAddr, sync::Arc, time::Duration};

use muster_core::{HwSigner, SoftSigner};
use muster_net::{
    relay_proto::{Frame, MAX_FRAME, auth_signature},
    relay_tls,
};
use muster_relay::{RelayOptions, RelayServer};
use tokio::net::{TcpStream, UdpSocket};
use tokio_rustls::{TlsConnector, client::TlsStream};

struct Fixture {
    addr: SocketAddr,
    client: rustls::ClientConfig,
    task: tokio::task::JoinHandle<anyhow::Result<()>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Fixture {
    async fn new() -> Self {
        let certified = rcgen::generate_simple_self_signed(vec!["relay.test".into()]).unwrap();
        let mut server = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![certified.cert.der().clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der()).into(),
        )
        .unwrap();
        server.alpn_protocols = vec![relay_tls::RELAY_ALPN.to_vec()];
        let mut roots = rustls::RootCertStore::empty();
        roots.add(certified.cert.der().clone()).unwrap();
        let client = relay_tls::client_config_with_roots(roots).unwrap();
        let server = RelayServer::bind_secure(
            "127.0.0.1:0".parse().unwrap(),
            server,
            RelayOptions::default(),
        )
        .await
        .unwrap();
        let addr = server.udp_addr().unwrap();
        assert_eq!(server.tcp_addr(), Some(addr));
        Self {
            addr,
            client,
            task: tokio::spawn(server.serve()),
        }
    }
    async fn tcp(&self) -> Carrier {
        let stream = TcpStream::connect(self.addr).await.unwrap();
        let stream = TlsConnector::from(Arc::new(self.client.clone()))
            .connect("relay.test".try_into().unwrap(), stream)
            .await
            .unwrap();
        Carrier::Tcp(Box::new(stream))
    }
    async fn quic(&self) -> Carrier {
        self.quic_with(relay_tls::Liveness::Fast).await
    }
    async fn quic_with(&self, liveness: relay_tls::Liveness) -> Carrier {
        let mut endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        endpoint.set_default_client_config(
            relay_tls::quic_client_config(&self.client, liveness).unwrap(),
        );
        let connection = endpoint
            .connect(self.addr, "relay.test")
            .unwrap()
            .await
            .unwrap();
        Carrier::Quic {
            connection,
            _endpoint: endpoint,
        }
    }
}

enum Carrier {
    Tcp(Box<TlsStream<TcpStream>>),
    Quic {
        connection: quinn::Connection,
        _endpoint: quinn::Endpoint,
    },
}
impl Carrier {
    async fn send(&mut self, frame: Frame) {
        let frame = frame.encode();
        match self {
            Self::Tcp(stream) => relay_tls::write_frame(stream, &frame).await.unwrap(),
            Self::Quic { connection, .. } => connection.send_datagram(frame.into()).unwrap(),
        }
    }
    async fn recv(&mut self) -> Frame {
        let bytes = tokio::time::timeout(Duration::from_secs(2), async {
            match self {
                Self::Tcp(stream) => relay_tls::read_frame(stream).await.unwrap(),
                Self::Quic { connection, .. } => connection.read_datagram().await.unwrap().to_vec(),
            }
        })
        .await
        .expect("relay reply");
        let frame = Frame::decode(&bytes).unwrap();
        if matches!(frame, Frame::ProbeAck { .. }) {
            assert_eq!(bytes.len(), MAX_FRAME);
        }
        frame
    }
    async fn challenge(&mut self, signer: &SoftSigner) -> [u8; 32] {
        self.send(Frame::Hello {
            id: signer.public_key().device_id(),
        })
        .await;
        let Frame::Challenge { nonce } = self.recv().await else {
            panic!("expected challenge");
        };
        nonce
    }
    async fn auth(&mut self, signer: &SoftSigner, nonce: [u8; 32]) {
        self.send(Frame::Auth {
            id: signer.public_key().device_id(),
            key: signer.public_key(),
            nonce,
            signature: auth_signature(signer, &nonce).unwrap(),
        })
        .await;
    }
    async fn register(&mut self, signer: &SoftSigner) {
        let nonce = self.challenge(signer).await;
        self.auth(signer, nonce).await;
        assert!(matches!(self.recv().await, Frame::Reflect { .. }));
    }
}

#[tokio::test]
async fn both_carriers_register_and_standby_keepalive_does_not_steal_data_route() {
    let fixture = Fixture::new().await;
    let signer = SoftSigner::generate();
    let id = signer.public_key().device_id();
    let mut tcp = fixture.tcp().await;
    let mut quic = fixture.quic().await;
    // A second HELLO must not overwrite the first carrier's challenge.
    let tcp_nonce = tcp.challenge(&signer).await;
    let quic_nonce = quic.challenge(&signer).await;
    tcp.auth(&signer, tcp_nonce).await;
    assert!(matches!(tcp.recv().await, Frame::Reflect { .. }));
    quic.auth(&signer, quic_nonce).await;
    assert!(matches!(quic.recv().await, Frame::Reflect { .. }));
    quic.send(Frame::MbList { key: [1; 32] }).await;
    assert!(matches!(quic.recv().await, Frame::MbListing { .. }));
    tcp.send(Frame::Keepalive { id }).await;
    assert!(matches!(tcp.recv().await, Frame::Reflect { .. }));
    tcp.send(Frame::Probe {
        id,
        nonce: u64::MAX,
    })
    .await;
    assert_eq!(tcp.recv().await, Frame::ProbeAck { nonce: u64::MAX });

    quic.send(Frame::Probe { id, nonce: 0x1020 }).await;
    assert_eq!(quic.recv().await, Frame::ProbeAck { nonce: 0x1020 });
    // A standby reconnect/late registration also must not steal preference.
    let mut reconnect = fixture.tcp().await;
    reconnect.register(&signer).await;
    let other = SoftSigner::generate();
    let mut sender = fixture.tcp().await;
    sender.register(&other).await;
    let payload = vec![42; MAX_FRAME - 33];
    sender
        .send(Frame::Fwd {
            dst: id,
            payload: payload.clone(),
        })
        .await;
    assert_eq!(
        quic.recv().await,
        Frame::Deliver {
            src: other.public_key().device_id(),
            payload
        }
    );
    // Receive-only path migration requires an authenticated explicit Activate.
    let mut stranger = fixture.tcp().await;
    stranger.send(Frame::Activate { id }).await;
    sender
        .send(Frame::Fwd {
            dst: id,
            payload: vec![8],
        })
        .await;
    assert!(matches!(quic.recv().await, Frame::Deliver { payload, .. } if payload == vec![8]));
    reconnect.send(Frame::Activate { id }).await;
    assert!(matches!(reconnect.recv().await, Frame::Reflect { .. }));
    sender
        .send(Frame::Fwd {
            dst: id,
            payload: vec![9],
        })
        .await;
    assert!(matches!(reconnect.recv().await, Frame::Deliver { payload, .. } if payload == vec![9]));
    // Removing one connection must preserve the sibling registration.
    drop(tcp);
    quic.send(Frame::Keepalive { id }).await;
    assert!(matches!(quic.recv().await, Frame::Reflect { .. }));
}

/// A phone's QUIC carrier keeps sparse keepalives: it stays connected and
/// registered across an idle spell longer than a desktop's 3 s timeout.
#[tokio::test]
async fn a_sparse_quic_carrier_survives_idle_spells() {
    let fixture = Fixture::new().await;
    let signer = SoftSigner::generate();
    let id = signer.public_key().device_id();
    let mut quic = fixture.quic_with(relay_tls::Liveness::Sparse).await;
    quic.register(&signer).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    quic.send(Frame::Probe { id, nonce: 7 }).await;
    assert_eq!(quic.recv().await, Frame::ProbeAck { nonce: 7 });
}

#[tokio::test]
async fn signed_challenge_cannot_be_replayed_on_another_carrier() {
    let fixture = Fixture::new().await;
    let signer = SoftSigner::generate();
    let mut tcp = fixture.tcp().await;
    let mut quic = fixture.quic().await;
    let nonce = tcp.challenge(&signer).await;
    quic.auth(&signer, nonce).await;
    assert!(matches!(quic.recv().await, Frame::Error { .. }));
    tcp.auth(&signer, nonce).await;
    assert!(matches!(tcp.recv().await, Frame::Reflect { .. }));
}

#[tokio::test]
async fn domain_and_trust_chain_are_checked_on_both_transports() {
    let fixture = Fixture::new().await;
    for (config, name) in [
        (fixture.client.clone(), "wrong.test"),
        (relay_tls::client_config().unwrap(), "relay.test"),
    ] {
        let stream = TcpStream::connect(fixture.addr).await.unwrap();
        let tcp = TlsConnector::from(Arc::new(config.clone()))
            .connect(name.try_into().unwrap(), stream)
            .await;
        assert!(tcp.is_err(), "TCP accepted invalid public identity");
        let mut endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        endpoint.set_default_client_config(
            relay_tls::quic_client_config(&config, relay_tls::Liveness::Fast).unwrap(),
        );
        let quic = endpoint.connect(fixture.addr, name).unwrap().await;
        assert!(quic.is_err(), "QUIC accepted invalid public identity");
    }
}

#[tokio::test]
async fn secure_udp_does_not_accept_plain_registration_frames() {
    let fixture = Fixture::new().await;
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let frame = Frame::Hello {
        id: SoftSigner::generate().public_key().device_id(),
    }
    .encode();
    socket.send_to(&frame, fixture.addr).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), socket.recv(&mut [0; 2048]))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn tcp_frame_bounds_are_checked_before_allocating_or_writing() {
    use tokio::io::AsyncWriteExt;
    for size in [0, MAX_FRAME as u32 + 1, u32::MAX] {
        let (mut writer, mut reader) = tokio::io::duplex(16);
        writer.write_u32(size).await.unwrap();
        assert_eq!(
            relay_tls::read_frame(&mut reader).await.unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
    }
    for frame in [vec![], vec![0; MAX_FRAME + 1]] {
        assert_eq!(
            relay_tls::write_frame(&mut tokio::io::sink(), &frame)
                .await
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::InvalidInput
        );
    }
}
