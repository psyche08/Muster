//! Claude Code: transcripts under `$CLAUDE_CONFIG_DIR/projects/<project>/<uuid>.jsonl`,
//! `sessions-index.json` per project for archived flags. How a transcript's
//! records become messages is the grammar in `profiles/claude.toml`, read by
//! the engine in `crate::jsonl`; this module keeps discovery, the head
//! metadata, the desktop registry and the [`Hooks`] the grammar cannot
//! express (titles, pasted content, the prompt queue, usage).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::jsonl::{Hooks, JsonlViewer};
use crate::model::{Message, Provider, Role, SessionMeta, SessionState, title_from};
use crate::profile::Records;
use crate::text::{is_wrapper, strip_hidden_blocks};
use crate::viewer::{Discovered, head_records, is_uuid};

/// The transcript record grammar (`profiles/claude.toml`).
pub fn records() -> &'static Records {
    crate::profile::CLAUDE_APP
        .records
        .expect("the Claude profile describes a jsonl source")
}

pub fn home() -> Option<PathBuf> {
    crate::jsonl::home(records())
}

/// What the grammar cannot say about a transcript.
pub struct ClaudeHooks;

pub static HOOKS: ClaudeHooks = ClaudeHooks;

impl Hooks for ClaudeHooks {
    /// A user's text with reminders stripped, pasted content unwrapped and
    /// command wrappers hidden; the assistant's with reminders stripped.
    fn text(&self, role: Role, raw: &str) -> String {
        if role == Role::User {
            user_text(raw)
        } else {
            strip_hidden_blocks(raw)
        }
    }
    fn title(&self, record: &Value) -> Option<(String, bool)> {
        title_record(record)
    }
    fn usage(
        &self,
        tracker: &mut crate::usage::Tracker,
        offset: u64,
        record: &Value,
    ) -> Option<Message> {
        tracker.claude(offset, record)
    }
    fn queued_text(&self, record: &Value) -> Option<String> {
        queued_text(record)
    }
}

/// The transcript viewer: the engine over the Claude grammar and hooks.
pub fn viewer(discovered: &Discovered) -> JsonlViewer {
    JsonlViewer::open(discovered, records(), &HOOKS)
}

