//! The network is a signed member roster: an append-only DAG of entries whose
//! fold is the membership state (ARCHITECTURE §3.4).
//!
//! Rules, restated where the code applies them:
//! - an entry is accepted only if its parents are accepted and its signer is
//!   an active admin in the fold of its causal past (Genesis signs itself);
//! - Remove is permanent; a removed device id is never re-added;
//! - Update is last-writer-wins by (lamport, hash);
//! - entries signed by an admin that a Remove does not causally include are
//!   invalid, so a stolen admin key cannot backfill;
//! - two active members claiming one vip: the smaller defining hash keeps it.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::net::Ipv4Addr;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::Error;
use crate::canonical::{Reader, Writer};
use crate::identity::{DeviceId, HwSigner, PublicKey, sign_domain, verify_domain};
use crate::scope::Scope;

pub const ROSTER_DOMAIN: &str = "MUSTER-ROSTER-V1";
const ENTRY_VERSION: u8 = 1;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntryHash(pub [u8; 32]);

impl EntryHash {
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
    /// The first four bytes as hex, for logs.
    pub fn short(&self) -> String {
        hex::encode(&self.0[..4])
    }
    pub fn from_hex(text: &str) -> Result<Self, Error> {
        let bytes =
            hex::decode(text.trim()).map_err(|e| Error::Malformed(format!("hash hex: {e}")))?;
        bytes
            .try_into()
            .map(Self)
            .map_err(|_| Error::Malformed("hash must be 32 bytes".into()))
    }
}

impl fmt::Debug for EntryHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EntryHash({})", hex::encode(&self.0[..4]))
    }
}

/// Identifies a network: the hash of its Genesis entry.
pub type NetworkId = EntryHash;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceKind {
    Desktop,
    Phone,
    Server,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Member,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRecord {
    pub device_id: DeviceId,
    pub public_key: PublicKey,
    pub presence_key: Option<PublicKey>,
    pub name: String,
    pub kind: DeviceKind,
    pub role: Role,
    /// Virtual IP on the vnet; phones have none.
    pub vip: Option<Ipv4Addr>,
    pub scopes: BTreeSet<Scope>,
}

impl DeviceRecord {
    pub fn new(public_key: PublicKey, name: impl Into<String>, kind: DeviceKind) -> Self {
        Self {
            device_id: public_key.device_id(),
            public_key,
            presence_key: None,
            name: name.into(),
            kind,
            role: Role::Member,
            vip: None,
            scopes: BTreeSet::new(),
        }
    }

    /// Canonical bytes, for carrying a record outside an entry (pairing).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        self.write(&mut w);
        w.finish()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(bytes);
        let record = Self::read(&mut r)?;
        r.finish()?;
        Ok(record)
    }

    fn write(&self, w: &mut Writer) {
        w.fixed(&self.device_id.0)
            .bytes(self.public_key.raw())
            .opt_bytes(self.presence_key.as_ref().map(|k| &k.raw()[..]))
            .str(&self.name)
            .u8(match self.kind {
                DeviceKind::Desktop => 0,
                DeviceKind::Phone => 1,
                DeviceKind::Server => 2,
            })
            .u8(role_tag(self.role))
            .u8(self.vip.is_some() as u8);
        if let Some(vip) = self.vip {
            w.fixed(&vip.octets());
        }
        write_scopes(w, &self.scopes);
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
        let device_id = DeviceId(r.fixed()?);
        let public_key = PublicKey::from_raw(r.bytes()?)?;
        if public_key.device_id() != device_id {
            return Err(Error::Malformed(
                "device id does not match public key".into(),
            ));
        }
        let presence_key = r.opt_bytes()?.map(PublicKey::from_raw).transpose()?;
        let name = r.str()?.to_string();
        let kind = match r.u8()? {
            0 => DeviceKind::Desktop,
            1 => DeviceKind::Phone,
            2 => DeviceKind::Server,
            other => return Err(Error::Malformed(format!("device kind {other}"))),
        };
        let role = read_role(r)?;
        let vip = match r.u8()? {
            0 => None,
            1 => Some(Ipv4Addr::from(r.fixed::<4>()?)),
            other => return Err(Error::Malformed(format!("vip tag {other}"))),
        };
        let scopes = read_scopes(r)?;
        Ok(Self {
            device_id,
            public_key,
            presence_key,
            name,
            kind,
            role,
            vip,
            scopes,
        })
    }
}

fn role_tag(role: Role) -> u8 {
    match role {
        Role::Admin => 0,
        Role::Member => 1,
    }
}

fn read_role(r: &mut Reader<'_>) -> Result<Role, Error> {
    match r.u8()? {
        0 => Ok(Role::Admin),
        1 => Ok(Role::Member),
        other => Err(Error::Malformed(format!("role {other}"))),
    }
}

fn write_scopes(w: &mut Writer, scopes: &BTreeSet<Scope>) {
    w.u32(scopes.len() as u32);
    for scope in scopes {
        w.str(&scope.to_string());
    }
}

fn read_scopes(r: &mut Reader<'_>) -> Result<BTreeSet<Scope>, Error> {
    let n = r.u32()?;
    if n > 4096 {
        return Err(Error::Malformed("too many scopes".into()));
    }
    let mut scopes = BTreeSet::new();
    for _ in 0..n {
        scopes.insert(r.str()?.parse()?);
    }
    Ok(scopes)
}

/// Fields an Update may change. `None` leaves the field alone.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceUpdate {
    pub name: Option<String>,
    pub role: Option<Role>,
    pub scopes: Option<BTreeSet<Scope>>,
    /// `Some(None)` clears the vip.
    pub vip: Option<Option<Ipv4Addr>>,
}

