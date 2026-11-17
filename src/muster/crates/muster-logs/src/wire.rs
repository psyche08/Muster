//! One upload on the wire: a signed manifest and the chunks it describes.
//!
//! ```text
//! "MLOG" | version u8 | manifest length u32 BE | manifest JSON
//!        | signature length u16 BE | DER signature | payload
//! ```
//!
//! The signature is the device key's, under [`DOMAIN`], over the manifest
//! bytes exactly as sent; the manifest names the key, the payload's SHA-256
//! and each chunk's compressed length, so the payload is covered too. The
//! device id is the hash of that key, as everywhere else: a collector learns
//! that the uploader holds the key, not that it is a member of the network
//! it names (ADR 0228).

use muster_core::identity::{sign_domain, verify_domain};
use muster_core::{DeviceId, HwSigner, PublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The signature's domain tag: an upload signature is never anything else.
pub const DOMAIN: &str = "MUSTER-LOGS-UPLOAD-V1";
pub const MAGIC: &[u8; 4] = b"MLOG";
pub const VERSION: u8 = 1;
pub const CONTENT_TYPE: &str = "application/x-muster-logs";
/// Where the collector takes uploads.
pub const PATH: &str = "/v1/logs";
/// The production collector, beside the relay on muster.linsheng.dev.
pub const DEFAULT_URL: &str = "https://muster.linsheng.dev:9444/v1/logs";

/// The longest manifest accepted.
pub const MAX_MANIFEST: usize = 64 * 1024;
/// Chunks in one upload at most: one per log file, with room to spare.
pub const MAX_CHUNKS: usize = 16;
/// Redacted bytes in one chunk at most.
pub const MAX_CHUNK_RAW: u64 = 4 << 20;
/// The longest upload body accepted.
pub const MAX_BODY: usize = 16 << 20;
/// Longest stream name; streams are the log files' base names.
const MAX_STREAM: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub v: u32,
    /// The device's P-256 public key: hex of the 65-byte uncompressed point.
    pub key: String,
    /// The network this device belongs to (hex of the genesis hash), the
    /// collector's grouping; `None` before the device joins one.
    pub network: Option<String>,
    /// desktop, server or phone.
    pub kind: String,
    pub version: String,
    pub commit: String,
    pub os: String,
    /// Unix seconds when the upload was sealed.
    pub created_at: u64,
    /// 16 random bytes, hex: two uploads never sign the same bytes.
    pub nonce: String,
    pub chunks: Vec<Chunk>,
    /// Hex SHA-256 of the payload: every chunk's gzip member, in order.
    pub payload_sha256: String,
}

/// What one log file gained: bytes `start..end` of the file (offsets in the
/// file as written, before redaction), redacted to `raw_len` bytes and
/// gzipped to `len` bytes of the payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chunk {
    /// The log's base name: `musterd`, `app`, `l4forwarder`.
    pub stream: String,
    /// Which file of that name (its inode): a rotated or recreated file is
    /// a new generation and starts again at 0.
    pub generation: u64,
    pub start: u64,
    pub end: u64,
    /// Bytes before `start` the device never sent: the backlog past the
    /// per-upload limit, or history before the first upload.
    #[serde(default)]
    pub skipped: u64,
    pub len: u64,
    pub raw_len: u64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WireError {
    #[error("malformed upload: {0}")]
    Malformed(String),
    #[error("bad signature: {0}")]
    Signature(String),
}

fn malformed(text: impl Into<String>) -> WireError {
    WireError::Malformed(text.into())
}

/// An upload whose signature and payload hash have been checked.
#[derive(Debug)]
pub struct Opened<'a> {
    pub manifest: Manifest,
    pub device: DeviceId,
    /// Each chunk with its gzip member.
    pub chunks: Vec<(Chunk, &'a [u8])>,
}

pub fn payload_digest(payload: &[u8]) -> String {
    hex::encode(Sha256::digest(payload))
}

