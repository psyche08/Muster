//! Codex's migrated local history. SQLite is opened read-only, including WAL.
use crate::{
    model::{Message, MessageKind, Provider, Role, SessionMeta, SessionState},
    text::{is_wrapper, strip_hidden_blocks},
    viewer::{Discovered, Polled, Viewer, is_uuid},
};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
fn open(path: &Path) -> rusqlite::Result<Connection> {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    db.busy_timeout(Duration::from_millis(100))?;
    Ok(db)
}
pub(crate) fn is(d: &Discovered) -> bool {
    d.provider == Provider::Codex
        && d.path
            .file_name()
            .is_some_and(|n| n == "thread_history_1.sqlite")
}
pub(crate) fn merge(home: &Path, found: &mut Vec<Discovered>) {
    if let Ok(rows) = scan(home) {
        for d in rows {
            if let Some(existing) = found.iter_mut().find(|s| s.id == d.id) {
                if d.entrypoint.as_deref() == Some("paginated") {
                    *existing = d;
                }
            } else {
                found.push(d);
            }
        }
    }
}
fn scan(home: &Path) -> rusqlite::Result<Vec<Discovered>> {
    let history = home.join("thread_history_1.sqlite");
    if !history.is_file() {
        return Ok(Vec::new());
    }
    let db = open(&home.join("state_5.sqlite"))?;
    let mut q=db.prepare("SELECT id, title, cwd, updated_at, source, thread_source, originator, history_mode, attached_thread_parent_id FROM threads WHERE archived = 0 ORDER BY updated_at DESC LIMIT 10000")?;
    let mut rows = q.query([])?;
    let history_db = open(&history)?;
    let titles = crate::codex::load_index(&home.join("session_index.jsonl"));
    let mut out = Vec::new();
    while let Some(r) = rows.next()? {
        let id: String = r.get(0)?;
        if !is_uuid(&id) {
            continue;
        }
        let source: String = r.get(4)?;
        let thread_source: Option<String> = r.get(5)?;
        let origin: Option<String> = r.get(6)?;
        if origin.as_deref() == Some("muster") {
            continue;
        }
        let source_json: Value = serde_json::from_str(&source).unwrap_or(Value::Null);
        let hidden = source_json.get("subagent").is_some()
            || matches!(
                thread_source.as_deref(),
                Some("guardian_review" | "subagent")
            );
        let parent = source_json["subagent"]["thread_spawn"]["parent_thread_id"]
            .as_str()
            .map(str::to_owned)
            .or(r.get::<_, Option<String>>(8)?)
            .filter(|s| is_uuid(s));
        let updated = r.get::<_, i64>(3)?.max(0) as u64;
        let message_time = last_message_time_from(&history_db, &id).unwrap_or(0);
        let title = titles
            .get(&id)
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or(r.get(1)?);
        out.push(Discovered {
            id,
            provider: Provider::Codex,
            path: history.clone(),
            modified: UNIX_EPOCH + Duration::from_secs(updated),
            size: 0,
            archived: false,
            hidden,
            parent_id: parent,
            relation: if hidden { "subagent" } else { "" },
            meta: SessionMeta {
                title,
                cwd: PathBuf::from(r.get::<_, String>(2)?),
                updated_unix: message_time,
                ..Default::default()
            },
            custom_title: true,
            entrypoint: r.get(7)?,
        });
    }
    Ok(out)
}
/// Native thread updated_at includes renames and metadata changes. Only a
/// supported user/assistant message supplies the displayed conversation time.
pub(crate) fn last_message_time(path: &Path, id: &str) -> rusqlite::Result<u64> {
    last_message_time_from(&open(path)?, id)
}
fn last_message_time_from(db: &Connection, id: &str) -> rusqlite::Result<u64> {
    let mut q = db.prepare("SELECT item_id, item_json, created_at_ms FROM thread_items WHERE thread_id=?1 AND item_type IN ('userMessage','agentMessage') AND length(item_json)<=1048576 ORDER BY rollout_ordinal DESC, item_id LIMIT 256")?;
    let mut rows = q.query([id])?;
    while let Some(row) = rows.next()? {
        let raw: String = row.get(1)?;
        let value: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
        if !matches!(value["type"].as_str(), Some("userMessage" | "agentMessage")) {
            continue;
        }
        if let Some(message) = item(&row.get::<_, String>(0)?, &raw, row.get(2)?)
            && message.ts_unix > 0
        {
            return Ok(message.ts_unix);
        }
    }
    Ok(0)
}

