//! Codex: rollout files under `$CODEX_HOME/sessions` (and
//! `archived_sessions`), `session_index.jsonl` for titles. How a rollout's
//! records become messages is the grammar in `profiles/codex.toml`, read by
//! the engine in `crate::jsonl`; this module keeps discovery, the head
//! metadata and the [`Hooks`] the grammar cannot express.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::jsonl::{Hooks, JsonlViewer};
use crate::model::{Message, Provider, Role, SessionMeta, SessionState, title_from};
use crate::profile::Records;
use crate::text::{is_wrapper, strip_hidden_blocks};
use crate::viewer::{Discovered, head_records, is_uuid};

/// The rollout record grammar (`profiles/codex.toml`).
pub fn records() -> &'static Records {
    crate::profile::CODEX_APP
        .records
        .expect("the Codex profile describes a jsonl source")
}

pub fn home() -> Option<PathBuf> {
    crate::jsonl::home(records())
}

/// What the grammar cannot say about a rollout: the model's memory
/// citations are stripped from its text, bootstrap wrappers are not
/// prompts, and turn usage is Codex's token accounting.
pub struct CodexHooks;

pub static HOOKS: CodexHooks = CodexHooks;

impl Hooks for CodexHooks {
    fn text(&self, role: Role, raw: &str) -> String {
        let text = if role == Role::Assistant {
            strip_hidden_blocks(raw)
        } else {
            raw.to_string()
        };
        if is_wrapper(&text) {
            String::new()
        } else {
            text
        }
    }
    fn usage(
        &self,
        tracker: &mut crate::usage::Tracker,
        offset: u64,
        record: &Value,
    ) -> Option<Message> {
        tracker.codex(offset, record)
    }
}

/// The rollout viewer: the engine over the Codex grammar and hooks.
pub fn viewer(discovered: &Discovered) -> JsonlViewer {
    JsonlViewer::open(discovered, records(), &HOOKS)
}

const MAX_FILES: usize = 10_000;
const MAX_DEPTH: usize = 12;

/// Every rollout under the two roots, with head metadata.
pub fn scan(home: &Path) -> Vec<Discovered> {
    let titles = load_index(&home.join("session_index.jsonl"));
    let mut out = Vec::new();
    for (root, archived) in [
        (home.join("sessions"), false),
        (home.join("archived_sessions"), true),
    ] {
        let mut files = Vec::new();
        walk(&root, 0, &mut files);
        for path in files {
            if out.len() >= MAX_FILES {
                break;
            }
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            let head = Head::read(&path);
            // Native CLI sessions are listed by the Rust Orca source. Their
            // Codex rollout is storage, not a second desktop session.
            if head.owned_by_muster {
                continue;
            }
            let Some(id) = head.id.clone().or_else(|| filename_id(&path)) else {
                continue;
            };
            let title = titles
                .get(&id)
                .cloned()
                .filter(|t| !t.trim().is_empty())
                .or(head.first_prompt.clone())
                .unwrap_or_else(|| format!("session {}", &id[..8.min(id.len())]));
            out.push(Discovered {
                id,
                provider: Provider::Codex,
                modified: meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                size: meta.len(),
                archived,
                hidden: head.hidden,
                parent_id: head.parent_id.clone(),
                relation: if head.hidden { "subagent" } else { "" },
                custom_title: false,
                meta: SessionMeta {
                    title,
                    cwd: head.cwd.clone().unwrap_or_default(),
                    updated_unix: 0,
                    archived,
                    state: SessionState::Idle,
                },
                entrypoint: None,
                path,
            });
        }
    }
    out
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH || out.len() >= MAX_FILES {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            walk(&path, depth + 1, out);
        } else if kind.is_file() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("rollout-") && name.ends_with(".jsonl") {
                out.push(path);
            }
        }
    }
}

fn filename_id(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    if stem.len() < 36 {
        return None;
    }
    let tail = &stem[stem.len() - 36..];
    is_uuid(tail).then(|| tail.to_ascii_lowercase())
}

fn load_index(path: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return out;
    };
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(id) = v["id"].as_str() else { continue };
        if !is_uuid(id) {
            continue;
        }
        if let Some(name) = v["thread_name"].as_str() {
            out.insert(id.to_ascii_lowercase(), name.trim().to_string());
        }
    }
    out
}

#[derive(Default)]
struct Head {
    id: Option<String>,
    cwd: Option<PathBuf>,
    hidden: bool,
    owned_by_muster: bool,
    parent_id: Option<String>,
    first_prompt: Option<String>,
}

