//! A session's outline (ADR 0160): its records, with the work of every
//! completed turn replaced by one `work` record. A daemon pages the outline by
//! turns and serves a work record's records when it is expanded. A client
//! collapses a turn it saw complete with the same rules, so cache proofs over
//! its outline and the daemon's agree.
use crate::{Message, history, v1};
use std::cell::OnceCell;
use std::collections::HashSet;
use std::ops::Range;

/// `AgentMessage.kind` of a work record.
pub const KIND: &str = "work";
/// Turns a client asks for per page.
pub const TURNS: u32 = 10;
/// Outline records a turn page holds at most when `tail` is unset.
pub const PAGE: usize = 200;
const PLAN: &str = "mcp__ccd_turn__update_plan";

/// The fields the outline reads from a transcript record.
pub trait Record {
    fn id(&self) -> &str;
    fn role(&self) -> &str;
    fn kind(&self) -> &str;
    fn phase(&self) -> &str;
    fn text(&self) -> &str;
    fn tool_name(&self) -> &str;
    fn tool_call_id(&self) -> &str;
    fn ts_unix(&self) -> u64;
}

impl Record for v1::AgentMessage {
    fn id(&self) -> &str {
        &self.id
    }
    fn role(&self) -> &str {
        &self.role
    }
    fn kind(&self) -> &str {
        &self.kind
    }
    fn phase(&self) -> &str {
        &self.phase
    }
    fn text(&self) -> &str {
        &self.text
    }
    fn tool_name(&self) -> &str {
        &self.tool_name
    }
    fn tool_call_id(&self) -> &str {
        &self.tool_call_id
    }
    fn ts_unix(&self) -> u64 {
        self.ts_unix
    }
}

impl<T: Record + ?Sized> Record for &T {
    fn id(&self) -> &str {
        (**self).id()
    }
    fn role(&self) -> &str {
        (**self).role()
    }
    fn kind(&self) -> &str {
        (**self).kind()
    }
    fn phase(&self) -> &str {
        (**self).phase()
    }
    fn text(&self) -> &str {
        (**self).text()
    }
    fn tool_name(&self) -> &str {
        (**self).tool_name()
    }
    fn tool_call_id(&self) -> &str {
        (**self).tool_call_id()
    }
    fn ts_unix(&self) -> u64 {
        (**self).ts_unix()
    }
}

pub fn is_compact_context(message: &impl Record) -> bool {
    if message.kind() == "compact_context" || message.phase() == "summary" {
        return true;
    }
    // Older viewers exposed provider continuation summaries as plain user text.
    // Match the provider preamble, never a casual mention of compaction.
    message.kind() == "text"
        && message.role() == "user"
        && [
            "This session is being continued from a previous conversation that ran out of context.",
            "Another language model started to solve this problem and produced a summary of its thinking process.",
            "Another language model began solving this problem and produced a summary of its thinking process.",
        ]
        .iter()
        .any(|prefix| message.text().trim_start().starts_with(prefix))
}

/// A visible prompt starts a turn.
pub fn starts_turn(message: &impl Record) -> bool {
    message.role() == "user"
        && message.kind() == "text"
        && !message.text().trim().is_empty()
        && !is_compact_context(message)
}

/// A usage footer ends its turn; the records after it start the next one.
pub fn ends_turn(message: &impl Record) -> bool {
    message.kind() == "turn_summary"
}

fn is_tool(message: &impl Record) -> bool {
    matches!(message.kind(), "tool_call" | "tool_result")
}

/// Where a turn's presentation puts a record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Its own row: the prompt, the final answer, an interaction, a card, a
    /// compaction summary or a turn status.
    Boundary,
    /// Inside a Worked group: tool records, intermediate prose, work records.
    Work,
    /// Not shown: empty text, superseded plan cards, a tool call a provider
    /// card replaces.
    Hidden,
}

