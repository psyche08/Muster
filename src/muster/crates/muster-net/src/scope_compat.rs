//! Content-free roster decoder probe on an authenticated member link.
//! Phones answer on their existing link; no general application service opens.
use crate::{
    Error, Node,
    stream::{read_message, write_message},
};
use muster_core::{
    DeviceId, Scope,
    canonical::{Reader, Writer},
};
use quinn::{RecvStream, SendStream};
use std::sync::Arc;

pub const SERVICE: &str = "roster-scopes";
pub const VERSION: u32 = 1;
pub const NETWORK_ACCESS_VERSION: u32 = 2;
const NETWORK_ACCESS_PROBE: &[u8] = b"MUSTER-NETWORK-ACCESS-MODE-V1";

pub fn supports(version: u32) -> bool {
    matches!(version, VERSION | NETWORK_ACCESS_VERSION)
}
const MAX: usize = 1024;

fn encode(scopes: &[Scope]) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.str("MUSTER-SCOPE-COMPAT-V1");
    for scope in scopes {
        writer.str(&scope.to_string());
    }
    writer.finish()
}
fn decode(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    let mut reader = Reader::new(bytes);
    if reader.str()? != "MUSTER-SCOPE-COMPAT-V1" {
        return Err(Error::Protocol("scope probe domain".into()));
    }
    // The protocol asks exactly these two appended scopes. It does not accept
    // arbitrary configuration or expose roster/member metadata.
    let first: Scope = reader.str()?.parse()?;
    let second: Scope = reader.str()?.parse()?;
    reader.finish()?;
    let mut scopes = vec![first, second];
    scopes.sort();
    if scopes != [Scope::MulticaView, Scope::MulticaControl] {
        return Err(Error::Protocol("unsupported scope probe".into()));
    }
    Ok(encode(&scopes))
}

pub async fn probe(node: &Arc<Node>, peer: DeviceId) -> Result<(), Error> {
    let request = encode(&[Scope::MulticaView, Scope::MulticaControl]);
    probe_supported(node, peer, VERSION, &request).await
}

pub async fn probe_network_access(node: &Arc<Node>, peer: DeviceId) -> Result<(), Error> {
    probe_supported(node, peer, NETWORK_ACCESS_VERSION, NETWORK_ACCESS_PROBE).await
}

async fn probe_supported(
    node: &Arc<Node>,
    peer: DeviceId,
    version: u32,
    request: &[u8],
) -> Result<(), Error> {
    let connection = match node.connection_to(&peer) {
        Some(connection) => connection,
        None => node.connect(peer).await?, // phones cannot be dialed
    };
    let (mut send, mut recv) = node.open_on(&connection, SERVICE, version).await?;
    write_message(&mut send, request).await?;
    let _ = send.finish();
    let response = read_message(&mut recv, MAX).await?;
    if response.as_deref() != Some(request)
        || !node
            .connection_to(&peer)
            .is_some_and(|current| current.stable_id() == connection.stable_id())
    {
        return Err(Error::Protocol(
            "member scope compatibility was not established".into(),
        ));
    }
    Ok(())
}

pub(crate) async fn serve(version: u32, mut send: SendStream, mut recv: RecvStream) {
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let bytes = read_message(&mut recv, MAX)
            .await?
            .ok_or_else(|| Error::Protocol("empty scope probe".into()))?;
        match version {
            VERSION => write_message(&mut send, &decode(&bytes)?).await,
            NETWORK_ACCESS_VERSION if bytes == NETWORK_ACCESS_PROBE => {
                write_message(&mut send, NETWORK_ACCESS_PROBE).await
            }
            _ => Err(Error::Protocol(
                "invalid network access compatibility probe".into(),
            )),
        }
    })
    .await;
    let _ = send.finish();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn probe_proves_canonical_order_and_rejects_other_or_trailing_data() {
        let expected = encode(&[Scope::MulticaView, Scope::MulticaControl]);
        assert_eq!(decode(&expected).unwrap(), expected);
        assert_eq!(
            decode(&encode(&[Scope::MulticaControl, Scope::MulticaView])).unwrap(),
            expected
        );
        assert!(decode(&encode(&[Scope::Agents, Scope::Terminals])).is_err());
        let mut trailing = expected.clone();
        trailing.push(0);
        assert!(decode(&trailing).is_err());
        let mut unknown = Writer::new();
        unknown
            .str("MUSTER-SCOPE-COMPAT-V1")
            .str("multica.future")
            .str("multica.view");
        assert!(decode(&unknown.finish()).is_err());
    }
}
