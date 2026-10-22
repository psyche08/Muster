//! The one reader of JSONL session sources (ARCHITECTURE §3.6.4, step 3).
//! Which records are messages, tool calls, tool results and turn boundaries,
//! and where their fields live, is the app's record grammar
//! (`profile::Records`, from `profiles/<app>.toml`); what the grammar cannot
//! say is a named [`Hooks`] implementation per source (how text is cleaned,
//! which records rename a session, turn usage, a prompt the app queued).
//! The engine turns appended records into [`Message`]s with the same ids,
//! kinds, texts and phases the per-source readers produced before it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::model::{Message, MessageKind, Provider, Role, SessionMeta, SessionState, parse_ts};
use crate::profile::{BlockIds, OtherRoles, Records, Where, field};
use crate::text::{clip, compact};
use crate::transcript::Tail;
use crate::viewer::{Discovered, Polled, Viewer};

/// What a source needs code for.
pub trait Hooks: Send + Sync {
    /// A text block as the reader should see it; blank means "not a message".
    fn text(&self, role: Role, raw: &str) -> String;
    /// A record that names the session: the title, and whether a person
    /// chose it (a given name is never replaced by a generated one).
    fn title(&self, _record: &Value) -> Option<(String, bool)> {
        None
    }
    /// The turn summary a record completes, if any.
    fn usage(
        &self,
        tracker: &mut crate::usage::Tracker,
        offset: u64,
        record: &Value,
    ) -> Option<Message>;
    /// The exact prompt a record carries outside a user message (a prompt
    /// the app queued for a running turn); confirms a send like a user
    /// message does.
    fn queued_text(&self, _record: &Value) -> Option<String> {
        None
    }
}

/// The source's state directory: the environment override, else the
/// directory under `$HOME`.
pub fn home(records: &Records) -> Option<PathBuf> {
    if let Ok(dir) = std::env::var(records.home.env)
        && !dir.is_empty()
    {
        return Some(PathBuf::from(dir));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(records.home.dir))
}

/// Whether a record is one the grammar says to ignore entirely.
pub fn skipped(records: &Records, record: &Value) -> bool {
    records.skip.iter().any(|m| m.matches(record))
}

/// The message envelope of a record, when the record is a message that is
/// not skipped.
pub fn message<'a>(records: &Records, record: &'a Value) -> Option<&'a Value> {
    (!skipped(records, record) && records.message.when.matches(record))
        .then(|| field(record, records.message.envelope))
}

/// The text of a content value's blocks of the given types, in order.
pub fn block_texts(content: &Value, types: &[&str]) -> Vec<String> {
    match content {
        Value::String(s) => vec![s.clone()],
        Value::Array(items) => items
            .iter()
            .filter(|b| b["type"].as_str().is_some_and(|t| types.contains(&t)))
            .filter_map(|b| b["text"].as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

/// Whether the content carries anything a person would see: non-blank
/// text, or a block of one of the given types.
pub fn has_content(content: &Value, types: &[&str]) -> bool {
    content.as_str().is_some_and(|s| !s.trim().is_empty())
        || content.as_array().is_some_and(|blocks| {
            blocks
                .iter()
                .any(|b| b["type"].as_str().is_some_and(|t| types.contains(&t)))
        })
}

/// A tool result's text: the texts of its parts, else the value itself.
fn output_text(value: &Value) -> String {
    match value {
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        other => compact(other),
    }
}

/// The first present of several fields.
fn first<'a>(value: &'a Value, paths: &[&str]) -> &'a Value {
    paths
        .iter()
        .map(|p| field(value, p))
        .find(|v| !v.is_null())
        .unwrap_or(&Value::Null)
}

/// A session's record timestamp, in Unix seconds (0 when absent).
pub fn timestamp(records: &Records, record: &Value) -> u64 {
    field(record, records.timestamp)
        .as_str()
        .map(parse_ts)
        .unwrap_or(0)
}

/// Whether the record ends a turn.
pub fn ends_turn(records: &Records, record: &Value) -> bool {
    !skipped(records, record) && records.turn.ends(record)
}

/// The engine over one session's records, wherever they come from: a
/// transcript file ([`JsonlViewer`]) or a cloud session's event stream
/// (`crate::cloud`). It keeps the turn state, the title and the tool names
/// between reads.
#[derive(Clone)]
pub struct Reader {
    provider: Provider,
    sidechain: bool,
    records: &'static Records,
    hooks: &'static dyn Hooks,
    usage: crate::usage::Tracker,
    meta: SessionMeta,
    state: SessionState,
    custom_title: bool,
    /// Records before the grammar's `bootstrap_until` are the app's own
    /// bootstrap and are never shown.
    bootstrap: bool,
    /// Tool call ids seen, to name their results.
    calls: HashMap<String, String>,
    cards: crate::claude_cards::Presentation,
}

/// The reader of one JSONL transcript file.
pub struct JsonlViewer {
    reader: Reader,
    tail: Tail,
    subagent: bool,
    encrypted_history: bool,
    readable: bool,
}

/// What every message of one record shares.
struct RecordAt<'a> {
    ts: u64,
    phase: &'a str,
}