impl DeviceUpdate {
    /// A backwards-compatible authorization epoch: old readers apply a no-op.
    pub fn is_presence_epoch(&self) -> bool {
        self.name.is_none() && self.role.is_none() && self.scopes.is_none() && self.vip.is_none()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Body {
    Genesis {
        name: String,
        vnet_cidr: (Ipv4Addr, u8),
        creator: DeviceRecord,
    },
    Add(DeviceRecord),
    Update {
        device_id: DeviceId,
        update: DeviceUpdate,
    },
    Remove {
        device_id: DeviceId,
    },
    /// Admin-signed, atomic address plan. Identity and membership stay intact.
    Readdress {
        vnet_cidr: (Ipv4Addr, u8),
        addresses: BTreeMap<DeviceId, Ipv4Addr>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub network_id: NetworkId,
    pub parents: Vec<EntryHash>,
    pub lamport: u64,
    pub body: Body,
    pub signer: DeviceId,
    pub signature: Vec<u8>,
}

impl Entry {
    /// Bytes that are hashed and signed: everything but the signature. A
    /// Genesis carries an all-zero network id because the id is its own hash.
    pub fn signed_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(ENTRY_VERSION);
        let network_id = match self.body {
            Body::Genesis { .. } => [0u8; 32],
            _ => self.network_id.0,
        };
        w.fixed(&network_id).u32(self.parents.len() as u32);
        for parent in &self.parents {
            w.fixed(&parent.0);
        }
        w.u64(self.lamport);
        match &self.body {
            Body::Genesis {
                name,
                vnet_cidr,
                creator,
            } => {
                w.u8(0)
                    .str(name)
                    .fixed(&vnet_cidr.0.octets())
                    .u8(vnet_cidr.1);
                creator.write(&mut w);
            }
            Body::Add(record) => {
                w.u8(1);
                record.write(&mut w);
            }
            Body::Update { device_id, update } => {
                w.u8(2).fixed(&device_id.0);
                w.opt_bytes(update.name.as_deref().map(str::as_bytes));
                match update.role {
                    Some(role) => w.u8(1).u8(role_tag(role)),
                    None => w.u8(0),
                };
                match &update.scopes {
                    Some(scopes) => {
                        w.u8(1);
                        write_scopes(&mut w, scopes);
                    }
                    None => {
                        w.u8(0);
                    }
                }
                match update.vip {
                    None => w.u8(0),
                    Some(None) => w.u8(1),
                    Some(Some(vip)) => w.u8(2).fixed(&vip.octets()),
                };
            }
            Body::Remove { device_id } => {
                w.u8(3).fixed(&device_id.0);
            }
            Body::Readdress {
                vnet_cidr,
                addresses,
            } => {
                w.u8(4)
                    .fixed(&vnet_cidr.0.octets())
                    .u8(vnet_cidr.1)
                    .u32(addresses.len() as u32);
                for (id, address) in addresses {
                    w.fixed(&id.0).fixed(&address.octets());
                }
            }
        }
        w.fixed(&self.signer.0);
        w.finish()
    }

    pub fn hash(&self) -> EntryHash {
        EntryHash(Sha256::digest(self.signed_bytes()).into())
    }

    /// Full wire form: signed bytes plus the signature.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.bytes(&self.signed_bytes()).bytes(&self.signature);
        w.finish()
    }

    pub fn decode(data: &[u8]) -> Result<Self, Error> {
        let mut outer = Reader::new(data);
        let signed = outer.bytes()?;
        let signature = outer.bytes()?.to_vec();
        outer.finish()?;
        let mut r = Reader::new(signed);
        if r.u8()? != ENTRY_VERSION {
            return Err(Error::Malformed("entry version".into()));
        }
        let network_id_bytes: [u8; 32] = r.fixed()?;
        let parent_count = r.u32()?;
        if parent_count > 64 {
            return Err(Error::Malformed("too many parents".into()));
        }
        let mut parents = Vec::with_capacity(parent_count as usize);
        for _ in 0..parent_count {
            parents.push(EntryHash(r.fixed()?));
        }
        let lamport = r.u64()?;
        let body = match r.u8()? {
            0 => {
                let name = r.str()?.to_string();
                let cidr_ip = Ipv4Addr::from(r.fixed::<4>()?);
                let cidr_bits = r.u8()?;
                let creator = DeviceRecord::read(&mut r)?;
                Body::Genesis {
                    name,
                    vnet_cidr: (cidr_ip, cidr_bits),
                    creator,
                }
            }
            1 => Body::Add(DeviceRecord::read(&mut r)?),
            2 => {
                let device_id = DeviceId(r.fixed()?);
                let name = r
                    .opt_bytes()?
                    .map(|b| String::from_utf8(b.to_vec()))
                    .transpose()
                    .map_err(|_| Error::Malformed("name utf-8".into()))?;
                let role = match r.u8()? {
                    0 => None,
                    1 => Some(read_role(&mut r)?),
                    other => return Err(Error::Malformed(format!("role tag {other}"))),
                };
                let scopes = match r.u8()? {
                    0 => None,
                    1 => Some(read_scopes(&mut r)?),
                    other => return Err(Error::Malformed(format!("scopes tag {other}"))),
                };
                let vip = match r.u8()? {
                    0 => None,
                    1 => Some(None),
                    2 => Some(Some(Ipv4Addr::from(r.fixed::<4>()?))),
                    other => return Err(Error::Malformed(format!("vip tag {other}"))),
                };
                Body::Update {
                    device_id,
                    update: DeviceUpdate {
                        name,
                        role,
                        scopes,
                        vip,
                    },
                }
            }
            3 => Body::Remove {
                device_id: DeviceId(r.fixed()?),
            },
            4 => {
                let vnet_cidr = (Ipv4Addr::from(r.fixed::<4>()?), r.u8()?);
                let count = r.u32()?;
                if count > 4096 {
                    return Err(Error::Malformed("too many addresses".into()));
                }
                let mut addresses = BTreeMap::new();
                for _ in 0..count {
                    let id = DeviceId(r.fixed()?);
                    let address = Ipv4Addr::from(r.fixed::<4>()?);
                    if addresses.insert(id, address).is_some() {
                        return Err(Error::Malformed("duplicate address owner".into()));
                    }
                }
                Body::Readdress {
                    vnet_cidr,
                    addresses,
                }
            }
            other => return Err(Error::Malformed(format!("body kind {other}"))),
        };
        let signer = DeviceId(r.fixed()?);
        r.finish()?;
        let mut entry = Self {
            network_id: EntryHash(network_id_bytes),
            parents,
            lamport,
            body,
            signer,
            signature,
        };
        if matches!(entry.body, Body::Genesis { .. }) {
            entry.network_id = entry.hash();
        } else if entry.signed_bytes() != signed {
            return Err(Error::Malformed("non-canonical entry encoding".into()));
        }
        Ok(entry)
    }

    pub fn device_id(&self) -> Option<DeviceId> {
        match &self.body {
            Body::Genesis { creator, .. } => Some(creator.device_id),
            Body::Add(record) => Some(record.device_id),
            Body::Update { device_id, .. } | Body::Remove { device_id } => Some(*device_id),
            Body::Readdress { .. } => None,
        }
    }
}

// ---------------------------------------------------------------- state

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub record: DeviceRecord,
    pub removed: bool,
    /// Hash of the entry that last set this record's fields (for vip tie-breaks).
    pub defined_by: EntryHash,
    pub last_update: (u64, EntryHash),
    #[serde(default)]
    pub presence_epoch: Option<(u64, EntryHash)>,
}

