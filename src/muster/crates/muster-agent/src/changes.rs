//! Recorded edits belong to a turn, not to the current dirty working tree.
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Component, Path};

use muster_proto::v1::TurnFileChange;
use serde_json::Value;

const MAX_DIFF: usize = 32 * 1024;
const MAX_FILES: usize = 32;

#[derive(Clone, Default)]
pub struct Changes {
    files: BTreeMap<String, TurnFileChange>,
    pending: HashMap<String, Vec<TurnFileChange>>,
    completed: HashSet<String>,
    incomplete: bool,
}

fn bounded(text: &str) -> (String, bool) {
    let mut end = text.len().min(MAX_DIFF);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].into(), end < text.len())
}

fn change(path: &str, kind: &str, diff: &str) -> TurnFileChange {
    let (diff, truncated) = bounded(diff);
    let mut added = 0;
    let mut removed = 0;
    let mut hunk = !diff.contains("--- ");
    for line in diff.lines() {
        if line.starts_with("@@") {
            hunk = true;
        }
        if line.starts_with('+') && (hunk || !line.starts_with("+++")) {
            added += 1;
        }
        if line.starts_with('-') && (hunk || !line.starts_with("---")) {
            removed += 1;
        }
    }
    TurnFileChange {
        path: path.into(),
        kind: kind.into(),
        additions: added,
        deletions: removed,
        counts_known: !diff.is_empty() && !truncated,
        diff,
        truncated,
        ..Default::default()
    }
}

fn lines_with(text: &str, prefix: char) -> String {
    text.lines()
        .map(|line| format!("{prefix}{line}\n"))
        .collect()
}

/// Model-supplied patches are only candidates until a success result arrives.
fn patch(text: &str) -> Vec<TurnFileChange> {
    let mut out = Vec::new();
    let mut path = String::new();
    let mut old = String::new();
    let mut kind = "update";
    let mut diff = String::new();
    let flush = |out: &mut Vec<TurnFileChange>, path: &str, old: &str, kind: &str, diff: &str| {
        if !path.is_empty() && out.len() <= MAX_FILES {
            let mut c = change(path, kind, diff);
            c.previous_path = old.into();
            out.push(c);
        }
    };
    for line in text.lines() {
        let header = [
            ("*** Add File: ", "add"),
            ("*** Update File: ", "update"),
            ("*** Delete File: ", "delete"),
        ]
        .into_iter()
        .find_map(|(prefix, kind)| line.strip_prefix(prefix).map(|p| (p, kind)));
        if let Some((next, next_kind)) = header {
            flush(&mut out, &path, &old, kind, &diff);
            path = next.into();
            old.clear();
            kind = next_kind;
            diff.clear();
        } else if let Some(next) = line.strip_prefix("*** Move to: ") {
            old = path.clone();
            path = next.into();
            kind = "rename";
        } else if !path.is_empty() && !line.starts_with("***") && diff.len() <= MAX_DIFF {
            diff.push_str(line);
            diff.push('\n');
        }
    }
    flush(&mut out, &path, &old, kind, &diff);
    out
}

/// A turn_diff replaces prior snapshots rather than double counting them.
fn unified(text: &str) -> Vec<TurnFileChange> {
    let mut out = Vec::new();
    let mut old = String::new();
    let mut path = String::new();
    let mut diff = String::new();
    let flush = |out: &mut Vec<TurnFileChange>, old: &str, path: &str, diff: &str| {
        let target = if path == "/dev/null" { old } else { path };
        if target.is_empty() || target.starts_with('"') || out.len() > MAX_FILES {
            return;
        }
        let kind = if old == "/dev/null" {
            "add"
        } else if path == "/dev/null" {
            "delete"
        } else if old != path {
            "rename"
        } else {
            "update"
        };
        let mut c = change(target, kind, diff);
        if kind == "rename" {
            c.previous_path = old.into();
        }
        out.push(c);
    };
    let clean = |s: &str| {
        s.split('\t')
            .next()
            .unwrap_or(s)
            .strip_prefix("a/")
            .or_else(|| s.split('\t').next().unwrap_or(s).strip_prefix("b/"))
            .unwrap_or(s)
            .to_string()
    };
    for line in text.lines() {
        if line.starts_with("diff --git ") {
            flush(&mut out, &old, &path, &diff);
            old.clear();
            path.clear();
            diff.clear();
        }
        if let Some(p) = line.strip_prefix("--- ") {
            old = clean(p);
        }
        if let Some(p) = line.strip_prefix("+++ ") {
            path = clean(p);
        }
        if diff.len() <= MAX_DIFF {
            diff.push_str(line);
            diff.push('\n');
        }
    }
    flush(&mut out, &old, &path, &diff);
    out
}