/// A prompt submitted while a turn is running is not written as a `user`
/// record. Claude Code first persists it in its prompt queue
/// (`queue-operation` / `enqueue`: the provider accepted it, like a Codex
/// queue item) and then delivers it into the running turn as a
/// `queued_command` attachment. Both carry the exact prompt. Task
/// notifications, messages from peers, other command modes, sidechains and
/// other queue operations are not the user's submission.
pub(crate) fn queued_text(record: &Value) -> Option<String> {
    match record["type"].as_str()? {
        "queue-operation" if record["operation"] == "enqueue" => {
            record["content"].as_str().map(str::to_owned)
        }
        "attachment" if record["isSidechain"] != true => {
            let attachment = &record["attachment"];
            if attachment["type"] != "queued_command"
                || attachment["commandMode"] != "prompt"
                || attachment["origin"]["kind"]
                    .as_str()
                    .is_some_and(|kind| kind != "human")
            {
                return None;
            }
            match &attachment["prompt"] {
                Value::String(text) => Some(text.clone()),
                Value::Array(parts) => Some(
                    parts
                        .iter()
                        .filter(|p| p["type"] == "text")
                        .filter_map(|p| p["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                _ => None,
            }
        }
        _ => None,
    }
}

const MAX_FILES: usize = 10_000;

#[path = "claude_sessions.rs"]
mod sessions;
pub use sessions::apply_desktop_relationships;

pub fn scan(home: &Path) -> Vec<Discovered> {
    let projects = home.join("projects");
    let Ok(rd) = std::fs::read_dir(&projects) else {
        return Vec::new();
    };
    let mut by_id: HashMap<String, Discovered> = HashMap::new();
    for project in rd.flatten() {
        if !project.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let dir = project.path();
        let archived = load_index(&dir.join("sessions-index.json"));
        let Ok(files) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut candidates = Vec::new();
        for file in files.flatten().take(MAX_FILES) {
            let path = file.path();
            if file.file_type().is_ok_and(|t| t.is_file()) {
                if let Some(id) = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .filter(|s| is_uuid(s))
                    && path.extension().is_some_and(|s| s == "jsonl")
                {
                    candidates.push((path.clone(), id.to_ascii_lowercase(), None));
                }
            } else if file.file_type().is_ok_and(|t| t.is_dir())
                && let Some(parent) = file
                    .file_name()
                    .to_str()
                    .filter(|s| is_uuid(s))
                    .map(str::to_ascii_lowercase)
            {
                for child in std::fs::read_dir(path.join("subagents"))
                    .into_iter()
                    .flatten()
                    .flatten()
                    .take(MAX_FILES)
                {
                    if !child.file_type().is_ok_and(|t| t.is_file()) {
                        continue;
                    }
                    let name = child.file_name();
                    let Some(agent) = name
                        .to_str()
                        .and_then(|s| s.strip_prefix("agent-"))
                        .and_then(|s| s.strip_suffix(".jsonl"))
                    else {
                        continue;
                    };
                    if agent.is_empty()
                        || agent.len() > 128
                        || !agent
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                    {
                        continue;
                    }
                    candidates.push((
                        child.path(),
                        format!("claude-subagent:{parent}:{agent}"),
                        Some(parent.clone()),
                    ));
                    if candidates.len() >= MAX_FILES {
                        break;
                    }
                }
            }
            if candidates.len() >= MAX_FILES {
                break;
            }
        }
        for (path, id, parent_id) in candidates {
            if by_id.len() >= MAX_FILES {
                break;
            }
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            if by_id.get(&id).is_some_and(|d| d.modified >= modified) {
                continue;
            }
            let head = Head::read(&path);
            let title = head
                .title
                .clone()
                .or(head.first_prompt.clone())
                .unwrap_or_else(|| format!("session {}", &id[..8]));
            let archived = archived.contains(&id) || head.archived;
            by_id.insert(
                id.clone(),
                Discovered {
                    id,
                    provider: Provider::Claude,
                    modified,
                    size: meta.len(),
                    archived,
                    hidden: head.sidechain || parent_id.is_some(),
                    relation: if parent_id.is_some() { "subagent" } else { "" },
                    parent_id,
                    custom_title: head.custom_title && head.title.is_some(),
                    meta: SessionMeta {
                        title,
                        cwd: head.cwd.clone().unwrap_or_default(),
                        updated_unix: 0,
                        archived,
                        state: SessionState::Idle,
                    },
                    entrypoint: head.entrypoint.clone(),
                    path,
                },
            );
        }
    }
    by_id.into_values().collect()
}

/// Claude.app records every Code conversation it can show under
/// `claude-code-sessions/<org>/<user>/<id>.json`, naming the Claude Code
/// session it drives as `cliSessionId`.
pub fn desktop_registry() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home).join("Library/Application Support/Claude/claude-code-sessions")
    })
}

/// A registry file's modification time and size, and the session it drives
/// with its archived flag.
type ArchiveEntry = (std::time::SystemTime, u64, Option<(String, bool)>);

static ARCHIVE_CACHE: std::sync::LazyLock<std::sync::Mutex<HashMap<PathBuf, ArchiveEntry>>> =
    std::sync::LazyLock::new(Default::default);