/// Signs `manifest` (whose `key` and `payload_sha256` must already describe
/// `signer` and `payload`) and frames it with the payload.
pub fn seal(
    manifest: &Manifest,
    signer: &dyn HwSigner,
    payload: &[u8],
) -> Result<Vec<u8>, muster_core::Error> {
    let json =
        serde_json::to_vec(manifest).map_err(|e| muster_core::Error::Malformed(e.to_string()))?;
    let signature = sign_domain(signer, DOMAIN, &json)?;
    let mut body = Vec::with_capacity(4 + 1 + 4 + json.len() + 2 + signature.len() + payload.len());
    body.extend_from_slice(MAGIC);
    body.push(VERSION);
    body.extend_from_slice(&(json.len() as u32).to_be_bytes());
    body.extend_from_slice(&json);
    body.extend_from_slice(&(signature.len() as u16).to_be_bytes());
    body.extend_from_slice(&signature);
    body.extend_from_slice(payload);
    Ok(body)
}

/// Reads and verifies an upload: framing, signature, payload hash, and the
/// manifest's own limits. Says nothing of time or duplicates; the collector
/// decides those.
pub fn open(body: &[u8]) -> Result<Opened<'_>, WireError> {
    if body.len() > MAX_BODY {
        return Err(malformed("body too large"));
    }
    let rest = body
        .strip_prefix(MAGIC.as_slice())
        .ok_or_else(|| malformed("not a muster log upload"))?;
    let (&version, rest) = rest.split_first().ok_or_else(|| malformed("truncated"))?;
    if version != VERSION {
        return Err(malformed(format!("version {version}")));
    }
    let (len, rest) = split_be::<4>(rest)?;
    let len = u32::from_be_bytes(len) as usize;
    if len > MAX_MANIFEST || len > rest.len() {
        return Err(malformed("manifest length"));
    }
    let (json, rest) = rest.split_at(len);
    let (sig_len, rest) = split_be::<2>(rest)?;
    let sig_len = u16::from_be_bytes(sig_len) as usize;
    if sig_len > rest.len() {
        return Err(malformed("signature length"));
    }
    let (signature, payload) = rest.split_at(sig_len);
    let manifest: Manifest =
        serde_json::from_slice(json).map_err(|e| malformed(format!("manifest: {e}")))?;
    if manifest.v != 1 {
        return Err(malformed(format!("manifest v{}", manifest.v)));
    }
    let key = hex::decode(&manifest.key)
        .ok()
        .and_then(|raw| PublicKey::from_raw(&raw).ok())
        .ok_or_else(|| malformed("key"))?;
    verify_domain(&key, DOMAIN, json, signature)
        .map_err(|e| WireError::Signature(e.to_string()))?;
    if payload_digest(payload) != manifest.payload_sha256 {
        return Err(malformed("payload does not match its digest"));
    }
    if let Some(network) = &manifest.network
        && !is_hex32(network)
    {
        return Err(malformed("network"));
    }
    if manifest.chunks.len() > MAX_CHUNKS {
        return Err(malformed("too many chunks"));
    }
    let mut chunks = Vec::with_capacity(manifest.chunks.len());
    let mut at = 0usize;
    for chunk in &manifest.chunks {
        if !valid_stream(&chunk.stream) {
            return Err(malformed(format!("stream {:?}", chunk.stream)));
        }
        if chunk.start > chunk.end || chunk.raw_len > MAX_CHUNK_RAW || chunk.raw_len == 0 {
            return Err(malformed(format!("chunk bounds of {}", chunk.stream)));
        }
        let len = usize::try_from(chunk.len).map_err(|_| malformed("chunk length"))?;
        let bytes = payload
            .get(
                at..at
                    .checked_add(len)
                    .ok_or_else(|| malformed("chunk length"))?,
            )
            .ok_or_else(|| malformed("chunk past the payload"))?;
        chunks.push((chunk.clone(), bytes));
        at += len;
    }
    if at != payload.len() {
        return Err(malformed("payload longer than its chunks"));
    }
    Ok(Opened {
        device: key.device_id(),
        manifest,
        chunks,
    })
}

