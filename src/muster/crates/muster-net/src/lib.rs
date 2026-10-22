//! muster-net: the one link between two devices (docs/ARCHITECTURE.md §3.5).
//! A QUIC connection per peer over a socket that picks direct or relay per
//! datagram; service streams by name; roster gossip; forward streams; and,
//! on desktops, the vnet over QUIC DATAGRAMs.

pub mod doh;
pub mod forward;
pub mod gossip;
pub mod heartbeat;
mod interfaces;
pub mod magicsock;
pub mod node;
pub mod pair;
pub mod presence;
pub mod reconnect;
pub mod relay_proto;
pub mod relay_tls;
pub mod secure_relay;
pub mod stream;
mod system_dns;
pub mod tls;
#[cfg(feature = "vnet")]
pub mod vnet;

pub use heartbeat::Build;
pub use node::{Node, NodeConfig, PeerInfo, RelayConfig, ServiceHandler, SharedPolicy};
pub use tls::SharedRoster;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("tls: {0}")]
    Tls(String),
    #[error("io: {0}")]
    Io(String),
    #[error("protocol: {0}")]
    Protocol(String),
    #[error("timeout: {0}")]
    Timeout(&'static str),
    #[error("not authorized: {0}")]
    Unauthorized(String),
    /// The member is offline (the relay's word, or its bounded reconnects
    /// failed): nothing was sent and no dial was made.
    #[error("offline: {0}")]
    Offline(String),
    #[error(transparent)]
    Core(#[from] muster_core::Error),
    #[error(transparent)]
    Connect(#[from] quinn::ConnectError),
    #[error(transparent)]
    Connection(#[from] quinn::ConnectionError),
}
