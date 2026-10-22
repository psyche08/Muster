//! CatDesk (bundle com.catpaw.cowork), ARCHITECTURE §3.6.2. The app keeps
//! its sessions in its own SQLite store, `catpaw-memory.db` in its data
//! directory: one `sessions` row per session (a JSON `extra`), and for the
//! current `ui-sdk` runtime one `ui_sdk_messages` row per message, keyed by
//! conversation and `seq`. Older `legacy` conversations keep a JSON array in
//! `conversations.messages`. The store is opened read-only and never
//! written; [`crate::catdesk_app`] adds what only the running app knows.
//!
//! Messages are append-only for the reader: a row is shown once it settled
//! (finished, or overtaken by a later user turn), in `seq` order, and its id
//! is the app's own `message_id`, stable across reads and restarts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde_json::Value;

use crate::model::{Message, MessageKind, Provider, Role, SessionMeta, SessionState, title_from};
use crate::text::{clip, compact, strip_hidden_blocks};
use crate::viewer::{Discovered, Polled, Viewer};

/// The store inside the app's data directory.
pub const DB: &str = "catpaw-memory.db";

/// An unfinished row older than this is not being written any more (the
/// turn was stopped or the app quit): it is shown as it is.
const STALE_MS: i64 = 10 * 60 * 1000;
const MAX_TEXT: usize = 16_384;

/// `~/Library/Application Support/catpaw-desk`, when its store exists.
pub fn home() -> Option<PathBuf> {
    let dir =
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support/catpaw-desk");
    dir.join(DB).is_file().then_some(dir)
}

fn open(db: &Path) -> rusqlite::Result<Connection> {
    // Never immutable=1: the app writes this WAL database while we read.
    let conn = Connection::open_with_flags(
        db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(Duration::from_millis(250))?;
    Ok(conn)
}

fn io(error: rusqlite::Error) -> std::io::Error {
    std::io::Error::other(error.to_string())
}

/// What changes when the store does: a write lands in the WAL first and
/// reaches the main file only at a checkpoint, so both count.
pub fn stamp(db: &Path) -> std::io::Result<(SystemTime, u64)> {
    let main = std::fs::metadata(db)?;
    let mut modified = main.modified().unwrap_or(UNIX_EPOCH);
    let mut len = main.len();
    let mut wal = db.as_os_str().to_owned();
    wal.push("-wal");
    if let Ok(wal) = std::fs::metadata(PathBuf::from(wal)) {
        modified = modified.max(wal.modified().unwrap_or(UNIX_EPOCH));
        len = len.wrapping_add(wal.len());
    }
    Ok((modified, len))
}

fn time_ms(ms: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(ms.max(0) as u64)
}

fn state_of(status: &str) -> SessionState {
    match status {
        "running" | "loading" | "streaming" => SessionState::Running,
        "action_required" => SessionState::WaitingApproval,
        "" => SessionState::Unknown,
        _ => SessionState::Idle,
    }
}

/// Every session the store lists, deleted ones left out.
pub fn scan(home: &Path) -> Vec<Discovered> {
    let db = home.join(DB);
    match scan_db(&db) {
        Ok(found) => found,
        Err(error) => {
            log::debug!("catdesk scan {}: {error}", db.display());
            Vec::new()
        }
    }
}

fn scan_db(db: &Path) -> rusqlite::Result<Vec<Discovered>> {
    let conn = open(db)?;
    // One read snapshot for the session rows and their message counts.
    let tx = conn.unchecked_transaction()?;
    let deleted: std::collections::HashSet<String> = tx
        .prepare("SELECT session_id FROM deleted_sessions")
        .and_then(|mut q| {
            q.query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<_>>()
        })
        .unwrap_or_default();
    let mut counts: HashMap<String, (u64, i64)> = HashMap::new();
    {
        let mut q = tx.prepare(
            "SELECT conversation_id, COUNT(*), MAX(updated_at_ms) FROM ui_sdk_messages GROUP BY conversation_id",
        )?;
        let rows = q.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<i64>>(2)?.unwrap_or(0),
            ))
        })?;
        for row in rows {
            let (conversation, count, updated) = row?;
            counts.insert(conversation, (count.max(0) as u64, updated));
        }
    }
    {
        let mut q = tx.prepare(
            "SELECT conversation_id, message_count, COALESCE(last_message_time, 0) FROM conversations WHERE runtime = 'legacy'",
        )?;
        let rows = q.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<i64>>(2)?.unwrap_or(0),
            ))
        })?;
        for row in rows {
            let (conversation, count, updated) = row?;
            counts
                .entry(conversation)
                .or_insert((count.max(0) as u64, updated));
        }
    }
    let mut q = tx.prepare("SELECT id, extra FROM sessions LIMIT 10001")?;
    let rows = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (id, extra) = row?;
        if deleted.contains(&id) || !crate::viewer::is_uuid(&id) {
            continue;
        }
        let Ok(extra) = serde_json::from_str::<Value>(&extra) else {
            continue;
        };
        let conversation = extra["conversationId"].as_str().unwrap_or(&id);
        let (count, last_message) = counts.get(conversation).copied().unwrap_or((0, 0));
        let updated = extra["updatedAt"]
            .as_i64()
            .or_else(|| extra["timestamp"].as_i64())
            .unwrap_or(0)
            .max(last_message);
        let status = extra["status"].as_str().unwrap_or("");
        // The app's own title, verbatim when it is one line: the desktop
        // sidebar is matched against it exactly.
        let title = extra["title"]
            .as_str()
            .map(|t| {
                if t.contains('\n') || t.chars().count() > 200 {
                    title_from(t)
                } else {
                    t.trim().to_string()
                }
            })
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| format!("session {}", &id[..8]));
        // Only paths the store itself keeps; the app reports live ones.
        let cwd = extra["orphanedProjectPath"]
            .as_str()
            .or_else(|| extra["archivedProjectPath"].as_str())
            .map(PathBuf::from)
            .unwrap_or_default();
        out.push(Discovered {
            id: id.to_ascii_lowercase(),
            provider: Provider::Catdesk,
            path: db.to_path_buf(),
            modified: time_ms(updated),
            size: count,
            archived: status == "archived" || extra["archivedAt"].is_i64(),
            hidden: false,
            parent_id: None,
            relation: "",
            // The store's title is already the one the app shows.
            custom_title: false,
            meta: SessionMeta {
                title,
                cwd,
                updated_unix: (updated.max(0) / 1000) as u64,
                archived: status == "archived",
                state: state_of(status),
            },
            entrypoint: None,
        });
    }
    Ok(out)
}