/// What distinguishes one message of a record: its role, kind and tool.
struct Part<'a> {
    role: Role,
    kind: MessageKind,
    tool: (&'a str, &'a str),
}

impl Part<'_> {
    fn text(role: Role) -> Self {
        Part {
            role,
            kind: MessageKind::Text,
            tool: ("", ""),
        }
    }
}

impl JsonlViewer {
    pub fn open(
        discovered: &Discovered,
        records: &'static Records,
        hooks: &'static dyn Hooks,
    ) -> Self {
        let mut reader = Reader::new(discovered.provider, records, hooks, discovered.meta.clone());
        // Only a separately discovered child file may include sidechain records.
        // A mixed parent transcript must keep filtering its embedded sidechains.
        reader.sidechain =
            discovered.provider == Provider::Claude && discovered.parent_id.is_some();
        Self {
            reader,
            tail: Tail::new(&discovered.path),
            subagent: discovered.hidden,
            encrypted_history: false,
            readable: false,
        }
    }
}

impl Reader {
    pub fn new(
        provider: Provider,
        records: &'static Records,
        hooks: &'static dyn Hooks,
        meta: SessionMeta,
    ) -> Self {
        Self {
            provider,
            sidechain: false,
            records,
            hooks,
            usage: Default::default(),
            meta,
            state: SessionState::Idle,
            custom_title: false,
            bootstrap: records.bootstrap_until.is_some(),
            calls: HashMap::new(),
            cards: Default::default(),
        }
    }

    pub fn provider(&self) -> Provider {
        self.provider
    }

    pub fn meta(&self) -> &SessionMeta {
        &self.meta
    }

