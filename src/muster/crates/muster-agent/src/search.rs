//! On-demand read-only search of authorized local transcripts, not the recent UI page.
use crate::{
    AgentService, Error,
    jsonl::Reader,
    model::{MessageKind, Role},
    viewer::Discovered,
};
use muster_proto::v1::{AgentSession, AgentSessionPage};
use std::{
    io::{BufRead, BufReader, Read},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use unicode_normalization::UnicodeNormalization;

static SEARCH_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
struct Cancel(Arc<AtomicBool>);
impl Drop for Cancel {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}
const MAX_LINE: u64 = 1 << 20;
const MAX_FILE: u64 = 64 << 20;
const MAX_TOTAL: u64 = 512 << 20;

fn normalized(s: &str) -> String {
    s.nfkc().flat_map(char::to_lowercase).collect()
}
fn snippet(text: &str, needle: &str) -> Option<String> {
    let chars: Vec<_> = text.nfkc().collect();
    let mut folded = String::new();
    let mut map = Vec::new();
    for (i, c) in chars.iter().enumerate() {
        for c in c.to_lowercase() {
            map.extend(std::iter::repeat_n(i, c.len_utf8()));
            folded.push(c);
        }
    }
    let at = folded.find(needle)?;
    let start = map[at].saturating_sub(65);
    let end = (start + 240).min(chars.len());
    Some(format!(
        "{}{}{}",
        if start > 0 { "…" } else { "" },
        chars[start..end]
            .iter()
            .collect::<String>()
            .replace('\n', " "),
        if end < chars.len() { "…" } else { "" }
    ))
}
impl AgentService {
    pub async fn search(
        self: &Arc<Self>,
        query: &str,
        cursor: &str,
        sessions: Vec<AgentSession>,
        limit: usize,
    ) -> Result<AgentSessionPage, Error> {
        let query = query.trim().to_string();
        if query.is_empty() || query.len() > 1024 {
            return Err(Error::Invalid("Search needs 1–1024 bytes of text".into()));
        }
        if cursor.len() > 256 {
            return Err(Error::Invalid("Invalid search cursor".into()));
        }
        let cursor = cursor.to_string();
        let slot = SEARCH_SLOTS
            .try_acquire()
            .map_err(|_| Error::Unavailable("Search is busy; try again shortly".into()))?;
        let documents = {
            let inner = self.inner.lock().unwrap();
            sessions
                .into_iter()
                .map(|s| {
                    let d = inner.catalog.get(&s.id).cloned();
                    (s, d)
                })
                .collect()
        };
        let cancelled = Cancel(Arc::new(AtomicBool::new(false)));
        let stop = cancelled.0.clone();
        let result = tokio::task::spawn_blocking(move || {
            let _slot = slot;
            scan(&query, &cursor, documents, limit.clamp(1, 200), &stop)
        })
        .await
        .map_err(|_| Error::Unavailable("Search worker stopped".into()))?;
        Ok(result)
    }
}
fn scan(
    query: &str,
    cursor: &str,
    mut documents: Vec<(AgentSession, Option<Discovered>)>,
    limit: usize,
    cancelled: &AtomicBool,
) -> AgentSessionPage {
    let needle = normalized(query);
    let start = Instant::now();
    let mut total = 0;
    let mut partial = false;
    let mut page = AgentSessionPage {
        query: query.into(),
        ..Default::default()
    };
    // Stable UUID ordering makes continuation independent of recency changes.
    documents.sort_by(|(a, _), (b, _)| a.id.cmp(&b.id));
    documents.retain(|(s, _)| s.id.as_str() > cursor);
    let mut last = cursor.to_string();
    for (mut session, discovered) in documents {
        if cancelled.load(Ordering::Relaxed)
            || start.elapsed() > Duration::from_secs(5)
            || total >= MAX_TOTAL
            || page.sessions.len() >= limit
        {
            page.next_cursor = last;
            partial |= cancelled.load(Ordering::Relaxed);
            break;
        }
        last = session.id.clone();
        if let Some(hit) = snippet(&session.title, &needle) {
            session.search_snippet = hit;
            page.sessions.push(session);
            continue;
        }
        let Some(d) = discovered.filter(|d| {
            !d.archived
                && d.provider.as_str() == session.provider
                && !d.hidden
                && matches!(
                    d.provider,
                    crate::Provider::Codex | crate::Provider::Claude | crate::Provider::Deepseek
                )
        }) else {
            partial = true;
            continue;
        };
        if crate::codex_history::is(&d) {
            let (hit, incomplete) = crate::codex_history::search(
                &d,
                |text| snippet(text, &needle),
                || cancelled.load(Ordering::Relaxed) || start.elapsed() > Duration::from_secs(5),
            );
            partial |= incomplete;
            if let Some(hit) = hit {
                session.search_snippet = hit;
                page.sessions.push(session);
            }
            continue;
        }
        let Some((records, hooks)) = d.provider.jsonl() else {
            partial = true;
            continue;
        };
        let Ok(file) = std::fs::File::open(&d.path) else {
            partial = true;
            continue;
        };
        // A log of Zstandard frames is searched in its decoded stream: the
        // limits and offsets count decoded bytes, as the viewer's do.
        let file: Box<dyn Read> = if d.path.extension().is_some_and(|e| e == "zstd") {
            Box::new(crate::transcript::FrameReader::new(file))
        } else {
            Box::new(file)
        };
        let mut input = BufReader::new(file);
        let mut parser = Reader::new(d.provider, records, hooks, d.meta.clone());
        let mut offset = 0;
        loop {
            if cancelled.load(Ordering::Relaxed)
                || start.elapsed() > Duration::from_secs(5)
                || offset >= MAX_FILE
                || total >= MAX_TOTAL
            {
                partial = true;
                break;
            }
            let mut bytes = Vec::new();
            let read = (&mut input)
                .take(MAX_LINE + 1)
                .read_until(b'\n', &mut bytes);
            let Ok(n) = read else {
                partial = true;
                break;
            };
            if n == 0 {
                break;
            }
            total += n as u64;
            if n as u64 > MAX_LINE {
                partial = true;
                break;
            }
            let record = serde_json::from_slice(&bytes);
            let at = offset;
            offset += n as u64;
            let Ok(record) = record else {
                partial = true;
                continue;
            };
            let (messages, _) = parser.read(&[(at, record)]);
            let hit = messages
                .iter()
                .filter(|m| {
                    m.kind == MessageKind::Text
                        && matches!(m.role, Role::User | Role::Assistant)
                        && m.phase != "reasoning"
                })
                .find_map(|m| snippet(&m.text, &needle));
            if let Some(hit) = hit {
                session.search_snippet = hit;
                page.sessions.push(session);
                break;
            }
        }
    }
    if partial {
        page.notice="Some transcripts exceeded the per-file limit or were unavailable. Showing matches from readable content.".into();
    }
    page
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_snippets_preserve_case_and_boundaries() {
        assert!(
            snippet("你好，Rust 世界", &normalized("ＲＵＳＴ"))
                .unwrap()
                .contains("Rust")
        );
        assert!(snippet("İstanbul", &normalized("İSTANBUL")).is_some());
        assert!(snippet("same title", "missing").is_none());
    }
    #[test]
    fn continuation_preserves_uuid_order_without_repeating_matches() {
        let docs = || {
            ["c", "a", "b"]
                .into_iter()
                .map(|id| {
                    (
                        AgentSession {
                            id: id.into(),
                            title: "Needle".into(),
                            ..Default::default()
                        },
                        None,
                    )
                })
                .collect()
        };
        let first = scan("needle", "", docs(), 2, &AtomicBool::new(false));
        assert_eq!(
            first
                .sessions
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        assert_eq!(first.next_cursor, "b");
        let second = scan(
            "needle",
            &first.next_cursor,
            docs(),
            2,
            &AtomicBool::new(false),
        );
        assert_eq!(second.sessions[0].id, "c");
        assert!(second.next_cursor.is_empty());
    }
    #[test]
    fn title_matches_are_not_limited_to_recent_rows() {
        let docs = (0..500)
            .map(|i| {
                (
                    AgentSession {
                        id: i.to_string(),
                        title: if i == 499 {
                            "Needle".into()
                        } else {
                            "Other".into()
                        },
                        ..Default::default()
                    },
                    None,
                )
            })
            .collect();
        let p = scan("needle", "", docs, 20, &AtomicBool::new(false));
        assert_eq!(p.sessions[0].id, "499");
    }
}

#[cfg(test)]
mod transcript_tests {
    use super::*;
    use crate::{
        model::{Provider, SessionMeta},
        viewer::Discovered,
    };
    #[test]
    fn searches_provider_text_but_not_tool_output_or_system_wrappers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        let records = [
            serde_json::json!({"type":"user","message":{"role":"user","content":"请检查大小写 Needle 的正文"}}),
            serde_json::json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Answer here"}]}}),
            serde_json::json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tool","content":"private tool marker"}]}}),
        ];
        std::fs::write(
            &path,
            records.iter().map(|v| format!("{v}\n")).collect::<String>(),
        )
        .unwrap();
        let d = Discovered {
            id: "session".into(),
            provider: Provider::Claude,
            path,
            modified: std::time::SystemTime::now(),
            size: 0,
            archived: false,
            hidden: false,
            parent_id: None,
            relation: "",
            meta: SessionMeta::default(),
            custom_title: false,
            entrypoint: None,
        };
        let docs = vec![(
            AgentSession {
                id: "session".into(),
                title: "Example".into(),
                provider: "claude".into(),
                ..Default::default()
            },
            Some(d),
        )];
        let page = scan("needle", "", docs.clone(), 10, &AtomicBool::new(false));
        assert_eq!(page.sessions.len(), 1);
        assert!(page.sessions[0].search_snippet.contains("正文"));
        assert!(
            scan(
                "private tool marker",
                "",
                docs.clone(),
                10,
                &AtomicBool::new(false)
            )
            .sessions
            .is_empty()
        );
        let p = scan("needle", "", docs, 10, &AtomicBool::new(true));
        assert!(p.sessions.is_empty());
        assert!(!p.notice.is_empty());
    }

    /// DeepSeek Harness logs are Zstandard frames: the person's and the
    /// model's text match, tool output and context the app added do not.
    #[test]
    fn searches_deepseek_frames_but_not_tool_output_or_context() {
        use ruzstd::encoding::{CompressionLevel, compress_to_vec};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.v4.jsonl.zstd");
        let id = "session-0b6c4a52-3d1e-4f8a-9c2b-6e7d8f901a23";
        let user = |seq: u64, text: &str, kind: &str| {
            serde_json::json!({"type":"user/message","seq":seq,"time":1_793_362_469_000_u64,
                "data":{"role":"user","id":format!("u{seq}"),"content":[{"type":"text","text":text}],
                    "source":{"kind":kind}}})
        };
        let frames = [
            vec![serde_json::json!({"type":"session","version":4,"id":id,"cwd":"/tmp"})],
            vec![
                user(1, "Current runtime context marker", "runtime-context"),
                user(2, "Please check the Wake Path", "user"),
            ],
            vec![
                serde_json::json!({"type":"tool/result","seq":3,"time":1_793_362_469_100_u64,
                "data":{"message":{"role":"tool","id":"r","toolCallId":"c",
                    "content":[{"type":"text","text":"private tool marker"}]}}}),
            ],
        ];
        let mut log = Vec::new();
        for records in frames {
            let text: String = records.iter().map(|r| format!("{r}\n")).collect();
            log.extend(compress_to_vec(text.as_bytes(), CompressionLevel::Fastest));
        }
        std::fs::write(&path, log).unwrap();
        let d = Discovered {
            id: id.into(),
            provider: Provider::Deepseek,
            path,
            modified: std::time::SystemTime::now(),
            size: 0,
            archived: false,
            hidden: false,
            parent_id: None,
            relation: "",
            meta: SessionMeta::default(),
            custom_title: false,
            entrypoint: None,
        };
        let docs = vec![(
            AgentSession {
                id: id.into(),
                title: "Example".into(),
                provider: "deepseek".into(),
                ..Default::default()
            },
            Some(d),
        )];
        let page = scan("wake path", "", docs.clone(), 10, &AtomicBool::new(false));
        assert_eq!(page.sessions.len(), 1);
        assert!(page.sessions[0].search_snippet.contains("Wake Path"));
        assert!(page.notice.is_empty(), "the whole log was read");
        for hidden in ["private tool marker", "runtime context marker"] {
            let page = scan(hidden, "", docs.clone(), 10, &AtomicBool::new(false));
            assert!(page.sessions.is_empty(), "{hidden}");
            assert!(page.notice.is_empty());
        }
    }
}