/// One session's messages, read from the store as they settle.
pub struct CatdeskViewer {
    db: PathBuf,
    session: String,
    conversation: Option<String>,
    meta: SessionMeta,
    state: SessionState,
    /// The last shown `ui_sdk_messages` row: (seq, message_id).
    last: Option<(i64, String)>,
    /// How many legacy messages were shown, and the array's byte length
    /// then (legacy conversations only).
    legacy_shown: Option<(usize, i64)>,
}

impl CatdeskViewer {
    pub fn open(d: &Discovered) -> Self {
        Self {
            db: d.path.clone(),
            session: d.id.clone(),
            conversation: None,
            meta: d.meta.clone(),
            state: SessionState::Unknown,
            last: None,
            legacy_shown: None,
        }
    }

    fn conversation(&mut self, conn: &Connection) -> rusqlite::Result<Option<String>> {
        if self.conversation.is_none() {
            self.conversation = conn
                .query_row(
                    "SELECT COALESCE(json_extract(extra, '$.conversationId'), id) FROM sessions WHERE id = ?1",
                    [&self.session],
                    |r| r.get::<_, String>(0),
                )
                .optional()?;
        }
        Ok(self.conversation.clone())
    }
}

impl Viewer for CatdeskViewer {
    fn provider(&self) -> Provider {
        Provider::Catdesk
    }
    fn meta(&self) -> &SessionMeta {
        &self.meta
    }
    fn state(&self) -> SessionState {
        self.state
    }
    fn path(&self) -> &Path {
        &self.db
    }
    fn stamp(&self) -> std::io::Result<(SystemTime, u64)> {
        stamp(&self.db)
    }
    fn poll(&mut self) -> std::io::Result<Polled> {
        let conn = open(&self.db).map_err(io)?;
        let tx = conn.unchecked_transaction().map_err(io)?;
        let mut polled = Polled::default();
        let Some(conversation) = self.conversation(&tx).map_err(io)? else {
            return Ok(polled);
        };
        let rows = ui_rows(&tx, &conversation, self.last.as_ref()).map_err(io)?;
        match rows {
            Rows::Rewritten => {
                // The app replaced earlier rows (an edited or regenerated
                // turn): what was shown no longer stands.
                self.last = None;
                polled.reset = true;
                let Rows::Appended(rows) = ui_rows(&tx, &conversation, None).map_err(io)? else {
                    return Ok(polled);
                };
                self.take_rows(rows, &mut polled);
            }
            Rows::Appended(rows) if rows.is_empty() && self.last.is_none() => {
                self.poll_legacy(&tx, &conversation, &mut polled)
                    .map_err(io)?;
            }
            Rows::Appended(rows) => self.take_rows(rows, &mut polled),
        }
        Ok(polled)
    }
}

