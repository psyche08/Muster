//! Heartbeat: every member link exchanges which build each device runs, when
//! it comes up and then every [`INTERVAL`]. Display only: a build is never an
//! authorization, trust or compatibility input.

use std::sync::{Arc, Weak};
use std::time::Duration;

use muster_core::canonical::{Reader, Writer};
use muster_core::identity::DeviceId;
use quinn::{Connection, RecvStream, SendStream};

use crate::Error;
use crate::node::Node;
use crate::stream::{read_message, write_message};

pub const SERVICE: &str = "heartbeat";
pub const VERSION: u32 = 1;
pub const INTERVAL: Duration = Duration::from_secs(60);
/// While a link is still being admitted (pairing) the peer refuses members'
/// services; ask again shortly.
const ADMISSION_RETRY: Duration = Duration::from_secs(2);
const MAX_MESSAGE: usize = 512;
const MAX_FIELD: usize = 64;

/// What a device runs: its release version and the source commit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Build {
    pub version: String,
    pub commit: String,
}

impl Build {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.str(&self.version).str(&self.commit);
        w.finish()
    }

    /// Later versions may append fields; they are ignored here.
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(bytes);
        Ok(Self {
            version: field(r.str()?),
            commit: field(r.str()?),
        })
    }
}

/// A peer's text is shown as is: bounded, without control characters.
fn field(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(MAX_FIELD)
        .collect::<String>()
        .trim()
        .to_string()
}

/// One beat: our build out, theirs in. Symmetric, so the same function
/// serves both the opener and the acceptor.
pub async fn exchange(
    node: &Arc<Node>,
    peer: DeviceId,
    send: &mut SendStream,
    recv: &mut RecvStream,
) -> Result<(), Error> {
    write_message(send, &node.build().encode()).await?;
    let theirs = read_message(recv, MAX_MESSAGE)
        .await?
        .ok_or_else(|| Error::Protocol("peer closed before its heartbeat".into()))?;
    node.record_build(peer, Build::decode(&theirs)?);
    Ok(())
}

pub async fn serve(node: &Arc<Node>, peer: DeviceId, mut send: SendStream, mut recv: RecvStream) {
    if let Err(error) = exchange(node, peer, &mut send, &mut recv).await {
        log::debug!("heartbeat from {}: {error}", peer.short());
    }
    let _ = send.finish();
}

/// Beats on one link until it closes or is replaced. A peer that does not
/// know the service closes the first beat unanswered; that link is not asked
/// again.
pub(crate) async fn run(node: Weak<Node>, peer: DeviceId, connection: Connection) {
    loop {
        let Some(node) = node.upgrade() else { return };
        if !node.is_current(&peer, &connection) {
            return;
        }
        let beat = async {
            let (mut send, mut recv) = node.open_on(&connection, SERVICE, VERSION).await?;
            let result = exchange(&node, peer, &mut send, &mut recv).await;
            let _ = send.finish();
            result
        };
        let next = match beat.await {
            Ok(()) => INTERVAL,
            Err(Error::Unauthorized(_)) => ADMISSION_RETRY,
            Err(error) => {
                log::debug!("heartbeat to {}: {error}", peer.short());
                return;
            }
        };
        drop(node);
        tokio::time::sleep(next).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_round_trip_bounded_and_without_control_characters() {
        let build = Build {
            version: "1.0.20".into(),
            commit: "15fc1359".into(),
        };
        assert_eq!(Build::decode(&build.encode()).unwrap(), build);
        let hostile = Build {
            version: format!("1.0\u{1b}[31m{}", "9".repeat(200)),
            commit: "abc\ndef".into(),
        };
        let decoded = Build::decode(&hostile.encode()).unwrap();
        assert_eq!(decoded.version.chars().count(), MAX_FIELD);
        assert!(!decoded.version.contains('\u{1b}'));
        assert_eq!(decoded.commit, "abcdef");
        let mut extended = build.encode();
        let mut future = Writer::new();
        future.str("future");
        extended.extend_from_slice(&future.finish());
        assert_eq!(Build::decode(&extended).unwrap(), build);
        assert!(Build::decode(&[0, 0]).is_err());
    }
}
