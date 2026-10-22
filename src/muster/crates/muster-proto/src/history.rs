//! Cache cursors carry content evidence, not just IDs: rewritten history resets.
use crate::{Message, v1};
use sha2::{Digest, Sha256};

pub fn prefix_hash(messages: &[v1::AgentMessage]) -> Vec<u8> {
    let mut hash = Sha256::new();
    for message in messages {
        let bytes = message.encode_to_vec();
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    hash.finalize().to_vec()
}
pub fn request(session: &str, messages: &[v1::AgentMessage]) -> v1::AgentMessagesRequest {
    v1::AgentMessagesRequest {
        session: session.into(),
        tail: 500,
        first_id: messages.first().map(|m| m.id.clone()).unwrap_or_default(),
        after_id: messages.last().map(|m| m.id.clone()).unwrap_or_default(),
        prefix_hash: prefix_hash(&messages[..messages.len().saturating_sub(1)]),
        ..Default::default()
    }
}
pub fn page(all: &[v1::AgentMessage], request: &v1::AgentMessagesRequest) -> v1::AgentMessagePage {
    let count = if request.tail == 0 {
        50
    } else {
        request.tail.min(2000)
    } as usize;
    let cached = (!request.after_id.is_empty())
        .then(|| {
            let first = all.iter().position(|m| m.id == request.first_id)?;
            let last = all.iter().position(|m| m.id == request.after_id)?;
            (last >= first && prefix_hash(&all[first..last]) == request.prefix_hash)
                .then_some((first, last))
        })
        .flatten();
    let (start, end, has_more, reset) = if let Some((first, last)) = cached {
        (last, (last + count).min(all.len()), first > 0, false)
    } else {
        let before = all.iter().position(|m| m.id == request.before_id);
        let end = before.unwrap_or(all.len());
        let start = end.saturating_sub(count);
        (
            start,
            end,
            start > 0,
            !request.after_id.is_empty() || (!request.before_id.is_empty() && before.is_none()),
        )
    };
    v1::AgentMessagePage {
        session: request.session.clone(),
        messages: all[start..end].to_vec(),
        has_more,
        reset,
        has_newer: cached.is_some() && end < all.len(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn msg(id: &str, text: &str) -> v1::AgentMessage {
        v1::AgentMessage {
            id: id.into(),
            text: text.into(),
            ..Default::default()
        }
    }
    #[test]
    fn incremental_updates_tail_and_detects_earlier_edits_and_deleted_cursors() {
        let cached = vec![msg("a", "old"), msg("b", "stream")];
        let request = request("s", &cached);
        let mut all = vec![
            msg("a", "old"),
            msg("b", "stream complete"),
            msg("c", "new"),
        ];
        let delta = page(&all, &request);
        assert!(!delta.reset);
        assert_eq!(delta.messages, &all[1..]);
        all[0].text = "edited".into();
        assert!(page(&all, &request).reset);
        all.remove(1);
        assert!(page(&all, &request).reset);
        assert!(
            page(
                &all,
                &v1::AgentMessagesRequest {
                    before_id: "b".into(),
                    ..Default::default()
                }
            )
            .reset
        );
    }
    #[test]
    fn large_gap_paginates_forward_without_skipping_or_duplicating() {
        let all: Vec<_> = (0..1300).map(|i| msg(&i.to_string(), "text")).collect();
        let mut cached = all[..2].to_vec();
        loop {
            let delta = page(&all, &request("s", &cached));
            assert!(!delta.reset);
            cached.pop();
            cached.extend(delta.messages);
            if !delta.has_newer {
                break;
            }
        }
        assert_eq!(cached, all);
    }
}
