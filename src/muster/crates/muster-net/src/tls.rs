//! TLS for the link (ARCHITECTURE §3.5): the certificate is a self-signed
//! container for the device key, signed by the hardware through rustls'
//! `SigningKey`; the verifier checks membership (or an explicit pin for a
//! pairing joiner) and the handshake signature, nothing else.

use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use muster_core::identity::{DeviceId, PublicKey, SharedSigner};
use muster_core::roster::Roster;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::sign::{CertifiedKey, Signer, SigningKey};
use rustls::{DigitallySignedStruct, DistinguishedName, SignatureAlgorithm, SignatureScheme};

use crate::Error;

pub const ALPN: &[u8] = b"muster/1";
pub const SERVER_NAME: &str = "muster";

pub type SharedRoster = Arc<RwLock<Roster>>;

#[derive(Debug)]
struct HwSigningKey(SharedSigner);

impl SigningKey for HwSigningKey {
    fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
        offered
            .contains(&SignatureScheme::ECDSA_NISTP256_SHA256)
            .then(|| Box::new(HwTlsSigner(self.0.clone())) as Box<dyn Signer>)
    }
    fn algorithm(&self) -> SignatureAlgorithm {
        SignatureAlgorithm::ECDSA
    }
}

#[derive(Debug)]
struct HwTlsSigner(SharedSigner);

impl Signer for HwTlsSigner {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, rustls::Error> {
        self.0
            .sign(message)
            .map_err(|e| rustls::Error::General(format!("hardware sign: {e}")))
    }
    fn scheme(&self) -> SignatureScheme {
        SignatureScheme::ECDSA_NISTP256_SHA256
    }
}

struct RcgenRemote {
    signer: SharedSigner,
    public_raw: Vec<u8>,
}

impl rcgen::RemoteKeyPair for RcgenRemote {
    fn public_key(&self) -> &[u8] {
        &self.public_raw
    }
    fn sign(&self, msg: &[u8]) -> Result<Vec<u8>, rcgen::Error> {
        self.signer
            .sign(msg)
            .map_err(|_| rcgen::Error::RemoteKeyError)
    }
    fn algorithm(&self) -> &'static rcgen::SignatureAlgorithm {
        &rcgen::PKCS_ECDSA_P256_SHA256
    }
}

/// This device as rustls sees it: one self-signed certificate around the
/// hardware key. Made once per process start; there is no lifecycle.
#[derive(Debug)]
pub struct TlsIdentity {
    pub certified: Arc<CertifiedKey>,
    pub device_id: DeviceId,
    pub public_key: PublicKey,
}

impl TlsIdentity {
    pub fn new(signer: SharedSigner) -> Result<Self, Error> {
        let public_key = signer.public_key();
        let key_pair = rcgen::KeyPair::from_remote(Box::new(RcgenRemote {
            signer: signer.clone(),
            public_raw: public_key.raw().to_vec(),
        }))
        .map_err(|e| Error::Tls(format!("remote key: {e}")))?;
        let mut params = rcgen::CertificateParams::new(vec![SERVER_NAME.to_string()])
            .map_err(|e| Error::Tls(e.to_string()))?;
        params.distinguished_name = rcgen::DistinguishedName::new();
        let certificate = params
            .self_signed(&key_pair)
            .map_err(|e| Error::Tls(format!("self-sign: {e}")))?;
        let der = certificate.der().clone();
        Ok(Self {
            certified: Arc::new(CertifiedKey::new(vec![der], Arc::new(HwSigningKey(signer)))),
            device_id: public_key.device_id(),
            public_key,
        })
    }
}

#[derive(Debug)]
struct FixedCert(Arc<CertifiedKey>);

impl rustls::server::ResolvesServerCert for FixedCert {
    fn resolve(&self, _hello: rustls::server::ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(self.0.clone())
    }
}

impl rustls::client::ResolvesClientCert for FixedCert {
    fn resolve(&self, _hints: &[&[u8]], _schemes: &[SignatureScheme]) -> Option<Arc<CertifiedKey>> {
        Some(self.0.clone())
    }
    fn has_certs(&self) -> bool {
        true
    }
}

/// Extracts the device id from a peer's leaf certificate.
pub fn device_id_of(cert: &CertificateDer<'_>) -> Result<(DeviceId, PublicKey), Error> {
    let (_, parsed) = x509_parser::parse_x509_certificate(cert.as_ref())
        .map_err(|e| Error::Tls(format!("peer certificate: {e}")))?;
    let spki = parsed.public_key();
    let raw = spki.subject_public_key.as_ref();
    let key = PublicKey::from_raw(raw).map_err(|e| Error::Tls(format!("peer key: {e}")))?;
    Ok((key.device_id(), key))
}