/// The Claude Code sessions Claude.app has archived: every desktop
/// conversation in the registry that drives them is archived, so the app
/// lists none of them (its Recents skip archived conversations). A file is
/// parsed again only when it changes; bounded like
/// [`desktop_registry_knows`], at most 4 MiB a file.
pub fn desktop_archived(root: &Path) -> HashSet<String> {
    let mut cache = ARCHIVE_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut present = HashSet::new();
    let mut entries = Vec::new();
    let mut seen = 0usize;
    let users = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .flat_map(|org| {
            std::fs::read_dir(org.path())
                .into_iter()
                .flatten()
                .flatten()
        });
    'files: for user in users {
        for file in std::fs::read_dir(user.path())
            .into_iter()
            .flatten()
            .flatten()
        {
            seen += 1;
            if seen > MAX_FILES {
                break 'files;
            }
            let path = file.path();
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            if path.extension().is_none_or(|e| e != "json")
                || !meta.is_file()
                || meta.len() > 4 << 20
            {
                continue;
            }
            let stamp = (
                meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                meta.len(),
            );
            let entry = match cache.get(&path) {
                Some((modified, len, entry)) if (*modified, *len) == stamp => entry.clone(),
                _ => {
                    let entry = std::fs::read(&path)
                        .ok()
                        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                        .and_then(|v| {
                            let id = v["cliSessionId"].as_str().filter(|id| is_uuid(id))?;
                            Some((
                                id.to_ascii_lowercase(),
                                v["isArchived"].as_bool().unwrap_or(false),
                            ))
                        });
                    cache.insert(path.clone(), (stamp.0, stamp.1, entry.clone()));
                    entry
                }
            };
            entries.extend(entry);
            present.insert(path);
        }
    }
    cache.retain(|path, _| present.contains(path));
    archived_sessions(entries)
}

/// Sessions every one of whose conversations is archived.
fn archived_sessions(entries: impl IntoIterator<Item = (String, bool)>) -> HashSet<String> {
    let mut all: HashMap<String, bool> = HashMap::new();
    for (id, archived) in entries {
        *all.entry(id).or_insert(true) &= archived;
    }
    all.into_iter()
        .filter_map(|(id, archived)| archived.then_some(id))
        .collect()
}

/// Whether any desktop conversation in the registry drives `session`.
/// Bounded: two directory levels, at most 10,000 files of at most 1 MiB.
pub fn desktop_registry_knows(root: &Path, session: &str) -> bool {
    let mut seen = 0usize;
    let Ok(orgs) = std::fs::read_dir(root) else {
        return false;
    };
    for org in orgs.flatten() {
        let Ok(users) = std::fs::read_dir(org.path()) else {
            continue;
        };
        for user in users.flatten() {
            let Ok(files) = std::fs::read_dir(user.path()) else {
                continue;
            };
            for file in files.flatten() {
                seen += 1;
                if seen > MAX_FILES {
                    return false;
                }
                let path = file.path();
                if path.extension().is_none_or(|e| e != "json")
                    || std::fs::metadata(&path).map_or(true, |m| !m.is_file() || m.len() > 1 << 20)
                {
                    continue;
                }
                let Ok(bytes) = std::fs::read(&path) else {
                    continue;
                };
                if serde_json::from_slice::<Value>(&bytes).is_ok_and(|v| {
                    v["cliSessionId"]
                        .as_str()
                        .is_some_and(|id| id.eq_ignore_ascii_case(session))
                }) {
                    return true;
                }
            }
        }
    }
    false
}

/// Inverse of the desktop-to-CLI surface identity. A complete registry scan
/// must identify exactly one active desktop entry, never the first title match.
pub fn desktop_navigation_id(root: &Path, session: &str) -> Option<String> {
    if !is_uuid(session) {
        return None;
    }
    let started = std::time::Instant::now();
    let mut seen = 0usize;
    let mut found = None;
    for org in std::fs::read_dir(root).ok()? {
        let org = org.ok()?;
        if !org.file_type().ok()?.is_dir() {
            continue;
        }
        for user in std::fs::read_dir(org.path()).ok()? {
            let user = user.ok()?;
            if !user.file_type().ok()?.is_dir() {
                continue;
            }
            for file in std::fs::read_dir(user.path()).ok()? {
                seen += 1;
                if seen > MAX_FILES || started.elapsed() > std::time::Duration::from_millis(250) {
                    return None;
                }
                let file = file.ok()?;
                let path = file.path();
                if path.extension().is_none_or(|e| e != "json") {
                    continue;
                }
                let stem = path.file_stem()?.to_str()?;
                let Some(uuid) = stem.strip_prefix("local_") else {
                    continue;
                };
                if !is_uuid(uuid) {
                    continue;
                }
                let meta = file.metadata().ok()?;
                if !file.file_type().ok()?.is_file() || meta.len() > 4 << 20 {
                    return None;
                }
                let v: Value = serde_json::from_slice(&std::fs::read(&path).ok()?).ok()?;
                if v["isArchived"].as_bool().unwrap_or(false) {
                    continue;
                }
                let id = v["cliSessionId"].as_str().filter(|id| is_uuid(id))?;
                if !id.eq_ignore_ascii_case(session) {
                    continue;
                }
                if v.get("sessionId")
                    .is_some_and(|id| id.as_str() != Some(stem))
                {
                    return None;
                }
                if found.is_some() {
                    return None;
                }
                found = Some(stem.to_owned());
            }
        }
    }
    found
}

