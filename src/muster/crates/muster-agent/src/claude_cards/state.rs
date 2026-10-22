//! Pending-card lifetime, incremental transcripts and desktop task receipts.
use super::presentation::{attachment_path, attachments};
use super::*;

#[derive(Clone, Default)]
pub struct Tracker {
    session: String,
    out: crate::questions::Snapshot,
    pending: Vec<PendingInteraction>,
    calls: std::collections::HashMap<String, (String, Value)>,
    tasks: std::collections::HashMap<String, String>,
    created: std::collections::HashMap<String, u64>,
}
impl Tracker {
    pub fn new(session: &str) -> Self {
        Self {
            session: session.into(),
            ..Default::default()
        }
    }
    pub fn record(&mut self, r: &Value) {
        if r["isSidechain"] == true
            || r["isMeta"] == true
            || r["isSyntheticResult"] == true
            || r["parent_tool_use_id"].is_string()
        {
            return;
        }
        let content = &r["message"]["content"];
        let human = r["type"] == "user"
            && r["isCompactSummary"] != true
            && !matches!(
                r["origin"]["kind"].as_str(),
                Some(
                    "task-notification"
                        | "peer"
                        | "channel"
                        | "coordinator"
                        | "observer"
                        | "observer-activity"
                        | "auto-continuation"
                )
            )
            && (content.as_str().is_some_and(|s| !s.trim().is_empty())
                || content.as_array().is_some_and(|b| {
                    b.iter().any(|v| v["type"] == "text")
                        && !b.iter().any(|v| v["type"] == "tool_result")
                }));
        if human {
            self.out.retired.extend(
                self.pending
                    .iter()
                    .filter(|q| is_wrap(q))
                    .map(|q| q.id.clone()),
            );
        }
        for b in content.as_array().into_iter().flatten() {
            if r["type"] == "assistant" && b["type"] == "tool_use" {
                let id = b["id"].as_str().unwrap_or("");
                let name = b["name"].as_str().unwrap_or("");
                let args = &b["input"];
                if id.is_empty() {
                    continue;
                }
                if name == WRAP {
                    self.out.retired.extend(
                        self.pending
                            .iter()
                            .filter(|q| is_wrap(q))
                            .map(|q| q.id.clone()),
                    );
                }
                let q = match name {
                    WRAP => wrap_up(args, id, &self.session),
                    START | SPAWN => proposal(args, id, &self.session, name == SPAWN),
                    "AskUserQuestion" => crate::questions::parse(args, id, &self.session),
                    _ => None,
                };
                if let Some(q) = q {
                    if q.questions
                        .iter()
                        .any(|q| q.id.starts_with(PROPOSAL_QUESTION))
                    {
                        let ts = r["timestamp"]
                            .as_str()
                            .map(crate::model::parse_ts)
                            .unwrap_or(0);
                        self.created.insert(q.id.clone(), ts);
                    }
                    self.pending.retain(|old| old.id != q.id);
                    self.pending.push(q);
                }
                if matches!(name, WRAP | START | SPAWN | DISMISS | "AskUserQuestion") {
                    self.calls.insert(id.into(), (name.into(), args.clone()));
                }
            } else if b["type"] == "tool_result" {
                let id = b["tool_use_id"].as_str().unwrap_or("");
                let qid = format!("question:{id}");
                let Some((name, args)) = self.calls.remove(id) else {
                    continue;
                };
                if b["is_error"] == true {
                    self.out.retired.insert(qid);
                    continue;
                }
                match name.as_str() {
                    "AskUserQuestion" => {
                        self.out.resolved.insert(qid);
                    }
                    START if args["initiation"] == "offer" => {
                        self.out.resolved.insert(qid);
                    }
                    START | SPAWN
                        if proposal(&args, id, &self.session, name == SPAWN).is_some() =>
                    {
                        if let Some(task) =
                            task_id(&r["toolUseResult"]).or_else(|| task_id(&b["content"]))
                        {
                            if let Some(q) = self.pending.iter_mut().find(|q| q.id == qid) {
                                q.questions[0].id = format!("{PROPOSAL_QUESTION}:{task}");
                            }
                            self.tasks.insert(task, qid);
                        }
                    }
                    START | DISMISS => {
                        let task = args["proposal_id"]
                            .as_str()
                            .or_else(|| args["task_id"].as_str());
                        if let Some(qid) = task.and_then(|id| self.tasks.get(id)) {
                            self.out.retired.insert(qid.clone());
                        }
                    }
                    WRAP => {
                        let result = output(&b["content"]);
                        if result.contains("shows no next-step box")
                            || result.contains("no next-step box is shown")
                            || result.contains("app draws no box")
                        {
                            self.out.retired.insert(qid);
                        }
                    }
                    _ => {}
                }
            }
        }

        self.pending
            .retain(|q| !self.out.retired.contains(&q.id) && !self.out.resolved.contains(&q.id));
    }
    pub fn snapshot_at(&self, now: u64) -> crate::questions::Snapshot {
        let mut out = self.out.clone();
        out.complete = true;
        let mut pending = self.pending.clone();
        pending.retain(|q| {
            if self
                .created
                .get(&q.id)
                .is_some_and(|ts| *ts > 0 && now.saturating_sub(*ts) >= 7 * 86400)
            {
                out.retired.insert(q.id.clone());
                false
            } else {
                true
            }
        });
        pending.retain(|q| !q.questions.iter().any(|q| q.id == PROPOSAL_QUESTION));
        pending.sort_by_key(|q| {
            (
                q.optional,
                q.questions
                    .iter()
                    .any(|q| q.id.starts_with(PROPOSAL_QUESTION)),
            )
        });
        if !pending.is_empty() {
            out.pending = Some(pending.remove(0));
        }
        out.additional = pending;
        out
    }
    pub fn snapshot(&self) -> crate::questions::Snapshot {
        self.snapshot_at(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        )
    }
}
pub fn snapshot(bytes: &[u8], session: &str) -> crate::questions::Snapshot {
    let mut state = Tracker::new(session);
    for line in bytes.split(|b| *b == b'\n') {
        if let Ok(record) = serde_json::from_slice::<Value>(line) {
            state.record(&record);
        }
    }
    state.snapshot()
}

