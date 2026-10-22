//! Pairing by QR (ARCHITECTURE §3.4): the admin shows an invite; the joiner
//! connects to the admin through the relay, pins the admin's key from the
//! invite, and proves it holds the invite secret with an HMAC over the TLS
//! exporter of that very connection. The camera is the trusted channel, so
//! there is no SAS to compare.

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::Error;
use crate::canonical::{Reader, Writer};
use crate::identity::{DeviceId, PublicKey};
use crate::roster::NetworkId;

const INVITE_PREFIX: &str = "muster1:";
const INVITE_VERSION: u8 = 1;
const PROOF_DOMAIN: &[u8] = b"MUSTER-INVITE-PROOF-V1";
pub const EXPORTER_LABEL: &[u8] = b"muster-invite";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invite {
    pub network_id: NetworkId,
    pub network_name: String,
    pub admin_id: DeviceId,
    pub admin_key: PublicKey,
    /// Relay the joiner should use to reach the admin, e.g. "muster.linsheng.dev:9443".
    pub relay: String,
    pub secret: [u8; 16],
    pub expires_unix: u64,
}

impl Invite {
    pub fn new(
        network_id: NetworkId,
        network_name: &str,
        admin_key: PublicKey,
        relay: &str,
        ttl_secs: u64,
    ) -> Self {
        let mut secret = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut secret);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        Self {
            network_id,
            network_name: network_name.to_string(),
            admin_id: admin_key.device_id(),
            admin_key,
            relay: relay.to_string(),
            secret,
            expires_unix: now + ttl_secs,
        }
    }

    /// The text the QR code carries.
    pub fn encode(&self) -> String {
        let mut w = Writer::new();
        w.u8(INVITE_VERSION)
            .fixed(&self.network_id.0)
            .str(&self.network_name)
            .bytes(self.admin_key.raw())
            .str(&self.relay)
            .fixed(&self.secret)
            .u64(self.expires_unix);
        format!(
            "{INVITE_PREFIX}{}",
            data_encoding::BASE64URL_NOPAD.encode(&w.finish())
        )
    }

    pub fn decode(text: &str) -> Result<Self, Error> {
        let body = text
            .trim()
            .strip_prefix(INVITE_PREFIX)
            .ok_or_else(|| Error::Malformed("not a muster invite".into()))?;
        let bytes = data_encoding::BASE64URL_NOPAD
            .decode(body.as_bytes())
            .map_err(|e| Error::Malformed(format!("invite base64: {e}")))?;
        let mut r = Reader::new(&bytes);
        if r.u8()? != INVITE_VERSION {
            return Err(Error::Malformed("invite version".into()));
        }
        let network_id = crate::roster::EntryHash(r.fixed()?);
        let network_name = r.str()?.to_string();
        let admin_key = PublicKey::from_raw(r.bytes()?)?;
        let relay = r.str()?.to_string();
        let secret = r.fixed()?;
        let expires_unix = r.u64()?;
        r.finish()?;
        Ok(Self {
            network_id,
            network_name,
            admin_id: admin_key.device_id(),
            admin_key,
            relay,
            secret,
            expires_unix,
        })
    }

    pub fn expired(&self, now_unix: u64) -> bool {
        now_unix > self.expires_unix
    }

    /// HMAC(secret, domain || exporter): proves the joiner read the QR, bound
    /// to this TLS connection so a relay cannot replay it elsewhere.
    pub fn proof(&self, exporter: &[u8]) -> [u8; 32] {
        proof(&self.secret, exporter)
    }

    pub fn verify_proof(&self, exporter: &[u8], candidate: &[u8]) -> bool {
        let expected = proof(&self.secret, exporter);
        candidate.len() == 32 && constant_time_eq(&expected, candidate)
    }
}

fn proof(secret: &[u8; 16], exporter: &[u8]) -> [u8; 32] {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("hmac accepts any key length");
    mac.update(PROOF_DOMAIN);
    mac.update(exporter);
    mac.finalize().into_bytes().into()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{HwSigner, SoftSigner};

    #[test]
    fn invite_round_trip_and_proof() {
        let admin = SoftSigner::generate();
        let invite = Invite::new(
            crate::roster::EntryHash([7; 32]),
            "home",
            admin.public_key(),
            "relay.example:9443",
            600,
        );
        let text = invite.encode();
        assert!(text.starts_with("muster1:"));
        let decoded = Invite::decode(&text).unwrap();
        assert_eq!(decoded, invite);
        let exporter = [1u8; 32];
        let proof = invite.proof(&exporter);
        assert!(decoded.verify_proof(&exporter, &proof));
        assert!(!decoded.verify_proof(&[2u8; 32], &proof));
        assert!(!invite.expired(invite.expires_unix));
        assert!(invite.expired(invite.expires_unix + 1));
    }
}