/// The desktop's current navigation title and its uniqueness, not the title
/// retained by an older transcript. A complete registry read is required:
/// partial evidence cannot establish that no other visible conversation has
/// the same title. Ambiguous desktop mappings fall back to the conservative
/// transcript lookup at the caller; the opened session ID is always verified.
pub fn desktop_navigation_title(root: &Path, session: &str) -> Option<(String, bool)> {
    let mut entries = Vec::new();
    let mut seen = 0;
    for org in std::fs::read_dir(root).ok()? {
        let org = org.ok()?;
        if !org.file_type().ok()?.is_dir() {
            continue;
        }
        for user in std::fs::read_dir(org.path()).ok()? {
            let user = user.ok()?;
            if !user.file_type().ok()?.is_dir() {
                continue;
            }
            for file in std::fs::read_dir(user.path()).ok()? {
                let path = file.ok()?.path();
                if path.extension().is_none_or(|e| e != "json") {
                    continue;
                }
                // scheduled-tasks.json is shared metadata, not a sidebar
                // conversation. Desktop session files use local_<uuid> or
                // the older bare UUID, like the surface resolver.
                let stem = path.file_stem()?.to_str()?;
                if !is_uuid(stem.strip_prefix("local_").unwrap_or(stem)) {
                    continue;
                }
                seen += 1;
                let meta = std::fs::metadata(&path).ok()?;
                if seen > MAX_FILES || !meta.is_file() || meta.len() > 4 << 20 {
                    return None;
                }
                let v: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
                if v["isArchived"].as_bool().unwrap_or(false) {
                    continue;
                }
                let id = v["cliSessionId"].as_str().filter(|id| is_uuid(id))?;
                let title = v["title"]
                    .as_str()
                    .filter(|title| !title.trim().is_empty())?;
                entries.push((id.to_ascii_lowercase(), title.to_owned()));
            }
        }
    }
    let titles: HashSet<_> = entries
        .iter()
        .filter(|(id, _)| id == session)
        .map(|(_, title)| title)
        .collect();
    if titles.len() != 1 {
        return None;
    }
    let title = (*titles.into_iter().next()?).clone();
    let unique = entries
        .iter()
        .all(|(id, other)| other != &title || id == session);
    Some((title, unique))
}

/// Claude.app is the only input channel for a Claude Code session. A
/// transcript written by the command line (`cli`, `sdk-cli`, e.g. `claude -p`)
/// that no desktop conversation drives has no desktop surface, so it cannot
/// be controlled; never look for it by title in the app's sidebar. Desktop
/// transcripts, and transcripts that record no client, are left to the
/// desktop lookup as before.
pub fn desktop_input(
    session: &str,
    entrypoint: Option<&str>,
    registered: impl FnOnce() -> bool,
) -> Result<(), crate::Error> {
    match entrypoint {
        Some(client) if client != "claude-desktop" && !registered() => {
            Err(crate::Error::Unavailable(format!(
                "Claude Code session {} was started from the command line ({client}), not in Claude.app, and no Claude.app conversation drives it. Muster types only into conversations open in a desktop app, so this session cannot be controlled; nothing was sent.",
                &session[..8.min(session.len())]
            )))
        }
        _ => Ok(()),
    }
}