fn text(v: &Value) -> String {
    v.as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| v.to_string())
}
fn clip(s: &str) -> String {
    s.chars().take(16_384).collect()
}
/// Provider item text shared by history rendering and search. Never search raw JSON.
pub(crate) fn item(id: &str, raw: &str, ts: i64) -> Option<Message> {
    let v: Value = serde_json::from_str(raw).ok()?;
    let (role, kind, body, phase, tool) = match v["type"].as_str()? {
        "userMessage" => (
            Role::User,
            MessageKind::Text,
            v["content"]
                .as_array()?
                .iter()
                .filter(|p| p["type"] == "text")
                .filter_map(|p| p["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            "".into(),
            "".into(),
        ),
        "agentMessage" => (
            Role::Assistant,
            MessageKind::Text,
            strip_hidden_blocks(v["text"].as_str()?),
            v["phase"].as_str().unwrap_or("").into(),
            "".into(),
        ),
        "reasoning" => (
            Role::Assistant,
            MessageKind::Text,
            v["summary"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            "reasoning".into(),
            "".into(),
        ),
        "commandExecution" => (
            Role::Assistant,
            MessageKind::ToolCall,
            format!(
                "{}\n{}",
                v["command"].as_str().unwrap_or(""),
                clip(v["aggregatedOutput"].as_str().unwrap_or(""))
            ),
            "".into(),
            "shell".into(),
        ),
        "mcpToolCall" | "dynamicToolCall" | "collabAgentToolCall" => (
            Role::Assistant,
            MessageKind::ToolCall,
            format!(
                "{} {}",
                v["tool"].as_str().unwrap_or("tool"),
                clip(&text(&v["arguments"]))
            ),
            "".into(),
            v["tool"].as_str().unwrap_or("tool").into(),
        ),
        "fileChange" => (
            Role::Assistant,
            MessageKind::ToolCall,
            clip(&text(&v["changes"])),
            "".into(),
            "file_change".into(),
        ),
        "webSearch" => (
            Role::Assistant,
            MessageKind::ToolCall,
            v["query"].as_str().unwrap_or("").into(),
            "".into(),
            "web_search".into(),
        ),
        "contextCompaction" => (
            Role::System,
            MessageKind::Text,
            "Context compacted".into(),
            "".into(),
            "".into(),
        ),
        _ => return None,
    };
    if body.trim().is_empty() || (kind == MessageKind::Text && is_wrapper(&body)) {
        return None;
    }
    Some(Message {
        id: format!("sqlite:{id}"),
        role,
        kind,
        text: body,
        ts_unix: (ts.max(0) / 1000) as u64,
        phase,
        usage: None,
        tool_name: tool,
        tool_call_id: id.into(),
        tool_status: String::new(),
    })
}
pub(crate) struct History {
    d: Discovered,
    previous: Vec<Message>,
    notice: String,
}
impl History {
    pub fn new(d: Discovered) -> Self {
        Self {
            d,
            previous: Vec::new(),
            notice: String::new(),
        }
    }
}
impl Viewer for History {
    fn provider(&self) -> Provider {
        Provider::Codex
    }
    fn meta(&self) -> &SessionMeta {
        &self.d.meta
    }
    fn custom_title(&self) -> bool {
        true
    }
    fn state(&self) -> SessionState {
        SessionState::Unknown
    }
    fn transcript_notice(&self) -> &str {
        &self.notice
    }
    fn path(&self) -> &Path {
        &self.d.path
    }
    fn stamp(&self) -> std::io::Result<(SystemTime, u64)> {
        crate::catdesk::stamp(&self.d.path)
    }
    fn poll(&mut self) -> std::io::Result<Polled> {
        let (messages, partial) = read(&self.d).map_err(std::io::Error::other)?;
        self.notice = if partial {
            "Showing available Codex local history; some items exceed the supported history window or format.".into()
        } else if messages.is_empty() {
            "Codex has no readable local history for this conversation.".into()
        } else {
            String::new()
        };
        let append = messages.starts_with(&self.previous);
        let next = if append {
            messages[self.previous.len()..].to_vec()
        } else {
            messages.clone()
        };
        self.previous = messages;
        Ok(Polled {
            messages: next,
            reset: !append,
            ..Default::default()
        })
    }
}
fn read(d: &Discovered) -> Result<(Vec<Message>, bool), String> {
    let db = open(&d.path).map_err(|e| e.to_string())?;
    let mut q=db.prepare("SELECT turn_id, item_id, item_json, created_at_ms, length(item_json) FROM thread_items WHERE thread_id=?1 ORDER BY rollout_ordinal DESC, item_id LIMIT 10001").map_err(|e|e.to_string())?;
    let mut rows = q.query([&d.id]).map_err(|e| e.to_string())?;
    let mut messages = Vec::new();
    let mut count = 0;
    let mut bytes = 0;
    let mut partial = false;
    let mut ids = HashSet::new();
    while let Some(r) = rows.next().map_err(|e| e.to_string())? {
        count += 1;
        let len = r.get::<_, i64>(4).map_err(|e| e.to_string())?.max(0) as u64;
        if count > 10000 || bytes + len > 64 << 20 {
            partial = true;
            break;
        }
        bytes += len;
        if len > 1 << 20 {
            partial = true;
            continue;
        }
        let id = format!(
            "{}:{}",
            r.get::<_, String>(0).map_err(|e| e.to_string())?,
            r.get::<_, String>(1).map_err(|e| e.to_string())?
        );
        let raw: String = r.get(2).map_err(|e| e.to_string())?;
        if let Some(m) = item(&id, &raw, r.get(3).map_err(|e| e.to_string())?) {
            if ids.insert(m.id.clone()) {
                messages.push(m);
            }
        } else {
            partial = true;
        }
    }
    messages.reverse();
    Ok((messages, partial))
}
/// A text-only indexed query on one authorized thread, bounded independently of UI paging.
pub(crate) fn search(
    d: &Discovered,
    mut matches: impl FnMut(&str) -> Option<String>,
    mut stop: impl FnMut() -> bool,
) -> (Option<String>, bool) {
    let mut run = || -> rusqlite::Result<(Option<String>, bool)> {
        let db = open(&d.path)?;
        let mut q=db.prepare("SELECT item_id, item_json, created_at_ms, length(item_json) FROM thread_items WHERE thread_id=?1 AND item_type IN ('userMessage','agentMessage') ORDER BY rollout_ordinal DESC LIMIT 10001")?;
        let mut rows = q.query([&d.id])?;
        let mut count = 0;
        let mut bytes = 0;
        while let Some(r) = rows.next()? {
            if stop() {
                return Ok((None, true));
            }
            count += 1;
            let len = r.get::<_, i64>(3)?.max(0) as u64;
            bytes += len;
            if count > 10000 || bytes > 64 << 20 {
                return Ok((None, true));
            }
            if len > 1 << 20 {
                return Ok((None, true));
            }
            if let Some(m) = item(&r.get::<_, String>(0)?, &r.get::<_, String>(1)?, r.get(2)?)
                && m.kind == MessageKind::Text
                && matches!(m.role, Role::User | Role::Assistant)
                && m.phase != "reasoning"
                && let Some(s) = matches(&m.text)
            {
                return Ok((Some(s), false));
            }
        }
        Ok((None, false))
    };
    run().unwrap_or((None, true))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_messages_are_typed_and_preserve_stable_ids() {
        let a = item(
            "turn:item",
            r#"{"type":"agentMessage","text":"Hello World","phase":"final_answer"}"#,
            1000,
        )
        .unwrap();
        assert_eq!(a.id, "sqlite:turn:item");
        assert_eq!(a.text, "Hello World");
        assert_eq!(a.ts_unix, 1);
        let t = item(
            "tool",
            r#"{"type":"commandExecution","command":"echo example","aggregatedOutput":"example"}"#,
            1000,
        )
        .unwrap();
        assert_eq!(t.kind, MessageKind::ToolCall);
        assert!(item("system", r#"{"type":"system","text":"hidden"}"#, 0).is_none());
    }
}

#[cfg(test)]
mod store_tests {
    use super::*;
    const ID: &str = "11111111-1111-4111-8111-111111111111";
    #[test]
    fn migrated_history_is_discovered_searched_and_updated_without_a_rollout() {
        let dir = tempfile::tempdir().unwrap();
        let state = Connection::open(dir.path().join("state_5.sqlite")).unwrap();
        state.execute_batch("CREATE TABLE threads(id TEXT,title TEXT,cwd TEXT,updated_at INTEGER,source TEXT,thread_source TEXT,originator TEXT,history_mode TEXT,attached_thread_parent_id TEXT,archived INTEGER);").unwrap();
        state.execute("INSERT INTO threads VALUES(?1,'Same title','/workspace/example',10,'vscode','user','Codex Desktop','paginated',NULL,0)",[ID]).unwrap();
        let db = Connection::open(dir.path().join("thread_history_1.sqlite")).unwrap();
        db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE thread_items(thread_id TEXT,turn_id TEXT,item_id TEXT,item_json TEXT,created_at_ms INTEGER,rollout_ordinal INTEGER,item_type TEXT);").unwrap();
        let raw = r#"{"type":"userMessage","content":[{"type":"text","text":"Find this unique phrase"}]}"#;
        db.execute(
            "INSERT INTO thread_items VALUES(?1,'turn','item',?2,1000,1,'userMessage')",
            [ID, raw],
        )
        .unwrap();
        let mut found = Vec::new();
        merge(dir.path(), &mut found);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].meta.title, "Same title");
        assert_eq!(
            found[0].meta.updated_unix, 1,
            "message time, not thread updated_at"
        );
        state
            .execute("UPDATE threads SET title='Renamed', updated_at=99999", [])
            .unwrap();
        db.execute(
            "INSERT INTO thread_items VALUES(?1,'turn','tool',?2,99999000,2,'commandExecution')",
            [ID, r#"{"type":"commandExecution","command":"example"}"#],
        )
        .unwrap();
        let mut renamed = Vec::new();
        merge(dir.path(), &mut renamed);
        assert_eq!(renamed[0].meta.title, "Renamed");
        assert_eq!(
            renamed[0].meta.updated_unix, 1,
            "rename and tool activity do not change message time"
        );
        db.execute("DELETE FROM thread_items WHERE item_id='tool'", [])
            .unwrap();
        let mut viewer = History::new(found[0].clone());
        let first = viewer.poll().unwrap();
        assert_eq!(first.messages.len(), 1);
        assert!(viewer.poll().unwrap().messages.is_empty());
        let (hit, partial) = search(
            &found[0],
            |text| text.contains("unique").then(|| text.into()),
            || false,
        );
        assert!(hit.is_some());
        assert!(!partial);
        db.execute(
            "UPDATE thread_items SET item_json=?1",
            [r#"{"type":"agentMessage","text":"Updated answer"}"#],
        )
        .unwrap();
        let changed = viewer.poll().unwrap();
        assert!(changed.reset);
        assert_eq!(changed.messages[0].text, "Updated answer");
        db.execute("DELETE FROM thread_items", []).unwrap();
        let deleted = viewer.poll().unwrap();
        assert!(deleted.reset);
        assert!(deleted.messages.is_empty());
    }
}