impl Member {
    pub fn active(&self) -> bool {
        !self.removed
    }
    pub fn is_admin(&self) -> bool {
        self.active() && self.record.role == Role::Admin
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub network_id: Option<NetworkId>,
    pub name: String,
    pub vnet_cidr: Option<(Ipv4Addr, u8)>,
    pub members: BTreeMap<DeviceId, Member>,
}

impl State {
    pub fn member(&self, id: &DeviceId) -> Option<&Member> {
        self.members.get(id)
    }

    pub fn active(&self, id: &DeviceId) -> Option<&Member> {
        self.members.get(id).filter(|m| m.active())
    }

    pub fn is_admin(&self, id: &DeviceId) -> bool {
        self.members.get(id).is_some_and(Member::is_admin)
    }

    pub fn active_members(&self) -> impl Iterator<Item = &Member> {
        self.members.values().filter(|m| m.active())
    }

    pub fn member_by_vip(&self, vip: Ipv4Addr) -> Option<&Member> {
        self.active_members().find(|m| m.record.vip == Some(vip))
    }

    /// The lowest free host address in the vnet range, skipping network,
    /// broadcast and every vip already assigned (active or removed — a
    /// removed device's address is not reused while stale routes may exist).
    pub fn free_vip(&self) -> Option<Ipv4Addr> {
        let (base, bits) = self.vnet_cidr?;
        if bits >= 31 {
            return None;
        }
        let base = u32::from(base) & (!0u32 << (32 - bits));
        let hosts = 1u32 << (32 - bits);
        let used: HashSet<Ipv4Addr> = self.members.values().filter_map(|m| m.record.vip).collect();
        (1..hosts - 1)
            .map(|offset| Ipv4Addr::from(base + offset))
            .find(|candidate| !used.contains(candidate))
    }

    fn apply(&mut self, entry: &Entry, hash: EntryHash) {
        match &entry.body {
            Body::Genesis {
                name,
                vnet_cidr,
                creator,
            } => {
                self.network_id = Some(hash);
                self.name = name.clone();
                self.vnet_cidr = Some(*vnet_cidr);
                let mut record = creator.clone();
                record.role = Role::Admin;
                self.members.insert(
                    record.device_id,
                    Member {
                        record,
                        removed: false,
                        defined_by: hash,
                        last_update: (entry.lamport, hash),
                        presence_epoch: None,
                    },
                );
            }
            Body::Add(record) => {
                self.members.entry(record.device_id).or_insert(Member {
                    record: record.clone(),
                    removed: false,
                    defined_by: hash,
                    last_update: (entry.lamport, hash),
                    presence_epoch: None,
                });
            }
            Body::Update { device_id, update } => {
                if update.is_presence_epoch()
                    && let Some(member) = self.members.get_mut(device_id)
                    && member
                        .presence_epoch
                        .is_none_or(|old| (entry.lamport, hash) > old)
                {
                    member.presence_epoch = Some((entry.lamport, hash));
                }
                if let Some(member) = self.members.get_mut(device_id)
                    && (entry.lamport, hash) > member.last_update
                {
                    if let Some(name) = &update.name {
                        member.record.name = name.clone();
                    }
                    if let Some(role) = update.role {
                        member.record.role = role;
                    }
                    if let Some(scopes) = &update.scopes {
                        member.record.scopes = scopes.clone();
                    }
                    if let Some(vip) = update.vip {
                        member.record.vip = vip;
                        member.defined_by = hash;
                    }
                    member.last_update = (entry.lamport, hash);
                }
            }
            Body::Remove { device_id } => {
                if let Some(member) = self.members.get_mut(device_id) {
                    member.removed = true;
                }
            }
            Body::Readdress {
                vnet_cidr,
                addresses,
            } => {
                self.vnet_cidr = Some(*vnet_cidr);
                for (id, address) in addresses {
                    if let Some(member) = self.members.get_mut(id)
                        && (entry.lamport, hash) > member.last_update
                    {
                        member.record.vip = Some(*address);
                        member.defined_by = hash;
                        member.last_update = (entry.lamport, hash);
                    }
                }
            }
        }
    }

