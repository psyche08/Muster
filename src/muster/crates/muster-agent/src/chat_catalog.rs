//! Desktop catalog semantics. ChatGPT's app-owned SQLite is a durable local
//! projection, not a server API: completeness/freshness are those of its sync.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use muster_proto::v1::{AgentChat, AgentChatCatalog};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::Error;
use crate::desktop_chat::{ChatApp, ChatKey, session_id};

const MAX_ROWS: usize = 100_000;

#[derive(Clone, PartialEq)]
struct Stamp {
    path: PathBuf,
    modified: Option<SystemTime>,
    revision: i64,
    host: String,
    complete: bool,
}

#[derive(Default)]
pub struct LocalCatalog {
    stamp: Option<Stamp>,
    catalog: AgentChatCatalog,
    // Includes non-chat rows too: a title shared with a Codex row is unsafe
    // for AX navigation. Titles NEVER classify a conversation's kind.
    titles: HashMap<String, Option<String>>,
}

impl LocalCatalog {
    pub fn read_home(&mut self, home: &Path) -> Result<AgentChatCatalog, Error> {
        let paths: Vec<_> = ["codex.db", "codex-dev.db"]
            .iter()
            .map(|name| home.join("sqlite").join(name))
            .filter(|p| p.is_file())
            .collect();
        let result = if let [path] = paths.as_slice() {
            self.read(path)
                .map_err(|e| Error::Unavailable(format!("ChatGPT local catalog: {e}")))
        } else {
            Err(Error::Unavailable("ChatGPT needs one supported local catalog database; no untyped sidebar fallback is used".into()))
        };
        if result.is_err() {
            *self = Self::default();
        }
        result
    }

    fn read(&mut self, path: &Path) -> Result<AgentChatCatalog, Box<dyn std::error::Error>> {
        // Do not use immutable=1: this live database uses WAL. Never create,
        // migrate, checkpoint, or write the other application's database.
        let mut conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.busy_timeout(Duration::from_millis(150))?;
        let tx = conn.transaction()?;
        let hosts = {
            let mut query = tx.prepare("SELECT host_id FROM local_thread_catalog_hosts WHERE host_kind = 'chatgpt' LIMIT 2")?;
            query
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        let [host] = hosts.as_slice() else {
            return Err(
                "no unique ChatGPT account catalog; open the app with one account and refresh"
                    .into(),
            );
        };
        let complete: bool = tx.query_row("SELECT initial_build_complete != 0 FROM local_thread_catalog_sync_state WHERE host_id = ?1", [host], |r| r.get(0))?;
        let revision = tx.query_row(
            "SELECT catalog_revision FROM local_thread_catalog_metadata WHERE id = 1",
            [],
            |r| r.get(0),
        )?;
        let stamp = Stamp {
            path: path.into(),
            modified: std::fs::metadata(path)?.modified().ok(),
            revision,
            host: host.clone(),
            complete,
        };
        if self.stamp.as_ref() == Some(&stamp) {
            return Ok(self.catalog.clone());
        }
        let mut catalog = AgentChatCatalog {
            provider: "codex".into(),
            new_session: session_id(ChatApp::ChatGpt, &ChatKey::New),
            history_complete: complete,
            history_warning: if complete {
                String::new()
            } else {
                "ChatGPT is still building its local catalog; showing the available subset".into()
            },
            ..Default::default()
        };
        let mut titles = HashMap::new();
        let mut count = 0;
        {
            let mut query = tx.prepare("SELECT host_id, thread_id, display_title, source_kind, conversation_origin FROM local_thread_catalog WHERE missing_candidate = 0 ORDER BY source_recency_at DESC, source_created_at DESC, thread_id LIMIT 100001")?;
            let mut rows = query.query([])?;
            while let Some(row) = rows.next()? {
                count += 1;
                if count > MAX_ROWS {
                    return Err("local catalog exceeds the supported row limit".into());
                }
                let row_host: String = row.get(0)?;
                let id: String = row.get(1)?;
                let title: String = row.get(2)?;
                let kind: String = row.get(3)?;
                let origin: Option<String> = row.get(4)?;
                let is_chat =
                    row_host == *host && kind == "chatgpt" && origin.as_deref() != Some("tpp");
                let valid_id = !id.is_empty()
                    && id.len() <= 128
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
                if !valid_id {
                    return Err("invalid catalog conversation id".into());
                }
                let candidate = is_chat.then(|| id.clone());
                titles
                    .entry(title.trim().to_string())
                    .and_modify(|entry| *entry = None)
                    .or_insert(candidate);
                if is_chat {
                    catalog.chats.push(AgentChat {
                        id: session_id(ChatApp::ChatGpt, &ChatKey::Id(id)),
                        title,
                        current: false,
                    });
                }
            }
        }
        tx.commit()?;
        self.stamp = Some(stamp);
        self.catalog = catalog.clone();
        self.titles = titles;
        Ok(catalog)
    }

    pub fn unique_title(&self, title: &str) -> Option<String> {
        self.titles.get(title.trim()).cloned().flatten()
    }
}

/// AX fallback has no tombstones or update cursor. Partial observations can
/// only upsert. Only a complete, successful sweep may remove missing rows.
#[derive(Default, Serialize, Deserialize)]
pub struct HistoryCache {
    version: u8,
    rows: Vec<(String, String)>,
    complete_at: u64,
    attempted_at: u64,
}

impl HistoryCache {
    pub fn restore(bytes: Option<&[u8]>) -> Self {
        bytes
            .and_then(|b| serde_json::from_slice::<Self>(b).ok())
            .filter(|c| c.version == 1 && c.rows.len() <= MAX_ROWS)
            .unwrap_or_default()
    }