impl Changes {
    pub fn incomplete(&self) -> bool {
        self.incomplete
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn record(&mut self, changes: Vec<TurnFileChange>) {
        for mut c in changes {
            if c.path.is_empty() || c.path.len() > 4096 {
                continue;
            }
            if let Some(old) = self.files.get_mut(&c.path) {
                // Multiple successful edit operations: retain their recorded hunks.
                let joined = format!("{}\n{}", old.diff, c.diff);
                let (diff, truncated) = bounded(&joined);
                old.diff = diff;
                old.additions = old.additions.saturating_add(c.additions);
                old.deletions = old.deletions.saturating_add(c.deletions);
                old.counts_known &= c.counts_known && !truncated;
                old.truncated |= c.truncated || truncated;
                if c.kind == "delete" {
                    old.kind = c.kind;
                }
            } else if self.files.len() < MAX_FILES {
                c.open_path.clear();
                self.files.insert(c.path.clone(), c);
            } else {
                self.incomplete = true;
            }
        }
    }

    pub fn codex(&mut self, p: &Value) {
        let kind = p["type"].as_str().unwrap_or("");
        let call = p["call_id"].as_str().unwrap_or("");
        if kind == "item_completed" && p["item"]["type"] == "FileChange" {
            let item = &p["item"];
            self.codex(&serde_json::json!({
                "type": "patch_apply_end", "call_id": item["id"],
                "status": item["status"], "changes": item["changes"]
            }));
            return;
        }
        match kind {
            "function_call" | "custom_tool_call"
                if p["name"]
                    .as_str()
                    .is_some_and(|n| n.ends_with("apply_patch")) =>
            {
                let input = p["input"]
                    .as_str()
                    .or_else(|| p["arguments"].as_str())
                    .unwrap_or("");
                let parsed = serde_json::from_str::<Value>(input).ok();
                let text = parsed
                    .as_ref()
                    .and_then(|v| v["patch"].as_str().or_else(|| v["input"].as_str()))
                    .unwrap_or(input);
                if self.pending.len() < 128 {
                    self.pending.insert(call.into(), patch(text));
                }
            }
            "function_call_output" | "custom_tool_call_output" => {
                if let Some(changes) = self.pending.remove(call) {
                    let output = p["output"].as_str().unwrap_or("");
                    if output.starts_with("Success.") && self.completed.insert(call.into()) {
                        self.record(changes);
                    }
                }
            }
            "patch_apply_end"
                if p["success"] == true
                    || (p["success"].is_null() && p["status"] == "completed") =>
            {
                if !call.is_empty() && !self.completed.insert(call.into()) {
                    return;
                }
                self.pending.remove(call);
                let Some(files) = p["changes"].as_object() else {
                    return;
                };
                let changes = files
                    .iter()
                    .take(MAX_FILES + 1)
                    .map(|(path, v)| {
                        let kind = v["type"].as_str().unwrap_or("update");
                        let diff = match kind {
                            "add" => lines_with(v["content"].as_str().unwrap_or(""), '+'),
                            "delete" => lines_with(v["content"].as_str().unwrap_or(""), '-'),
                            _ => v["unified_diff"].as_str().unwrap_or("").into(),
                        };
                        let mut c = change(path, kind, &diff);
                        if let Some(next) = v["move_path"].as_str() {
                            c.previous_path = path.clone();
                            c.path = next.into();
                            c.kind = "rename".into();
                        }
                        c
                    })
                    .collect();
                self.record(changes);
            }
            "turn_diff" => {
                if let Some(diff) = p["unified_diff"].as_str() {
                    let changes = unified(diff);
                    if diff.is_empty() || !changes.is_empty() {
                        self.files.clear();
                        self.record(changes);
                    } else {
                        self.incomplete = true;
                    }
                }
            }
            _ => {}
        }
    }

    pub fn claude(&mut self, record: &Value) {
        let Some(blocks) = record["message"]["content"].as_array() else {
            return;
        };
        for block in blocks {
            if block["type"] == "tool_use" {
                let name = block["name"].as_str().unwrap_or("");
                let input = &block["input"];
                let Some(path) = input["file_path"].as_str() else {
                    continue;
                };
                let id = block["id"].as_str().unwrap_or("");
                let edits: Vec<_> = match name {
                    "Edit" => vec![input],
                    "MultiEdit" => input["edits"]
                        .as_array()
                        .map(|e| e.iter().collect())
                        .unwrap_or_default(),
                    "Write" => Vec::new(),
                    _ => continue,
                };
                let mut diff = String::new();
                for edit in edits {
                    diff.push_str("@@ recorded edit @@\n");
                    diff.push_str(&lines_with(edit["old_string"].as_str().unwrap_or(""), '-'));
                    diff.push_str(&lines_with(edit["new_string"].as_str().unwrap_or(""), '+'));
                }
                if self.pending.len() < 128 {
                    self.pending
                        .insert(id.into(), vec![change(path, "update", &diff)]);
                }
            } else if block["type"] == "tool_result" {
                let id = block["tool_use_id"].as_str().unwrap_or("");
                let Some(mut changes) = self.pending.remove(id) else {
                    continue;
                };
                if block["is_error"] == true || !self.completed.insert(id.into()) {
                    continue;
                }
                // Claude's result carries the exact hunks, including replace_all.
                let result = &record["toolUseResult"];
                if let Some(hunks) = result["structuredPatch"].as_array() {
                    let mut diff = String::new();
                    for hunk in hunks {
                        diff.push_str(&format!(
                            "@@ -{},{} +{},{} @@\n",
                            hunk["oldStart"], hunk["oldLines"], hunk["newStart"], hunk["newLines"]
                        ));
                        if let Some(lines) = hunk["lines"].as_array() {
                            for line in lines {
                                if let Some(s) = line.as_str() {
                                    diff.push_str(s);
                                    diff.push('\n');
                                }
                            }
                        }
                    }
                    changes[0] = change(&changes[0].path, "update", &diff);
                } else if result["type"] == "create" {
                    changes[0] = change(
                        &changes[0].path,
                        "add",
                        &lines_with(result["content"].as_str().unwrap_or(""), '+'),
                    );
                }
                self.record(changes);
            }
        }
    }

    pub fn snapshot(&self, cwd: &Path) -> Vec<TurnFileChange> {
        let mut remaining = 256 * 1024;
        self.files
            .values()
            .cloned()
            .map(|mut c| {
                if c.diff.len() > remaining {
                    let mut end = remaining;
                    while !c.diff.is_char_boundary(end) {
                        end -= 1;
                    }
                    c.diff.truncate(end);
                    c.truncated = true;
                    c.counts_known = false;
                }
                remaining -= c.diff.len();
                let path = Path::new(&c.path);
                let rel = if path.is_absolute() {
                    path.strip_prefix(cwd).ok()
                } else {
                    Some(path)
                };
                if c.kind != "delete"
                    && let Some(rel) = rel
                    && !rel.as_os_str().is_empty()
                    && rel
                        .components()
                        .all(|part| matches!(part, Component::Normal(_) | Component::CurDir))
                {
                    c.open_path = rel.to_string_lossy().into_owned();
                }
                c
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn codex_waits_for_success_deduplicates_and_preserves_renames() {
        let mut c = Changes::default();
        let input = "*** Begin Patch\n*** Update File: /repo/a.rs\n*** Move to: /repo/b.rs\n@@\n-old\n+new\n*** End Patch";
        c.codex(
            &json!({"type":"custom_tool_call","name":"apply_patch","call_id":"a","input":input}),
        );
        assert!(c.snapshot(Path::new("/repo")).is_empty());
        c.codex(&json!({"type":"custom_tool_call_output","call_id":"a","output":"failed"}));
        assert!(c.snapshot(Path::new("/repo")).is_empty());
        c.codex(
            &json!({"type":"custom_tool_call","name":"apply_patch","call_id":"b","input":input}),
        );
        c.codex(&json!({"type":"custom_tool_call_output","call_id":"b","output":"Success. Updated the following files:"}));
        c.codex(&json!({"type":"patch_apply_end","call_id":"b","success":true,"changes":{"/repo/b.rs":{"type":"update","unified_diff":"-old\n+new\n"}}}));
        let files = c.snapshot(Path::new("/repo"));
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].kind, "rename");
        assert_eq!(files[0].previous_path, "/repo/a.rs");
        assert_eq!(files[0].open_path, "b.rs");
        assert_eq!((files[0].additions, files[0].deletions), (1, 1));
        c.reset();
        assert!(c.snapshot(Path::new("/repo")).is_empty());
    }
    #[test]
    fn snapshot_diff_replaces_prior_edits_and_deleted_or_outside_files_cannot_open() {
        let mut c = Changes::default();
        let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ /dev/null\n@@ -1 +0 @@\n-old\n";
        c.codex(&json!({"type":"turn_diff","unified_diff":diff}));
        c.codex(&json!({"type":"turn_diff","unified_diff":diff}));
        let files = c.snapshot(Path::new("/repo"));
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].deletions, 1);
        assert_eq!(files[0].kind, "delete");
        assert!(files[0].open_path.is_empty());
        c.record(vec![
            change("/outside/a", "update", "+x"),
            change("../a", "update", "+x"),
        ]);
        assert!(
            c.snapshot(Path::new("/repo"))
                .iter()
                .all(|f| f.open_path.is_empty())
        );
    }
    #[test]
    fn claude_uses_successful_result_hunks_not_failed_edit_intentions() {
        let mut c = Changes::default();
        let call = |id: &str| json!({"type":"assistant","message":{"content":[{"type":"tool_use","name":"Edit","id":id,"input":{"file_path":"/repo/文件.swift","old_string":"old","new_string":"new"}}]}});
        c.claude(&call("a"));
        c.claude(&json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"a","is_error":true}]}}));
        assert!(c.snapshot(Path::new("/repo")).is_empty());
        c.claude(&call("b"));
        let result = json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"b"}]},"toolUseResult":{"structuredPatch":[{"oldStart":1,"oldLines":1,"newStart":1,"newLines":2,"lines":["-old","+new","+extra"]}]}});
        c.claude(&result);
        c.claude(&result);
        let files = c.snapshot(Path::new("/repo"));
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].open_path, "文件.swift");
        assert_eq!((files[0].additions, files[0].deletions), (2, 1));
        assert!(files[0].diff.contains("@@ -1,1 +1,2 @@"));
    }
}
