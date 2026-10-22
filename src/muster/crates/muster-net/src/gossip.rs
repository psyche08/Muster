//! Roster gossip: on every new link both sides compare heads and send what
//! the other lacks. New entries a device authors or learns are pushed to
//! every connected peer, and membership is re-enforced on each change so a
//! removed device is cut off within the two-second budget.

use muster_core::canonical::{Reader, Writer};
use muster_core::roster::{Entry, EntryHash};
use quinn::{RecvStream, SendStream};

use crate::Error;
use crate::node::Node;
use crate::stream::{MAX_MESSAGE, read_message, write_message};

pub const SERVICE: &str = "roster";
pub const VERSION: u32 = 1;

fn encode_heads(heads: &[EntryHash]) -> Vec<u8> {
    let mut w = Writer::new();
    w.u16(heads.len() as u16);
    for head in heads {
        w.fixed(&head.0);
    }
    w.finish()
}

fn decode_heads(bytes: &[u8]) -> Result<Vec<EntryHash>, Error> {
    let mut r = Reader::new(bytes);
    let count = r.u16()?;
    let mut heads = Vec::with_capacity(count as usize);
    for _ in 0..count {
        heads.push(EntryHash(r.fixed()?));
    }
    r.finish()?;
    Ok(heads)
}

fn encode_entries(entries: &[Entry]) -> Vec<u8> {
    let mut w = Writer::new();
    w.u16(entries.len() as u16);
    for entry in entries {
        w.bytes(&entry.encode());
    }
    w.finish()
}

fn decode_entries(bytes: &[u8]) -> Result<Vec<Entry>, Error> {
    let mut r = Reader::new(bytes);
    let count = r.u16()?;
    let mut entries = Vec::with_capacity(count as usize);
    for _ in 0..count {
        entries.push(Entry::decode(r.bytes()?)?);
    }
    r.finish()?;
    Ok(entries)
}

/// One exchange: heads out, heads in, entries out, entries in. Symmetric,
/// so the same function serves both the opener and the acceptor.
pub async fn exchange(
    node: &std::sync::Arc<Node>,
    send: &mut SendStream,
    recv: &mut RecvStream,
) -> Result<usize, Error> {
    let my_heads = node.roster().read().unwrap().heads();
    write_message(send, &encode_heads(&my_heads)).await?;
    let their_heads = read_message(recv, MAX_MESSAGE)
        .await?
        .ok_or_else(|| Error::Protocol("peer closed before sending heads".into()))?;
    let their_heads = decode_heads(&their_heads)?;
    let missing_for_them = node.roster().read().unwrap().entries_after(&their_heads);
    write_message(send, &encode_entries(&missing_for_them)).await?;
    let theirs = read_message(recv, MAX_MESSAGE)
        .await?
        .ok_or_else(|| Error::Protocol("peer closed before sending entries".into()))?;
    let theirs = decode_entries(&theirs)?;
    let accepted = node.absorb_entries(theirs);
    Ok(accepted)
}

pub async fn serve(node: &std::sync::Arc<Node>, mut send: SendStream, mut recv: RecvStream) {
    match exchange(node, &mut send, &mut recv).await {
        Ok(n) if n > 0 => log::info!("roster sync accepted {n} entries"),
        Ok(_) => {}
        Err(error) => log::debug!("roster sync failed: {error}"),
    }
    let _ = send.finish();
}
