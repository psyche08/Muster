//! Frames between a device and the relay (ARCHITECTURE §3.5). The relay
//! learns who is where and moves opaque bytes; it verifies one thing, that a
//! registrant holds the key its id names, so nobody can squat another
//! device's id and swallow its traffic.
//!
//!   0x01 HELLO     id[32]                              → CHALLENGE
//!   0x02 AUTH      id[32] pubkey(bytes) nonce[32] sig(bytes)   → REFLECT (registered)
//!   0x03 FWD       dst[32] payload                     forwarded as DELIVER to dst
//!   0x04 DELIVER   src[32] payload
//!   0x05 MB_PUT    key[32] item(bytes)                 → MB_ACK
//!   0x06 MB_LIST   key[32]                             → MB_LISTING
//!   0x07 MB_GET    key[32] hash[32]                    → MB_ITEM
//!   0x08 KEEPALIVE id[32]                              → REFLECT
//!   0x0b PROBE     id[32] nonce(u64) zero-padding      → PROBE_ACK
//!   0x83 PROBE_ACK nonce(u64) zero-padding
//!   0x0a ACTIVATE  id[32]                              → REFLECT (preferred return carrier)
//!   0x0c FEATURES  bits(u32)                           opt this carrier into the frames below
//!   0x0d WATCH     id[32]                              → PRESENT once id registers
//!   0x84 ABSENT    id[32]       (FEATURES_PRESENCE)    a FWD's dst has been unregistered past the grace
//!   0x88 PRESENT   id[32]       (FEATURES_PRESENCE)    a watched id registered
//!   0x81 REFLECT   addr
//!   0x82 CHALLENGE nonce[32]
//!   0x85 MB_ACK    key[32] hash[32]
//!   0x86 MB_LISTING key[32] u16 count, hash[32]...
//!   0x87 MB_ITEM   key[32] hash[32] item(bytes)
//!   0x8f ERROR     text
//!
//! Every frame fits one UDP datagram; the secure carriers send the same
//! bytes as one QUIC DATAGRAM or one TLS length frame. Payloads of FWD/DELIVER start with a type
//! byte: 0x20 QUIC packet, 0x21 direct-path candidates.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use muster_core::Error as CoreError;
use muster_core::canonical::{Reader, Writer};
use muster_core::identity::{DeviceId, HwSigner, PublicKey, sign_domain, verify_domain};

pub const RELAY_AUTH_DOMAIN: &str = "MUSTER-RELAY-AUTH-V1";

pub const HELLO: u8 = 0x01;
pub const AUTH: u8 = 0x02;
pub const FWD: u8 = 0x03;
pub const DELIVER: u8 = 0x04;
pub const MB_PUT: u8 = 0x05;
pub const MB_LIST: u8 = 0x06;
pub const MB_GET: u8 = 0x07;
pub const KEEPALIVE: u8 = 0x08;
/// A phone leaves a push token so the relay can wake it for a member's
/// traffic while it is offline (§8 of the risks: iOS background).
pub const PUSH_REG: u8 = 0x09;
/// Authenticated explicit inbound-carrier selection, including receive-only clients.
pub const ACTIVATE: u8 = 0x0a;
pub const PROBE: u8 = 0x0b;
pub const PROBE_ACK: u8 = 0x83;
pub const REFLECT: u8 = 0x81;
pub const CHALLENGE: u8 = 0x82;
pub const MB_ACK: u8 = 0x85;
pub const MB_LISTING: u8 = 0x86;
pub const MB_ITEM: u8 = 0x87;
pub const ERROR: u8 = 0x8f;
/// A registered client opts one carrier into the presence frames.
pub const FEATURES: u8 = 0x0c;
pub const WATCH: u8 = 0x0d;
pub const ABSENT: u8 = 0x84;
pub const PRESENT: u8 = 0x88;

/// `FEATURES` bit: this carrier understands ABSENT and PRESENT (an older
/// client would drop its carrier on a frame kind it does not know, so the
/// relay sends them only to carriers that asked).
pub const FEATURES_PRESENCE: u32 = 1;

pub const PAYLOAD_QUIC: u8 = 0x20;
pub const PAYLOAD_CANDIDATES: u8 = 0x21;

