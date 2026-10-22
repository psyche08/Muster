//! Pairing over the link (ARCHITECTURE §3.4). The joiner pins the admin from
//! the invite, connects, proves the invite secret against the TLS exporter,
//! and receives the roster once the admin approves. While invites are open
//! the admin accepts non-member clients; the pair service is the only one
//! such a client may use.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use muster_core::DeviceId;
use muster_core::canonical::{Reader, Writer};
use muster_core::invite::{EXPORTER_LABEL, Invite};
use muster_core::roster::{Author, DeviceRecord, Entry, Roster};
use quinn::{Connection, RecvStream, SendStream};
use sha2::{Digest, Sha256};
use tokio::sync::oneshot;

use crate::Error;
use crate::node::Node;
use crate::stream::{MAX_MESSAGE, read_message, write_message};

pub const SERVICE: &str = "pair";
pub const VERSION: u32 = 1;
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(300);

/// A joiner waiting for the admin's answer.
#[derive(Clone, Debug)]
pub struct Candidate {
    pub invite_id: [u8; 32],
    pub record: DeviceRecord,
}

pub enum Verdict {
    Approve(DeviceRecord),
    Reject(String),
}

/// How the admin decides: auto for tests and unattended servers, a UI or CLI
/// prompt in musterd. The record may be edited (scopes, vip) before approval.
pub type Approver = Arc<dyn Fn(Candidate) -> oneshot::Receiver<Verdict> + Send + Sync>;

#[derive(Default)]
pub struct Pairing {
    invites: Mutex<HashMap<[u8; 32], Invite>>,
    approver: Mutex<Option<Approver>>,
}

impl Pairing {
    pub fn set_approver(&self, approver: Approver) {
        *self.approver.lock().unwrap() = Some(approver);
    }

    pub fn open(&self, invite: Invite) {
        self.invites
            .lock()
            .unwrap()
            .insert(invite_id(&invite), invite);
    }

    fn take(&self, id: &[u8; 32]) -> Option<Invite> {
        let mut invites = self.invites.lock().unwrap();
        let now = unix_now();
        invites.retain(|_, invite| !invite.expired(now));
        invites.get(id).cloned()
    }