    pub fn needs_full(&self, now: u64) -> bool {
        let age = now.checked_sub(self.complete_at).unwrap_or(u64::MAX);
        let retry = now.checked_sub(self.attempted_at).unwrap_or(u64::MAX);
        (self.complete_at == 0 || age >= 600) && (self.attempted_at == 0 || retry >= 60)
    }

    pub fn attempted(&mut self, now: u64) {
        self.version = 1;
        self.attempted_at = now;
    }

    pub fn merge(&mut self, catalog: &mut AgentChatCatalog, now: u64) {
        if !catalog.app_running || !catalog.chat_mode {
            return;
        }
        if catalog.history_complete {
            self.rows.clear();
            self.complete_at = now;
        }
        let mut merged: Vec<(String, String)> = catalog
            .chats
            .iter()
            .map(|r| (r.id.clone(), r.title.clone()))
            .collect();
        let seen: std::collections::HashSet<_> = merged.iter().map(|(id, _)| id.clone()).collect();
        merged.extend(
            self.rows
                .iter()
                .filter(|(id, _)| !seen.contains(id))
                .cloned(),
        );
        merged.truncate(MAX_ROWS);
        self.rows = merged;
        self.version = 1;
        catalog.chats = self
            .rows
            .iter()
            .map(|(id, title)| AgentChat {
                id: id.clone(),
                title: title.clone(),
                current: *id == catalog.current,
            })
            .collect();
        if !catalog.history_complete && !self.rows.is_empty() {
            let note = "Cached history + visible updates; removals and off-screen renames reconcile on a complete scan (every 10 minutes)";
            catalog.history_warning = if catalog.history_warning.is_empty() {
                note.into()
            } else {
                format!("{}; {note}", catalog.history_warning)
            };
        }
        // history_complete describes this observation, never a cached claim.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database(home: &Path) -> Connection {
        std::fs::create_dir_all(home.join("sqlite")).unwrap();
        let conn = Connection::open(home.join("sqlite/codex.db")).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;
            CREATE TABLE local_thread_catalog_hosts(host_id TEXT, host_kind TEXT);
            CREATE TABLE local_thread_catalog_sync_state(host_id TEXT, initial_build_complete INTEGER);
            CREATE TABLE local_thread_catalog_metadata(id INTEGER, catalog_revision INTEGER);
            CREATE TABLE local_thread_catalog(host_id TEXT, thread_id TEXT, display_title TEXT, source_kind TEXT, conversation_origin TEXT, missing_candidate INTEGER DEFAULT 0, source_recency_at REAL DEFAULT 0, source_created_at REAL DEFAULT 0);
            INSERT INTO local_thread_catalog_hosts VALUES ('account', 'chatgpt');
            INSERT INTO local_thread_catalog_sync_state VALUES ('account', 1);
            INSERT INTO local_thread_catalog_metadata VALUES (1, 1);
            INSERT INTO local_thread_catalog(host_id,thread_id,display_title,source_kind,conversation_origin) VALUES
                ('account','chat-1111','Codex in the title is still a chat','chatgpt',NULL),
                ('account','work-1111','Ordinary sounding title','chatgpt','tpp'),
                ('local','code-1111','Ordinary sounding title','vscode',NULL),
                ('account','chat-2222','Duplicate','chatgpt',NULL),
                ('local','code-2222','Duplicate','cli',NULL);").unwrap();
        conn
    }