/// Stream complete records with bounded allocation. Never accept a partial
/// final result as proof of delivery or question resolution.
pub fn records(path: &std::path::Path, visit: impl FnMut(&Value)) -> std::io::Result<()> {
    records_from(path, 0, visit).map(|_| ())
}
fn records_from(
    path: &std::path::Path,
    offset: u64,
    mut visit: impl FnMut(&Value),
) -> std::io::Result<u64> {
    use std::io::{BufRead, Read, Seek};
    let mut reader = std::io::BufReader::new(std::fs::File::open(path)?);
    reader.seek(std::io::SeekFrom::Start(offset))?;
    let mut complete = offset;
    loop {
        let mut line = Vec::new();
        let count = reader
            .by_ref()
            .take(16 * 1024 * 1024)
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            break;
        }
        if line.last() != Some(&b'\n') {
            // Skip an oversized record without allocating for the remainder.
            // An incomplete record is retried from its start on the next read.
            while !reader.fill_buf()?.is_empty() {
                let buf = reader.fill_buf()?;
                let n = buf
                    .iter()
                    .position(|b| *b == b'\n')
                    .map(|i| i + 1)
                    .unwrap_or(buf.len());
                let end = buf[n - 1] == b'\n';
                reader.consume(n);
                if end {
                    complete = reader.stream_position()?;
                    break;
                }
            }
            continue;
        }
        complete = reader.stream_position()?;
        if let Ok(record) = serde_json::from_slice(&line) {
            visit(&record);
        }
    }
    Ok(complete)
}

/// Cached transcript semantics; registry state and expiry are refreshed even
/// when no JSONL record was appended (e.g. the user started a proposal in UI).
pub struct TrackedFile {
    path: std::path::PathBuf,
    modified: std::time::SystemTime,
    length: u64,
    offset: u64,
    tracker: Tracker,
}
impl TrackedFile {
    pub fn new(path: &std::path::Path, session: &str) -> Self {
        Self {
            path: path.into(),
            modified: std::time::UNIX_EPOCH,
            length: 0,
            offset: 0,
            tracker: Tracker::new(session),
        }
    }
    pub fn read(
        &mut self,
        path: &std::path::Path,
        session: &str,
    ) -> Option<crate::questions::Snapshot> {
        let meta = std::fs::metadata(path).ok()?;
        let modified = meta.modified().ok()?;
        if self.path != path
            || meta.len() < self.length
            || meta.len() == self.length && modified != self.modified
        {
            *self = Self::new(path, session);
        }
        if meta.len() != self.length || modified != self.modified {
            self.offset = records_from(path, self.offset, |r| self.tracker.record(r)).ok()?;
            self.length = meta.len();
            self.modified = modified;
        }
        let mut snapshot = self.tracker.snapshot();
        desktop_tasks(&mut snapshot, session);
        Some(snapshot)
    }
}