/// Who may connect / be connected to.
#[derive(Clone)]
pub struct Trust {
    pub roster: SharedRoster,
    /// Accept a non-member as a client (the pairing joiner). Per-stream
    /// authorization then confines it to the pair service.
    pub accept_unknown_clients: Arc<AtomicBool>,
}

impl Trust {
    pub fn new(roster: SharedRoster) -> Self {
        Self {
            roster,
            accept_unknown_clients: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_member(&self, id: &DeviceId) -> bool {
        self.roster
            .read()
            .map(|r| r.state().active(id).is_some())
            .unwrap_or(false)
    }
}

impl Debug for Trust {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Trust")
    }
}

#[derive(Debug)]
struct Verifier {
    trust: Trust,
    /// Explicit pin (the joiner pins the admin from the invite).
    pin: Option<DeviceId>,
    server_side: bool,
    provider: Arc<CryptoProvider>,
}

impl Verifier {
    fn check(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
    ) -> Result<(), rustls::Error> {
        if !intermediates.is_empty() {
            return Err(rustls::Error::General(
                "peer sent a chain; a key container has none".into(),
            ));
        }
        let (id, _) =
            device_id_of(end_entity).map_err(|e| rustls::Error::General(e.to_string()))?;
        if let Some(pin) = self.pin {
            return if id == pin {
                Ok(())
            } else {
                Err(rustls::Error::General(format!(
                    "peer {} is not the pinned {}",
                    id.short(),
                    pin.short()
                )))
            };
        }
        if self.trust.is_member(&id) {
            return Ok(());
        }
        if self.server_side && self.trust.accept_unknown_clients.load(Ordering::Relaxed) {
            return Ok(());
        }
        Err(rustls::Error::General(format!(
            "peer {} is not a member",
            id.short()
        )))
    }

    fn verify13(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }
}

impl ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        self.check(end_entity, intermediates)?;
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::PeerIncompatible(
            rustls::PeerIncompatible::Tls12NotOffered,
        ))
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.verify13(message, cert, dss)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ECDSA_NISTP256_SHA256]
    }
}

impl ClientCertVerifier for Verifier {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }
    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        self.check(end_entity, intermediates)?;
        Ok(ClientCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::PeerIncompatible(
            rustls::PeerIncompatible::Tls12NotOffered,
        ))
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.verify13(message, cert, dss)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ECDSA_NISTP256_SHA256]
    }
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// QUIC's smallest path MTU (RFC 9000 §14). MTU discovery falls back to it
/// when it detects a black hole, and a link never runs below it.
pub const QUIC_MIN_MTU: u16 = 1200;

/// The largest vnet IP packet one DATAGRAM always carries: QUIC's minimum
/// path MTU less the worst-case 1-RTT overhead. That is the flags byte, a
/// 20-byte connection ID, a 4-byte packet number, the 16-byte AEAD tag and
/// a DATAGRAM frame's type and length (9). A larger TUN MTU loses
/// full-sized packets without notice whenever discovery falls back.
pub const VNET_MTU: u16 = QUIC_MIN_MTU - (1 + 20 + 4 + 16 + 9);

fn transport_config() -> Arc<quinn::TransportConfig> {
    Arc::new(transport())
}

/// A member link ends after this long without a packet. Three seconds (the
/// old value) closed a phone's relayed links whenever its relay carrier was
/// re-registered or a radio paused: each such gap became a disconnection, a
/// reconnect, and a broken stream. Presence (not this timeout) decides what
/// members are shown as.
pub const LINK_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
/// Each side's keepalive on a quiet member link: three per idle timeout, so
/// two may be lost. Once a second (the old value) woke a phone's radio for
/// nothing. A dead link is found within the idle timeout plus one keepalive
/// (the first keepalive after the last packet restarts the idle timer).
pub const LINK_KEEPALIVE: std::time::Duration = std::time::Duration::from_secs(5);

fn transport() -> quinn::TransportConfig {
    let mut transport = quinn::TransportConfig::default();
    transport.min_mtu(QUIC_MIN_MTU);
    transport.keep_alive_interval(Some(LINK_KEEPALIVE));
    transport.max_idle_timeout(Some(LINK_IDLE_TIMEOUT.try_into().expect("fits")));
    transport.datagram_receive_buffer_size(Some(4 * 1024 * 1024));
    transport.datagram_send_buffer_size(4 * 1024 * 1024);
    // 1280-byte vnet packets need room before path MTU discovery finishes.
    // A larger first MTU lets bulk vnet traffic use fewer, fuller packets.
    transport.initial_mtu(1350);
    // MagicSock hides direct/relay migration from Quinn. A direct route
    // must not grow packets beyond what its relay fallback can encapsulate.
    let mut discovery = quinn::MtuDiscoveryConfig::default();
    discovery.upper_bound(crate::relay_proto::MAX_QUIC_DATAGRAM);
    transport.mtu_discovery_config(Some(discovery));
    transport
}