fn load_index(path: &Path) -> HashSet<String> {
    let mut out = HashSet::new();
    let Ok(meta) = std::fs::metadata(path) else {
        return out;
    };
    if !meta.is_file() || meta.len() > 16 * 1024 * 1024 {
        return out;
    }
    let Ok(text) = std::fs::read(path) else {
        return out;
    };
    let Ok(value) = serde_json::from_slice::<Value>(&text) else {
        return out;
    };
    let Some(entries) = value["entries"].as_array() else {
        return out;
    };
    for entry in entries {
        let archived = entry["archived"].as_bool().unwrap_or(false)
            || entry["isArchived"].as_bool().unwrap_or(false);
        if !archived {
            continue;
        }
        if let Some(id) = entry["sessionId"].as_str()
            && is_uuid(id)
        {
            out.insert(id.to_ascii_lowercase());
        }
    }
    out
}

/// A title record's text, and whether the user gave it (`custom-title`).
/// Claude Desktop shows a user's name over generated titles.
fn title_record(record: &Value) -> Option<(String, bool)> {
    let (key, custom) = match record["type"].as_str()? {
        "custom-title" => ("customTitle", true),
        "ai-title" => ("aiTitle", false),
        "summary" => ("summary", false),
        _ => return None,
    };
    record[key]
        .as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| (title_from(t), custom))
}

/// How far back from the end a changed transcript is searched for a rename.
const TITLE_TAIL: u64 = 1 << 20;

/// The title a changed transcript should list, given what was known before
/// (`prior`: the previous scan's or a viewer's title, else the file head's).
/// A rename near the end wins; a generated title never replaces a user's.
/// Title records leave the tail window as the conversation grows, so a
/// window without one keeps the prior title rather than the head's.
pub fn refreshed_title(path: &Path, prior: (String, bool)) -> (String, bool) {
    let last = |custom_only: bool| {
        crate::recency::last_record_within(path, TITLE_TAIL, |record| {
            title_record(record).filter(|(_, custom)| *custom || !custom_only)
        })
        .ok()
        .flatten()
    };
    match last(true).or_else(|| if prior.1 { None } else { last(false) }) {
        Some((title, custom)) => (title, custom || prior.1),
        None => prior,
    }
}

#[derive(Default)]
struct Head {
    cwd: Option<PathBuf>,
    title: Option<String>,
    custom_title: bool,
    first_prompt: Option<String>,
    sidechain: bool,
    archived: bool,
    entrypoint: Option<String>,
}

impl Head {
    fn read(path: &Path) -> Self {
        let mut head = Head::default();
        for record in head_records(path, 128) {
            if head.cwd.is_none()
                && let Some(cwd) = record["cwd"].as_str()
            {
                head.cwd = Some(PathBuf::from(cwd));
            }
            if record["isSidechain"].as_bool() == Some(true) {
                head.sidechain = true;
            }
            if head.entrypoint.is_none()
                && let Some(entrypoint) = record["entrypoint"].as_str()
            {
                head.entrypoint = Some(entrypoint.to_owned());
            }
            if record["archived"].as_bool() == Some(true)
                || record["isArchived"].as_bool() == Some(true)
            {
                head.archived = true;
            }
            let kind = record["type"].as_str().unwrap_or("");
            if let Some((title, custom)) = title_record(&record)
                && (custom || head.title.is_none())
            {
                head.title = Some(title);
                head.custom_title |= custom;
            }
            if head.first_prompt.is_none()
                && kind == "user"
                && record["isMeta"].as_bool() != Some(true)
            {
                for text in text_blocks(&record["message"]["content"]) {
                    let text = user_text(&text);
                    if !text.trim().is_empty() {
                        head.first_prompt = Some(title_from(&text));
                        break;
                    }
                }
            }
        }
        head
    }
}

