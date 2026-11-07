//! Cache cursors carry content evidence, not just IDs: rewritten history resets.
use crate::{Message, v1};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};

/// Index for one append-only source generation. The owner MUST clear it before
/// replacing, editing or truncating any existing message. Never use a file stamp
/// alone as permission to retain this index across a source rewrite.
#[derive(Default)]
pub struct Index {
    positions: HashMap<String, usize>,
    indexed: usize,
    ranges: VecDeque<(usize, usize, Sha256)>,
}
impl Index {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    /// Indexes the IDs of records appended since the last call.
    pub fn extend<'a, T>(&mut self, all: &'a [T], id: impl Fn(&'a T) -> &'a str) {
        if all.len() < self.indexed {
            self.clear();
        }
        for (i, message) in all.iter().enumerate().skip(self.indexed) {
            self.positions.entry(id(message).to_owned()).or_insert(i);
        }
        self.indexed = all.len();
    }
    /// Forgets the records from `from` on, whose IDs are `ids`, before the
    /// owner replaces them. Proofs over earlier records stay valid.
    pub fn truncate<'a>(&mut self, from: usize, ids: impl Iterator<Item = &'a str>) {
        for id in ids {
            if self.positions.get(id).is_some_and(|&i| i >= from) {
                self.positions.remove(id);
            }
        }
        self.indexed = self.indexed.min(from);
        self.ranges.retain(|(_, end, _)| *end <= from);
    }
    pub fn position(&self, id: &str) -> Option<usize> {
        self.positions.get(id).copied()
    }
    pub fn page_from<'a, T>(
        &mut self,
        all: &'a [T],
        request: &v1::AgentMessagesRequest,
        id: impl Fn(&'a T) -> &'a str,
        project: impl Fn(&T) -> v1::AgentMessage,
    ) -> v1::AgentMessagePage {
        self.extend(all, &id);
        page_with_index(
            all,
            request,
            id,
            |m| project(m).encode_to_vec(),
            &project,
            Some(self),
            records(request),
            |_| 0,
        )
    }
    fn hash<T>(
        &mut self,
        all: &[T],
        first: usize,
        last: usize,
        encode: impl Fn(&T) -> Vec<u8>,
    ) -> Vec<u8> {
        let found = self
            .ranges
            .iter()
            .position(|(start, end, _)| *start == first && *end <= last);
        let (_, end, mut hash) =
            found
                .and_then(|i| self.ranges.remove(i))
                .unwrap_or((first, first, Sha256::new()));
        for message in &all[end..last] {
            let bytes = encode(message);
            hash.update((bytes.len() as u64).to_be_bytes());
            hash.update(bytes);
        }
        let proof = hash.clone().finalize().to_vec();
        self.ranges.push_back((first, last, hash));
        while self.ranges.len() > 8 {
            self.ranges.pop_front();
        }
        proof
    }
}