    fn resolve_vip_conflicts(&mut self) {
        let mut by_vip: HashMap<Ipv4Addr, Vec<(EntryHash, DeviceId)>> = HashMap::new();
        for member in self.members.values().filter(|m| m.active()) {
            if let Some(vip) = member.record.vip {
                by_vip
                    .entry(vip)
                    .or_default()
                    .push((member.defined_by, member.record.device_id));
            }
        }
        for claimants in by_vip.values_mut() {
            if claimants.len() < 2 {
                continue;
            }
            claimants.sort();
            for (_, loser) in &claimants[1..] {
                if let Some(member) = self.members.get_mut(loser) {
                    member.record.vip = None;
                }
            }
        }
    }
}

// --------------------------------------------------------------- roster

/// Every entry we hold, and the state they fold to.
#[derive(Clone, Debug, Default)]
pub struct Roster {
    entries: HashMap<EntryHash, Entry>,
    accepted: Vec<EntryHash>, // topological order
    state: State,
    presence_bindings: HashMap<DeviceId, crate::presence::Binding>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Insert {
    Accepted,
    Duplicate,
}

impl Roster {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_entries(entries: impl IntoIterator<Item = Entry>) -> Result<Self, Error> {
        let mut roster = Self::new();
        let mut pending: Vec<Entry> = entries.into_iter().collect();
        // Insert in dependency order regardless of the order given.
        let mut progress = true;
        while progress && !pending.is_empty() {
            progress = false;
            let mut rest = Vec::new();
            for entry in pending {
                match roster.insert(entry.clone()) {
                    Ok(_) => progress = true,
                    Err(Error::MissingParents(_)) => rest.push(entry),
                    Err(error) => return Err(error),
                }
            }
            pending = rest;
        }
        if let Some(entry) = pending.first() {
            return Err(Error::MissingParents(entry.parents.clone()));
        }
        Ok(roster)
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn verify_presence_binding(&self, binding: &crate::presence::Binding) -> Result<(), Error> {
        let base = self.fold_subset(&self.accepted.iter().copied().collect());
        binding.verify(
            &base,
            self.get(&binding.epoch)
                .filter(|_| self.accepted.contains(&binding.epoch)),
        )
    }

    pub fn install_presence_binding(
        &mut self,
        binding: crate::presence::Binding,
    ) -> Result<(), Error> {
        self.verify_presence_binding(&binding)?;
        self.presence_bindings.insert(binding.device, binding);
        self.recompute();
        Ok(())
    }

    pub fn presence_binding(&self, device: &DeviceId) -> Option<&crate::presence::Binding> {
        self.presence_bindings
            .get(device)
            .filter(|binding| self.verify_presence_binding(binding).is_ok())
    }

    pub fn network_id(&self) -> Option<NetworkId> {
        self.state.network_id
    }

    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.accepted.iter().filter_map(|h| self.entries.get(h))
    }

