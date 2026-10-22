//! Forward streams (ARCHITECTURE §3.5): a member asks the target to connect
//! it to one of the target's own loopback ports. The same `net:` scope and
//! open-port policy the vnet packet filter uses decide it. Phones reach
//! SSH this way; nothing is ever forwarded on to a third machine.

use muster_core::DeviceId;
use muster_core::canonical::{Reader, Writer};
use muster_core::policy::{Decision, authorize_port};
use muster_core::scope::Proto;
use quinn::{RecvStream, SendStream};

use crate::Error;
use crate::node::Node;
use crate::stream::{read_message, write_message};

pub const SERVICE: &str = "forward";
pub const VERSION: u32 = 1;

fn encode_request(port: u16) -> Vec<u8> {
    let mut w = Writer::new();
    w.u8(1).u16(port); // proto 1 = tcp; udp forwarding is not offered
    w.finish()
}

fn decode_request(bytes: &[u8]) -> Result<u16, Error> {
    let mut r = Reader::new(bytes);
    if r.u8()? != 1 {
        return Err(Error::Protocol("only tcp forwards exist".into()));
    }
    let port = r.u16()?;
    r.finish()?;
    Ok(port)
}

/// Target side. Decides, dials loopback, then copies bytes both ways.
pub async fn serve(
    node: &std::sync::Arc<Node>,
    peer: DeviceId,
    mut send: SendStream,
    mut recv: RecvStream,
) {
    let result: Result<(), Error> = async {
        let request = read_message(&mut recv, 64)
            .await?
            .ok_or_else(|| Error::Protocol("no forward request".into()))?;
        let port = decode_request(&request)?;
        let decision = {
            let roster = node.roster().read().unwrap();
            let policy = node.policy().read().unwrap();
            authorize_port(roster.state(), &peer, &policy, Proto::Tcp, port)
        };
        if decision != Decision::Allow {
            let reason = match decision {
                Decision::Deny(reason) => reason,
                _ => "presence required",
            };
            log::info!(
                "forward {}:{port} denied for {}: {reason}",
                "127.0.0.1",
                peer.short()
            );
            write_message(&mut send, &[1u8]).await?;
            let _ = send.finish();
            return Ok(());
        }
        let target = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        let mut tcp = match tokio::net::TcpStream::connect(target).await {
            Ok(tcp) => tcp,
            Err(error) => {
                log::info!("forward {target} for {}: {error}", peer.short());
                write_message(&mut send, &[2u8]).await?;
                let _ = send.finish();
                return Ok(());
            }
        };
        write_message(&mut send, &[0u8]).await?;
        let mut stream = tokio::io::join(recv, send);
        let _ = tokio::io::copy_bidirectional(&mut tcp, &mut stream).await;
        Ok(())
    }
    .await;
    if let Err(error) = result {
        log::debug!("forward stream for {}: {error}", peer.short());
    }
}

/// Accessor side: returns the raw byte stream to the target's port.
pub async fn open(
    node: &std::sync::Arc<Node>,
    peer: DeviceId,
    port: u16,
) -> Result<(SendStream, RecvStream), Error> {
    let (mut send, mut recv) = node.open(peer, SERVICE, VERSION).await?;
    write_message(&mut send, &encode_request(port)).await?;
    let status = read_message(&mut recv, 8)
        .await?
        .ok_or_else(|| Error::Protocol("target closed the forward stream".into()))?;
    match status.first() {
        Some(0) => Ok((send, recv)),
        Some(1) => Err(Error::Unauthorized(format!(
            "port {port} not granted or not open on target"
        ))),
        Some(2) => Err(Error::Io(format!(
            "target could not connect to its port {port}"
        ))),
        _ => Err(Error::Protocol("bad forward status".into())),
    }
}
