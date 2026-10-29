//! Read-only provider follow-up queues. Native controls remain the only write path.
use crate::{Error, PendingInteraction, model::Provider};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

pub const KIND: &str = "queued_message";
pub const SEND_NOW: &str = "Send now";
pub type Source = Arc<dyn Fn() -> Result<Vec<QueuedMessage>, Error> + Send + Sync>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueuedMessage {
    pub id: String,
    source_id: String,
    /// Retained when the app-server takes over a local deferred item. Physical
    /// migration is not a new user submission.
    client_source_id: Option<String>,
    pub text: String,
    pub can_send: bool,
}
impl QueuedMessage {
    fn identities(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.source_id.as_str()).chain(self.client_source_id.as_deref())
    }
    #[cfg(test)]
    pub(crate) fn test(source_id: &str, text: &str) -> Self {
        Self {
            id: format!("queued:000:{source_id}"),
            source_id: source_id.into(),
            client_source_id: None,
            text: text.into(),
            can_send: true,
        }
    }
    pub fn interaction(&self, session: &str) -> PendingInteraction {
        PendingInteraction {
            id: self.id.clone(),
            session: session.into(),
            kind: KIND.into(),
            prompt: if self.text.is_empty() {
                "Message with attachments".into()
            } else {
                crate::text::clip(&self.text, 8192)
            },
            optional: true,
            options: if self.can_send {
                vec![SEND_NOW.into()]
            } else {
                Vec::new()
            },
            ..Default::default()
        }
    }
}

type CachedQueueState = (
    PathBuf,
    SystemTime,
    u64,
    HashMap<String, Vec<QueuedMessage>>,
);
#[derive(Default)]
pub struct Reader {
    cached: Option<CachedQueueState>,
    claude: HashMap<PathBuf, ClaudeQueue>,
}

impl Reader {
    pub fn read(&mut self, home: &Path) -> Result<&HashMap<String, Vec<QueuedMessage>>, Error> {
        let path = home.join(".codex-global-state.json");
        let meta = std::fs::metadata(&path)
            .map_err(|_| Error::Unavailable("Cannot read the native message queue".into()))?;
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        if meta.len() > 16 * 1024 * 1024 {
            return Err(Error::Unavailable(
                "Native queue state exceeds the read limit".into(),
            ));
        }
        if !self
            .cached
            .as_ref()
            .is_some_and(|(p, t, n, _)| *p == path && *t == modified && *n == meta.len())
        {
            let bytes = std::fs::read(&path)
                .map_err(|_| Error::Unavailable("Cannot read the native message queue".into()))?;
            let queues = parse(&bytes)?;
            self.cached = Some((path, modified, meta.len(), queues));
        }
        Ok(&self.cached.as_ref().unwrap().3)
    }
}

/// Claude persists enqueue/dequeue/remove events in its session transcript.
/// Read them incrementally; never reconstruct acceptance from an AX click.
struct ClaudeQueue {
    tail: crate::transcript::Tail,
    pending: Vec<(u64, Option<String>)>,
}

