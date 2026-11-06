//! Cache cursors carry content evidence, not just IDs: rewritten history resets.
use crate::{Message, v1};
use sha2::{Digest, Sha256};

pub fn prefix_hash(messages: &[v1::AgentMessage]) -> Vec<u8> {
    hash_encoded(messages.iter().map(Message::encode_to_vec))
}
fn hash_encoded(messages: impl Iterator<Item = Vec<u8>>) -> Vec<u8> {
    let mut hash = Sha256::new();
    for bytes in messages {
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    hash.finalize().to_vec()
}
pub fn request(session: &str, messages: &[v1::AgentMessage]) -> v1::AgentMessagesRequest {
    v1::AgentMessagesRequest {
        session: session.into(),
        // Show the current conversation promptly; older tool output remains
        // available through pagination instead of delaying every chat entry.
        tail: 50,
        first_id: messages.first().map(|m| m.id.clone()).unwrap_or_default(),
        after_id: messages.last().map(|m| m.id.clone()).unwrap_or_default(),
        prefix_hash: prefix_hash(&messages[..messages.len().saturating_sub(1)]),
        ..Default::default()
    }
}
pub fn page(all: &[v1::AgentMessage], request: &v1::AgentMessagesRequest) -> v1::AgentMessagePage {
    page_with(
        all,
        request,
        |m| &m.id,
        Message::encode_to_vec,
        Clone::clone,
    )
}

/// Select and validate against source IDs before projecting message bodies.
/// Only the cache proof and the returned page need protobuf conversion.
pub fn page_from<T>(
    all: &[T],
    request: &v1::AgentMessagesRequest,
    id: impl Fn(&T) -> &str,
    project: impl Fn(&T) -> v1::AgentMessage,
) -> v1::AgentMessagePage {
    page_with(all, request, id, |m| project(m).encode_to_vec(), &project)
}

fn page_with<T>(
    all: &[T],
    request: &v1::AgentMessagesRequest,
    id: impl Fn(&T) -> &str,
    encode: impl Fn(&T) -> Vec<u8>,
    project: impl Fn(&T) -> v1::AgentMessage,
) -> v1::AgentMessagePage {
    let count = if request.tail == 0 {
        50
    } else {
        request.tail.min(2000)
    } as usize;
    let cached = (!request.after_id.is_empty())
        .then(|| {
            let first = all.iter().position(|m| id(m) == request.first_id)?;
            let last = all.iter().position(|m| id(m) == request.after_id)?;
            if last < first {
                return None;
            }
            let hash = hash_encoded(all[first..last].iter().map(&encode));
            (hash == request.prefix_hash).then_some((first, last))
        })
        .flatten();
    let (start, end, has_more, reset) = if let Some((first, last)) = cached {
        (last, (last + count).min(all.len()), first > 0, false)
    } else {
        let before = all.iter().position(|m| id(m) == request.before_id);
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
        messages: all[start..end].iter().map(project).collect(),
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
    fn projected_pages_do_not_convert_unrelated_history() {
        use std::cell::Cell;
        let all: Vec<_> = (0..10_000).map(|i| msg(&i.to_string(), "body")).collect();
        let conversions = Cell::new(0);
        let request = v1::AgentMessagesRequest {
            tail: 10,
            ..Default::default()
        };
        let result = page_from(
            &all,
            &request,
            |m| &m.id,
            |m| {
                conversions.set(conversions.get() + 1);
                m.clone()
            },
        );
        assert_eq!(result.messages, all[9_990..]);
        assert_eq!(conversions.get(), 10);
        let request = super::request("s", &all[9_980..9_995]);
        conversions.set(0);
        let result = page_from(
            &all,
            &request,
            |m| &m.id,
            |m| {
                conversions.set(conversions.get() + 1);
                m.clone()
            },
        );
        assert_eq!(result.messages, all[9_994..]);
        assert!(!result.reset);
        assert_eq!(conversions.get(), 14 + 6);
    }

    #[test]
    fn projected_history_preserves_cursor_validation_and_boundaries() {
        struct Source {
            key: String,
            body: String,
        }
        let sources: Vec<_> = (0..12)
            .map(|i| Source {
                key: i.to_string(),
                body: format!("body {i}"),
            })
            .collect();
        let project = |m: &Source| msg(&m.key, &m.body);
        let all: Vec<_> = sources.iter().map(project).collect();
        let mut valid = request("s", &all[3..8]);
        valid.tail = 2;
        let mut invalid = valid.clone();
        invalid.prefix_hash = vec![0; 32];
        let mut reversed = valid.clone();
        reversed.first_id = "9".into();
        for (request, start, end, reset, more, newer) in [
            (valid, 7, 9, false, true, true),
            (invalid, 10, 12, true, true, false),
            (reversed, 10, 12, true, true, false),
            (
                v1::AgentMessagesRequest {
                    tail: 3,
                    before_id: "4".into(),
                    ..Default::default()
                },
                1,
                4,
                false,
                true,
                false,
            ),
            (
                v1::AgentMessagesRequest {
                    tail: 3,
                    before_id: "gone".into(),
                    ..Default::default()
                },
                9,
                12,
                true,
                true,
                false,
            ),
            (
                v1::AgentMessagesRequest {
                    tail: 20,
                    ..Default::default()
                },
                0,
                12,
                false,
                false,
                false,
            ),
        ] {
            let page = page_from(&sources, &request, |m| &m.key, project);
            assert_eq!(page.messages, all[start..end]);
            assert_eq!(
                (page.reset, page.has_more, page.has_newer),
                (reset, more, newer)
            );
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