/// Largest frame either side accepts; keeps mailbox items and payloads inside
/// one datagram with room for UDP/IP headers.
pub const MAX_FRAME: usize = 1400;
/// FWD/DELIVER header plus the QUIC payload tag. Every virtual path must
/// retain room for this envelope, including a direct path that later relays.
pub const MAX_QUIC_DATAGRAM: u16 = (MAX_FRAME - 1 - 32 - 1) as u16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Frame {
    Hello {
        id: DeviceId,
    },
    Auth {
        id: DeviceId,
        key: PublicKey,
        nonce: [u8; 32],
        signature: Vec<u8>,
    },
    Fwd {
        dst: DeviceId,
        payload: Vec<u8>,
    },
    Deliver {
        src: DeviceId,
        payload: Vec<u8>,
    },
    MbPut {
        key: [u8; 32],
        item: Vec<u8>,
    },
    MbList {
        key: [u8; 32],
    },
    MbGet {
        key: [u8; 32],
        hash: [u8; 32],
    },
    Keepalive {
        id: DeviceId,
    },
    Probe {
        id: DeviceId,
        nonce: u64,
    },
    ProbeAck {
        nonce: u64,
    },
    Activate {
        id: DeviceId,
    },
    PushReg {
        id: DeviceId,
        kind: u8,
        token: Vec<u8>,
    },
    Reflect {
        addr: SocketAddr,
    },
    Challenge {
        nonce: [u8; 32],
    },
    MbAck {
        key: [u8; 32],
        hash: [u8; 32],
    },
    MbListing {
        key: [u8; 32],
        hashes: Vec<[u8; 32]>,
    },
    MbItem {
        key: [u8; 32],
        hash: [u8; 32],
        item: Vec<u8>,
    },
    Error {
        text: String,
    },
    Features {
        bits: u32,
    },
    Watch {
        id: DeviceId,
    },
    /// The relay's authority: `id` is not registered, and has not been for
    /// longer than a re-registration takes.
    Absent {
        id: DeviceId,
    },
    Present {
        id: DeviceId,
    },
}

