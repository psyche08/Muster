//! muster-core: the trust model of the rebuilt network (docs/ARCHITECTURE.md
//! §3.4). No I/O, no async, no platform code: a device key behind a trait, a
//! signed member roster, scopes, the local policy, the pairing invite, and the
//! desktop apps whose sessions members may be granted.

pub mod apps;
pub mod canonical;
pub mod identity;
pub mod invite;
pub mod locked_use;
pub mod policy;
pub mod presence;
pub mod roster;
pub mod scope;

pub use identity::{DeviceId, HwSigner, PublicKey, SharedSigner, SoftSigner};
pub use invite::Invite;
pub use policy::{
    Decision, LocalPolicy, Permission, PortRule, TargetState, authorize, authorize_port,
};
pub use roster::{
    Author, Body, DeviceKind, DeviceRecord, DeviceUpdate, Entry, EntryHash, Member, NetworkId,
    Role, Roster, State,
};
pub use scope::{Proto, Scope};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("identity: {0}")]
    Identity(String),
    #[error("signature: {0}")]
    Signature(String),
    #[error("truncated input")]
    Truncated,
    #[error("malformed: {0}")]
    Malformed(String),
    #[error("entry rejected: {0}")]
    Rejected(String),
    #[error("missing parents")]
    MissingParents(Vec<EntryHash>),
}
