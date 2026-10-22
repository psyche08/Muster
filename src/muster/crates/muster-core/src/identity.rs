//! Device identity: a P-256 key the hardware holds, identified by the SHA-256
//! of its SubjectPublicKeyInfo. Everything trust-bearing hangs off that hash;
//! certificates are throwaway containers (ARCHITECTURE §3.4).

use std::fmt;
use std::sync::Arc;

use p256::ecdsa::signature::Verifier as _;
use p256::ecdsa::{DerSignature, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::Error;

/// DER prefix of a SubjectPublicKeyInfo for id-ecPublicKey / prime256v1,
/// followed by the 65-byte uncompressed point.
const P256_SPKI_PREFIX: [u8; 26] = [
    0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a,
    0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
];

/// Uncompressed X9.63 P-256 public key.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct PublicKey([u8; 65]);

impl PublicKey {
    pub fn from_raw(raw: &[u8]) -> Result<Self, Error> {
        let bytes: [u8; 65] = raw.try_into().map_err(|_| {
            Error::Identity(format!("public key must be 65 bytes, got {}", raw.len()))
        })?;
        if bytes[0] != 0x04 {
            return Err(Error::Identity(
                "public key is not an uncompressed point".into(),
            ));
        }
        VerifyingKey::from_sec1_bytes(&bytes)
            .map_err(|e| Error::Identity(format!("invalid P-256 point: {e}")))?;
        Ok(Self(bytes))
    }

    pub fn raw(&self) -> &[u8; 65] {
        &self.0
    }

    pub fn spki_der(&self) -> Vec<u8> {
        let mut der = P256_SPKI_PREFIX.to_vec();
        der.extend_from_slice(&self.0);
        der
    }

    pub fn device_id(&self) -> DeviceId {
        DeviceId(Sha256::digest(self.spki_der()).into())
    }

    /// Verifies an ECDSA-P256-SHA256 DER signature over `message` (unhashed).
    pub fn verify(&self, message: &[u8], der_signature: &[u8]) -> Result<(), Error> {
        let key =
            VerifyingKey::from_sec1_bytes(&self.0).map_err(|e| Error::Identity(e.to_string()))?;
        let signature =
            DerSignature::from_bytes(der_signature).map_err(|e| Error::Signature(e.to_string()))?;
        key.verify(message, &signature)
            .map_err(|_| Error::Signature("signature does not verify".into()))
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PublicKey({})", self.device_id())
    }
}

/// SHA-256 of the SPKI DER. Hex in wire and storage forms, a grouped base32
/// fingerprint of the first 80 bits when a person has to compare it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceId(pub [u8; 32]);

impl DeviceId {
    pub fn from_hex(text: &str) -> Result<Self, Error> {
        let bytes =
            hex::decode(text.trim()).map_err(|e| Error::Identity(format!("device id hex: {e}")))?;
        bytes
            .try_into()
            .map(Self)
            .map_err(|_| Error::Identity("device id must be 32 bytes".into()))
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// "K7QN-4M2X-9WTA-XXXX": the rendering src/internal/enroll used, so
    /// nothing changes for a person who has learned to read one.
    pub fn fingerprint(&self) -> String {
        let text = data_encoding::BASE32_NOPAD.encode(&self.0[..10]);
        text.as_bytes()
            .chunks(4)
            .map(|c| std::str::from_utf8(c).unwrap_or(""))
            .collect::<Vec<_>>()
            .join("-")
    }

    pub fn short(&self) -> String {
        hex::encode(&self.0[..4])
    }
}

impl fmt::Debug for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DeviceId({})", self.short())
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// What the hardware has to provide, and nothing more. Implemented by the
/// software key below, by the Secure Enclave library in muster-platform, and
/// by the app shells' callbacks.
pub trait HwSigner: Send + Sync + fmt::Debug {
    fn public_key(&self) -> PublicKey;
    /// ECDSA-P256-SHA256 over `message` (unhashed), DER encoded.
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error>;
    fn backend(&self) -> &'static str;
}

pub type SharedSigner = Arc<dyn HwSigner>;

/// A P-256 key in memory. The reference signer, the Linux/server signer, and
/// what every test uses.
#[derive(Clone)]
pub struct SoftSigner(p256::ecdsa::SigningKey);

impl SoftSigner {
    pub fn generate() -> Self {
        Self(p256::ecdsa::SigningKey::random(
            &mut p256::elliptic_curve::rand_core::OsRng,
        ))
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes().into()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        p256::ecdsa::SigningKey::from_slice(bytes)
            .map(Self)
            .map_err(|e| Error::Identity(format!("soft key: {e}")))
    }
}

impl fmt::Debug for SoftSigner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SoftSigner({})", self.public_key().device_id().short())
    }
}

impl HwSigner for SoftSigner {
    fn public_key(&self) -> PublicKey {
        let point = self.0.verifying_key().to_encoded_point(false);
        PublicKey::from_raw(point.as_bytes()).expect("p256 produces valid points")
    }

    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        use p256::ecdsa::signature::Signer as _;
        let signature: DerSignature = self.0.sign(message);
        Ok(signature.as_bytes().to_vec())
    }

    fn backend(&self) -> &'static str {
        "soft"
    }
}

// Hex strings in JSON and storage; bytes on the wire (canonical encoding).
impl serde::Serialize for DeviceId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> serde::Deserialize<'de> for DeviceId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(deserializer)?;
        DeviceId::from_hex(&text).map_err(serde::de::Error::custom)
    }
}

impl serde::Serialize for PublicKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex::encode(self.0))
    }
}

impl<'de> serde::Deserialize<'de> for PublicKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(deserializer)?;
        let bytes = hex::decode(text.trim()).map_err(serde::de::Error::custom)?;
        PublicKey::from_raw(&bytes).map_err(serde::de::Error::custom)
    }
}

/// Signs `message` under a domain tag so a signature made for one purpose
/// can never be replayed as another.
pub fn sign_domain(signer: &dyn HwSigner, domain: &str, message: &[u8]) -> Result<Vec<u8>, Error> {
    signer.sign(&domain_message(domain, message))
}

pub fn verify_domain(
    key: &PublicKey,
    domain: &str,
    message: &[u8],
    signature: &[u8],
) -> Result<(), Error> {
    key.verify(&domain_message(domain, message), signature)
}

fn domain_message(domain: &str, message: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(domain.len() + 1 + message.len());
    out.extend_from_slice(domain.as_bytes());
    out.push(0);
    out.extend_from_slice(message);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soft_signer_round_trip() {
        let signer = SoftSigner::generate();
        let key = signer.public_key();
        let signature = sign_domain(&signer, "TEST", b"hello").unwrap();
        verify_domain(&key, "TEST", b"hello", &signature).unwrap();
        assert!(verify_domain(&key, "OTHER", b"hello", &signature).is_err());
        assert!(verify_domain(&key, "TEST", b"hellp", &signature).is_err());
        let restored = SoftSigner::from_bytes(&signer.to_bytes()).unwrap();
        assert_eq!(restored.public_key(), key);
    }

    #[test]
    fn fingerprint_shape() {
        let id = DeviceId([0xab; 32]);
        let fp = id.fingerprint();
        assert_eq!(fp.split('-').count(), 4);
        assert_eq!(fp.replace('-', "").len(), 16);
        assert_eq!(DeviceId::from_hex(&id.to_hex()).unwrap(), id);
    }
}