    pub fn get(&self, hash: &EntryHash) -> Option<&Entry> {
        self.entries.get(hash)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Entries no other entry names as a parent: what a new entry builds on
    /// and what gossip compares.
    pub fn heads(&self) -> Vec<EntryHash> {
        let mut referenced = HashSet::new();
        for entry in self.entries.values() {
            referenced.extend(entry.parents.iter().copied());
        }
        let mut heads: Vec<EntryHash> = self
            .entries
            .keys()
            .filter(|h| !referenced.contains(*h))
            .copied()
            .collect();
        heads.sort();
        heads
    }

    pub fn next_lamport(&self) -> u64 {
        self.entries
            .values()
            .map(|e| e.lamport)
            .max()
            .map_or(0, |l| l + 1)
    }

    /// Entries the peer lacks, given the heads it reported: everything not in
    /// the causal past of those heads, in an order it can insert directly.
    pub fn entries_after(&self, peer_heads: &[EntryHash]) -> Vec<Entry> {
        let known = self.causal_past(peer_heads);
        self.accepted
            .iter()
            .filter(|h| !known.contains(*h))
            .filter_map(|h| self.entries.get(h).cloned())
            .collect()
    }

    fn causal_past(&self, heads: &[EntryHash]) -> HashSet<EntryHash> {
        let mut seen = HashSet::new();
        let mut stack: Vec<EntryHash> = heads.to_vec();
        while let Some(hash) = stack.pop() {
            if !seen.insert(hash) {
                continue;
            }
            if let Some(entry) = self.entries.get(&hash) {
                stack.extend(entry.parents.iter().copied());
            }
        }
        seen
    }

    /// Validates and adds one entry. Entries whose parents are unknown are
    /// refused with `MissingParents` so the caller can fetch them first.
    pub fn insert(&mut self, entry: Entry) -> Result<Insert, Error> {
        let hash = entry.hash();
        if self.entries.contains_key(&hash) {
            return Ok(Insert::Duplicate);
        }
        let missing: Vec<EntryHash> = entry
            .parents
            .iter()
            .filter(|p| !self.entries.contains_key(*p))
            .copied()
            .collect();
        if !missing.is_empty() {
            return Err(Error::MissingParents(missing));
        }
        let is_genesis = matches!(entry.body, Body::Genesis { .. });
        if is_genesis {
            if !self.entries.is_empty() {
                return Err(Error::Rejected("a roster has exactly one genesis".into()));
            }
            if !entry.parents.is_empty() {
                return Err(Error::Rejected("genesis has no parents".into()));
            }
        } else {
            let Some(network_id) = self.state.network_id else {
                return Err(Error::Rejected("no genesis yet".into()));
            };
            if entry.network_id != network_id {
                return Err(Error::Rejected("entry belongs to another network".into()));
            }
            if entry.parents.is_empty() {
                return Err(Error::Rejected("non-genesis entry needs parents".into()));
            }
        }
        self.validate(&entry)?;
        self.entries.insert(hash, entry);
        self.recompute();
        if !self.accepted.contains(&hash) {
            self.entries.remove(&hash);
            self.recompute();
            return Err(Error::Rejected(
                "entry was invalidated by the roster it joined".into(),
            ));
        }
        Ok(Insert::Accepted)
    }

    /// Signature and authority check against the fold of the entry's past.
    fn validate(&self, entry: &Entry) -> Result<(), Error> {
        let past = self.fold_subset(&self.causal_past(&entry.parents));
        let signer_key = match &entry.body {
            Body::Genesis { creator, .. } => {
                if creator.device_id != entry.signer {
                    return Err(Error::Rejected(
                        "genesis must be signed by its creator".into(),
                    ));
                }
                creator.public_key.clone()
            }
            _ => {
                let member = past
                    .members
                    .get(&entry.signer)
                    .ok_or_else(|| Error::Rejected("signer is not a member".into()))?;
                if !member.is_admin() {
                    return Err(Error::Rejected("signer is not an active admin".into()));
                }
                member.record.public_key.clone()
            }
        };
        verify_domain(
            &signer_key,
            ROSTER_DOMAIN,
            &entry.signed_bytes(),
            &entry.signature,
        )?;
        match &entry.body {
            Body::Genesis { vnet_cidr, .. } => {
                if vnet_cidr.1 < 8 || vnet_cidr.1 > 30 {
                    return Err(Error::Rejected("vnet prefix must be /8 … /30".into()));
                }
            }
            Body::Add(record) => {
                if past.members.contains_key(&record.device_id) {
                    return Err(Error::Rejected(
                        "device already in roster (removed ids are never re-added)".into(),
                    ));
                }
            }
            Body::Update { device_id, .. } => {
                if past.active(device_id).is_none() {
                    return Err(Error::Rejected("update names no active member".into()));
                }
            }
            Body::Remove { device_id } => {
                if past.active(device_id).is_none() {
                    return Err(Error::Rejected("remove names no active member".into()));
                }
            }
            Body::Readdress {
                vnet_cidr,
                addresses,
            } => {
                validate_address_plan(&past, *vnet_cidr, addresses)?;
            }
        }
        Ok(())
    }

    fn topo_order(&self, subset: &HashSet<EntryHash>) -> Vec<EntryHash> {
        let mut indegree: HashMap<EntryHash, usize> = HashMap::new();
        let mut children: HashMap<EntryHash, Vec<EntryHash>> = HashMap::new();
        for hash in subset {
            let entry = &self.entries[hash];
            let parents: Vec<EntryHash> = entry
                .parents
                .iter()
                .filter(|p| subset.contains(*p))
                .copied()
                .collect();
            indegree.insert(*hash, parents.len());
            for parent in parents {
                children.entry(parent).or_default().push(*hash);
            }
        }
        let key = |h: &EntryHash| (self.entries[h].lamport, *h);
        let mut ready: BTreeSet<(u64, EntryHash)> = indegree
            .iter()
            .filter(|(_, d)| **d == 0)
            .map(|(h, _)| key(h))
            .collect();
        let mut order = Vec::with_capacity(subset.len());
        while let Some(next) = ready.iter().next().copied() {
            ready.remove(&next);
            order.push(next.1);
            for child in children.get(&next.1).into_iter().flatten() {
                let d = indegree.get_mut(child).unwrap();
                *d -= 1;
                if *d == 0 {
                    ready.insert(key(child));
                }
            }
        }
        order
    }

    fn fold_subset(&self, subset: &HashSet<EntryHash>) -> State {
        let mut state = State::default();
        for hash in self.topo_order(subset) {
            state.apply(&self.entries[&hash], hash);
        }
        state.resolve_vip_conflicts();
        state
    }

    /// Recomputes the accepted set from scratch: cheap at roster sizes of
    /// tens of entries, and the only way to apply the removed-admin rule.
    fn recompute(&mut self) {
        let mut excluded: HashSet<EntryHash> = HashSet::new();
        loop {
            let candidates: HashSet<EntryHash> = self
                .entries
                .keys()
                .filter(|h| !excluded.contains(*h))
                .copied()
                .collect();
            let order = self.topo_order(&candidates);
            let mut accepted: Vec<EntryHash> = Vec::new();
            let mut accepted_set: HashSet<EntryHash> = HashSet::new();
            for hash in order {
                let entry = &self.entries[&hash];
                if !entry.parents.iter().all(|p| accepted_set.contains(p)) {
                    continue;
                }
                let past = self.fold_subset(
                    &accepted_set
                        .intersection(&self.causal_past(&entry.parents))
                        .copied()
                        .collect(),
                );
                let ok = match &entry.body {
                    Body::Genesis { creator, .. } => {
                        accepted.is_empty()
                            && creator.device_id == entry.signer
                            && verify_domain(
                                &creator.public_key,
                                ROSTER_DOMAIN,
                                &entry.signed_bytes(),
                                &entry.signature,
                            )
                            .is_ok()
                    }
                    _ => past.members.get(&entry.signer).is_some_and(|m| {
                        m.is_admin()
                            && verify_domain(
                                &m.record.public_key,
                                ROSTER_DOMAIN,
                                &entry.signed_bytes(),
                                &entry.signature,
                            )
                            .is_ok()
                    }),
                };
                if ok {
                    accepted.push(hash);
                    accepted_set.insert(hash);
                }
            }
            // Removed-admin rule: an accepted Remove(X) invalidates every entry
            // X signed that the Remove does not causally include.
            let mut newly_excluded = HashSet::new();
            for hash in &accepted {
                if let Body::Remove { device_id } = &self.entries[hash].body {
                    let past = self.causal_past(&[*hash]);
                    for other in &accepted {
                        if other != hash
                            && self.entries[other].signer == *device_id
                            && !past.contains(other)
                        {
                            newly_excluded.insert(*other);
                        }
                    }
                }
            }
            if newly_excluded.is_empty() {
                self.accepted = accepted;
                self.state = self.fold_subset(&accepted_set);
                for binding in self.presence_bindings.values() {
                    if binding
                        .verify(
                            &self.state,
                            self.entries
                                .get(&binding.epoch)
                                .filter(|_| accepted_set.contains(&binding.epoch)),
                        )
                        .is_ok()
                        && let Some(member) = self.state.members.get_mut(&binding.device)
                    {
                        member.record.presence_key = Some(binding.key.clone());
                    }
                }
                return;
            }
            excluded.extend(newly_excluded);
        }
    }
}

// -------------------------------------------------------------- builders

/// Signs entries on top of a roster with this device's key.
pub struct Author<'a> {
    pub roster: &'a Roster,
    pub signer: &'a dyn HwSigner,
}

impl Author<'_> {
    fn finish(
        &self,
        body: Body,
        network_id: NetworkId,
        parents: Vec<EntryHash>,
    ) -> Result<Entry, Error> {
        let mut entry = Entry {
            network_id,
            parents,
            lamport: self.roster.next_lamport(),
            body,
            signer: self.signer.public_key().device_id(),
            signature: Vec::new(),
        };
        entry.signature = sign_domain(self.signer, ROSTER_DOMAIN, &entry.signed_bytes())?;
        if matches!(entry.body, Body::Genesis { .. }) {
            entry.network_id = entry.hash();
        }
        Ok(entry)
    }