enum Rows {
    Appended(Vec<UiRow>),
    Rewritten,
}

struct UiRow {
    seq: i64,
    message_id: String,
    role: String,
    payload: Value,
    created_ms: i64,
    updated_ms: i64,
}

fn ui_rows(
    conn: &Connection,
    conversation: &str,
    after: Option<&(i64, String)>,
) -> rusqlite::Result<Rows> {
    if let Some((seq, id)) = after {
        let current: Option<String> = conn
            .query_row(
                "SELECT message_id FROM ui_sdk_messages WHERE conversation_id = ?1 AND seq = ?2",
                rusqlite::params![conversation, seq],
                |r| r.get(0),
            )
            .optional()?;
        if current.as_deref() != Some(id.as_str()) {
            return Ok(Rows::Rewritten);
        }
    }
    let mut q = conn.prepare(
        "SELECT seq, message_id, role, payload, COALESCE(created_at_ms, updated_at_ms), updated_at_ms \
         FROM ui_sdk_messages WHERE conversation_id = ?1 AND seq > ?2 ORDER BY seq",
    )?;
    let rows = q.query_map(
        rusqlite::params![conversation, after.map(|a| a.0).unwrap_or(i64::MIN)],
        |r| {
            Ok(UiRow {
                seq: r.get(0)?,
                message_id: r.get(1)?,
                role: r.get(2)?,
                payload: serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or(Value::Null),
                created_ms: r.get(4)?,
                updated_ms: r.get(5)?,
            })
        },
    )?;
    Ok(Rows::Appended(rows.collect::<rusqlite::Result<_>>()?))
}

impl CatdeskViewer {
    fn take_rows(&mut self, rows: Vec<UiRow>, polled: &mut Polled) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let last_user = rows.iter().rposition(|r| r.role == "user");
        let mut waiting = false;
        for (index, row) in rows.iter().enumerate() {
            let settled = row.payload["finished"] == true
                || !matches!(row.role.as_str(), "assistant" | "tool")
                || last_user.is_some_and(|u| u > index)
                || now - row.updated_ms > STALE_MS;
            if !settled {
                waiting = true;
                break;
            }
            let messages = ui_messages(row);
            if row.role == "user" && self.meta.title.starts_with("session ") {
                if let Some(m) = messages.iter().find(|m| m.kind == MessageKind::Text) {
                    self.meta.title = title_from(&m.text);
                    polled.meta_changed = true;
                }
            }
            polled.messages.extend(messages);
            self.last = Some((row.seq, row.message_id.clone()));
        }
        // A pending row or an unanswered prompt means the model is working.
        let shown_last = self
            .last
            .as_ref()
            .and_then(|(seq, _)| rows.iter().find(|r| r.seq == *seq));
        let state = if waiting {
            SessionState::Running
        } else if let Some(row) = shown_last {
            if row.role == "user" {
                SessionState::Running
            } else {
                SessionState::Idle
            }
        } else {
            self.state
        };
        if state != self.state {
            self.state = state;
            polled.state = Some(state);
        }
    }

    fn poll_legacy(
        &mut self,
        conn: &Connection,
        conversation: &str,
        polled: &mut Polled,
    ) -> rusqlite::Result<()> {
        let length: Option<i64> = conn
            .query_row(
                "SELECT length(messages) FROM conversations WHERE conversation_id = ?1 AND runtime = 'legacy'",
                [conversation],
                |r| r.get(0),
            )
            .optional()?;
        let Some(length) = length else {
            return Ok(());
        };
        // The store changes for every session; reparse only this one's array.
        if self.legacy_shown.is_some_and(|(_, l)| l == length) {
            return Ok(());
        }
        let messages: String = conn.query_row(
            "SELECT messages FROM conversations WHERE conversation_id = ?1",
            [conversation],
            |r| r.get(0),
        )?;
        let items = serde_json::from_str::<Value>(&messages)
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        let shown = self.legacy_shown.map(|(n, _)| n).unwrap_or(0);
        if items.len() < shown {
            polled.reset = true;
        }
        let from = if polled.reset { 0 } else { shown };
        for item in &items[from.min(items.len())..] {
            polled.messages.extend(legacy_messages(item));
        }
        self.legacy_shown = Some((items.len(), length));
        if self.state == SessionState::Unknown {
            self.state = SessionState::Idle;
            polled.state = Some(SessionState::Idle);
        }
        Ok(())
    }
}