pub(crate) fn read_claude(
    reader: &mut Reader,
    path: &Path,
    session: &str,
) -> Result<Vec<QueuedMessage>, Error> {
    if reader.claude.len() >= 64 && !reader.claude.contains_key(path) {
        reader.claude.clear();
    }
    let state = reader
        .claude
        .entry(path.into())
        .or_insert_with(|| ClaudeQueue {
            tail: crate::transcript::Tail::new(path),
            pending: Vec::new(),
        });
    let read = state.tail.read().map_err(|e| Error::Io(e.to_string()))?;
    if read.truncated {
        state.pending.clear();
    }
    for (offset, record) in read.records {
        if record["type"] != "queue-operation"
            || record["isSidechain"] == true
            || record["sessionId"].as_str() != Some(session)
        {
            continue;
        }
        match record["operation"].as_str() {
            Some("enqueue") => {
                // Keep slots without user text: their dequeue must not remove
                // the next visible prompt. Never invent an action for Claude.
                state
                    .pending
                    .push((offset, record["content"].as_str().map(str::to_owned)));
            }
            Some("dequeue") if !state.pending.is_empty() => {
                state.pending.remove(0);
            }
            Some("remove") => {
                if let Some(text) = record["content"].as_str()
                    && let Some(index) = state
                        .pending
                        .iter()
                        .position(|(_, t)| t.as_deref() == Some(text))
                {
                    state.pending.remove(index);
                }
            }
            _ => {}
        }
    }
    if state.pending.len() > 256 {
        return Err(Error::Unavailable(
            "Claude queue exceeds the read limit".into(),
        ));
    }
    Ok(state
        .pending
        .iter()
        .enumerate()
        .filter_map(|(index, (offset, text))| {
            let text = text.as_ref()?;
            if text.trim().is_empty() || crate::text::is_wrapper(text) {
                return None;
            }
            let hash = hex::encode(Sha256::digest(format!("{session}:{offset}:{text}")));
            Some(QueuedMessage {
                id: format!("queued:{index:03}:{hash}"),
                source_id: format!("claude:{session}:{offset}"),
                client_source_id: None,
                text: text.clone(),
                can_send: false,
            })
        })
        .collect())
}

/// The current app-server keeps its queue in SQLite (including uncheckpointed WAL).
pub fn read_session(
    reader: &mut Reader,
    home: &Path,
    session: &str,
) -> Result<Vec<QueuedMessage>, Error> {
    let mut result = if home.join(".codex-global-state.json").exists() {
        reader.read(home)?.get(session).cloned().unwrap_or_default()
    } else {
        Vec::new()
    };
    let path = home.join("queue_1.sqlite");
    if path.exists() {
        let db =
            rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|_| Error::Unavailable("Cannot read the native server queue".into()))?;
        db.busy_timeout(std::time::Duration::from_millis(100))
            .map_err(|e| Error::Unavailable(e.to_string()))?;
        let mut query = db.prepare("SELECT id, payload_json FROM queued_items WHERE thread_id=?1 ORDER BY queue_order, created_at_ms, id LIMIT 257")
            .map_err(|_| Error::Unavailable("Native server queue format is unavailable".into()))?;
        let rows = query
            .query_map([session], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(|e| Error::Unavailable(e.to_string()))?;
        let mut server = Vec::new();
        let mut bytes_read = 0;
        for row in rows {
            let (id, json) = row.map_err(|e| Error::Unavailable(e.to_string()))?;
            bytes_read += json.len();
            if json.len() > 4 * 1024 * 1024 || bytes_read > 16 * 1024 * 1024 || server.len() >= 256
            {
                return Err(Error::Unavailable(
                    "Native server queue exceeds the read limit".into(),
                ));
            }
            let payload: Value = serde_json::from_str(&json)
                .map_err(|_| Error::Unavailable("Cannot decode native queued message".into()))?;
            let input = payload["input"]
                .as_array()
                .ok_or_else(|| Error::Unavailable("Unsupported native queued input".into()))?;
            let text = input
                .iter()
                .filter_map(input_text)
                .collect::<Vec<_>>()
                .join("\n");
            let client = payload["clientUserMessageId"]
                .as_str()
                .or_else(|| payload["client_user_message_id"].as_str())
                .filter(|id| !id.is_empty());
            if client.is_some_and(|id| result.iter().any(|m| m.source_id == format!("local:{id}")))
            {
                continue;
            }
            let token = serde_json::to_vec(&(session, &id, &payload))
                .map_err(|e| Error::Unavailable(e.to_string()))?;
            server.push(QueuedMessage {
                source_id: format!("server:{id}"),
                client_source_id: client.map(|id| format!("local:{id}")),
                id: format!(
                    "queued:{:03}:{}",
                    server.len(),
                    hex::encode(Sha256::digest(token))
                ),
                can_send: !text.trim().is_empty(),
                text,
            });
        }
        // The native view displays server items before local deferred submissions.
        server.append(&mut result);
        result = server;
    }
    for (index, message) in result.iter_mut().enumerate() {
        let hash = message.id.rsplit(':').next().unwrap();
        message.id = format!("queued:{index:03}:{hash}");
    }
    Ok(result)
}