fn text_blocks(content: &Value) -> Vec<String> {
    match content {
        Value::String(s) => vec![s.clone()],
        Value::Array(items) => items
            .iter()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

/// What a user record shows: reminder blocks stripped; text Claude Desktop
/// sent as pasted content unwrapped; slash-command wrappers
/// (`<command-name>`, `<local-command-stdout>`…) hidden. Empty = not a
/// prompt the user typed or pasted.
pub(crate) fn user_text(text: &str) -> String {
    let text = unwrap_pasted(&strip_hidden_blocks(text));
    if is_wrapper(&text) {
        String::new()
    } else {
        text
    }
}

/// Claude Desktop sends pasted text as `<pasted_content id="…">…` closed by
/// `</pasted_content>` (newer builds repeat the id there). The user pasted
/// it, so the reader sees the text without the tags.
fn unwrap_pasted(text: &str) -> String {
    const OPEN: &str = "<pasted_content";
    const CLOSE: &str = "</pasted_content";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        let tag = &rest[start + OPEN.len()..];
        let Some(end) = tag
            .find('>')
            .filter(|_| tag.starts_with(['>', ' ', '\t', '\n']))
        else {
            out.push_str(&rest[..start + OPEN.len()]);
            rest = tag;
            continue;
        };
        out.push_str(&rest[..start]);
        let body = &tag[end + 1..];
        let (pasted, after) = match body.find(CLOSE) {
            Some(close) => {
                let tail = &body[close..];
                (
                    &body[..close],
                    tail.find('>').map_or("", |i| &tail[i + 1..]),
                )
            }
            None => (body, ""),
        };
        out.push_str(pasted.trim_matches(['\r', '\n']));
        rest = after;
    }
    out.push_str(rest);
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewer::Viewer as _;
    use std::io::Write;

    /// Claude.app lists a session while any conversation driving it is not
    /// archived; a file that changes is read again.
    #[test]
    fn sessions_archived_in_the_desktop_registry() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("org").join("user");
        std::fs::create_dir_all(&user).unwrap();
        let hidden = "11111111-1111-4111-8111-111111111111";
        let shown = "22222222-2222-4222-8222-222222222222";
        let write = |name: &str, id: &str, archived: bool| {
            std::fs::write(
                user.join(name),
                format!(
                    r#"{{"cliSessionId":"{}","isArchived":{archived}}}"#,
                    id.to_uppercase()
                ),
            )
            .unwrap()
        };
        write("local_a.json", hidden, true);
        write("local_b.json", shown, true);
        write("local_c.json", shown, false);
        std::fs::write(user.join("notes.txt"), "not json").unwrap();
        assert_eq!(
            desktop_archived(dir.path()),
            HashSet::from([hidden.to_string()])
        );
        write("local_a.json", hidden, false);
        assert!(desktop_archived(dir.path()).is_empty());
        assert!(desktop_archived(&dir.path().join("missing")).is_empty());
    }

    #[test]
    fn command_line_sessions_without_a_desktop_conversation_cannot_be_controlled() {
        let dir = tempfile::tempdir().unwrap();
        let cli = "c86f6c98-b9c2-4d8c-9c4f-7291c03453c0";
        let user = dir.path().join("org").join("user");
        std::fs::create_dir_all(&user).unwrap();
        std::fs::write(
            user.join("local_404ab842-58db-4bc7-a64e-81916f114fc0.json"),
            r#"{"cliSessionId":"93CE5C70-7289-48fd-8d80-26ff3e14cfbb"}"#,
        )
        .unwrap();
        let knows = |id: &str| desktop_registry_knows(dir.path(), id);
        assert!(knows("93ce5c70-7289-48fd-8d80-26ff3e14cfbb"));
        assert!(!knows(cli));
        for client in ["sdk-cli", "cli"] {
            let error = desktop_input(cli, Some(client), || knows(cli)).unwrap_err();
            assert!(
                matches!(&error, crate::Error::Unavailable(m) if m.contains("nothing was sent")),
                "{error}"
            );
        }
        // Desktop transcripts, unrecorded clients and registered sessions
        // keep the desktop lookup.
        assert!(desktop_input(cli, Some("claude-desktop"), || false).is_ok());
        assert!(desktop_input(cli, None, || false).is_ok());
        assert!(desktop_input(cli, Some("sdk-cli"), || true).is_ok());
    }

    #[test]
    fn scans_titles_archives_and_parses_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let project = home.join("projects/-Users-x-repo");
        std::fs::create_dir_all(&project).unwrap();
        let id = "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d";
        let other = "b1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d";
        std::fs::write(
            project.join("sessions-index.json"),
            format!(r#"{{"version":1,"entries":[{{"sessionId":"{other}","isArchived":true}}]}}"#),
        )
        .unwrap();
        std::fs::write(project.join(format!("{other}.jsonl")), "{\"type\":\"user\",\"cwd\":\"/x\",\"message\":{\"role\":\"user\",\"content\":\"old\"}}\n").unwrap();
        let path = project.join(format!("{id}.jsonl"));
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"{{"type":"custom-title","sessionId":"{id}","customTitle":"Muster migration"}}"#
        )
        .unwrap();
        writeln!(f, r#"{{"type":"user","uuid":"u1","cwd":"/repo","timestamp":"2026-08-26T10:00:00Z","message":{{"role":"user","content":"hello <system-reminder>secret</system-reminder>"}}}}"#).unwrap();
        writeln!(f, r#"{{"type":"user","uuid":"u1b","isMeta":true,"message":{{"role":"user","content":"meta"}}}}"#).unwrap();
        writeln!(f, r#"{{"type":"user","uuid":"u1c","message":{{"role":"user","content":"<command-name>/clear</command-name>"}}}}"#).unwrap();
        writeln!(f, r#"{{"type":"assistant","uuid":"a1","timestamp":"2026-08-26T10:00:01Z","message":{{"role":"assistant","stop_reason":"tool_use","content":[{{"type":"thinking","thinking":"hmm"}},{{"type":"tool_use","id":"tool-1","name":"Read","input":{{"file_path":"/repo/a"}}}}]}}}}"#).unwrap();
        writeln!(f, r#"{{"type":"user","uuid":"u2","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"tool-1","content":"contents"}}]}}}}"#).unwrap();
        writeln!(f, r#"{{"type":"user","uuid":"side","isSidechain":true,"message":{{"role":"user","content":"hidden"}}}}"#).unwrap();

        let found = scan(home);
        assert_eq!(found.len(), 2);
        let main = found.iter().find(|d| d.id == id).unwrap();
        assert_eq!(main.meta.title, "Muster migration");
        assert_eq!(main.meta.cwd, PathBuf::from("/repo"));
        assert!(!main.archived);
        assert!(found.iter().find(|d| d.id == other).unwrap().archived);

        let mut viewer = viewer(main);
        let polled = viewer.poll().unwrap();
        let texts: Vec<_> = polled.messages.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(polled.messages[1].to_proto().tool_call_id, "tool-1");
        assert_eq!(polled.messages[2].to_proto().tool_call_id, "tool-1");
        assert_eq!(
            texts,
            vec![
                "hello",
                "Read {\"file_path\":\"/repo/a\"}",
                "Read: contents"
            ]
        );
        assert!(polled.messages[1].id.ends_with(":1"));
        assert_eq!(viewer.state(), SessionState::Running);

        writeln!(f, r#"{{"type":"assistant","uuid":"a2","message":{{"role":"assistant","stop_reason":"end_turn","content":[{{"type":"text","text":"done"}}]}}}}"#).unwrap();
        let polled = viewer.poll().unwrap();
        assert_eq!(polled.messages.len(), 2);
        assert_eq!(
            polled.messages[1].kind,
            crate::model::MessageKind::TurnSummary
        );
        assert_eq!(polled.messages[0].text, "done");
        assert_eq!(polled.messages[0].phase, "final_answer");
        assert_eq!(viewer.state(), SessionState::Idle);
    }

    #[test]
    fn pasted_content_from_claude_desktop_is_the_users_prompt() {
        assert_eq!(
            user_text(
                "\n\n<pasted_content id=\"fe42\">\n请分析 Muster 多次发送失败。\n\n<b>keep</b>\n</pasted_content id=\"fe42\">\n"
            ),
            "请分析 Muster 多次发送失败。\n\n<b>keep</b>"
        );
        assert_eq!(
            user_text(
                "see <pasted_content>a</pasted_content> and <pasted_content id=\"x\">b</pasted_content>"
            ),
            "see a and b"
        );
        assert_eq!(
            user_text("<pasted_content id=\"x\">unterminated"),
            "unterminated"
        );
        assert_eq!(user_text("<pasted_contents>x"), "");
        assert_eq!(user_text("<command-name>/clear</command-name>"), "");
        assert_eq!(user_text("<system-reminder>r</system-reminder>"), "");

        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("projects/-Volumes-repo");
        std::fs::create_dir_all(&project).unwrap();
        let id = "c1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d";
        let mut f = std::fs::File::create(project.join(format!("{id}.jsonl"))).unwrap();
        writeln!(f, r#"{{"type":"user","uuid":"u1","cwd":"/repo","origin":{{"kind":"human"}},"entrypoint":"claude-desktop","message":{{"role":"user","content":"\n\n<pasted_content id=\"fe42\">\nAnalyze the failures.\n</pasted_content id=\"fe42\">\n"}}}}"#).unwrap();
        writeln!(f, r#"{{"type":"assistant","uuid":"a1","message":{{"role":"assistant","stop_reason":"tool_use","content":[{{"type":"text","text":"Reading the logs."}}]}}}}"#).unwrap();
        let found = scan(dir.path());
        let session = found.iter().find(|d| d.id == id).unwrap();
        assert_eq!(session.meta.title, "Analyze the failures.");
        let mut viewer = viewer(session);
        let polled = viewer.poll().unwrap();
        assert_eq!(polled.messages[0].role, Role::User);
        assert_eq!(polled.messages[0].text, "Analyze the failures.");
        assert!(polled.messages[1].phase.is_empty());
        assert_eq!(viewer.state(), SessionState::Running);
    }
}

#[cfg(test)]
mod navigation_identity_tests {
    use super::*;
    const CLI: &str = "11111111-1111-4111-8111-111111111111";
    const DESKTOP: &str = "local_22222222-2222-4222-8222-222222222222";

    #[test]
    fn inverse_mapping_is_unique_live_and_distinct_from_cli_identity() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("org/user");
        std::fs::create_dir_all(&user).unwrap();
        let file = user.join(format!("{DESKTOP}.json"));
        let write = |id: &str, archived: bool| {
            std::fs::write(
                &file,
                serde_json::json!({"sessionId":DESKTOP,"cliSessionId":id,"isArchived":archived})
                    .to_string(),
            )
            .unwrap()
        };
        write(CLI, false);
        std::fs::write(user.join("scheduled-tasks.json"), "unrelated metadata").unwrap();
        assert_eq!(
            desktop_navigation_id(dir.path(), CLI).as_deref(),
            Some(DESKTOP)
        );
        assert_eq!(desktop_navigation_id(dir.path(), DESKTOP), None);
        assert_eq!(desktop_navigation_id(dir.path(), "last"), None);
        write(CLI, true);
        assert_eq!(desktop_navigation_id(dir.path(), CLI), None);
        write("33333333-3333-4333-8333-333333333333", false);
        assert_eq!(desktop_navigation_id(dir.path(), CLI), None);
        write(CLI, false);
        let other = dir.path().join("org/other-user");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::copy(&file, other.join(format!("{DESKTOP}.json"))).unwrap();
        assert_eq!(desktop_navigation_id(dir.path(), CLI), None);
    }

    #[test]
    fn incomplete_or_malformed_registry_cannot_authorize_navigation() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("org/user");
        std::fs::create_dir_all(&user).unwrap();
        let file = user.join(format!("{DESKTOP}.json"));
        for value in [
            "{".into(),
            "{}".into(),
            serde_json::json!({"cliSessionId":CLI,"sessionId":"local_wrong"}).to_string(),
            " ".repeat((4 << 20) + 1),
        ] {
            std::fs::write(&file, value).unwrap();
            assert_eq!(desktop_navigation_id(dir.path(), CLI), None);
        }
        assert_eq!(
            desktop_navigation_id(&dir.path().join("missing"), CLI),
            None
        );
    }
}