impl Head {
    fn read(path: &Path) -> Self {
        let mut head = Head::default();
        let mut after_turn_context = false;
        for record in head_records(path, 64) {
            let kind = record["type"].as_str().unwrap_or("");
            let payload = &record["payload"];
            if head.cwd.is_none() {
                if let Some(cwd) = record["cwd"].as_str().or_else(|| payload["cwd"].as_str()) {
                    head.cwd = Some(PathBuf::from(cwd));
                }
            }
            match kind {
                "session_meta" => {
                    head.owned_by_muster = payload["originator"].as_str() == Some("muster");
                    if let Some(id) = payload["id"]
                        .as_str()
                        .or_else(|| payload["session_id"].as_str())
                        && is_uuid(id)
                    {
                        head.id = Some(id.to_ascii_lowercase());
                    }
                    head.hidden = is_subagent(payload);
                    head.parent_id =
                        subagent_parent(payload).filter(|id| Some(id) != head.id.as_ref());
                }
                "turn_context" => after_turn_context = true,
                "response_item"
                    if after_turn_context
                        && head.first_prompt.is_none()
                        && payload["type"] == "message"
                        && payload["role"] == "user" =>
                {
                    for text in text_blocks(&payload["content"]) {
                        if !is_wrapper(&text) {
                            head.first_prompt = Some(title_from(&text));
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
        head
    }
}

/// Forks alone are ordinary sessions. Only provider-declared subagents may
/// use a fork origin as the parent when an explicit parent is unavailable.
fn subagent_parent(payload: &Value) -> Option<String> {
    if !is_subagent(payload) {
        return None;
    }
    for key in ["parentThreadId", "parent_thread_id", "parentThreadID"] {
        if let Some(id) = payload[key].as_str().filter(|id| is_uuid(id)) {
            return Some(id.to_ascii_lowercase());
        }
    }
    for source in [
        "source",
        "thread_source",
        "threadSource",
        "session_source",
        "sessionSource",
    ] {
        for agent in ["subagent", "subAgent"] {
            for spawn in ["thread_spawn", "threadSpawn", "spawn"] {
                for key in ["parent_thread_id", "parentThreadId"] {
                    if let Some(id) = payload[source][agent][spawn][key]
                        .as_str()
                        .filter(|id| is_uuid(id))
                    {
                        return Some(id.to_ascii_lowercase());
                    }
                }
            }
        }
    }
    payload["forked_from_id"]
        .as_str()
        .filter(|id| is_uuid(id))
        .map(str::to_ascii_lowercase)
}

fn is_subagent(payload: &Value) -> bool {
    for key in ["parentThreadId", "parent_thread_id", "parentThreadID"] {
        if payload[key].as_str().is_some_and(|s| !s.is_empty()) {
            return true;
        }
    }
    for key in [
        "threadSource",
        "thread_source",
        "source",
        "sessionSource",
        "session_source",
    ] {
        match &payload[key] {
            Value::String(s) => {
                let normal: String = s
                    .chars()
                    .filter(|c| !matches!(c, '_' | '-' | ' '))
                    .collect::<String>()
                    .to_ascii_lowercase();
                if normal == "subagent" {
                    return true;
                }
            }
            Value::Object(map) if map.contains_key("subAgent") || map.contains_key("subagent") => {
                return true;
            }
            _ => {}
        }
    }
    false
}

fn text_blocks(content: &Value) -> Vec<String> {
    match content {
        Value::String(s) => vec![s.clone()],
        Value::Array(items) => items
            .iter()
            .filter(|b| {
                matches!(
                    b["type"].as_str(),
                    Some("input_text" | "output_text" | "text")
                )
            })
            .filter_map(|b| b["text"].as_str().map(str::to_string))
            .filter(|t| !t.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewer::Viewer as _;
    use std::io::Write;

    #[test]
    fn native_muster_rollouts_are_not_discovered_as_desktop_sessions() {
        let dir = tempfile::tempdir().unwrap();
        for root in ["sessions", "archived_sessions"] {
            let folder = dir.path().join(root);
            std::fs::create_dir_all(&folder).unwrap();
            for (index, originator) in [Some("muster"), Some("codex_cli_rs"), None]
                .into_iter()
                .enumerate()
            {
                let id = format!("0f8d6b3a-2f0e-4c53-9d1a-3a4b5c6d7e8{index}");
                let record = serde_json::json!({
                    "type": "session_meta",
                    "payload": {"id": id, "originator": originator, "source": "vscode"}
                });
                std::fs::write(
                    folder.join(format!("rollout-{id}.jsonl")),
                    record.to_string(),
                )
                .unwrap();
            }
        }
        let found = scan(dir.path());
        assert_eq!(found.len(), 4);
        assert_eq!(found.iter().filter(|session| session.archived).count(), 2);
        assert!(found.iter().all(|session| !session.id.ends_with('0')));
    }

    #[test]
    fn scans_parses_and_hides_bootstrap() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let day = home.join("sessions/2026/09/24");
        std::fs::create_dir_all(&day).unwrap();
        let id = "0f8d6b3a-2f0e-4c53-9d1a-3a4b5c6d7e8f";
        let path = day.join(format!("rollout-2026-09-24T10-00-00-{id}.jsonl"));
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, r#"{{"timestamp":"2026-09-24T10:00:00Z","type":"session_meta","payload":{{"id":"{id}","cwd":"/repo"}}}}"#).unwrap();
        writeln!(f, r##"{{"type":"response_item","payload":{{"type":"message","role":"user","content":[{{"type":"input_text","text":"# AGENTS.md instructions"}}]}}}}"##).unwrap();
        writeln!(
            f,
            r#"{{"type":"turn_context","payload":{{"model":"gpt-test"}}}}"#
        )
        .unwrap();
        writeln!(f, r#"{{"timestamp":"2026-09-24T10:00:01Z","type":"response_item","payload":{{"type":"message","role":"user","content":[{{"type":"input_text","text":"<environment_context>x</environment_context>"}},{{"type":"input_text","text":"hello"}}]}}}}"#).unwrap();
        writeln!(
            f,
            r#"{{"type":"event_msg","payload":{{"type":"task_started"}}}}"#
        )
        .unwrap();
        writeln!(f, r#"{{"type":"response_item","payload":{{"type":"function_call","name":"shell","call_id":"c1","arguments":"{{\"cmd\":\"pwd\"}}"}}}}"#).unwrap();
        writeln!(f, r#"{{"type":"response_item","payload":{{"type":"function_call_output","call_id":"c1","output":"/repo"}}}}"#).unwrap();
        std::fs::write(
            home.join("session_index.jsonl"),
            format!(r#"{{"id":"{id}","thread_name":"Remote Coding"}}"#),
        )
        .unwrap();

        let found = scan(home);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, id);
        assert_eq!(found[0].meta.title, "Remote Coding");
        assert_eq!(found[0].meta.cwd, PathBuf::from("/repo"));
        assert!(!found[0].hidden);

        let mut viewer = viewer(&found[0]);
        let polled = viewer.poll().unwrap();
        let texts: Vec<_> = polled.messages.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(
            texts,
            vec!["hello", "shell {\"cmd\":\"pwd\"}", "shell: /repo"]
        );
        assert_eq!(polled.messages[0].role, Role::User);
        assert_eq!(
            polled.messages[2].kind,
            crate::model::MessageKind::ToolResult
        );
        assert_eq!(polled.messages[1].to_proto().tool_call_id, "c1");
        assert_eq!(polled.messages[2].to_proto().tool_call_id, "c1");
        assert_eq!(viewer.state(), SessionState::Running);

        writeln!(f, r#"{{"type":"response_item","payload":{{"type":"message","role":"assistant","content":[{{"type":"output_text","text":"done `\n<oai-mem-citation>p</oai-mem-citation>`"}}]}}}}"#).unwrap();
        writeln!(
            f,
            r#"{{"type":"event_msg","payload":{{"type":"task_complete"}}}}"#
        )
        .unwrap();
        let polled = viewer.poll().unwrap();
        assert_eq!(polled.messages.len(), 2);
        assert_eq!(
            polled.messages[1].kind,
            crate::model::MessageKind::TurnSummary
        );
        assert_eq!(polled.messages[0].text, "done ``");
        assert_eq!(viewer.state(), SessionState::Idle);
    }

    #[test]
    fn subagent_parent_uses_declared_ancestry_not_ordinary_forks() {
        use serde_json::json;
        let parent = "11111111-1111-4111-8111-111111111111";
        for metadata in [
            json!({"parent_thread_id":parent}),
            json!({"source":{"subagent":{"thread_spawn":{"parent_thread_id":parent}}}}),
            json!({"source":{"subAgent":{"spawn":{"parentThreadId":parent}}}}),
            json!({"source":{"subagent":{"other":"review"}},"forked_from_id":parent}),
        ] {
            assert_eq!(subagent_parent(&metadata).as_deref(), Some(parent));
        }
        assert!(subagent_parent(&json!({"source":"desktop","forked_from_id":parent})).is_none());
        assert!(subagent_parent(&json!({"parent_thread_id":"invalid"})).is_none());
    }

    #[test]
    fn subagent_rollouts_are_hidden_and_archived_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        std::fs::create_dir_all(home.join("archived_sessions")).unwrap();
        let id = "1f8d6b3a-2f0e-4c53-9d1a-3a4b5c6d7e8f";
        std::fs::write(
            home.join(format!("archived_sessions/rollout-2026-09-24T10-00-00-{id}.jsonl")),
            format!(r#"{{"type":"session_meta","payload":{{"id":"{id}","source":{{"subagent":{{"kind":"review"}}}}}}}}"#),
        )
        .unwrap();
        let found = scan(home);
        assert_eq!(found.len(), 1);
        assert!(found[0].archived);
        assert!(found[0].hidden);
        assert_eq!(found[0].meta.title, "session 1f8d6b3a");
    }
}
