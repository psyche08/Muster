//! Public relay TLS is ordinary WebPKI, independent of device-pinned PeerLink TLS.
//! Both TCP/TLS and UDP/QUIC use the same domain certificate and ALPN.
use std::{io, path::Path, sync::Arc, time::Duration};

use anyhow::{Context, Result, ensure};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::relay_proto::MAX_FRAME;

pub const RELAY_ALPN: &[u8] = b"muster-relay/1";

pub fn client_config() -> Result<rustls::ClientConfig> {
    let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    client_config_with_roots(roots)
}

/// Explicit roots support private test CAs, never an accept-all verifier.
pub fn client_config_with_roots(roots: rustls::RootCertStore) -> Result<rustls::ClientConfig> {
    let mut config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])?
    .with_root_certificates(roots)
    .with_no_client_auth();
    config.alpn_protocols = vec![RELAY_ALPN.to_vec()];
    Ok(config)
}

/// The full certificate chain and matching private key are read only at startup.
/// Operators restart after atomic renewal; errors never fall back to plaintext.
pub fn server_config(cert: &Path, key: &Path) -> Result<rustls::ServerConfig> {
    let certs = rustls_pemfile::certs(&mut io::BufReader::new(std::fs::File::open(cert)?))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(!certs.is_empty(), "relay certificate chain is empty");
    let key = rustls_pemfile::private_key(&mut io::BufReader::new(std::fs::File::open(key)?))?
        .context("relay private key is missing")?;
    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])?
    .with_no_client_auth()
    .with_single_cert(certs, key)?;
    config.alpn_protocols = vec![RELAY_ALPN.to_vec()];
    Ok(config)
}

/// A relay carrier's QUIC keepalive and idle timeout. A desktop notices a
/// dead carrier within 3 s (D13); a phone keeps one carrier alive with
/// sparse keepalives. The relay accepts both: each connection uses the
/// smaller idle timeout of its two sides, and a desktop keeps its own 1 s
/// keepalive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Liveness {
    Fast,
    Sparse,
}

fn transport_config(liveness: Liveness) -> Arc<quinn::TransportConfig> {
    let (keepalive, idle) = match liveness {
        Liveness::Fast => (1, 3),
        Liveness::Sparse => (10, 30),
    };
    let mut config = quinn::TransportConfig::default();
    config.max_concurrent_bidi_streams(0u8.into());
    config.max_concurrent_uni_streams(0u8.into());
    config.datagram_receive_buffer_size(Some(256 * MAX_FRAME));
    config.datagram_send_buffer_size(256 * MAX_FRAME);
    config.keep_alive_interval(Some(Duration::from_secs(keepalive)));
    config.max_idle_timeout(Some(Duration::from_secs(idle).try_into().expect("fits")));
    // Encapsulated PeerLink packets need more than QUIC's 1200-byte minimum.
    // Paths that cannot carry a frame must use the TCP carrier instead.
    config.initial_mtu(1452);
    config.min_mtu(1280);
    let mut discovery = quinn::MtuDiscoveryConfig::default();
    discovery.upper_bound(1500);
    config.mtu_discovery_config(Some(discovery));
    Arc::new(config)
}

pub fn quic_client_config(
    config: &rustls::ClientConfig,
    liveness: Liveness,
) -> Result<quinn::ClientConfig> {
    let crypto = quinn::crypto::rustls::QuicClientConfig::try_from(config.clone())?;
    let mut result = quinn::ClientConfig::new(Arc::new(crypto));
    result.transport_config(transport_config(liveness));
    Ok(result)
}

pub fn quic_server_config(config: rustls::ServerConfig) -> Result<quinn::ServerConfig> {
    let crypto = quinn::crypto::rustls::QuicServerConfig::try_from(config)?;
    let mut result = quinn::ServerConfig::with_crypto(Arc::new(crypto));
    // Sparse on the relay: a desktop's own 1 s keepalive and 3 s idle
    // timeout still govern its connection; a phone's govern its own.
    result.transport_config(transport_config(Liveness::Sparse));
    Ok(result)
}

/// One TCP frame = u32 network-order length followed by a relay_proto frame.
pub async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<Vec<u8>> {
    let len = reader.read_u32().await? as usize;
    if !(1..=MAX_FRAME).contains(&len) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid relay frame length",
        ));
    }
    let mut frame = vec![0; len];
    reader.read_exact(&mut frame).await?;
    Ok(frame)
}

pub async fn write_frame<W: AsyncWrite + Unpin>(writer: &mut W, frame: &[u8]) -> io::Result<()> {
    if !(1..=MAX_FRAME).contains(&frame.len()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid relay frame length",
        ));
    }
    writer.write_u32(frame.len() as u32).await?;
    writer.write_all(frame).await?;
    writer.flush().await
}