    #[test]
    fn typed_source_filters_work_and_codex_without_title_heuristics_and_reads_wal() {
        let dir = tempfile::tempdir().unwrap();
        let conn = database(dir.path());
        let mut source = LocalCatalog::default();
        let catalog = source.read_home(dir.path()).unwrap();
        assert_eq!(catalog.chats.len(), 2);
        assert!(catalog.history_complete);
        assert_eq!(
            source
                .unique_title("Codex in the title is still a chat")
                .as_deref(),
            Some("chat-1111")
        );
        assert_eq!(source.unique_title("Duplicate"), None);
        assert_eq!(source.unique_title("Ordinary sounding title"), None);
        // No row read on an unchanged revision; the app bumps the revision
        // in the same transaction as every visible catalog mutation.
        assert_eq!(source.read_home(dir.path()).unwrap(), catalog);
        conn.execute_batch("BEGIN;
            UPDATE local_thread_catalog SET display_title='Renamed' WHERE thread_id='chat-1111';
            DELETE FROM local_thread_catalog WHERE thread_id='chat-2222';
            INSERT INTO local_thread_catalog(host_id,thread_id,display_title,source_kind) VALUES ('account','chat-3333','New','chatgpt');
            UPDATE local_thread_catalog_metadata SET catalog_revision=2;
            COMMIT;").unwrap();
        let next = source.read_home(dir.path()).unwrap();
        assert_eq!(next.chats.len(), 2);
        assert!(next.chats.iter().any(|c| c.title == "New"));
        assert!(next.chats.iter().any(|c| c.title == "Renamed"));
        assert!(!next.chats.iter().any(|c| c.title == "Duplicate"));
        // Durable source recovers without a Muster full-history scan.
        assert_eq!(LocalCatalog::default().read_home(dir.path()).unwrap(), next);
        conn.execute_batch("UPDATE local_thread_catalog SET missing_candidate=1 WHERE thread_id='chat-3333'; UPDATE local_thread_catalog_metadata SET catalog_revision=3;").unwrap();
        assert_eq!(source.read_home(dir.path()).unwrap().chats.len(), 1);
    }

    #[test]
    fn partial_build_account_change_and_schema_loss_never_reuse_old_rows() {
        let dir = tempfile::tempdir().unwrap();
        let conn = database(dir.path());
        let mut source = LocalCatalog::default();
        source.read_home(dir.path()).unwrap();
        conn.execute_batch("UPDATE local_thread_catalog_sync_state SET initial_build_complete=0")
            .unwrap();
        let partial = source.read_home(dir.path()).unwrap();
        assert!(!partial.history_complete);
        assert!(!partial.history_warning.is_empty());
        conn.execute_batch("INSERT INTO local_thread_catalog_hosts VALUES ('other','chatgpt')")
            .unwrap();
        assert!(source.read_home(dir.path()).is_err());
        assert_eq!(
            source.unique_title("Codex in the title is still a chat"),
            None
        );
        conn.execute_batch("DELETE FROM local_thread_catalog_hosts WHERE host_id='other'; DROP TABLE local_thread_catalog_metadata;").unwrap();
        assert!(source.read_home(dir.path()).is_err());
        assert!(source.catalog.chats.is_empty());
    }

    #[test]
    fn missing_or_ambiguous_database_is_not_created_or_selected_arbitrarily() {
        let dir = tempfile::tempdir().unwrap();
        let mut source = LocalCatalog::default();
        assert!(source.read_home(dir.path()).is_err());
        assert!(!dir.path().join("sqlite/codex.db").exists());
        let _conn = database(dir.path());
        std::fs::write(dir.path().join("sqlite/codex-dev.db"), b"not a database").unwrap();
        assert!(source.read_home(dir.path()).is_err());
    }

    fn observation(rows: &[(&str, &str)], complete: bool) -> AgentChatCatalog {
        AgentChatCatalog {
            app_running: true,
            chat_mode: true,
            history_complete: complete,
            chats: rows
                .iter()
                .map(|(id, title)| AgentChat {
                    id: (*id).into(),
                    title: (*title).into(),
                    current: false,
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn partial_observations_upsert_but_only_complete_sweeps_delete() {
        let mut cache = HistoryCache::default();
        assert!(cache.needs_full(1000));
        cache.attempted(1000);
        cache.merge(&mut observation(&[("a", "A"), ("b", "B")], true), 1000);
        assert!(!cache.needs_full(1001));
        let mut next = observation(&[("b", "Renamed"), ("c", "New")], false);
        next.current = "b".into();
        cache.merge(&mut next, 1001);
        assert_eq!(
            next.chats.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["b", "c", "a"]
        );
        assert!(next.chats[0].current);
        assert!(!next.history_complete);
        let saved = serde_json::to_vec(&cache).unwrap();
        let mut recovered = HistoryCache::restore(Some(&saved));
        assert!(!recovered.needs_full(1002));
        assert!(recovered.needs_full(1600));
        let mut final_page = observation(&[("c", "New")], true);
        recovered.merge(&mut final_page, 1600);
        assert_eq!(final_page.chats.len(), 1);
        recovered.merge(&mut observation(&[], true), 1601);
        assert!(recovered.rows.is_empty());
    }

    #[test]
    fn incomplete_failed_scans_back_off_and_corrupt_cache_rebuilds() {
        let mut cache = HistoryCache::restore(Some(b"broken"));
        assert!(cache.needs_full(1000));
        cache.attempted(1000);
        let bytes = serde_json::to_vec(&cache).unwrap();
        let mut cache = HistoryCache::restore(Some(&bytes));
        assert!(!cache.needs_full(1001));
        assert!(cache.needs_full(1060));
        assert!(cache.needs_full(900)); // clock rollback never freezes refresh
        cache.merge(&mut observation(&[("a", "A")], false), 1060);
        let mut unavailable = AgentChatCatalog {
            history_complete: true,
            ..Default::default()
        };
        cache.merge(&mut unavailable, 1061);
        assert_eq!(cache.rows.len(), 1);
        assert!(
            HistoryCache::restore(Some(
                br#"{"version":99,"rows":[],"complete_at":1000,"attempted_at":1000}"#
            ))
            .needs_full(1001)
        );
    }

    #[tokio::test]
    async fn source_ids_do_not_rebind_a_new_chats_runtime_identity() {
        let dir = tempfile::tempdir().unwrap();
        let conn = database(dir.path());
        let agent = crate::AgentService::start(crate::AgentConfig {
            codex_home: Some(dir.path().into()),
            claude_home: None,
            catdesk_home: None,
            claude_desktop: None,
            ..Default::default()
        });
        agent.local_chatgpt_catalog().unwrap();
        let key = ChatKey::Title("Fresh chat".into());
        let first = agent
            .chat_observed(ChatApp::ChatGpt, None, key.clone(), "Fresh chat", vec![])
            .unwrap();
        conn.execute_batch("INSERT INTO local_thread_catalog(host_id,thread_id,display_title,source_kind) VALUES ('account','chat-new1','Fresh chat','chatgpt'); UPDATE local_thread_catalog_metadata SET catalog_revision=2;").unwrap();
        agent.local_chatgpt_catalog().unwrap();
        let observed = agent
            .chat_observed(
                ChatApp::ChatGpt,
                Some(&first),
                key.clone(),
                "Fresh chat",
                vec![],
            )
            .unwrap();
        assert_eq!(observed, first);
        assert!(agent.chat_row_matches(ChatApp::ChatGpt, &key, "codex-chat-chat-new1"));
        assert!(agent.chat_row_matches(ChatApp::ChatGpt, &key, &first));
        conn.execute_batch("INSERT INTO local_thread_catalog(host_id,thread_id,display_title,source_kind) VALUES ('local','code-new1','Fresh chat','cli'); UPDATE local_thread_catalog_metadata SET catalog_revision=3;").unwrap();
        agent.local_chatgpt_catalog().unwrap();
        assert!(!agent.chat_row_matches(ChatApp::ChatGpt, &key, &first));
    }

    /// Opt-in read-only verification; prints counts, never account IDs/titles.
    #[test]
    #[ignore]
    fn inspect_local_catalog() {
        let home = std::env::var_os("MUSTER_TEST_CATALOG_HOME").expect("explicit catalog home");
        let mut source = LocalCatalog::default();
        let started = std::time::Instant::now();
        let first = source.read_home(Path::new(&home)).unwrap();
        let second = source.read_home(Path::new(&home)).unwrap();
        assert_eq!(first, second);
        println!(
            "typed chats={}, complete={}, two reads={:?}",
            first.chats.len(),
            first.history_complete,
            started.elapsed()
        );
    }
}