    fn on_head(&self, body: Body) -> Result<Entry, Error> {
        let network_id = self
            .roster
            .network_id()
            .ok_or_else(|| Error::Rejected("no network yet".into()))?;
        self.finish(body, network_id, self.roster.heads())
    }

    pub fn genesis(
        &self,
        name: &str,
        vnet_cidr: (Ipv4Addr, u8),
        mut creator: DeviceRecord,
    ) -> Result<Entry, Error> {
        if !self.roster.is_empty() {
            return Err(Error::Rejected("roster already has a genesis".into()));
        }
        creator.role = Role::Admin;
        if creator.kind != DeviceKind::Phone && creator.vip.is_none() {
            creator.vip = State {
                vnet_cidr: Some(vnet_cidr),
                ..State::default()
            }
            .free_vip();
        }
        self.finish(
            Body::Genesis {
                name: name.to_string(),
                vnet_cidr,
                creator,
            },
            EntryHash([0; 32]),
            Vec::new(),
        )
    }

    /// Adds a device; desktops and servers get the next free vip unless the
    /// record already names one.
    pub fn add(&self, mut record: DeviceRecord) -> Result<Entry, Error> {
        if record.kind != DeviceKind::Phone && record.vip.is_none() {
            record.vip = self.roster.state().free_vip();
        }
        self.on_head(Body::Add(record))
    }

    pub fn update(&self, device_id: DeviceId, update: DeviceUpdate) -> Result<Entry, Error> {
        self.on_head(Body::Update { device_id, update })
    }

    pub fn remove(&self, device_id: DeviceId) -> Result<Entry, Error> {
        self.on_head(Body::Remove { device_id })
    }