/// Classifies one turn's records (a segment between prompts and turn
/// summaries). Only a completed turn has a trailing reply.
pub fn classify<R: Record>(turn: &[R], completed: bool) -> Vec<Class> {
    // Older transcripts do not label prose phases. Keep the complete trailing
    // reply (possibly several text blocks) outside Worked. Empty text records
    // do not hide it; a later tool call still means this prose was work in progress.
    let trailing_start = if completed {
        turn.iter()
            .rposition(|m| {
                !(m.kind() == "turn_status"
                    || m.kind() == "text"
                        && (m.text().trim().is_empty()
                            || (m.role() == "assistant" && m.phase().is_empty())))
            })
            .map_or(0, |index| index + 1)
    } else {
        turn.len()
    };
    let card_calls: HashSet<_> = turn
        .iter()
        .filter(|m| m.kind() == "provider_card")
        .map(|m| m.tool_call_id())
        .collect();
    // Incremental plan updates replace the prior snapshot within this turn.
    let latest_plan = turn
        .iter()
        .rposition(|m| m.kind() == "provider_card" && m.tool_name() == PLAN);
    turn.iter()
        .enumerate()
        .map(|(index, m)| {
            if m.kind() == "tool_call" && card_calls.contains(m.tool_call_id())
                || m.kind() == "provider_card"
                    && m.tool_name() == PLAN
                    && latest_plan.is_some_and(|latest| latest > index)
            {
                Class::Hidden
            } else if is_tool(m) || m.kind() == KIND {
                Class::Work
            } else if m.kind() == "text" && m.text().trim().is_empty() {
                Class::Hidden
            } else if m.kind() == "turn_status"
                || is_compact_context(m)
                || m.role() == "user"
                || m.kind() == "interaction"
                || m.kind() == "provider_card"
                || m.phase() == "final_answer"
                || index >= trailing_start
            {
                Class::Boundary
            } else {
                Class::Work
            }
        })
        .collect()
}

/// The work spans of a completed turn: each Worked group's records, from its
/// first work record to its last, with the hidden records between them. Work
/// records already collapsed are never part of a span.
pub fn spans<R: Record>(turn: &[R]) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut open: Option<Range<usize>> = None;
    for (index, (message, class)) in turn.iter().zip(classify(turn, true)).enumerate() {
        match class {
            Class::Hidden => {}
            Class::Work if message.kind() != KIND => {
                open = Some(open.map_or(index..index + 1, |span| span.start..index + 1));
            }
            _ => out.extend(open.take()),
        }
    }
    out.extend(open);
    out
}

/// The work record standing for `span`, whose encoded records hash to `hash`.
pub fn stub<R: Record>(span: &[R], hash: Vec<u8>) -> v1::AgentMessage {
    let (first, last) = (&span[0], &span[span.len() - 1]);
    v1::AgentMessage {
        id: work_id(first.id()),
        role: "assistant".into(),
        kind: KIND.into(),
        ts_unix: first.ts_unix(),
        work: Some(v1::AgentWorkSpan {
            first_id: first.id().into(),
            last_id: last.id().into(),
            count: span.len() as u32,
            hash,
            first_is_tool: is_tool(first),
            end_unix: last.ts_unix(),
        }),
        ..Default::default()
    }
}

fn work_id(first: &str) -> String {
    format!("{KIND}:{first}")
}

/// The ID of the Worked group a work record stands for, the same ID its
/// records produce once loaded.
pub fn group_id(work: &v1::AgentWorkSpan) -> String {
    if work.first_is_tool {
        format!("working:activity:{}", work.first_id)
    } else {
        format!("working:{}", work.first_id)
    }
}