    /// `meta().title` is a name the user gave the session.
    pub fn custom_title(&self) -> bool {
        self.custom_title
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    /// The source was rewritten: earlier messages are void.
    pub fn reset(&mut self) {
        self.usage = Default::default();
        self.bootstrap = self.records.bootstrap_until.is_some();
        self.calls.clear();
        self.cards = Default::default();
        self.custom_title = false;
    }

    /// The messages of records appended since the last read, each at its
    /// offset (a file position or an event sequence number), which names
    /// its messages; and whether the title, cwd or state changed.
    pub fn read(&mut self, records: &[(u64, Value)]) -> (Vec<Message>, bool) {
        let before = (self.meta.title.clone(), self.meta.cwd.clone(), self.state);
        let mut out = Vec::new();
        for (offset, record) in records {
            let normalized;
            let record = if self.sidechain && record["isSidechain"] == true {
                normalized = {
                    let mut value = record.clone();
                    value["isSidechain"] = Value::Bool(false);
                    value
                };
                &normalized
            } else {
                record
            };
            let summary = self.hooks.usage(&mut self.usage, *offset, record);
            // A new prompt may close an interrupted previous turn: its
            // summary then stays before the prompt in the transcript.
            let summary_first = self
                .records
                .summary_before
                .as_ref()
                .is_some_and(|m| m.matches(record));
            if summary_first && let Some(summary) = summary.clone() {
                out.push(summary);
            }
            self.ingest(*offset, record, &mut out);
            if !summary_first && let Some(summary) = summary {
                out.push(summary);
            }
        }
        let changed = before != (self.meta.title.clone(), self.meta.cwd.clone(), self.state);
        (out, changed)
    }

    /// One message of the record at `at`.
    fn message_of(at: &RecordAt<'_>, id: String, part: Part<'_>, text: String) -> Message {
        Message {
            id,
            role: part.role,
            kind: part.kind,
            text,
            ts_unix: at.ts,
            usage: None,
            phase: at.phase.to_string(),
            tool_name: part.tool.0.to_string(),
            tool_call_id: part.tool.1.to_string(),
        }
    }

    fn tool_call(&mut self, value: &Value, id: String, at: &RecordAt<'_>) -> Message {
        let rule = &self.records.tool_call;
        let name = field(value, rule.name)
            .as_str()
            .unwrap_or("tool")
            .to_string();
        let call_id = field(value, rule.id).as_str().unwrap_or("").to_string();
        if !call_id.is_empty() {
            self.calls.insert(call_id.clone(), name.clone());
        }
        let args = first(value, rule.arguments);
        if self.provider == Provider::Claude {
            self.cards.call(&call_id, &name, args);
        }
        let text = format!("{name} {}", clip(&compact(args), 16_384));
        let part = Part {
            role: Role::Assistant,
            kind: MessageKind::ToolCall,
            tool: (&name, &call_id),
        };
        Self::message_of(at, id, part, text)
    }

    fn tool_result(
        &mut self,
        value: &Value,
        metadata: &Value,
        id: String,
        at: &RecordAt<'_>,
    ) -> Option<Message> {
        let rule = &self.records.tool_result;
        let call_id = field(value, rule.id).as_str().unwrap_or("").to_string();
        let name = self.calls.remove(&call_id).unwrap_or_else(|| "tool".into());
        if self.provider == Provider::Claude {
            if let Some(body) = self.cards.result(&call_id, value, metadata) {
                return Some(Self::message_of(
                    at,
                    id,
                    Part {
                        role: Role::Assistant,
                        kind: MessageKind::ProviderCard,
                        tool: (&name, &call_id),
                    },
                    body,
                ));
            }
        }
        let output = clip(&output_text(field(value, rule.output)), 16_384);
        if output.trim().is_empty() {
            return None;
        }
        let error = rule
            .error
            .is_some_and(|path| field(value, path).as_bool() == Some(true));
        let interaction = rule.interaction_tools.contains(&name.as_str());
        let text = if error {
            format!("{name} (error): {output}")
        } else {
            format!("{name}: {output}")
        };
        let (role, kind) = if interaction {
            (Role::Assistant, MessageKind::Interaction)
        } else {
            (Role::Tool, MessageKind::ToolResult)
        };
        let part = Part {
            role,
            kind,
            tool: (&name, &call_id),
        };
        Some(Self::message_of(at, id, part, text))
    }

    fn ingest(&mut self, offset: u64, record: &Value, out: &mut Vec<Message>) {
        let records = self.records;
        if skipped(records, record) {
            return;
        }
        if self.meta.cwd.as_os_str().is_empty()
            && let Some(cwd) = first(record, records.cwd).as_str()
        {
            self.meta.cwd = PathBuf::from(cwd);
        }
        if let Some((title, custom)) = self.hooks.title(record) {
            if custom || !self.custom_title {
                self.meta.title = title;
                self.custom_title |= custom;
            }
            return;
        }
        if self.bootstrap
            && let Some(until) = &records.bootstrap_until
            && until.matches(record)
        {
            self.bootstrap = false;
            self.calls.clear();
            self.cards = Default::default();
        }
        let ts = timestamp(records, record);
        let id = offset.to_string();
        // The turn phase a message or tool record reports, if the grammar
        // has one; tool records keep the message envelope.
        let phase = records
            .message
            .phase
            .map(|p| {
                field(field(record, records.message.envelope), p)
                    .as_str()
                    .unwrap_or("")
                    .to_string()
            })
            .unwrap_or_default();
        let at = RecordAt { ts, phase: &phase };
        let ends = records.turn.ends(record);
        let starts = records
            .turn
            .started
            .as_ref()
            .is_some_and(|m| m.matches(record));
        // Messages of this record, buffered so the bootstrap can drop them
        // and a turn end can mark its final answer.
        let mut messages: Vec<Message> = Vec::new();
        let mut user_sent = false;
        if records.message.when.matches(record) {
            let envelope = field(record, records.message.envelope);
            let rule = &records.message;
            let role = field(envelope, rule.role)
                .as_str()
                .or_else(|| rule.role_fallback.and_then(|p| field(record, p).as_str()));
            let role = match (role, rule.other_roles) {
                (Some("user"), _) => Role::User,
                (Some("assistant"), _) => Role::Assistant,
                (_, OtherRoles::Assistant) => Role::Assistant,
                (_, OtherRoles::Skip) => return,
            };
            let content = field(envelope, rule.content);
            let block_id = |i: usize| {
                if i == 0 {
                    id.clone()
                } else {
                    format!("{id}:{i}")
                }
            };
            match content {
                Value::String(raw) => {
                    let text = self.hooks.text(role, raw);
                    if !text.trim().is_empty() {
                        messages.push(Self::message_of(&at, id.clone(), Part::text(role), text));
                    }
                    user_sent = role == Role::User;
                }
                Value::Array(blocks) => {
                    let mut texts = 0usize;
                    for (i, block) in blocks.iter().enumerate() {
                        let kind = block["type"].as_str().unwrap_or("");
                        if rule.text_blocks.contains(&kind) {
                            let index = match rule.block_ids {
                                BlockIds::All => i,
                                BlockIds::Text => texts,
                            };
                            let raw = block["text"].as_str().unwrap_or("");
                            if raw.is_empty() && rule.block_ids == BlockIds::Text {
                                continue;
                            }
                            texts += 1;
                            let text = self.hooks.text(role, raw);
                            if text.trim().is_empty() {
                                continue;
                            }
                            messages.push(Self::message_of(
                                &at,
                                block_id(index),
                                Part::text(role),
                                text,
                            ));
                        } else if matches!(records.tool_call.at, Where::Block(t) if t == kind) {
                            let message = self.tool_call(block, block_id(i), &at);
                            messages.push(message);
                        } else if matches!(records.tool_result.at, Where::Block(t) if t == kind) {
                            if let Some(message) =
                                self.tool_result(block, &record["toolUseResult"], block_id(i), &at)
                            {
                                if message.kind == MessageKind::ProviderCard
                                    && message.tool_name == crate::claude_cards::WRAP
                                {
                                    if let Some(plan) = self.cards.plan_body() {
                                        messages.push(Self::message_of(
                                            &at,
                                            block_id(blocks.len() + i),
                                            Part {
                                                role: Role::Assistant,
                                                kind: MessageKind::ProviderCard,
                                                tool: (
                                                    crate::claude_cards::PLAN,
                                                    &message.tool_call_id,
                                                ),
                                            },
                                            plan,
                                        ));
                                    }
                                }
                                messages.push(message);
                            }
                        }
                    }
                }
                _ => {}
            }
            if role == Role::User && messages.iter().any(|m| m.kind == MessageKind::Text) {
                user_sent = true;
            }
            if ends && records.turn.mark_final {
                for message in messages.iter_mut().filter(|m| m.kind == MessageKind::Text) {
                    message.phase = "final_answer".into();
                }
            }
        } else if let Where::Record(m) = &records.tool_call.at
            && m.matches(record)
        {
            let envelope = field(record, records.message.envelope);
            let message = self.tool_call(envelope, id.clone(), &at);
            messages.push(message);
        } else if let Where::Record(m) = &records.tool_result.at
            && m.matches(record)
        {
            let envelope = field(record, records.message.envelope);
            if let Some(message) = self.tool_result(envelope, &Value::Null, id.clone(), &at) {
                messages.push(message);
            }
        }
        if starts {
            self.state = SessionState::Running;
        }
        if ends {
            self.state = SessionState::Idle;
        }
        if self.bootstrap {
            return;
        }
        if records.turn.user_text_starts && user_sent && !ends {
            self.state = SessionState::Running;
        }
        if self.meta.title.starts_with("session ")
            && let Some(prompt) = messages
                .iter()
                .find(|m| m.role == Role::User && m.kind == MessageKind::Text)
        {
            self.meta.title = crate::model::title_from(&prompt.text);
        }
        out.extend(messages);
    }
}

impl Viewer for JsonlViewer {
    fn provider(&self) -> Provider {
        self.reader.provider
    }
    fn meta(&self) -> &SessionMeta {
        &self.reader.meta
    }
    fn custom_title(&self) -> bool {
        self.reader.custom_title
    }
    fn state(&self) -> SessionState {
        self.reader.state
    }
    fn transcript_notice(&self) -> &str {
        if self.subagent && self.encrypted_history && !self.readable {
            "This subagent's local record contains only an encrypted summary. Readable messages are not available yet."
        } else {
            ""
        }
    }
    fn path(&self) -> &Path {
        self.tail.path()
    }
    fn poll(&mut self) -> std::io::Result<Polled> {
        let previous_notice = self.transcript_notice().to_owned();
        let read = self.tail.read()?;
        let mut polled = Polled::default();
        if read.truncated {
            self.reader.reset();
            self.encrypted_history = false;
            self.readable = false;
            polled.reset = true;
        }
        self.encrypted_history |= read.records.iter().any(|(_, r)| {
            r["type"] == "response_item"
                && r["payload"]["type"] == "compaction"
                && r["payload"]["encrypted_content"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty())
        });
        let (messages, meta_changed) = self.reader.read(&read.records);
        self.readable |= !messages.is_empty();
        polled.messages = messages;
        polled.state = Some(self.reader.state);
        polled.meta_changed = meta_changed || previous_notice != self.transcript_notice();
        Ok(polled)
    }
}