fn input_text(input: &Value) -> Option<String> {
    if !matches!(input["type"].as_str(), Some("text" | "input_text")) {
        return None;
    }
    let text = input["text"].as_str()?;
    if let Some(elements) = input["text_elements"].as_array().filter(|v| v.len() == 1) {
        let e = &elements[0];
        if e["byteRange"]["start"] == 0
            && e["byteRange"]["end"].as_u64() == Some(text.len() as u64)
            && let Some(json) = e["placeholder"]
                .as_str()
                .and_then(|p| p.strip_prefix("codex-untrusted-app-input:"))
            && let Ok(app) = serde_json::from_str::<Value>(json)
            && let Some(message) = app["message"]["text"].as_str()
        {
            return Some(message.to_owned());
        }
    }
    Some(text.into())
}

/// A newly persisted queue item is acceptance, not model execution/completion.
#[derive(Clone)]
pub struct Admission {
    provider: Provider,
    before: HashSet<String>,
    text: String,
    source: Source,
}
impl Admission {
    pub fn new(source: Source, provider: &str, text: &str) -> Result<Self, Error> {
        let provider = Provider::parse(provider)
            .ok_or_else(|| Error::Invalid("Unknown queue provider".into()))?;
        let before = source()?
            .iter()
            .flat_map(|m| m.identities().map(str::to_owned))
            .collect();
        Ok(Self {
            provider,
            before,
            text: text.trim_end().into(),
            source,
        })
    }
    pub fn confirmed(&self) -> bool {
        (self.source)().is_ok_and(|items| {
            items
                .iter()
                .filter(|m| {
                    !m.identities().any(|id| self.before.contains(id))
                        && crate::receipt::text_matches(self.provider, &self.text, &m.text)
                })
                .count()
                == 1
        })
    }
}

fn parse(bytes: &[u8]) -> Result<HashMap<String, Vec<QueuedMessage>>, Error> {
    let invalid = || Error::Unavailable("Native message queue format is unavailable".into());
    let state: Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let Some(raw) = state.get("queued-follow-ups") else {
        return Ok(HashMap::new());
    };
    let queues = raw.as_object().ok_or_else(invalid)?;
    let mut result = HashMap::new();
    for (session, entries) in queues {
        let entries = entries.as_array().ok_or_else(invalid)?;
        if entries.len() > 256 {
            return Err(invalid());
        }
        let mut ids = HashSet::new();
        let mut messages = Vec::new();
        for (position, entry) in entries.iter().enumerate() {
            let id = entry["id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(invalid)?;
            if !ids.insert(id) {
                return Err(invalid());
            }
            let text = entry["text"].as_str().ok_or_else(invalid)?;
            let context = entry
                .get("context")
                .filter(|v| v.is_object())
                .ok_or_else(invalid)?;
            // Queues for a different execution host are not local desktop actions.
            let host = entry["submission"]["hostId"].as_str();
            if host.is_some_and(|h| h != "local") {
                continue;
            }
            let status = entry["submission"]["status"].as_str();
            let can_send = entry["canSendNow"] != false
                && status.is_none_or(|s| s == "queued")
                && !text.trim().is_empty();
            // Editing text or attachments invalidates previously rendered actions.
            let token = serde_json::to_vec(&(session, id, text, context)).map_err(|_| invalid())?;
            messages.push(QueuedMessage {
                source_id: format!("local:{id}"),
                client_source_id: None,
                id: format!(
                    "queued:{position:03}:{}",
                    hex::encode(Sha256::digest(token))
                ),
                text: text.into(),
                can_send,
            });
        }
        result.insert(session.clone(), messages);
    }
    Ok(result)
}

pub fn selected<'a>(messages: &'a [QueuedMessage], id: &str) -> Result<&'a QueuedMessage, Error> {
    let message = messages
        .iter()
        .find(|m| m.id == id)
        .ok_or_else(|| Error::NotFound("This queued message changed or was already sent".into()))?;
    if !message.can_send {
        return Err(Error::Conflict(
            "This queued message cannot be sent now".into(),
        ));
    }
    if messages
        .iter()
        .filter(|m| normalize(&m.text) == normalize(&message.text))
        .count()
        != 1
    {
        return Err(Error::Conflict(
            "Identical queued messages cannot be uniquely selected in the desktop app".into(),
        ));
    }
    Ok(message)
}