fn text_message(id: String, role: Role, text: String, ts_unix: u64) -> Message {
    Message {
        id,
        role,
        kind: MessageKind::Text,
        text,
        ts_unix,
        usage: None,
        phase: String::new(),
        tool_name: String::new(),
        tool_call_id: String::new(),
    }
}

fn tool_messages(
    id: &str,
    name: &str,
    call_id: &str,
    input: &str,
    output: Option<&str>,
    error: bool,
    ts_unix: u64,
) -> Vec<Message> {
    let tool = |id: String, role, kind, text| Message {
        id,
        role,
        kind,
        text,
        ts_unix,
        usage: None,
        phase: String::new(),
        tool_name: name.to_string(),
        tool_call_id: call_id.to_string(),
    };
    let mut out = vec![tool(
        format!("{id}:call"),
        Role::Assistant,
        MessageKind::ToolCall,
        format!("{name} {}", clip(input, MAX_TEXT)),
    )];
    if let Some(output) = output.filter(|o| !o.trim().is_empty()) {
        let output = clip(output, MAX_TEXT);
        out.push(tool(
            id.to_string(),
            Role::Tool,
            MessageKind::ToolResult,
            if error {
                format!("{name} (error): {output}")
            } else {
                format!("{name}: {output}")
            },
        ));
    }
    out
}

/// The text of a `content` that is a string or a list of text parts.
fn content_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter(|p| p["type"].as_str().is_none_or(|t| t == "text"))
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// One `ui_sdk_messages` row as the messages a reader sees.
fn ui_messages(row: &UiRow) -> Vec<Message> {
    let p = &row.payload;
    let ts = (row.created_ms.max(0) / 1000) as u64;
    let id = row.message_id.clone();
    match row.role.as_str() {
        "user" => {
            let text = strip_hidden_blocks(&content_text(&p["content"]));
            if text.is_empty() {
                return Vec::new();
            }
            vec![text_message(id, Role::User, text, ts)]
        }
        "assistant" => {
            let text = strip_hidden_blocks(&content_text(&p["content"]));
            if text.is_empty() {
                return Vec::new();
            }
            let mut message = text_message(id, Role::Assistant, text, ts);
            // The turn's closing answer: a finished reply that calls no tool.
            if p["finished"] == true && p["extra"]["hasToolCalls"] == false {
                message.phase = "final_answer".into();
            }
            vec![message]
        }
        "tool" => {
            let name = p["toolName"].as_str().unwrap_or("tool");
            let call_id = p["toolCallId"].as_str().unwrap_or("");
            let input = compact(&p["modelData"]["content"]);
            let output = (p["finished"] == true).then(|| compact(&p["toolResult"]));
            let error = matches!(p["status"].as_str(), Some("error" | "failed"));
            tool_messages(&id, name, call_id, &input, output.as_deref(), error, ts)
        }
        "task-notification" => {
            let text = strip_hidden_blocks(&content_text(&p["content"]));
            if text.is_empty() {
                return Vec::new();
            }
            vec![text_message(id, Role::System, text, ts)]
        }
        _ => Vec::new(),
    }
}

/// `2026-03-23T03:31:43.978+0000` (no colon in the offset) as unix seconds.
fn legacy_time(s: &str) -> u64 {
    let fixed = match s.len().checked_sub(5).map(|i| s.split_at(i)) {
        Some((head, tail))
            if (tail.starts_with('+') || tail.starts_with('-'))
                && tail[1..].bytes().all(|b| b.is_ascii_digit()) =>
        {
            format!("{head}{}:{}", &tail[..3], &tail[3..])
        }
        _ => s.to_string(),
    };
    crate::model::parse_ts(&fixed)
}

