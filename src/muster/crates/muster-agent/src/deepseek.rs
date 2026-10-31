//! DeepSeek Harness (bundle com.deepseek.dsh) sessions, read only.
//!
//! The app keeps one append-only event log per session under `~/.dsh`
//! (`DSH_HOME`): `sessions/--<project>--/<session id>/session.v4.jsonl.zstd`,
//! a stream of Zstandard frames, one per flush; a log kept uncompressed is
//! `session.v4.jsonl`. Its first record is the session's header (id, cwd,
//! parent). The one JSONL engine reads the events over the profile's record
//! grammar (`profiles/deepseek.toml`) and the hooks here. The app also keeps
//! the titles it shows (`storages/session_projcache`) and the sessions it
//! archived (`storages/workspace.json`). Nothing here writes to the app or
//! drives it: its profile records no controls.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::jsonl::Hooks;
use crate::model::{Message, Provider, Role, SessionMeta, title_from};
use crate::viewer::{Discovered, is_uuid};

/// The session format this build reads; another generation is skipped.
const FORMAT: u64 = 4;

/// How much of a log the scan reads for its header.
const HEAD: u64 = 1 << 20;

pub struct DeepseekHooks;

pub static HOOKS: DeepseekHooks = DeepseekHooks;

impl Hooks for DeepseekHooks {
    /// The app records a message as it was written; the grammar already
    /// leaves out the context the app adds for the model.
    fn text(&self, _role: Role, raw: &str) -> String {
        raw.trim().to_string()
    }
    /// `session/title`: the first prompt at once, then the model's title;
    /// one a person gave is never replaced by a generated one.
    fn title(&self, record: &Value) -> Option<(String, bool)> {
        if record["type"] != "session/title" {
            return None;
        }
        let data = &record["data"];
        let title = title_from(data["title"].as_str()?);
        (!title.is_empty()).then(|| (title, data["source"]["kind"] == "user"))
    }
    fn usage(
        &self,
        tracker: &mut crate::usage::Tracker,
        offset: u64,
        record: &Value,
    ) -> Option<Message> {
        tracker.deepseek(offset, record)
    }
}

/// The app's state directory, when it holds sessions.
pub fn home() -> Option<PathBuf> {
    let records = Provider::Deepseek.profile().records?;
    let home = crate::jsonl::home(records)?;
    home.join("sessions").is_dir().then_some(home)
}

/// Every session log under `home`.
pub fn scan(home: &Path) -> Vec<Discovered> {
    let archived = archived(home);
    let Ok(projects) = std::fs::read_dir(home.join("sessions")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for project in projects.flatten() {
        let Ok(sessions) = std::fs::read_dir(project.path()) else {
            continue;
        };
        for dir in sessions.flatten() {
            if let Some(found) = session(home, &dir.path(), &archived) {
                out.push(found);
            }
        }
    }
    out
}

/// A session's id as the app writes it: `session-<uuid>`.
fn valid_id(id: &str) -> bool {
    id.strip_prefix("session-").is_some_and(is_uuid)
}

/// The log the app reads in a session directory: the highest generation,
/// compressed or not. `None` when that is not the format this build reads.
fn log(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(u64, PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(rest) = name.strip_prefix("session") else {
            continue;
        };
        let rest = rest.strip_suffix(".zstd").unwrap_or(rest);
        let Some(version) = rest.strip_suffix(".jsonl") else {
            continue;
        };
        let version = match version {
            "" => 0,
            v => match v.strip_prefix(".v").and_then(|n| n.parse().ok()) {
                Some(n) => n,
                None => continue,
            },
        };
        // Of one generation, the compressed log is the canonical one.
        let better = best.as_ref().is_none_or(|(v, path)| {
            version > *v || (version == *v && path.extension().is_none_or(|e| e != "zstd"))
        });
        if better {
            best = Some((version, entry.path()));
        }
    }
    match best? {
        (FORMAT, path) => Some(path),
        (version, path) => {
            log::debug!(
                "deepseek: {} is session format {version}, not {FORMAT}; skipped",
                path.display()
            );
            None
        }
    }
}

fn session(home: &Path, dir: &Path, archived: &HashSet<String>) -> Option<Discovered> {
    let path = log(dir)?;
    let zstd = path.extension().is_some_and(|e| e == "zstd");
    let header = crate::transcript::first_record(&path, zstd, HEAD)?;
    if header["type"] != "session" || header["version"].as_u64() != Some(FORMAT) {
        return None;
    }
    let id = header["id"].as_str().filter(|id| valid_id(id))?.to_string();
    let stat = std::fs::metadata(&path).ok()?;
    let modified = stat.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    // A subagent's session names its parent; a fork is a session of its own.
    let subagent = header["origin"] == "subagent";
    let parent_id = header["parentSession"]
        .as_str()
        .filter(|parent| subagent && valid_id(parent))
        .map(str::to_string);
    let title = title(home, &id).unwrap_or_else(|| format!("session {}", &id[8..16]));
    let is_archived = archived.contains(&id);
    let mut found = Discovered {
        id,
        provider: Provider::Deepseek,
        path,
        modified,
        size: stat.len(),
        archived: is_archived,
        hidden: subagent,
        parent_id,
        relation: if subagent { "subagent" } else { "" },
        meta: SessionMeta {
            title,
            cwd: header["cwd"]
                .as_str()
                .map(PathBuf::from)
                .unwrap_or_default(),
            archived: is_archived,
            ..Default::default()
        },
        custom_title: false,
        entrypoint: None,
    };
    found.meta.updated_unix = found.modified_unix();
    Some(found)
}

/// The title the app shows for a session, from its projection cache.
fn title(home: &Path, id: &str) -> Option<String> {
    let path = home
        .join("storages/session_projcache/sessions")
        .join(format!("{id}.json"));
    let cache: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let title = title_from(cache["record"]["rows"]["title"]["val"].as_str()?);
    (!title.is_empty()).then_some(title)
}

/// The sessions the app archived.
fn archived(home: &Path) -> HashSet<String> {
    let Ok(bytes) = std::fs::read(home.join("storages/workspace.json")) else {
        return HashSet::new();
    };
    let Ok(workspace) = serde_json::from_slice::<Value>(&bytes) else {
        return HashSet::new();
    };
    workspace["global"]["archivedSessionIds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}