    pub fn any_open(&self) -> bool {
        let mut invites = self.invites.lock().unwrap();
        let now = unix_now();
        invites.retain(|_, invite| !invite.expired(now));
        !invites.is_empty()
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

pub fn invite_id(invite: &Invite) -> [u8; 32] {
    Sha256::digest(invite.secret).into()
}

fn exporter(connection: &Connection) -> Result<[u8; 32], Error> {
    let mut out = [0u8; 32];
    connection
        .export_keying_material(&mut out, EXPORTER_LABEL, &[])
        .map_err(|_| Error::Tls("exporter unavailable".into()))?;
    Ok(out)
}

fn encode_request(invite_id: &[u8; 32], proof: &[u8; 32], record: &DeviceRecord) -> Vec<u8> {
    let mut w = Writer::new();
    w.fixed(invite_id).fixed(proof).bytes(&record.to_bytes());
    w.finish()
}

fn decode_request(bytes: &[u8]) -> Result<([u8; 32], [u8; 32], DeviceRecord), Error> {
    let mut r = Reader::new(bytes);
    let invite_id = r.fixed()?;
    let proof = r.fixed()?;
    let record = DeviceRecord::from_bytes(r.bytes()?)?;
    r.finish()?;
    Ok((invite_id, proof, record))
}

const STATUS_PENDING: u8 = 0;
const STATUS_ACCEPTED: u8 = 1;
const STATUS_REJECTED: u8 = 2;

fn encode_response(status: u8, entries: &[Entry], reason: &str) -> Vec<u8> {
    let mut w = Writer::new();
    w.u8(status).str(reason).u16(entries.len() as u16);
    for entry in entries {
        w.bytes(&entry.encode());
    }
    w.finish()
}

fn decode_response(bytes: &[u8]) -> Result<(u8, String, Vec<Entry>), Error> {
    let mut r = Reader::new(bytes);
    let status = r.u8()?;
    let reason = r.str()?.to_string();
    let count = r.u16()?;
    let mut entries = Vec::with_capacity(count as usize);
    for _ in 0..count {
        entries.push(Entry::decode(r.bytes()?)?);
    }
    r.finish()?;
    Ok((status, reason, entries))
}

/// Admin side.
pub async fn serve(
    node: &Arc<Node>,
    peer: DeviceId,
    connection: &Connection,
    mut send: SendStream,
    mut recv: RecvStream,
    generation: u64,
) {
    let result: Result<(), Error> = async {
        let request = read_message(&mut recv, 8192)
            .await?
            .ok_or_else(|| Error::Protocol("no pair request".into()))?;
        let (invite_id, proof, record) = decode_request(&request)?;
        if record.device_id != peer {
            return Err(Error::Unauthorized(
                "record is not for the connecting device".into(),
            ));
        }
        let Some(invite) = node.pairing().take(&invite_id) else {
            write_message(
                &mut send,
                &encode_response(STATUS_REJECTED, &[], "no such invite"),
            )
            .await?;
            return Err(Error::Unauthorized("unknown or expired invite".into()));
        };
        if !invite.verify_proof(&exporter(connection)?, &proof) {
            write_message(
                &mut send,
                &encode_response(STATUS_REJECTED, &[], "bad proof"),
            )
            .await?;
            return Err(Error::Unauthorized("invite proof does not verify".into()));
        }
        // An active member scanning a fresh invite (a phone doing its setup
        // again) is not a new device: nobody is asked, no entry is written,
        // it just gets the roster again and its link is adopted.
        let already_member = node
            .roster()
            .read()
            .unwrap()
            .state()
            .active(&peer)
            .is_some();
        if already_member {
            node.with_generation(generation, || {
                node.pairing().invites.lock().unwrap().remove(&invite_id);
                Ok(())
            })?;
            let entries: Vec<Entry> = node.roster().read().unwrap().entries().cloned().collect();
            write_message(&mut send, &encode_response(STATUS_ACCEPTED, &entries, "")).await?;
            log::info!(
                "re-paired {} ({}): already a member",
                peer.short(),
                peer.fingerprint()
            );
            node.adopt(peer, connection.clone(), generation);
            return Ok(());
        }
        let approver = node
            .pairing()
            .approver
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| Error::Protocol("no approver installed".into()))?;
        write_message(&mut send, &encode_response(STATUS_PENDING, &[], "")).await?;
        let verdict = tokio::select! {
            biased;
            _ = connection.closed() => return Err(Error::Io("pairing connection closed".into())),
            verdict = tokio::time::timeout(APPROVAL_TIMEOUT, approver(Candidate { invite_id, record })) => {
                verdict.map_err(|_| Error::Timeout("pairing approval"))?
                    .map_err(|_| Error::Protocol("approver dropped".into()))?
            }
        };
        match verdict {
            Verdict::Approve(record) => {
                let entries = node.author_add(record, generation)?;
                node.pairing().invites.lock().unwrap().remove(&invite_id);
                write_message(&mut send, &encode_response(STATUS_ACCEPTED, &entries, "")).await?;
                log::info!("paired {} ({})", peer.short(), peer.fingerprint());
                // From here on this is a member link: tracked, synced, and
                // subject to membership enforcement like any other.
                node.adopt(peer, connection.clone(), generation);
            }
            Verdict::Reject(reason) => {
                write_message(&mut send, &encode_response(STATUS_REJECTED, &[], &reason)).await?;
            }
        }
        Ok(())
    }
    .await;
    if let Err(error) = result {
        log::info!("pairing with {} failed: {error}", peer.short());
    }
    let _ = send.finish();
    if !node.pairing().any_open() {
        node.trust()
            .accept_unknown_clients
            .store(false, Ordering::Relaxed);
    }
}

/// Joiner side: returns the roster the admin handed over; the caller
/// installs it.
pub async fn join(
    node: &Arc<Node>,
    invite: &Invite,
    record: DeviceRecord,
) -> Result<Roster, Error> {
    if invite.expired(unix_now()) {
        return Err(Error::Unauthorized("invite has expired".into()));
    }
    let (connection, mut send, mut recv) =
        tokio::time::timeout(crate::reconnect::CONNECT_TIMEOUT, async {
            let connection = node.connect_pinned(invite.admin_id).await?;
            let (send, recv) = node.open_on(&connection, SERVICE, VERSION).await?;
            Ok::<_, Error>((connection, send, recv))
        })
        .await
        .map_err(|_| Error::Timeout("pair connect (3 seconds)"))??;
    let proof = invite.proof(&exporter(&connection)?);
    write_message(
        &mut send,
        &encode_request(&invite_id(invite), &proof, &record),
    )
    .await?;
    loop {
        let reply = read_message(&mut recv, MAX_MESSAGE)
            .await?
            .ok_or_else(|| Error::Protocol("admin closed the pairing stream".into()))?;
        let (status, reason, entries) = decode_response(&reply)?;
        match status {
            STATUS_PENDING => {
                log::info!("waiting for approval on {}", invite.admin_id.short());
            }
            STATUS_ACCEPTED => {
                let roster = Roster::from_entries(entries)?;
                if roster.network_id() != Some(invite.network_id) {
                    return Err(Error::Unauthorized(
                        "admin sent a roster for another network".into(),
                    ));
                }
                if roster.state().active(&record.device_id).is_none() {
                    return Err(Error::Protocol(
                        "roster does not include this device".into(),
                    ));
                }
                return Ok(roster);
            }
            _ => return Err(Error::Unauthorized(format!("pairing rejected: {reason}"))),
        }
    }
}

/// Convenience for the admin: author an Add on the current roster head.
pub(crate) fn author_add(
    roster: &mut Roster,
    signer: &dyn muster_core::HwSigner,
    record: DeviceRecord,
) -> Result<Entry, Error> {
    let entry = Author { roster, signer }.add(record)?;
    roster.insert(entry.clone())?;
    Ok(entry)
}