impl Frame {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        match self {
            Frame::Hello { id } => {
                w.u8(HELLO).fixed(&id.0);
            }
            Frame::Auth {
                id,
                key,
                nonce,
                signature,
            } => {
                w.u8(AUTH)
                    .fixed(&id.0)
                    .bytes(key.raw())
                    .fixed(nonce)
                    .bytes(signature);
            }
            Frame::Fwd { dst, payload } => {
                w.u8(FWD).fixed(&dst.0).fixed(payload);
            }
            Frame::Deliver { src, payload } => {
                w.u8(DELIVER).fixed(&src.0).fixed(payload);
            }
            Frame::MbPut { key, item } => {
                w.u8(MB_PUT).fixed(key).bytes(item);
            }
            Frame::MbList { key } => {
                w.u8(MB_LIST).fixed(key);
            }
            Frame::MbGet { key, hash } => {
                w.u8(MB_GET).fixed(key).fixed(hash);
            }
            Frame::Keepalive { id } => {
                w.u8(KEEPALIVE).fixed(&id.0);
            }
            Frame::Probe { id, nonce } => {
                w.u8(PROBE)
                    .fixed(&id.0)
                    .u64(*nonce)
                    .fixed(&[0; MAX_FRAME - 41]);
            }
            Frame::ProbeAck { nonce } => {
                w.u8(PROBE_ACK).u64(*nonce).fixed(&[0; MAX_FRAME - 9]);
            }
            Frame::Activate { id } => {
                w.u8(ACTIVATE).fixed(&id.0);
            }
            Frame::PushReg { id, kind, token } => {
                w.u8(PUSH_REG).fixed(&id.0).u8(*kind).bytes(token);
            }
            Frame::Reflect { addr } => {
                w.u8(REFLECT);
                write_addr(&mut w, *addr);
            }
            Frame::Challenge { nonce } => {
                w.u8(CHALLENGE).fixed(nonce);
            }
            Frame::MbAck { key, hash } => {
                w.u8(MB_ACK).fixed(key).fixed(hash);
            }
            Frame::MbListing { key, hashes } => {
                w.u8(MB_LISTING).fixed(key).u16(hashes.len() as u16);
                for hash in hashes {
                    w.fixed(hash);
                }
            }
            Frame::MbItem { key, hash, item } => {
                w.u8(MB_ITEM).fixed(key).fixed(hash).bytes(item);
            }
            Frame::Error { text } => {
                w.u8(ERROR).str(text);
            }
            Frame::Features { bits } => {
                w.u8(FEATURES).u32(*bits);
            }
            Frame::Watch { id } => {
                w.u8(WATCH).fixed(&id.0);
            }
            Frame::Absent { id } => {
                w.u8(ABSENT).fixed(&id.0);
            }
            Frame::Present { id } => {
                w.u8(PRESENT).fixed(&id.0);
            }
        }
        w.finish()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, CoreError> {
        let mut r = Reader::new(bytes);
        let kind = r.u8()?;
        let frame = match kind {
            HELLO => Frame::Hello {
                id: DeviceId(r.fixed()?),
            },
            AUTH => Frame::Auth {
                id: DeviceId(r.fixed()?),
                key: PublicKey::from_raw(r.bytes()?)?,
                nonce: r.fixed()?,
                signature: r.bytes()?.to_vec(),
            },
            FWD => {
                let dst = DeviceId(r.fixed()?);
                Frame::Fwd {
                    dst,
                    payload: bytes[33..].to_vec(),
                }
            }
            DELIVER => {
                let src = DeviceId(r.fixed()?);
                Frame::Deliver {
                    src,
                    payload: bytes[33..].to_vec(),
                }
            }
            MB_PUT => Frame::MbPut {
                key: r.fixed()?,
                item: r.bytes()?.to_vec(),
            },
            MB_LIST => Frame::MbList { key: r.fixed()? },
            MB_GET => Frame::MbGet {
                key: r.fixed()?,
                hash: r.fixed()?,
            },
            KEEPALIVE => Frame::Keepalive {
                id: DeviceId(r.fixed()?),
            },
            PROBE => {
                let id = DeviceId(r.fixed()?);
                let nonce = r.u64()?;
                if r.fixed::<{ MAX_FRAME - 41 }>()?
                    .iter()
                    .any(|byte| *byte != 0)
                {
                    return Err(CoreError::Malformed("nonzero relay probe padding".into()));
                }
                Frame::Probe { id, nonce }
            }
            PROBE_ACK => {
                let nonce = r.u64()?;
                if r.fixed::<{ MAX_FRAME - 9 }>()?
                    .iter()
                    .any(|byte| *byte != 0)
                {
                    return Err(CoreError::Malformed(
                        "nonzero relay probe acknowledgement padding".into(),
                    ));
                }
                Frame::ProbeAck { nonce }
            }
            ACTIVATE => Frame::Activate {
                id: DeviceId(r.fixed()?),
            },
            PUSH_REG => Frame::PushReg {
                id: DeviceId(r.fixed()?),
                kind: r.u8()?,
                token: r.bytes()?.to_vec(),
            },
            REFLECT => Frame::Reflect {
                addr: read_addr(&mut r)?,
            },
            CHALLENGE => Frame::Challenge { nonce: r.fixed()? },
            MB_ACK => Frame::MbAck {
                key: r.fixed()?,
                hash: r.fixed()?,
            },
            MB_LISTING => {
                let key = r.fixed()?;
                let count = r.u16()?;
                let mut hashes = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    hashes.push(r.fixed()?);
                }
                Frame::MbListing { key, hashes }
            }
            MB_ITEM => Frame::MbItem {
                key: r.fixed()?,
                hash: r.fixed()?,
                item: r.bytes()?.to_vec(),
            },
            ERROR => Frame::Error {
                text: r.str()?.to_string(),
            },
            FEATURES => Frame::Features { bits: r.u32()? },
            WATCH => Frame::Watch {
                id: DeviceId(r.fixed()?),
            },
            ABSENT => Frame::Absent {
                id: DeviceId(r.fixed()?),
            },
            PRESENT => Frame::Present {
                id: DeviceId(r.fixed()?),
            },
            other => return Err(CoreError::Malformed(format!("relay frame kind {other:#x}"))),
        };
        if !matches!(frame, Frame::Fwd { .. } | Frame::Deliver { .. }) {
            r.finish()?;
        }
        Ok(frame)
    }
}

pub fn write_addr(w: &mut Writer, addr: SocketAddr) {
    match addr.ip() {
        IpAddr::V4(ip) => w.u8(4).fixed(&ip.octets()),
        IpAddr::V6(ip) => w.u8(6).fixed(&ip.octets()),
    };
    w.u16(addr.port());
}

pub fn read_addr(r: &mut Reader<'_>) -> Result<SocketAddr, CoreError> {
    let ip = match r.u8()? {
        4 => IpAddr::V4(Ipv4Addr::from(r.fixed::<4>()?)),
        6 => IpAddr::V6(std::net::Ipv6Addr::from(r.fixed::<16>()?)),
        other => return Err(CoreError::Malformed(format!("address family {other}"))),
    };
    Ok(SocketAddr::new(ip, r.u16()?))
}

pub fn encode_candidates(candidates: &[SocketAddr]) -> Vec<u8> {
    let mut w = Writer::new();
    w.u8(PAYLOAD_CANDIDATES).u16(candidates.len() as u16);
    for candidate in candidates {
        write_addr(&mut w, *candidate);
    }
    w.finish()
}