/// Collapses, in a client's outline, the turn that ends just before `end`,
/// where a prompt or a turn summary has arrived. Returns each work record with
/// the records it replaced. A turn whose start is not loaded stays as it is:
/// with `head_whole` false, records before the first boundary may continue a
/// turn that started earlier.
pub fn collapse(
    messages: &mut Vec<v1::AgentMessage>,
    end: usize,
    head_whole: bool,
) -> Vec<(v1::AgentMessage, Vec<v1::AgentMessage>)> {
    let end = end.min(messages.len());
    let start = match messages[..end]
        .iter()
        .rposition(|m| starts_turn(m) || ends_turn(m))
    {
        Some(i) if ends_turn(&messages[i]) => i + 1,
        Some(i) => i,
        None if head_whole => 0,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    for span in spans(&messages[start..end]).into_iter().rev() {
        let range = start + span.start..start + span.end;
        let at = range.start;
        let records: Vec<_> = messages.drain(range).collect();
        let work = stub(&records, history::prefix_hash(&records));
        messages.insert(at, work.clone());
        out.push((work, records));
    }
    out.reverse();
    out
}

/// One outline record: a transcript record, or a completed turn's work.
pub enum Item {
    Record(usize),
    Work(Work),
}

pub struct Work {
    pub range: Range<usize>,
    id: String,
    hash: OnceCell<Vec<u8>>,
}

impl Work {
    /// The span's hash over `encoded`, its records in the cache-proof form.
    /// Computed once; the owner clears the outline when records change.
    pub fn hash(&self, encoded: impl FnOnce() -> Vec<u8>) -> &[u8] {
        self.hash.get_or_init(encoded)
    }
}

/// A source's outline, maintained as records are appended. Like
/// `history::Index`, the owner MUST clear it before replacing, editing or
/// truncating any existing record.
#[derive(Default)]
pub struct Outline {
    items: Vec<Item>,
    /// Items that start a turn, in order.
    turns: Vec<usize>,
    /// The first record of the latest turn, which is never collapsed.
    open: usize,
    /// Items before the latest turn's.
    closed: usize,
    indexed: usize,
    index: history::Index,
}

impl Outline {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Whether item `i` starts a turn.
    pub fn starts_turn(&self, i: usize) -> bool {
        self.turns.binary_search(&i).is_ok()
    }

    /// Follows records appended since the last call.
    pub fn update<R: Record>(&mut self, records: &[R]) {
        if records.len() < self.indexed {
            self.clear();
        }
        let mut reopened = false;
        for i in self.indexed..records.len() {
            let message = &records[i];
            let next = if ends_turn(message) {
                i + 1
            } else if starts_turn(message) {
                i
            } else {
                continue;
            };
            if !reopened {
                // The latest turn's records were listed one by one.
                let ids = self.items[self.closed..]
                    .iter()
                    .map(|item| item_id(records, item));
                self.index.truncate(self.closed, ids);
                self.items.truncate(self.closed);
                self.turns
                    .truncate(self.turns.partition_point(|&t| t < self.closed));
                reopened = true;
            }
            self.close(records, self.open..i);
            if next > i {
                self.push(records, i);
            }
            self.open = next;
            self.closed = self.items.len();
        }
        let from = if reopened {
            self.open
        } else {
            self.indexed.max(self.open)
        };
        for i in from..records.len() {
            self.push(records, i);
        }
        self.indexed = records.len();
    }

    fn push<R: Record>(&mut self, records: &[R], i: usize) {
        if starts_turn(&records[i]) {
            self.turns.push(self.items.len());
        }
        self.items.push(Item::Record(i));
    }

    fn close<R: Record>(&mut self, records: &[R], turn: Range<usize>) {
        let mut next = turn.start;
        for span in spans(&records[turn.clone()]) {
            let span = turn.start + span.start..turn.start + span.end;
            for i in next..span.start {
                self.push(records, i);
            }
            next = span.end;
            self.items.push(Item::Work(Work {
                id: work_id(records[span.start].id()),
                range: span,
                hash: OnceCell::new(),
            }));
        }
        for i in next..turn.end {
            self.push(records, i);
        }
    }

    /// A page of the outline (`request.turns` set) or of one work record's
    /// records (`request.work_id` set).
    pub fn page<R: Record>(
        &mut self,
        records: &[R],
        request: &v1::AgentMessagesRequest,
        project: impl Fn(&R) -> v1::AgentMessage,
    ) -> v1::AgentMessagePage {
        self.update(records);
        let Self {
            items,
            turns,
            index,
            ..
        } = self;
        index.extend(items, |item| item_id(records, item));
        let count = if request.tail == 0 {
            PAGE
        } else {
            request.tail.min(2000) as usize
        };
        let mut page = if request.work_id.is_empty() {
            let wanted = request.turns.max(1) as usize;
            history::page_with_index(
                items,
                request,
                |item| item_id(records, item),
                |item| message(records, item, &project).encode_to_vec(),
                |item| message(records, item, &project),
                Some(index),
                count,
                |end| {
                    let before = turns.partition_point(|&t| t < end);
                    before.checked_sub(wanted).map_or(0, |i| turns[i])
                },
            )
        } else {
            work_page(records, items, index, request, count, &project)
        };
        page.session = request.session.clone();
        page.outline = true;
        page
    }
}

fn item_id<'a, R: Record>(records: &'a [R], item: &'a Item) -> &'a str {
    match item {
        Item::Record(i) => records[*i].id(),
        Item::Work(work) => &work.id,
    }
}

/// An outline item as sent: the record itself, or its work record.
pub fn message<R: Record>(
    records: &[R],
    item: &Item,
    project: impl Fn(&R) -> v1::AgentMessage,
) -> v1::AgentMessage {
    match item {
        Item::Record(i) => project(&records[*i]),
        Item::Work(work) => {
            let span = &records[work.range.clone()];
            let hash = work
                .hash(|| history::hash_encoded(span.iter().map(|r| project(r).encode_to_vec())));
            stub(span, hash.to_vec())
        }
    }
}

fn work_page<R: Record>(
    records: &[R],
    items: &[Item],
    index: &history::Index,
    request: &v1::AgentMessagesRequest,
    count: usize,
    project: &impl Fn(&R) -> v1::AgentMessage,
) -> v1::AgentMessagePage {
    let reset = v1::AgentMessagePage {
        reset: true,
        ..Default::default()
    };
    let Some(item @ Item::Work(work)) = index.position(&request.work_id).map(|i| &items[i]) else {
        return reset;
    };
    if message(records, item, project)
        .work
        .is_none_or(|span| span.hash != request.work_hash)
    {
        return reset;
    }
    let span = work.range.clone();
    let end = if request.before_id.is_empty() {
        span.end
    } else {
        match records[span.clone()]
            .iter()
            .position(|r| r.id() == request.before_id)
        {
            Some(i) => span.start + i,
            None => return reset,
        }
    };
    let start = end.saturating_sub(count).max(span.start);
    let messages = history::bounded_page(&records[start..end], false, project);
    v1::AgentMessagePage {
        has_more: start > span.start || messages.len() < end - start,
        messages,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(id: &str, role: &str, kind: &str, text: &str) -> v1::AgentMessage {
        v1::AgentMessage {
            id: id.into(),
            role: role.into(),
            kind: kind.into(),
            text: text.into(),
            ..Default::default()
        }
    }
    fn prompt(id: &str) -> v1::AgentMessage {
        m(id, "user", "text", "do it")
    }
    fn tool(id: &str) -> v1::AgentMessage {
        m(id, "assistant", "tool_call", "{}")
    }
    fn prose(id: &str) -> v1::AgentMessage {
        m(id, "assistant", "text", "thinking aloud")
    }
    fn summary(id: &str) -> v1::AgentMessage {
        m(id, "assistant", "turn_summary", "")
    }
    /// A turn: prompt, `n` tool calls with prose before every second one,
    /// final answer.
    fn turn(t: usize, n: usize) -> Vec<v1::AgentMessage> {
        let mut out = vec![prompt(&format!("p{t}"))];
        for i in 0..n {
            if i % 2 == 1 {
                out.push(prose(&format!("x{t}-{i}")));
            }
            out.push(tool(&format!("t{t}-{i}")));
        }
        out.push(m(&format!("a{t}"), "assistant", "text", "done"));
        out
    }
    fn ids(messages: &[v1::AgentMessage]) -> Vec<&str> {
        messages.iter().map(|m| m.id.as_str()).collect()
    }
    fn outline(records: &[v1::AgentMessage]) -> Vec<v1::AgentMessage> {
        let mut outline = Outline::default();
        outline.update(records);
        outline
            .items()
            .iter()
            .map(|item| message(records, item, Clone::clone))
            .collect()
    }

    #[test]
    fn completed_turns_collapse_and_the_latest_stays_whole() {
        let mut records = turn(0, 3);
        records.extend(turn(1, 2));
        let all = outline(&records);
        assert_eq!(
            ids(&all),
            ["p0", "work:t0-0", "a0", "p1", "t1-0", "x1-1", "t1-1", "a1"]
        );
        let work = all[1].work.as_ref().unwrap();
        assert_eq!(
            (work.first_id.as_str(), work.last_id.as_str()),
            ("t0-0", "t0-2")
        );
        assert_eq!(work.count, 4);
        assert!(work.first_is_tool);
        assert_eq!(work.hash, history::prefix_hash(&records[1..5]));
        assert_eq!(group_id(work), "working:activity:t0-0");
    }

    #[test]
    fn hidden_records_split_nothing_and_boundaries_split_spans() {
        let mut card = m("card", "assistant", "provider_card", "{}");
        card.tool_call_id = "c1".into();
        card.tool_name = "AskUserQuestion".into();
        let mut replaced = tool("asked");
        replaced.tool_call_id = "c1".into();
        let mut final_answer = prose("final");
        final_answer.phase = "final_answer".into();
        let records = vec![
            prompt("p"),
            m("empty0", "assistant", "text", " "),
            prose("x0"),
            m("empty1", "assistant", "text", ""),
            tool("t0"),
            replaced,
            card,
            tool("t1"),
            final_answer,
            m("status", "system", "turn_status", "Interrupted"),
            prompt("p2"),
        ];
        assert_eq!(
            ids(&outline(&records)),
            [
                "p", "empty0", "work:x0", "asked", "card", "work:t1", "final", "status", "p2"
            ]
        );
        let collapsed = outline(&records);
        assert_eq!(collapsed[2].work.as_ref().unwrap().count, 3, "x0..=t0");
        assert!(!collapsed[2].work.as_ref().unwrap().first_is_tool);
        assert_eq!(group_id(collapsed[2].work.as_ref().unwrap()), "working:x0");
    }

    #[test]
    fn summaries_end_turns_and_unphased_trailing_prose_is_the_answer() {
        let records = vec![
            prompt("p"),
            tool("t"),
            prose("answer-1"),
            prose("answer-2"),
            summary("s"),
            tool("after-summary"),
        ];
        assert_eq!(
            ids(&outline(&records)),
            ["p", "work:t", "answer-1", "answer-2", "s", "after-summary"]
        );
    }

    #[test]
    fn appending_matches_a_fresh_outline_and_local_collapse() {
        let mut records = Vec::new();
        for t in 0..4 {
            records.extend(turn(t, 5));
            if t == 1 {
                records.push(summary("s1"));
            }
        }
        let mut incremental = Outline::default();
        let mut client: Vec<v1::AgentMessage> = Vec::new();
        for (i, record) in records.iter().enumerate() {
            client.push(record.clone());
            if starts_turn(record) || ends_turn(record) {
                let end = client.len() - 1;
                collapse(&mut client, end, true);
            }
            incremental.update(&records[..=i]);
            let incremental: Vec<_> = incremental
                .items()
                .iter()
                .map(|item| message(&records, item, Clone::clone))
                .collect();
            assert_eq!(incremental, outline(&records[..=i]), "after {}", record.id);
            assert_eq!(client, incremental, "client after {}", record.id);
        }
    }

    #[test]
    fn local_collapse_needs_the_turn_start() {
        let records = turn(0, 3);
        let mut partial = records[2..].to_vec();
        partial.push(prompt("p1"));
        let before = partial.clone();
        assert!(collapse(&mut partial, before.len() - 1, false).is_empty());
        assert_eq!(partial, before);
        let mut whole = records.clone();
        whole.push(prompt("p1"));
        let replaced = collapse(&mut whole, records.len(), false);
        assert_eq!(replaced.len(), 1);
        assert_eq!(replaced[0].1, records[1..5]);
        assert_eq!(ids(&whole), ["p0", "work:t0-0", "a0", "p1"]);
    }

    #[test]
    fn turn_pages_hold_the_last_turns_capped_and_proofs_cover_the_outline() {
        let mut records = Vec::new();
        for t in 0..30 {
            records.extend(turn(t, 40));
        }
        let mut outline = Outline::default();
        let first = outline.page(
            &records,
            &v1::AgentMessagesRequest {
                session: "s".into(),
                turns: 10,
                ..Default::default()
            },
            Clone::clone,
        );
        assert!(first.outline && first.has_more && !first.reset);
        assert_eq!(first.messages[0].id, "p20");
        // 9 collapsed turns of 3 records, then the latest one in full.
        assert_eq!(first.messages.len(), 9 * 3 + turn(29, 40).len());
        let mut capped = v1::AgentMessagesRequest {
            turns: 10,
            tail: 20,
            ..Default::default()
        };
        assert_eq!(
            outline.page(&records, &capped, Clone::clone).messages.len(),
            20
        );
        capped.before_id = "p20".into();
        capped.tail = 0;
        let earlier = outline.page(&records, &capped, Clone::clone);
        assert_eq!(earlier.messages[0].id, "p10");
        assert_eq!(earlier.messages.last().unwrap().id, "a19");
        // A proof over the cached outline returns only what follows it.
        let mut cached = first.messages.clone();
        records.push(prompt("p30"));
        let mut proof = history::request("s", &cached);
        proof.turns = 10;
        let delta = outline.page(&records, &proof, Clone::clone);
        assert!(
            delta.reset,
            "the latest turn collapsed under the cached records"
        );
        cached.truncate(9 * 3);
        let mut proof = history::request("s", &cached);
        proof.turns = 10;
        let delta = outline.page(&records, &proof, Clone::clone);
        assert!(!delta.reset && !delta.has_newer);
        assert_eq!(
            ids(&delta.messages),
            ["a28", "p29", "work:t29-0", "a29", "p30"]
        );
    }

    #[test]
    fn work_pages_serve_a_span_and_reset_when_it_changed() {
        let mut records = turn(0, 6);
        records.extend(turn(1, 1));
        let mut outline = Outline::default();
        let page = outline.page(
            &records,
            &v1::AgentMessagesRequest {
                turns: 10,
                ..Default::default()
            },
            Clone::clone,
        );
        let work = page.messages[1].clone();
        let span = work.work.clone().unwrap();
        let mut request = v1::AgentMessagesRequest {
            work_id: work.id.clone(),
            work_hash: span.hash.clone(),
            tail: 4,
            ..Default::default()
        };
        let newest = outline.page(&records, &request, Clone::clone);
        assert!(newest.outline && newest.has_more && !newest.reset);
        assert_eq!(newest.messages, records[6..10]);
        request.before_id = newest.messages[0].id.clone();
        request.tail = 100;
        let rest = outline.page(&records, &request, Clone::clone);
        assert!(!rest.has_more);
        assert_eq!(rest.messages, records[1..6]);
        request.work_hash = vec![0; 32];
        assert!(outline.page(&records, &request, Clone::clone).reset);
        request.work_id = "work:missing".into();
        request.work_hash = span.hash;
        assert!(outline.page(&records, &request, Clone::clone).reset);
    }
}