    pub fn readdress(
        &self,
        vnet_cidr: (Ipv4Addr, u8),
        addresses: BTreeMap<DeviceId, Ipv4Addr>,
    ) -> Result<Entry, Error> {
        validate_address_plan(self.roster.state(), vnet_cidr, &addresses)?;
        self.on_head(Body::Readdress {
            vnet_cidr,
            addresses,
        })
    }
}

fn validate_address_plan(
    state: &State,
    (base, bits): (Ipv4Addr, u8),
    addresses: &BTreeMap<DeviceId, Ipv4Addr>,
) -> Result<(), Error> {
    if !(8..=30).contains(&bits) {
        return Err(Error::Rejected("vnet prefix must be /8 … /30".into()));
    }
    let mask = !0u32 << (32 - bits);
    let network = u32::from(base);
    if network & mask != network {
        return Err(Error::Rejected("vnet CIDR must be canonical".into()));
    }
    let expected: BTreeSet<_> = state
        .active_members()
        .filter(|m| m.record.kind != DeviceKind::Phone)
        .map(|m| m.record.device_id)
        .collect();
    if addresses.keys().copied().collect::<BTreeSet<_>>() != expected {
        return Err(Error::Rejected(
            "address plan must include exactly the active desktop/server members".into(),
        ));
    }
    let mut used = BTreeSet::new();
    for address in addresses.values() {
        let ip = u32::from(*address);
        if ip & mask != network || ip == network || ip == network | !mask || !used.insert(*address)
        {
            return Err(Error::Rejected(
                "address plan contains a duplicate or invalid host address".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::SoftSigner;

    fn record(signer: &SoftSigner, name: &str, kind: DeviceKind) -> DeviceRecord {
        DeviceRecord::new(signer.public_key(), name, kind)
    }

    fn cidr() -> (Ipv4Addr, u8) {
        (Ipv4Addr::new(192, 168, 14, 0), 24)
    }

    #[test]
    fn readdress_is_signed_atomic_and_preserves_membership() {
        let admin = SoftSigner::generate();
        let member = SoftSigner::generate();
        let phone = SoftSigner::generate();
        let mut roster = Roster::new();
        let genesis = Author {
            roster: &roster,
            signer: &admin,
        }
        .genesis("home", cidr(), record(&admin, "admin", DeviceKind::Desktop))
        .unwrap();
        roster.insert(genesis).unwrap();
        for (signer, kind) in [(&member, DeviceKind::Desktop), (&phone, DeviceKind::Phone)] {
            let entry = Author {
                roster: &roster,
                signer: &admin,
            }
            .add(record(signer, "peer", kind))
            .unwrap();
            roster.insert(entry).unwrap();
        }
        let before = roster.state().clone();
        let plan = BTreeMap::from([
            (
                admin.public_key().device_id(),
                "192.168.13.11".parse().unwrap(),
            ),
            (
                member.public_key().device_id(),
                "192.168.13.12".parse().unwrap(),
            ),
        ]);
        let target = ("192.168.13.0".parse().unwrap(), 24);
        let entry = Author {
            roster: &roster,
            signer: &admin,
        }
        .readdress(target, plan.clone())
        .unwrap();
        assert_eq!(Entry::decode(&entry.encode()).unwrap(), entry);
        let unauthorized = Author {
            roster: &roster,
            signer: &member,
        }
        .readdress(target, plan.clone())
        .unwrap();
        assert!(roster.insert(unauthorized).is_err());
        for bad in [
            BTreeMap::new(),
            BTreeMap::from([
                (
                    admin.public_key().device_id(),
                    "192.168.13.11".parse().unwrap(),
                ),
                (
                    member.public_key().device_id(),
                    "192.168.13.11".parse().unwrap(),
                ),
            ]),
        ] {
            assert!(
                Author {
                    roster: &roster,
                    signer: &admin
                }
                .readdress(target, bad)
                .is_err()
            );
        }
        for invalid in ["192.168.13.0", "192.168.13.255", "192.168.14.12"] {
            let mut bad = plan.clone();
            bad.insert(member.public_key().device_id(), invalid.parse().unwrap());
            assert!(
                Author {
                    roster: &roster,
                    signer: &admin
                }
                .readdress(target, bad)
                .is_err()
            );
        }
        let mut tampered = entry.clone();
        if let Body::Readdress { addresses, .. } = &mut tampered.body {
            addresses.insert(
                member.public_key().device_id(),
                "192.168.13.22".parse().unwrap(),
            );
        }
        assert!(roster.insert(tampered).is_err());
        roster.insert(entry.clone()).unwrap();
        assert_eq!(roster.network_id(), before.network_id);
        assert_eq!(roster.state().vnet_cidr, Some(target));
        for (id, old) in &before.members {
            let new = &roster.state().members[id].record;
            assert_eq!(new.public_key, old.record.public_key);
            assert_eq!(new.presence_key, old.record.presence_key);
            assert_eq!(new.scopes, old.record.scopes);
            assert_eq!(new.role, old.record.role);
            assert_eq!(new.vip, plan.get(id).copied());
        }
        assert_eq!(roster.insert(entry).unwrap(), Insert::Duplicate);
    }

    #[test]
    fn genesis_add_update_remove() {
        let admin = SoftSigner::generate();
        let phone = SoftSigner::generate();
        let mut roster = Roster::new();
        let genesis = Author {
            roster: &roster,
            signer: &admin,
        }
        .genesis("home", cidr(), record(&admin, "mac-a", DeviceKind::Desktop))
        .unwrap();
        assert_eq!(roster.insert(genesis.clone()).unwrap(), Insert::Accepted);
        assert_eq!(roster.network_id(), Some(genesis.hash()));
        let creator = roster
            .state()
            .member(&admin.public_key().device_id())
            .unwrap();
        assert!(creator.is_admin());
        assert_eq!(creator.record.vip, Some(Ipv4Addr::new(192, 168, 14, 1)));

        let mut phone_record = record(&phone, "iphone", DeviceKind::Phone);
        phone_record.scopes.insert(Scope::AgentView);
        let add = Author {
            roster: &roster,
            signer: &admin,
        }
        .add(phone_record)
        .unwrap();
        roster.insert(add).unwrap();
        let phone_id = phone.public_key().device_id();
        assert!(roster.state().active(&phone_id).is_some());
        assert_eq!(roster.state().active(&phone_id).unwrap().record.vip, None);

        // A member cannot sign.
        let bad = Author {
            roster: &roster,
            signer: &phone,
        }
        .update(
            phone_id,
            DeviceUpdate {
                role: Some(Role::Admin),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(matches!(roster.insert(bad), Err(Error::Rejected(_))));

        let promote = Author {
            roster: &roster,
            signer: &admin,
        }
        .update(
            phone_id,
            DeviceUpdate {
                role: Some(Role::Admin),
                ..Default::default()
            },
        )
        .unwrap();
        roster.insert(promote).unwrap();
        assert!(roster.state().is_admin(&phone_id));

        let remove = Author {
            roster: &roster,
            signer: &phone,
        }
        .remove(admin.public_key().device_id())
        .unwrap();
        roster.insert(remove).unwrap();
        assert!(
            roster
                .state()
                .active(&admin.public_key().device_id())
                .is_none()
        );
        // The removed admin can no longer add anyone.
        let late = Author {
            roster: &roster,
            signer: &admin,
        }
        .add(record(&SoftSigner::generate(), "x", DeviceKind::Server))
        .unwrap();
        assert!(matches!(roster.insert(late), Err(Error::Rejected(_))));
        // Removed ids are never re-added.
        let readd = Author {
            roster: &roster,
            signer: &phone,
        }
        .add(record(&admin, "mac-a-again", DeviceKind::Desktop))
        .unwrap();
        assert!(matches!(roster.insert(readd), Err(Error::Rejected(_))));
    }

    #[test]
    fn wire_round_trip_and_tamper() {
        let admin = SoftSigner::generate();
        let roster = Roster::new();
        let genesis = Author {
            roster: &roster,
            signer: &admin,
        }
        .genesis("home", cidr(), record(&admin, "mac-a", DeviceKind::Desktop))
        .unwrap();
        let decoded = Entry::decode(&genesis.encode()).unwrap();
        assert_eq!(decoded, genesis);
        assert_eq!(decoded.hash(), genesis.hash());
        let mut bytes = genesis.encode();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        let tampered = Entry::decode(&bytes).unwrap();
        assert!(Roster::new().insert(tampered).is_err());
    }

    /// Module and provider scopes, mixed with the older ones, survive the
    /// canonical re-encoding a decoder insists on.
    #[test]
    fn module_scopes_round_trip_in_an_update() {
        let admin = SoftSigner::generate();
        let phone = SoftSigner::generate();
        let mut roster = Roster::new();
        let genesis = Author {
            roster: &roster,
            signer: &admin,
        }
        .genesis("home", cidr(), record(&admin, "mac-a", DeviceKind::Desktop))
        .unwrap();
        roster.insert(genesis).unwrap();
        let add = Author {
            roster: &roster,
            signer: &admin,
        }
        .add(record(&phone, "iphone", DeviceKind::Phone))
        .unwrap();
        roster.insert(add).unwrap();
        let scopes: BTreeSet<Scope> = [
            "terminals",
            "agents:codex",
            "agent.view",
            "chats:claude",
            "agents",
            "locked.control",
            "net:tcp/22",
            "agents:not-yet-known",
            "chats",
        ]
        .iter()
        .map(|s| s.parse().unwrap())
        .collect();
        let update = Author {
            roster: &roster,
            signer: &admin,
        }
        .update(
            phone.public_key().device_id(),
            DeviceUpdate {
                scopes: Some(scopes.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        let decoded = Entry::decode(&update.encode()).unwrap();
        assert_eq!(decoded, update);
        roster.insert(decoded).unwrap();
        assert_eq!(
            roster
                .state()
                .active(&phone.public_key().device_id())
                .unwrap()
                .record
                .scopes,
            scopes
        );
    }

    #[test]
    fn concurrent_admins_and_stolen_key_backfill() {
        let a = SoftSigner::generate();
        let b = SoftSigner::generate();
        let mut roster = Roster::new();
        let genesis = Author {
            roster: &roster,
            signer: &a,
        }
        .genesis("home", cidr(), record(&a, "a", DeviceKind::Desktop))
        .unwrap();
        roster.insert(genesis).unwrap();
        let mut b_record = record(&b, "b", DeviceKind::Desktop);
        b_record.role = Role::Admin;
        let add_b = Author {
            roster: &roster,
            signer: &a,
        }
        .add(b_record)
        .unwrap();
        roster.insert(add_b).unwrap();
        assert_eq!(
            roster
                .state()
                .active(&b.public_key().device_id())
                .unwrap()
                .record
                .vip,
            Some(Ipv4Addr::new(192, 168, 14, 2))
        );

        // Two admins add different devices concurrently (same parents).
        let mut fork_a = roster.clone();
        let mut fork_b = roster.clone();
        let c = SoftSigner::generate();
        let d = SoftSigner::generate();
        let add_c = Author {
            roster: &fork_a,
            signer: &a,
        }
        .add(record(&c, "c", DeviceKind::Server))
        .unwrap();
        let add_d = Author {
            roster: &fork_b,
            signer: &b,
        }
        .add(record(&d, "d", DeviceKind::Server))
        .unwrap();
        fork_a.insert(add_c.clone()).unwrap();
        fork_b.insert(add_d.clone()).unwrap();
        // Both allocated the same vip; after merge only one keeps it.
        assert_eq!(
            add_c
                .device_id()
                .and_then(|id| fork_a.state().active(&id).unwrap().record.vip),
            Some(Ipv4Addr::new(192, 168, 14, 3))
        );
        fork_a.insert(add_d.clone()).unwrap();
        fork_b.insert(add_c.clone()).unwrap();
        assert_eq!(fork_a.state(), fork_b.state());
        let vips: Vec<Option<Ipv4Addr>> = [c.public_key().device_id(), d.public_key().device_id()]
            .iter()
            .map(|id| fork_a.state().active(id).unwrap().record.vip)
            .collect();
        assert_eq!(vips.iter().filter(|v| v.is_some()).count(), 1);
        assert_eq!(fork_a.heads().len(), 2);

        // b's key is stolen: a removes b. Anything b signs that the Remove
        // does not include is invalid, even if it claims an earlier lamport.
        let mut merged = fork_a.clone();
        let remove_b = Author {
            roster: &merged,
            signer: &a,
        }
        .remove(b.public_key().device_id())
        .unwrap();
        merged.insert(remove_b).unwrap();
        let e = SoftSigner::generate();
        let backfill = Author {
            roster: &fork_b,
            signer: &b,
        }
        .add(record(&e, "e", DeviceKind::Server))
        .unwrap();
        assert!(matches!(merged.insert(backfill), Err(Error::Rejected(_))));
        assert!(merged.state().active(&e.public_key().device_id()).is_none());
        assert_eq!(merged.heads().len(), 1);
    }

    #[test]
    fn from_entries_any_order() {
        let a = SoftSigner::generate();
        let mut roster = Roster::new();
        let genesis = Author {
            roster: &roster,
            signer: &a,
        }
        .genesis("home", cidr(), record(&a, "a", DeviceKind::Desktop))
        .unwrap();
        roster.insert(genesis.clone()).unwrap();
        let add = Author {
            roster: &roster,
            signer: &a,
        }
        .add(record(&SoftSigner::generate(), "p", DeviceKind::Phone))
        .unwrap();
        roster.insert(add.clone()).unwrap();
        let rebuilt = Roster::from_entries(vec![add, genesis]).unwrap();
        assert_eq!(rebuilt.state(), roster.state());
        assert_eq!(rebuilt.entries_after(&roster.heads()).len(), 0);
        assert_eq!(rebuilt.entries_after(&[]).len(), 2);
    }
}