pub fn decode_candidates(payload: &[u8]) -> Result<Vec<SocketAddr>, CoreError> {
    let mut r = Reader::new(payload);
    if r.u8()? != PAYLOAD_CANDIDATES {
        return Err(CoreError::Malformed("not a candidate list".into()));
    }
    let count = r.u16()?;
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        out.push(read_addr(&mut r)?);
    }
    r.finish()?;
    Ok(out)
}

/// The signed message of an AUTH: the challenge nonce alone, under the relay
/// domain, so a signature never doubles as anything else.
pub fn auth_signature(signer: &dyn HwSigner, nonce: &[u8; 32]) -> Result<Vec<u8>, CoreError> {
    sign_domain(signer, RELAY_AUTH_DOMAIN, nonce)
}

pub fn verify_auth(
    id: &DeviceId,
    key: &PublicKey,
    nonce: &[u8; 32],
    signature: &[u8],
) -> Result<(), CoreError> {
    if key.device_id() != *id {
        return Err(CoreError::Identity(
            "registered id is not the hash of the presented key".into(),
        ));
    }
    verify_domain(key, RELAY_AUTH_DOMAIN, nonce, signature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use muster_core::SoftSigner;

    #[test]
    fn health_probes_require_exact_maximum_frame_and_zero_padding() {
        let id = SoftSigner::generate().public_key().device_id();
        for (frame, header) in [
            (
                Frame::Probe {
                    id,
                    nonce: u64::MAX,
                },
                41,
            ),
            (Frame::ProbeAck { nonce: u64::MAX }, 9),
        ] {
            let mut bytes = frame.encode();
            assert_eq!(bytes.len(), MAX_FRAME);
            assert!(bytes[header..].iter().all(|byte| *byte == 0));
            assert_eq!(Frame::decode(&bytes).unwrap(), frame);
            // Small heartbeats must not falsely validate a path that loses full-sized packets.
            assert!(Frame::decode(&bytes[..header]).is_err());
            assert!(Frame::decode(&bytes[..MAX_FRAME - 1]).is_err());
            bytes.push(0);
            assert!(Frame::decode(&bytes).is_err());
            bytes.pop();
            bytes[header] = 1;
            assert!(Frame::decode(&bytes).is_err());
            bytes[header] = 0;
            bytes[MAX_FRAME - 1] = 1;
            assert!(Frame::decode(&bytes).is_err());
        }
    }

    #[test]
    fn frames_round_trip() {
        let signer = SoftSigner::generate();
        let key = signer.public_key();
        let id = key.device_id();
        let nonce = [9u8; 32];
        let frames = vec![
            Frame::Hello { id },
            Frame::Auth {
                id,
                key: key.clone(),
                nonce,
                signature: auth_signature(&signer, &nonce).unwrap(),
            },
            Frame::Fwd {
                dst: id,
                payload: vec![PAYLOAD_QUIC, 1, 2, 3],
            },
            Frame::Deliver {
                src: id,
                payload: encode_candidates(&[
                    "10.0.0.1:5".parse().unwrap(),
                    "[::1]:6".parse().unwrap(),
                ]),
            },
            Frame::MbPut {
                key: [1; 32],
                item: vec![7; 40],
            },
            Frame::MbList { key: [1; 32] },
            Frame::MbGet {
                key: [1; 32],
                hash: [2; 32],
            },
            Frame::Keepalive { id },
            Frame::Reflect {
                addr: "203.0.113.9:4433".parse().unwrap(),
            },
            Frame::Challenge { nonce },
            Frame::MbAck {
                key: [1; 32],
                hash: [2; 32],
            },
            Frame::MbListing {
                key: [1; 32],
                hashes: vec![[3; 32], [4; 32]],
            },
            Frame::MbItem {
                key: [1; 32],
                hash: [2; 32],
                item: vec![1],
            },
            Frame::Error {
                text: "nope".into(),
            },
            Frame::Features {
                bits: FEATURES_PRESENCE,
            },
            Frame::Watch { id },
            Frame::Absent { id },
            Frame::Present { id },
        ];
        for frame in frames {
            assert_eq!(Frame::decode(&frame.encode()).unwrap(), frame);
        }
        if let Frame::Auth {
            id,
            key,
            nonce,
            signature,
        } = Frame::decode(
            &Frame::Auth {
                id,
                key: key.clone(),
                nonce,
                signature: auth_signature(&signer, &nonce).unwrap(),
            }
            .encode(),
        )
        .unwrap()
        {
            verify_auth(&id, &key, &nonce, &signature).unwrap();
            assert!(verify_auth(&id, &key, &[0; 32], &signature).is_err());
        }
        let candidates =
            decode_candidates(&encode_candidates(&["10.0.0.1:5".parse().unwrap()])).unwrap();
        assert_eq!(candidates.len(), 1);
    }
}