pub fn prefix_hash(messages: &[v1::AgentMessage]) -> Vec<u8> {
    hash_encoded(messages.iter().map(Message::encode_to_vec))
}
pub(crate) fn hash_encoded<B: AsRef<[u8]>>(messages: impl Iterator<Item = B>) -> Vec<u8> {
    let mut hash = Sha256::new();
    for bytes in messages {
        let bytes = bytes.as_ref();
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    hash.finalize().to_vec()
}
/// Records a record page holds: `tail`, 50 when unset, at most 2,000.
fn records(request: &v1::AgentMessagesRequest) -> usize {
    if request.tail == 0 {
        50
    } else {
        request.tail.min(2000) as usize
    }
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
pub fn page_from<'a, T>(
    all: &'a [T],
    request: &v1::AgentMessagesRequest,
    id: impl Fn(&'a T) -> &'a str,
    project: impl Fn(&T) -> v1::AgentMessage,
) -> v1::AgentMessagePage {
    page_with(all, request, id, |m| project(m).encode_to_vec(), &project)
}

fn page_with<'a, T>(
    all: &'a [T],
    request: &v1::AgentMessagesRequest,
    id: impl Fn(&'a T) -> &'a str,
    encode: impl Fn(&T) -> Vec<u8>,
    project: impl Fn(&T) -> v1::AgentMessage,
) -> v1::AgentMessagePage {
    page_with_index(
        all,
        request,
        id,
        encode,
        project,
        None,
        records(request),
        |_| 0,
    )
}

/// A page of `all`. With a valid proof, the `count` entries after the cached
/// ones; otherwise the last `count` before `before_id`, starting no earlier
/// than `earliest(end)`. `index`, when given, must already be extended.
#[allow(clippy::too_many_arguments)]
pub(crate) fn page_with_index<'a, T>(
    all: &'a [T],
    request: &v1::AgentMessagesRequest,
    id: impl Fn(&'a T) -> &'a str,
    encode: impl Fn(&T) -> Vec<u8>,
    project: impl Fn(&T) -> v1::AgentMessage,
    mut index: Option<&mut Index>,
    count: usize,
    earliest: impl Fn(usize) -> usize,
) -> v1::AgentMessagePage {
    let cached = (!request.after_id.is_empty())
        .then(|| {
            let position = |key: &str| match &index {
                Some(index) => index.positions.get(key).copied(),
                None => all.iter().position(|m| id(m) == key),
            };
            let first = position(&request.first_id)?;
            let last = position(&request.after_id)?;
            if last < first {
                return None;
            }
            let hash = match &mut index {
                Some(index) => index.hash(all, first, last, &encode),
                None => hash_encoded(all[first..last].iter().map(&encode)),
            };
            (hash == request.prefix_hash).then_some((first, last))
        })
        .flatten();
    let before = if request.before_id.is_empty() {
        Some(all.len())
    } else {
        match &index {
            Some(index) => index.positions.get(&request.before_id).copied(),
            None => all.iter().position(|m| id(m) == request.before_id),
        }
    };
    let cached = cached.filter(|(_, last)| before.is_some_and(|end| *last < end));
    let (start, end, has_more, reset) = if let Some((first, last)) = cached {
        (
            last,
            (last + count).min(before.unwrap_or(all.len())),
            first > 0,
            false,
        )
    } else {
        let before = match &index {
            Some(index) => index.positions.get(&request.before_id).copied(),
            None => all.iter().position(|m| id(m) == request.before_id),
        };
        let end = before.unwrap_or(all.len());
        let start = end.saturating_sub(count).max(earliest(end).min(end));
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
        has_newer: cached.is_some() && end < before.unwrap_or(all.len()),
        ..Default::default()
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
    fn checkpoints_reuse_encoded_prefixes_and_reset_after_rewrites() {
        use std::cell::Cell;
        let mut all: Vec<_> = (0..10_000).map(|i| msg(&i.to_string(), "body")).collect();
        let mut index = Index::default();
        let conversions = Cell::new(0);
        let project = |m: &v1::AgentMessage| {
            conversions.set(conversions.get() + 1);
            m.clone()
        };
        let request = request("s", &all);
        assert!(!index.page_from(&all, &request, |m| &m.id, project).reset);
        assert_eq!(conversions.get(), 10_000);
        conversions.set(0);
        assert!(!index.page_from(&all, &request, |m| &m.id, project).reset);
        assert_eq!(
            conversions.get(),
            1,
            "only the mutable tail is projected again"
        );
        all.push(msg("10000", "append"));
        let extended = super::request("s", &all);
        conversions.set(0);
        assert!(!index.page_from(&all, &extended, |m| &m.id, project).reset);
        assert_eq!(conversions.get(), 2, "extend the checkpoint by one record");
        all[0].text = "rewritten".into();
        index.clear();
        assert!(index.page_from(&all, &extended, |m| &m.id, project).reset);
    }

    #[test]
    fn cached_older_pages_validate_contents_and_stop_at_exclusive_cursor() {
        let mut all: Vec<_> = (0..1000).map(|i| msg(&i.to_string(), "body")).collect();
        let mut query = request("s", &all[600..800]);
        query.before_id = "800".into();
        query.tail = 200;
        let result = page(&all, &query);
        assert_eq!(result.messages, all[799..800]);
        assert!(!result.reset);
        assert!(!result.has_newer);
        assert!(result.has_more);
        all[610].text = "changed under same id".into();
        assert!(page(&all, &query).reset);
        let mut index = Index::default();
        assert_eq!(
            index.page_from(&all, &query, |m| &m.id, Clone::clone),
            page(&all, &query)
        );
        query.before_id = "missing".into();
        assert!(page(&all, &query).reset);
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