fn split_be<const N: usize>(bytes: &[u8]) -> Result<([u8; N], &[u8]), WireError> {
    if bytes.len() < N {
        return Err(malformed("truncated"));
    }
    let (head, rest) = bytes.split_at(N);
    Ok((head.try_into().expect("N bytes"), rest))
}

/// A stream name is a file's base name: short, lowercase, no separators.
pub fn valid_stream(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_STREAM
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

pub fn is_hex32(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use muster_core::SoftSigner;

    fn manifest(signer: &SoftSigner, payload: &[u8], chunks: Vec<Chunk>) -> Manifest {
        Manifest {
            v: 1,
            key: hex::encode(signer.public_key().raw()),
            network: Some("ab".repeat(32)),
            kind: "desktop".into(),
            version: "1.1.46".into(),
            commit: "0000000".into(),
            os: "macos".into(),
            created_at: 1_763_000_000,
            nonce: "00".repeat(16),
            chunks,
            payload_sha256: payload_digest(payload),
        }
    }

    fn chunk(stream: &str, len: usize) -> Chunk {
        Chunk {
            stream: stream.into(),
            generation: 7,
            start: 0,
            end: 10,
            skipped: 0,
            len: len as u64,
            raw_len: 10,
        }
    }

    #[test]
    fn sealed_upload_opens_with_its_chunks_and_device() {
        let signer = SoftSigner::generate();
        let payload = b"aaaabbbbbb";
        let m = manifest(&signer, payload, vec![chunk("app", 4), chunk("musterd", 6)]);
        let body = seal(&m, &signer, payload).unwrap();
        let opened = open(&body).unwrap();
        assert_eq!(opened.device, signer.public_key().device_id());
        assert_eq!(opened.manifest, m);
        assert_eq!(opened.chunks[0].1, b"aaaa");
        assert_eq!(opened.chunks[1].1, b"bbbbbb");
    }

    #[test]
    fn any_changed_byte_is_refused() {
        let signer = SoftSigner::generate();
        let payload = b"aaaa";
        let body = seal(
            &manifest(&signer, payload, vec![chunk("app", 4)]),
            &signer,
            payload,
        )
        .unwrap();
        for i in 0..body.len() {
            let mut changed = body.clone();
            changed[i] ^= 0x01;
            assert!(open(&changed).is_err(), "byte {i} changed and still opened");
        }
        assert!(open(&body[..body.len() - 1]).is_err());
    }

    #[test]
    fn another_key_cannot_sign_for_a_device() {
        let signer = SoftSigner::generate();
        let other = SoftSigner::generate();
        let payload = b"aaaa";
        let m = manifest(&signer, payload, vec![chunk("app", 4)]);
        let body = seal(&m, &other, payload).unwrap();
        assert!(matches!(open(&body), Err(WireError::Signature(_))));
    }

    #[test]
    fn chunk_lengths_must_cover_the_payload_exactly() {
        let signer = SoftSigner::generate();
        let payload = b"aaaa";
        for chunks in [vec![chunk("app", 3)], vec![chunk("app", 5)], vec![]] {
            let body = seal(&manifest(&signer, payload, chunks), &signer, payload).unwrap();
            assert!(open(&body).is_err());
        }
    }

    #[test]
    fn stream_names_are_plain_base_names() {
        for good in ["musterd", "app", "l4forwarder", "a-b_c"] {
            assert!(valid_stream(good), "{good}");
        }
        for bad in ["", "../x", "App", "a/b", "-x", "a.log", &"x".repeat(33)] {
            assert!(!valid_stream(bad), "{bad}");
        }
    }
}