pub fn delivered_key(link: &str) -> Option<(&str, usize)> {
    let (id, index) = link.strip_prefix("muster-delivered:")?.rsplit_once(':')?;
    if id.is_empty()
        || id.len() > 160
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    let index = index.parse::<usize>().ok()?;
    (index < 1000).then_some((id, index))
}
pub fn delivered_file(
    transcript: &std::path::Path,
    link: &str,
) -> Result<std::path::PathBuf, crate::Error> {
    let (id, index) = delivered_key(link)
        .ok_or_else(|| crate::Error::Invalid("Invalid delivered file link".into()))?;
    let mut declared = false;
    let mut path = None;
    records(transcript, |r| {
        if r["isSidechain"] == true || r["isMeta"] == true {
            return;
        }
        for b in r["message"]["content"].as_array().into_iter().flatten() {
            if r["type"] == "assistant" && b["type"] == "tool_use" && b["id"] == id {
                declared = b["name"] == "SendUserFile";
            }
            if declared && b["type"] == "tool_result" && b["tool_use_id"] == id {
                path = if b["is_error"] == true {
                    None
                } else {
                    attachments(&r["toolUseResult"])
                        .get(index)
                        .filter(|a| a["pathValidated"] == true)
                        .and_then(|a| attachment_path(a))
                        .map(std::path::PathBuf::from)
                };
            }
        }
    })
    .map_err(|e| crate::Error::Io(e.to_string()))?;
    let path = path.ok_or_else(|| {
        crate::Error::NotFound("This file has no confirmed delivery in this session".into())
    })?;
    let meta = std::fs::symlink_metadata(&path).map_err(|e| crate::Error::Io(e.to_string()))?;
    if !meta.is_file() {
        return Err(crate::Error::Invalid(
            "Delivered attachment is not a regular file".into(),
        ));
    }
    Ok(path)
}

pub fn desktop_tasks(snapshot: &mut crate::questions::Snapshot, session: &str) {
    let Some(root) = crate::claude::desktop_registry() else {
        return;
    };
    desktop_tasks_at(&root, snapshot, session);
}
type TaskRegistryEntry = (std::time::SystemTime, u64, String, Value);
pub(super) fn desktop_tasks_at(
    root: &std::path::Path,
    snapshot: &mut crate::questions::Snapshot,
    session: &str,
) {
    use std::collections::HashMap;
    static CACHE: std::sync::LazyLock<
        std::sync::Mutex<HashMap<std::path::PathBuf, TaskRegistryEntry>>,
    > = std::sync::LazyLock::new(Default::default);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let mut seen = 0;
    let mut present = std::collections::HashSet::new();
    let users = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .flat_map(|o| std::fs::read_dir(o.path()).into_iter().flatten().flatten());
    'files: for user in users {
        for entry in std::fs::read_dir(user.path())
            .into_iter()
            .flatten()
            .flatten()
        {
            seen += 1;
            if seen > 10000 {
                break 'files;
            }
            let path = entry.path();
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if path.extension().is_none_or(|e| e != "json")
                || !meta.is_file()
                || meta.len() > 4 * 1024 * 1024
            {
                continue;
            }
            let stamp = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
            present.insert(path.clone());
            if cache
                .get(&path)
                .is_none_or(|(ts, size, _, _)| *ts != stamp || *size != meta.len())
            {
                let Some(r) = std::fs::read(&path)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                else {
                    cache.remove(&path);
                    continue;
                };
                cache.insert(path.clone(), (stamp, meta.len(), r["cliSessionId"].as_str().unwrap_or("").into(), serde_json::json!({
                    "resolved": r["resolvedBackgroundTaskSuggestions"],
                    "active": r["backgroundTaskSuggestions"].as_array().map(|a| a.iter().map(|p| serde_json::json!({"id":p["id"], "expiresAt":p["expiresAt"], "accepted":p["accepted"]})).collect::<Vec<_>>())
                })));
            }
            if let Some((_, _, id, tasks)) = cache.get(&path).filter(|(_, _, id, _)| id == session)
            {
                let _ = id;
                reconcile_tasks(snapshot, tasks);
            }
        }
    }
    cache.retain(|p, _| present.contains(p));
}
pub(super) fn reconcile_tasks(snapshot: &mut crate::questions::Snapshot, tasks: &Value) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let retired: Vec<_> = snapshot
        .iter()
        .filter_map(|q| {
            let task = q.questions.first()?.id.strip_prefix("claude:proposal:")?;
            let resolved = matches!(
                tasks["resolved"][task].as_str(),
                Some("started" | "started_notified" | "dismissed" | "expired")
            );
            let inactive = tasks["active"].as_array().into_iter().flatten().any(|p| {
                p["id"] == task
                    && (p["accepted"] == true
                        || p["expiresAt"].as_u64().is_some_and(|ts| ts <= now))
            });
            (resolved || inactive).then(|| q.id.clone())
        })
        .collect();
    snapshot.retired.extend(retired);
    snapshot.pending = snapshot
        .pending
        .take()
        .filter(|q| !snapshot.retired.contains(&q.id));
    snapshot
        .additional
        .retain(|q| !snapshot.retired.contains(&q.id));
    if snapshot.pending.is_none() && !snapshot.additional.is_empty() {
        snapshot.pending = Some(snapshot.additional.remove(0));
    }
}