pub fn server_config(identity: &TlsIdentity, trust: Trust) -> Result<quinn::ServerConfig, Error> {
    let provider = provider();
    let verifier = Verifier {
        trust,
        pin: None,
        server_side: true,
        provider: provider.clone(),
    };
    let mut tls = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| Error::Tls(e.to_string()))?
        .with_client_cert_verifier(Arc::new(verifier))
        .with_cert_resolver(Arc::new(FixedCert(identity.certified.clone())));
    tls.alpn_protocols = vec![ALPN.to_vec()];
    let crypto = quinn::crypto::rustls::QuicServerConfig::try_from(tls)
        .map_err(|e| Error::Tls(e.to_string()))?;
    let mut config = quinn::ServerConfig::with_crypto(Arc::new(crypto));
    config.transport_config(transport_config());
    Ok(config)
}

/// Client config; `pin` overrides membership (the pairing joiner is not a
/// member yet and pins the admin from the invite).
pub fn client_config(
    identity: &TlsIdentity,
    trust: Trust,
    pin: Option<DeviceId>,
) -> Result<quinn::ClientConfig, Error> {
    let provider = provider();
    let verifier = Verifier {
        trust,
        pin,
        server_side: false,
        provider: provider.clone(),
    };
    let mut tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| Error::Tls(e.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_client_cert_resolver(Arc::new(FixedCert(identity.certified.clone())));
    tls.alpn_protocols = vec![ALPN.to_vec()];
    tls.enable_early_data = false;
    let crypto = quinn::crypto::rustls::QuicClientConfig::try_from(tls)
        .map_err(|e| Error::Tls(e.to_string()))?;
    let mut config = quinn::ClientConfig::new(Arc::new(crypto));
    config.transport_config(transport_config());
    Ok(config)
}

/// The peer's device id, from the certificate it proved possession of.
pub fn peer_device_id(connection: &quinn::Connection) -> Result<DeviceId, Error> {
    let identity = connection
        .peer_identity()
        .ok_or_else(|| Error::Tls("peer sent no certificate".into()))?;
    let certs = identity
        .downcast::<Vec<CertificateDer<'static>>>()
        .map_err(|_| Error::Tls("unexpected peer identity type".into()))?;
    let leaf = certs
        .first()
        .ok_or_else(|| Error::Tls("empty certificate list".into()))?;
    Ok(device_id_of(leaf)?.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use muster_core::SoftSigner;

    /// Every vnet packet must fit a DATAGRAM even after MTU discovery falls
    /// back to QUIC's minimum, on a link built from the real configuration.
    #[tokio::test]
    async fn vnet_packets_fit_a_datagram_at_quic_minimum_mtu() {
        let at_minimum = || {
            let mut transport = transport();
            transport.initial_mtu(QUIC_MIN_MTU);
            transport.mtu_discovery_config(None);
            Arc::new(transport)
        };
        let identity = |signer: SoftSigner| TlsIdentity::new(Arc::new(signer)).unwrap();
        let server_identity = identity(SoftSigner::generate());
        let client_identity = identity(SoftSigner::generate());
        let open = Trust::new(Arc::new(RwLock::new(Roster::new())));
        open.accept_unknown_clients.store(true, Ordering::Relaxed);
        let mut server_config = server_config(&server_identity, open.clone()).unwrap();
        server_config.transport_config(at_minimum());
        let server =
            quinn::Endpoint::server(server_config, "127.0.0.1:0".parse().unwrap()).unwrap();
        let mut client_config =
            client_config(&client_identity, open, Some(server_identity.device_id)).unwrap();
        client_config.transport_config(at_minimum());
        let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = server.local_addr().unwrap();
        let accept = tokio::spawn(async move { server.accept().await.unwrap().await.unwrap() });
        let connection = client
            .connect_with(client_config, address, SERVER_NAME)
            .unwrap()
            .await
            .unwrap();
        let accepted = accept.await.unwrap();
        for side in [&connection, &accepted] {
            let capacity = side.max_datagram_size().expect("datagrams enabled");
            assert!(
                capacity >= usize::from(VNET_MTU),
                "{capacity} bytes per DATAGRAM at MTU {QUIC_MIN_MTU} < vnet MTU {VNET_MTU}"
            );
            // The former 1,280-byte TUN MTU did not fit: the 2026-10-03 loss.
            assert!(capacity < 1280);
        }
    }
}