pub fn ordered_interactions(items: &[PendingInteraction]) -> Vec<PendingInteraction> {
    let mut queued: Vec<_> = items.iter().filter(|i| i.kind == KIND).cloned().collect();
    queued.sort_by(|a, b| a.id.cmp(&b.id));
    items
        .iter()
        .filter(|i| i.kind != KIND)
        .cloned()
        .chain(queued)
        .collect()
}

/// Display accounting only: a clipped preview may describe an admitted send,
/// but it must never settle a receipt or release an unconfirmed-write latch.
/// Both comparisons preserve submitted literal Markdown; only the observed
/// Codex representation is eligible for equal-label URL unwrapping.
pub fn display_matches(provider: Option<Provider>, submitted: &str, preview: &str) -> bool {
    let submitted = normalize(submitted);
    let matches = |preview: &str| {
        let preview = normalize(preview);
        match preview.strip_suffix('…') {
            Some(prefix) if !prefix.is_empty() => submitted.starts_with(prefix),
            _ => submitted == preview,
        }
    };
    matches(preview)
        || (provider == Some(Provider::Codex)
            && matches(&crate::receipt::unwrap_url_links(preview)))
}

pub fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn item(id: &str, text: &str) -> Value {
        serde_json::json!({"id":id,"text":text,"context":{"prompt":text}})
    }
    #[test]
    fn formatted_queue_admission_preserves_identity_uniqueness_and_full_text() {
        const SUBMITTED: &str = "Read https://example.com/plan";
        const FORMATTED: &str = "Read [https://example.com/plan](https://example.com/plan)";
        for provider in ["codex", "claude", "catdesk"] {
            let items = Arc::new(std::sync::Mutex::new(vec![QueuedMessage::test(
                "old", "before",
            )]));
            let failed = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let (current, failure) = (items.clone(), failed.clone());
            let source: Source = Arc::new(move || {
                if failure.load(std::sync::atomic::Ordering::SeqCst) {
                    Err(Error::Unavailable("queue unavailable".into()))
                } else {
                    Ok(current.lock().unwrap().clone())
                }
            });
            let admission = Admission::new(source.clone(), provider, SUBMITTED).unwrap();
            items.lock().unwrap()[0].text = FORMATTED.into();
            assert!(!admission.confirmed(), "an edited old item cannot confirm");
            for text in [
                "Read [https://example.com/plan](https://example.com/other)",
                "Read [https://example.com/plan](https://example.com/plan) extra",
                "Read https://example.com/…",
                "Read  https://example.com/plan",
                "Read `[https://example.com/plan](https://example.com/plan)`",
            ] {
                items.lock().unwrap().truncate(1);
                items.lock().unwrap().push(QueuedMessage::test("new", text));
                assert!(!admission.confirmed(), "{provider}: {text}");
            }
            items.lock().unwrap()[1].text = FORMATTED.into();
            assert_eq!(admission.confirmed(), provider == "codex");
            items.lock().unwrap()[1].text = SUBMITTED.into();
            assert!(admission.confirmed());
            items
                .lock()
                .unwrap()
                .push(QueuedMessage::test("duplicate", SUBMITTED));
            assert!(!admission.confirmed(), "two new matches are ambiguous");
            items.lock().unwrap().pop();
            failed.store(true, std::sync::atomic::Ordering::SeqCst);
            assert!(!admission.confirmed());
            assert!(Admission::new(source, provider, SUBMITTED).is_err());
            failed.store(false, std::sync::atomic::Ordering::SeqCst);
            items.lock().unwrap().clear();
            assert!(!admission.confirmed(), "queue removal is not evidence");
        }
    }

    #[test]
    fn formatted_local_and_server_queue_items_confirm_without_a_transcript() {
        for server in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let db = if server {
                let db = rusqlite::Connection::open(dir.path().join("queue_1.sqlite")).unwrap();
                db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE queued_items (id TEXT, thread_id TEXT, payload_json TEXT, queue_order INTEGER, created_at_ms INTEGER);").unwrap();
                Some(db)
            } else {
                None
            };
            let home = dir.path().to_owned();
            let reader = std::sync::Mutex::new(Reader::default());
            let source: Source =
                Arc::new(move || read_session(&mut reader.lock().unwrap(), &home, "s"));
            let admission =
                Admission::new(source.clone(), "codex", "Read https://example.com/plan").unwrap();
            let text = "Read [https://example.com/plan](https://example.com/plan)";
            if let Some(db) = db.as_ref() {
                let payload = serde_json::json!({"input":[{"type":"text","text":text}]});
                db.execute(
                    "INSERT INTO queued_items VALUES ('new','other',?1,0,0)",
                    [payload.to_string()],
                )
                .unwrap();
                assert!(!admission.confirmed(), "another session cannot confirm");
                db.execute(
                    "INSERT INTO queued_items VALUES ('new','s',?1,0,0)",
                    [payload.to_string()],
                )
                .unwrap();
            } else {
                let state = serde_json::json!({"queued-follow-ups":{"s":[item("new",text)]}});
                std::fs::write(
                    dir.path().join(".codex-global-state.json"),
                    state.to_string(),
                )
                .unwrap();
            }
            assert!(admission.confirmed());
            let before_existing =
                Admission::new(source, "codex", "Read https://example.com/plan").unwrap();
            assert!(
                !before_existing.confirmed(),
                "already queued is not newly accepted"
            );
        }
    }

    #[test]
    fn migration_of_an_existing_local_item_is_not_a_new_receipt() {
        for client_key in ["clientUserMessageId", "client_user_message_id"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(".codex-global-state.json");
            let text = "Read https://example.com/plan";
            std::fs::write(
                &path,
                serde_json::json!({"queued-follow-ups":{"s":[item("old-client",text)]}})
                    .to_string(),
            )
            .unwrap();
            let db = rusqlite::Connection::open(dir.path().join("queue_1.sqlite")).unwrap();
            db.execute_batch("CREATE TABLE queued_items (id TEXT, thread_id TEXT, payload_json TEXT, queue_order INTEGER, created_at_ms INTEGER);").unwrap();
            let home = dir.path().to_owned();
            let reader = std::sync::Mutex::new(Reader::default());
            let source: Source =
                Arc::new(move || read_session(&mut reader.lock().unwrap(), &home, "s"));
            let admission = Admission::new(source.clone(), "codex", text).unwrap();
            let payload = |client: &str| {
                serde_json::json!({(client_key):client, "input":[{"type":"text","text":"Read [https://example.com/plan](https://example.com/plan)"}]}).to_string()
            };
            db.execute(
                "INSERT INTO queued_items VALUES ('server-old','s',?1,0,0)",
                [payload("old-client")],
            )
            .unwrap();
            assert_eq!(
                source().unwrap().len(),
                1,
                "local/server views of one item are deduplicated"
            );
            std::fs::write(&path, r#"{"queued-follow-ups":{}}"#).unwrap();
            assert!(
                !admission.confirmed(),
                "an old local item migrated, nothing new was accepted"
            );
            // A fresh item has both a new physical and a new client identity.
            db.execute(
                "INSERT INTO queued_items VALUES ('server-new','s',?1,1,0)",
                [payload("new-client")],
            )
            .unwrap();
            assert!(admission.confirmed());
            let baseline = Admission::new(source, "codex", text).unwrap();
            db.execute(
                "UPDATE queued_items SET payload_json=?1 WHERE id='server-new'",
                [payload("edited-client")],
            )
            .unwrap();
            assert!(
                !baseline.confirmed(),
                "changing client metadata does not bypass the old physical identity"
            );
        }
    }

    #[test]
    fn formatted_preview_matching_is_display_only_and_provider_scoped() {
        let formatted = "Read [https://example.com/plan](https://example.com/plan) then…";
        assert!(display_matches(
            Some(Provider::Codex),
            "Read https://example.com/plan then review it",
            formatted
        ));
        assert!(!display_matches(
            Some(Provider::Claude),
            "Read https://example.com/plan then review it",
            formatted
        ));
        assert!(!display_matches(
            None,
            "Read https://example.com/plan then review it",
            formatted
        ));
        assert!(!display_matches(
            Some(Provider::Codex),
            "Read https://example.com/plan then review it",
            "Read [https://example.com/plan](https://example.com/other) then…"
        ));
        assert!(
            display_matches(Some(Provider::Codex), formatted, formatted),
            "literal submitted Markdown remains literal"
        );
        assert!(
            !crate::receipt::text_matches(
                Provider::Codex,
                "Read https://example.com/plan then review it",
                formatted
            ),
            "a preview must never become a receipt"
        );
    }

    #[test]
    fn claude_queue_tracks_fifo_removals_partial_records_and_rewrites() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        let row = |op: &str, text: Option<&str>| {
            serde_json::json!({
                "type":"queue-operation", "sessionId":"s", "operation":op, "content":text
            })
        };
        for value in [
            row("enqueue", None),
            row("enqueue", Some("one")),
            row("enqueue", Some("two")),
        ] {
            writeln!(f, "{value}").unwrap();
        }
        let mut reader = Reader::default();
        let q = read_claude(&mut reader, &path, "s").unwrap();
        assert_eq!(
            q.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(),
            ["one", "two"]
        );
        assert!(q.iter().all(|m| m.interaction("s").options.is_empty()));
        assert_eq!(read_claude(&mut reader, &path, "s").unwrap(), q);
        writeln!(f, "{}", row("dequeue", None)).unwrap();
        writeln!(f, "{}", row("remove", Some("two"))).unwrap();
        assert_eq!(read_claude(&mut reader, &path, "s").unwrap()[0].text, "one");
        // A partial event must not retire a pending message yet.
        write!(f, "{}", row("dequeue", None)).unwrap();
        assert_eq!(read_claude(&mut reader, &path, "s").unwrap().len(), 1);
        writeln!(f).unwrap();
        assert!(read_claude(&mut reader, &path, "s").unwrap().is_empty());
        std::fs::write(&path, format!("{}\n", row("enqueue", Some("new")))).unwrap();
        assert_eq!(read_claude(&mut reader, &path, "s").unwrap()[0].text, "new");
    }

    #[test]
    fn sqlite_wal_queue_is_session_scoped_ordered_and_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let db = rusqlite::Connection::open(dir.path().join("queue_1.sqlite")).unwrap();
        db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE queued_items (id TEXT, thread_id TEXT, payload_json TEXT, queue_order INTEGER, created_at_ms INTEGER);").unwrap();
        let add = |id: &str, session: &str, order: i64, text: &str| {
            let payload = serde_json::json!({"input":[{"type":"text","text":text,"text_elements":[]}],"clientUserMessageId":id});
            db.execute(
                "INSERT INTO queued_items VALUES (?1,?2,?3,?4,0)",
                rusqlite::params![id, session, payload.to_string(), order],
            )
            .unwrap();
        };
        add("b", "s", 2, "second");
        add("a", "s", 1, "first");
        add("c", "other", 0, "elsewhere");
        let mut reader = Reader::default();
        let queue = read_session(&mut reader, dir.path(), "s").unwrap();
        assert_eq!(
            queue.iter().map(|q| q.text.as_str()).collect::<Vec<_>>(),
            ["first", "second"]
        );
        let old = queue[0].id.clone();
        db.execute("UPDATE queued_items SET queue_order=3 WHERE id='a'", [])
            .unwrap();
        let next = read_session(&mut reader, dir.path(), "s").unwrap();
        assert!(
            selected(&next, &old).is_err(),
            "reordered rows invalidate stale actions"
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM queued_items", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            3
        );
        db.execute("DELETE FROM queued_items WHERE thread_id='s'", [])
            .unwrap();
        assert!(
            read_session(&mut reader, dir.path(), "s")
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn queue_admission_requires_a_new_provider_item_not_an_edit_or_reorder() {
        let current = Arc::new(std::sync::Mutex::new(vec![QueuedMessage {
            id: "queued:000:old".into(),
            source_id: "local:1".into(),
            client_source_id: None,
            text: "before".into(),
            can_send: true,
        }]));
        let state = current.clone();
        let source: Source = Arc::new(move || Ok(state.lock().unwrap().clone()));
        let admission = Admission::new(source, "codex", "hello").unwrap();
        {
            let mut state = current.lock().unwrap();
            state[0].text = "hello".into();
            state[0].id = "queued:001:edited".into();
        }
        assert!(!admission.confirmed());
        current.lock().unwrap().push(QueuedMessage {
            id: "queued:000:new".into(),
            source_id: "server:2".into(),
            client_source_id: None,
            text: "hello".into(),
            can_send: true,
        });
        assert!(admission.confirmed());
    }
    #[test]
    fn serialized_app_input_displays_its_message_and_preserves_action_context() {
        let raw = "Respond to the user input in the context of our conversation.";
        let input = serde_json::json!({"type":"text","text":raw,"text_elements":[{"byteRange":{"start":0,"end":raw.len()},"placeholder":format!("codex-untrusted-app-input:{}",serde_json::json!({"message":{"text":"真实待发消息"}}))}]});
        assert_eq!(input_text(&input).as_deref(), Some("真实待发消息"));
    }
    #[test]
    fn scope_order_and_revisions_bind_the_action() {
        let state = serde_json::json!({"queued-follow-ups":{"a":[item("1","first"),item("2","second")],"b":[item("1","first")]}});
        let queue = parse(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(
            queue["a"]
                .iter()
                .map(|m| m.text.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert_ne!(queue["a"][0].id, queue["b"][0].id);
        let mut changed = state.clone();
        changed["queued-follow-ups"]["a"][0]["context"]["fileAttachments"] =
            serde_json::json!([{"path":"new"}]);
        let edited = parse(&serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(selected(&edited["a"], &queue["a"][0].id).is_err());
        assert_eq!(queue["a"][0].interaction("a").options, [SEND_NOW]);
    }
    #[test]
    fn unknown_sending_foreign_host_and_duplicates_do_not_replay() {
        for status in ["sending", "pending", "outcome-unknown", "future"] {
            let mut entry = item("1", "hello");
            entry["submission"] = serde_json::json!({"hostId":"local","status":status});
            let q = parse(
                &serde_json::to_vec(&serde_json::json!({"queued-follow-ups":{"s":[entry]}}))
                    .unwrap(),
            )
            .unwrap();
            assert!(selected(&q["s"], &q["s"][0].id).is_err());
        }
        let mut foreign = item("3", "remote");
        foreign["submission"] = serde_json::json!({"hostId":"remote"});
        let q = parse(&serde_json::to_vec(&serde_json::json!({"queued-follow-ups":{"s":[item("1","same"),item("2","same"),foreign]}})).unwrap()).unwrap();
        assert_eq!(q["s"].len(), 2);
        assert!(selected(&q["s"], &q["s"][0].id).is_err());
        assert!(parse(br#"{"queued-follow-ups":{"s":[{}]}}"#).is_err());
    }
}