/// One element of a legacy conversation's message array.
fn legacy_messages(item: &Value) -> Vec<Message> {
    let id = item["messageId"].as_str().unwrap_or("");
    if id.is_empty() {
        return Vec::new();
    }
    let ts = item["createTime"].as_str().map(legacy_time).unwrap_or(0);
    let role = match item["type"].as_str() {
        Some("user") => Role::User,
        Some("assistant") => Role::Assistant,
        Some("tool") => Role::Tool,
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for (i, part) in item["content"].as_array().into_iter().flatten().enumerate() {
        let pid = format!("{id}:{i}");
        match part["type"].as_str() {
            Some("text") if role != Role::Tool => {
                let text = strip_hidden_blocks(part["text"].as_str().unwrap_or(""));
                if !text.is_empty() {
                    out.push(text_message(pid, role, text, ts));
                }
            }
            Some("tool_use") => out.extend(tool_messages(
                &pid,
                part["toolName"].as_str().unwrap_or("tool"),
                part["toolCallId"].as_str().unwrap_or(""),
                &compact(&part["toolParams"]),
                None,
                false,
                ts,
            )),
            Some("tool_result") => {
                let name = part["toolName"].as_str().unwrap_or("tool");
                let output = clip(&compact(&part["toolResult"]), MAX_TEXT);
                if output.trim().is_empty() {
                    continue;
                }
                out.push(Message {
                    id: pid,
                    role: Role::Tool,
                    kind: MessageKind::ToolResult,
                    text: if part["isError"] == true {
                        format!("{name} (error): {output}")
                    } else {
                        format!("{name}: {output}")
                    },
                    ts_unix: ts,
                    usage: None,
                    phase: String::new(),
                    tool_name: name.to_string(),
                    tool_call_id: part["toolCallId"].as_str().unwrap_or("").to_string(),
                });
            }
            _ => {}
        }
    }
    out
}

/// Confirms a send from a user row the app appended after the press with
/// exactly the sent text, never from the AX click itself.
pub struct Receipt {
    db: PathBuf,
    conversation: String,
    after: i64,
    text: String,
    invalidated: bool,
}

impl Receipt {
    pub fn new(db: &Path, session: &str, text: &str) -> std::io::Result<Self> {
        let conn = open(db).map_err(io)?;
        let conversation: String = conn
            .query_row(
                "SELECT COALESCE(json_extract(extra, '$.conversationId'), id) FROM sessions WHERE id = ?1",
                [session],
                |r| r.get(0),
            )
            .optional()
            .map_err(io)?
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no such session"))?;
        let after: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(seq), -1) FROM ui_sdk_messages WHERE conversation_id = ?1",
                [&conversation],
                |r| r.get(0),
            )
            .map_err(io)?;
        Ok(Self {
            db: db.to_path_buf(),
            conversation,
            after,
            text: text.trim_end().into(),
            invalidated: false,
        })
    }

    /// The conversation lost rows it had at the press: "newly appended"
    /// can no longer be told from rewritten.
    pub fn invalidated(&self) -> bool {
        self.invalidated
    }

    pub fn confirmed(&mut self) -> bool {
        let Ok(conn) = open(&self.db) else {
            return false;
        };
        let Ok(max) = conn.query_row(
            "SELECT COALESCE(MAX(seq), -1) FROM ui_sdk_messages WHERE conversation_id = ?1",
            [&self.conversation],
            |r| r.get::<_, i64>(0),
        ) else {
            return false;
        };
        self.invalidated |= max < self.after;
        if self.invalidated {
            return false;
        }
        let Ok(mut q) = conn.prepare(
            "SELECT payload FROM ui_sdk_messages WHERE conversation_id = ?1 AND seq > ?2 AND role = 'user'",
        ) else {
            return false;
        };
        let Ok(rows) = q.query_map(rusqlite::params![self.conversation, self.after], |r| {
            r.get::<_, String>(0)
        }) else {
            return false;
        };
        rows.flatten()
            .filter_map(|payload| serde_json::from_str::<Value>(&payload).ok())
            .any(|p| content_text(&p["content"]).trim_end() == self.text)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The app's schema as of CatDesk 2026.917 (only what Muster reads).
    pub(crate) fn store(dir: &Path) -> PathBuf {
        let db = dir.join(DB);
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE sessions (id TEXT PRIMARY KEY, extra TEXT NOT NULL,
               conversation_id TEXT GENERATED ALWAYS AS (json_extract(extra, '$.conversationId')) STORED);
             CREATE TABLE conversations (conversation_id TEXT PRIMARY KEY, project_id TEXT NOT NULL,
               messages TEXT NOT NULL, message_count INTEGER NOT NULL DEFAULT 0,
               first_message_time INTEGER, last_message_time INTEGER,
               updated_at INTEGER DEFAULT (strftime('%s', 'now')),
               runtime TEXT NOT NULL DEFAULT 'legacy', schema_version INTEGER NOT NULL DEFAULT 1);
             CREATE TABLE ui_sdk_messages (conversation_id TEXT NOT NULL, seq INTEGER NOT NULL,
               message_id TEXT NOT NULL, role TEXT NOT NULL, payload TEXT NOT NULL,
               created_at_ms INTEGER, updated_at_ms INTEGER NOT NULL,
               schema_version INTEGER NOT NULL DEFAULT 1, PRIMARY KEY (conversation_id, seq));
             CREATE TABLE deleted_sessions (session_id TEXT PRIMARY KEY, conversation_id TEXT,
               deleted_at INTEGER NOT NULL DEFAULT 0);",
        )
        .unwrap();
        db
    }

    pub(crate) fn add_session(db: &Path, id: &str, title: &str, status: &str) {
        let conn = Connection::open(db).unwrap();
        let extra = serde_json::json!({"id": id, "title": title, "conversationId": id,
            "status": status, "timestamp": 1_789_364_192_619_i64, "updatedAt": 1_789_364_314_301_i64,
            "projectId": "p"});
        conn.execute(
            "INSERT INTO sessions (id, extra) VALUES (?1, ?2)",
            rusqlite::params![id, extra.to_string()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO conversations (conversation_id, project_id, messages, runtime) VALUES (?1, 'p', '[]', 'ui-sdk')",
            [id],
        )
        .unwrap();
    }

    pub(crate) fn add_row(db: &Path, conversation: &str, seq: i64, role: &str, payload: Value) {
        let conn = Connection::open(db).unwrap();
        let id = payload["id"]
            .as_str()
            .or_else(|| payload["toolCallId"].as_str())
            .unwrap_or("x")
            .to_string();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        conn.execute(
            "INSERT OR REPLACE INTO ui_sdk_messages (conversation_id, seq, message_id, role, payload, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            rusqlite::params![conversation, seq, format!("{role}:{id}"), role, payload.to_string(), now],
        )
        .unwrap();
    }

    const S1: &str = "e6841c79-d515-4c39-aa56-3b70c404b093";

    /// `MUSTER_CATDESK_HOME=<dir with a copy of the store> cargo test -p
    /// muster-agent inspect_store -- --ignored --nocapture`: content-free
    /// counts per session from a real store.
    #[test]
    #[ignore = "reads a real CatDesk store"]
    fn inspect_store() {
        let home = PathBuf::from(std::env::var_os("MUSTER_CATDESK_HOME").unwrap());
        let found = scan(&home);
        println!("sessions={}", found.len());
        for d in &found {
            let mut viewer = CatdeskViewer::open(d);
            let polled = viewer.poll().unwrap();
            let mut kinds: HashMap<&str, usize> = HashMap::new();
            for m in &polled.messages {
                *kinds.entry(m.kind.as_str()).or_default() += 1;
            }
            let finals = polled
                .messages
                .iter()
                .filter(|m| m.phase == "final_answer")
                .count();
            println!(
                "{} archived={} rows={} messages={} {kinds:?} final={finals} state={:?} title_chars={}",
                &d.id[..8],
                d.archived,
                d.size,
                polled.messages.len(),
                viewer.state(),
                d.meta.title.chars().count()
            );
        }
    }

    #[test]
    fn scans_sessions_and_hides_deleted_ones() {
        let dir = tempfile::tempdir().unwrap();
        let db = store(dir.path());
        add_session(&db, S1, "Sandbox 查询接口接入指南", "success");
        let gone = "f7779b23-8b79-4b62-b2bf-429fad56db1b";
        add_session(&db, gone, "gone", "success");
        let archived = "45b71b93-f867-4e52-9b62-70ec9798abcc";
        add_session(&db, archived, "old", "archived");
        Connection::open(&db)
            .unwrap()
            .execute(
                "INSERT INTO deleted_sessions (session_id) VALUES (?1)",
                [gone],
            )
            .unwrap();
        add_row(
            &db,
            S1,
            1,
            "user",
            serde_json::json!({"id":"u1","role":"user","content":"hi"}),
        );
        let mut found = scan(dir.path());
        found.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].id, archived);
        assert!(found[0].archived);
        assert_eq!(found[1].id, S1);
        assert_eq!(found[1].meta.title, "Sandbox 查询接口接入指南");
        assert_eq!(found[1].provider, Provider::Catdesk);
        assert_eq!(found[1].size, 1);
        assert!(!found[1].archived);
    }

    #[test]
    fn shows_settled_rows_in_order_and_waits_for_streaming_ones() {
        let dir = tempfile::tempdir().unwrap();
        let db = store(dir.path());
        add_session(&db, S1, "t", "success");
        add_row(
            &db,
            S1,
            1,
            "user",
            serde_json::json!({"id":"u1","role":"user","content":"列出文件"}),
        );
        add_row(
            &db,
            S1,
            2,
            "assistant",
            serde_json::json!({"id":"a1","role":"assistant","finished":true,
            "content":"我先看看","extra":{"hasToolCalls":true}}),
        );
        add_row(
            &db,
            S1,
            3,
            "tool",
            serde_json::json!({"role":"tool","toolName":"list_dir","toolCallId":"call_1",
            "modelData":{"content":"{\"relative_workspace_path\":\".\"}"},"toolResult":"a.txt","status":"success","finished":true}),
        );
        add_row(
            &db,
            S1,
            4,
            "assistant",
            serde_json::json!({"id":"a2","role":"assistant","finished":false,"content":"只有一"}),
        );
        let found = scan(dir.path());
        let mut viewer = CatdeskViewer::open(&found[0]);
        let polled = viewer.poll().unwrap();
        let kinds: Vec<_> = polled.messages.iter().map(|m| (m.role, m.kind)).collect();
        assert_eq!(
            kinds,
            [
                (Role::User, MessageKind::Text),
                (Role::Assistant, MessageKind::Text),
                (Role::Assistant, MessageKind::ToolCall),
                (Role::Tool, MessageKind::ToolResult),
            ]
        );
        assert_eq!(polled.messages[0].id, "user:u1");
        assert_eq!(polled.messages[2].id, "tool:call_1:call");
        assert_eq!(polled.messages[3].text, "list_dir: a.txt");
        assert_eq!(polled.messages[3].tool_call_id, "call_1");
        assert_eq!(viewer.state(), SessionState::Running);
        assert!(viewer.poll().unwrap().messages.is_empty());
        // The reply finishes: shown once, as the turn's final answer.
        add_row(
            &db,
            S1,
            4,
            "assistant",
            serde_json::json!({"id":"a2","role":"assistant","finished":true,
            "content":"只有一个文件","extra":{"hasToolCalls":false}}),
        );
        let polled = viewer.poll().unwrap();
        assert_eq!(polled.messages.len(), 1);
        assert_eq!(polled.messages[0].text, "只有一个文件");
        assert_eq!(polled.messages[0].phase, "final_answer");
        assert_eq!(viewer.state(), SessionState::Idle);
        // Rows the app rewrote void what was shown.
        Connection::open(&db)
            .unwrap()
            .execute("DELETE FROM ui_sdk_messages WHERE seq > 1", [])
            .unwrap();
        add_row(
            &db,
            S1,
            2,
            "assistant",
            serde_json::json!({"id":"b1","role":"assistant","finished":true,"content":"重来"}),
        );
        let polled = viewer.poll().unwrap();
        assert!(polled.reset);
        assert_eq!(polled.messages.len(), 2);
        assert_eq!(polled.messages[1].text, "重来");
    }

    #[test]
    fn a_later_user_turn_settles_an_abandoned_row() {
        let dir = tempfile::tempdir().unwrap();
        let db = store(dir.path());
        add_session(&db, S1, "t", "success");
        add_row(
            &db,
            S1,
            1,
            "user",
            serde_json::json!({"id":"u1","role":"user","content":"one"}),
        );
        add_row(
            &db,
            S1,
            2,
            "tool",
            serde_json::json!({"role":"tool","toolName":"run","toolCallId":"c",
            "modelData":{"content":"{}"},"toolResult":"","status":"executing","finished":false}),
        );
        add_row(
            &db,
            S1,
            3,
            "user",
            serde_json::json!({"id":"u2","role":"user","content":"two"}),
        );
        let found = scan(dir.path());
        let mut viewer = CatdeskViewer::open(&found[0]);
        let texts: Vec<_> = viewer
            .poll()
            .unwrap()
            .messages
            .into_iter()
            .map(|m| m.text)
            .collect();
        assert_eq!(texts, ["one", "run {}", "two"]);
    }

    #[test]
    fn reads_legacy_conversations() {
        let dir = tempfile::tempdir().unwrap();
        let db = store(dir.path());
        add_session(&db, S1, "t", "success");
        let items = serde_json::json!([
            {"type":"user","messageId":"m1","createTime":"2026-03-23T03:31:43.978+0000",
             "content":[{"type":"text","text":"<system-reminder>x</system-reminder>帮我看看"}]},
            {"type":"assistant","messageId":"m2","createTime":"2026-03-23T03:31:44.000+0000",
             "content":[{"type":"text","text":"好的"},{"type":"tool_use","toolCallId":"t1","toolName":"read_file","toolParams":"{\"target_file\":\"a\"}"}]},
            {"type":"tool","messageId":"m3","content":[{"type":"tool_result","toolCallId":"t1","toolName":"read_file","toolResult":"boom","isError":true}]},
            {"type":"agent_duration","messageId":"m4","content":null}
        ]);
        Connection::open(&db)
            .unwrap()
            .execute(
                "UPDATE conversations SET runtime = 'legacy', messages = ?1 WHERE conversation_id = ?2",
                rusqlite::params![items.to_string(), S1],
            )
            .unwrap();
        let found = scan(dir.path());
        let mut viewer = CatdeskViewer::open(&found[0]);
        let polled = viewer.poll().unwrap();
        let texts: Vec<_> = polled.messages.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "帮我看看",
                "好的",
                "read_file {\"target_file\":\"a\"}",
                "read_file (error): boom"
            ]
        );
        assert_eq!(
            polled.messages[0].ts_unix,
            crate::model::parse_ts("2026-03-23T03:31:43Z")
        );
        assert!(viewer.poll().unwrap().messages.is_empty());
    }

    #[test]
    fn receipt_needs_a_new_user_row_with_the_exact_text() {
        let dir = tempfile::tempdir().unwrap();
        let db = store(dir.path());
        add_session(&db, S1, "t", "success");
        add_row(
            &db,
            S1,
            1,
            "user",
            serde_json::json!({"id":"u1","role":"user","content":"hello"}),
        );
        let mut receipt = Receipt::new(&db, S1, "hello").unwrap();
        assert!(
            !receipt.confirmed(),
            "an earlier identical prompt is not a receipt"
        );
        add_row(
            &db,
            S1,
            2,
            "user",
            serde_json::json!({"id":"u2","role":"user","content":"hello"}),
        );
        assert!(receipt.confirmed());
        Connection::open(&db)
            .unwrap()
            .execute("DELETE FROM ui_sdk_messages", [])
            .unwrap();
        assert!(!receipt.confirmed());
        assert!(receipt.invalidated());
    }

    #[test]
    fn stamp_follows_the_wal() {
        let dir = tempfile::tempdir().unwrap();
        let db = store(dir.path());
        add_session(&db, S1, "t", "success");
        // Keep a writer open so the WAL is not folded away on close.
        let writer = Connection::open(&db).unwrap();
        writer
            .execute_batch("PRAGMA wal_autocheckpoint=0;")
            .unwrap();
        let before = stamp(&db).unwrap();
        writer
            .execute(
                "INSERT INTO ui_sdk_messages VALUES (?1, 1, 'user:u', 'user', '{}', 0, 0, 1)",
                [S1],
            )
            .unwrap();
        assert_ne!(stamp(&db).unwrap(), before);
    }
}
