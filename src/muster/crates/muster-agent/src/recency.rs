//! Last conversation-message time, independent of transcript filesystem mtime.
use crate::model::Provider;
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// When a message a person reads was written: the grammar's message rule
/// with a user or assistant role and content (text or media).
fn timestamp(record: &Value, provider: Provider) -> Option<u64> {
    use crate::profile::field;
    let (records, _) = provider.jsonl()?;
    let message = crate::jsonl::message(records, record)?;
    if !matches!(
        field(message, records.message.role).as_str(),
        Some("user" | "assistant")
    ) {
        return None;
    }
    let rule = &records.message;
    let types: Vec<&str> = rule
        .text_blocks
        .iter()
        .chain(rule.media_blocks)
        .copied()
        .collect();
    if !crate::jsonl::has_content(field(message, rule.content), &types) {
        return None;
    }
    let time = crate::jsonl::timestamp(records, record);
    (time > 0).then_some(time)
}

/// Scan backwards, keeping only one bounded JSON line in memory. Ignore a
/// partial final line: it will be reconsidered after the next file change.
pub fn last_message_time(path: &Path, provider: Provider) -> std::io::Result<u64> {
    Ok(last_record(path, |r| timestamp(r, provider))?.unwrap_or(0))
}

pub fn activity(path: &Path, provider: Provider, process_start: u64) -> crate::model::SessionState {
    activity_at(path, provider, process_start).0
}

/// The activity the newest dated record implies: the grammar's turn rule
/// (an end is idle, a start or, where the source has no events, any
/// message is running, a final-phase message is idle) and when.
pub fn activity_at(
    path: &Path,
    provider: Provider,
    process_start: u64,
) -> (crate::model::SessionState, u64) {
    activity_at_scope(path, provider, process_start, false)
}

/// A discovered Claude child has its own transcript whose conversation records
/// are marked sidechain. Normalize only that scope, just like its JSONL viewer.
pub(crate) fn subagent_activity_at(
    path: &Path,
    provider: Provider,
    process_start: u64,
) -> (crate::model::SessionState, u64) {
    activity_at_scope(path, provider, process_start, provider == Provider::Claude)
}

fn activity_at_scope(
    path: &Path,
    provider: Provider,
    process_start: u64,
    sidechain: bool,
) -> (crate::model::SessionState, u64) {
    use crate::model::SessionState as S;
    use crate::profile::field;
    let Some((records, _)) = provider.jsonl() else {
        return (S::Unknown, 0);
    };
    last_record(path, |r| {
        let normalized;
        let r = if sidechain && r["isSidechain"] == true {
            normalized = {
                let mut record = r.clone();
                record["isSidechain"] = Value::Bool(false);
                record
            };
            &normalized
        } else {
            r
        };
        let ts = crate::jsonl::timestamp(records, r);
        if ts == 0 || crate::jsonl::skipped(records, r) {
            return None;
        }
        let turn = &records.turn;
        let state = if turn.ends(r) {
            S::Idle
        } else if turn.started.as_ref().is_some_and(|m| m.matches(r)) {
            S::Running
        } else if records.message.when.matches(r) {
            let envelope = field(r, records.message.envelope);
            let final_phase = turn.final_phase.is_some_and(|phase| {
                records
                    .message
                    .phase
                    .is_some_and(|p| field(envelope, p) == phase)
            });
            if final_phase {
                S::Idle
            } else if turn.activity_on_messages {
                S::Running
            } else {
                return None;
            }
        } else {
            return None;
        };
        // A start left behind by a previous worker launch is not current work.
        Some((
            if state == S::Running && ts < process_start / 1_000_000 {
                S::Unknown
            } else {
                state
            },
            ts,
        ))
    })
    .ok()
    .flatten()
    .unwrap_or((S::Unknown, 0))
}

fn last_record<T>(path: &Path, select: impl Fn(&Value) -> Option<T>) -> std::io::Result<Option<T>> {
    last_record_within(path, u64::MAX, select)
}

/// [`last_record`] over at most the last `window` bytes; a line that starts
/// before the window is not read.
pub(crate) fn last_record_within<T>(
    path: &Path,
    window: u64,
    select: impl Fn(&Value) -> Option<T>,
) -> std::io::Result<Option<T>> {
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let floor = len.saturating_sub(window);
    let mut offset = len;
    let mut chunk = [0u8; 65536];
    let mut line = Vec::new();
    let mut complete = false;
    let mut oversized = false;
    while offset > floor {
        let n = (offset - floor).min(chunk.len() as u64) as usize;
        offset -= n as u64;
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut chunk[..n])?;
        for &byte in chunk[..n].iter().rev() {
            if byte == b'\n' {
                if complete && !oversized {
                    line.reverse();
                    if let Ok(record) = serde_json::from_slice::<Value>(&line) {
                        if let Some(ts) = select(&record) {
                            return Ok(Some(ts));
                        }
                    }
                }
                complete = true;
                oversized = false;
                line.clear();
            } else if complete && !oversized {
                if line.len() == 4 * 1024 * 1024 {
                    oversized = true;
                    line.clear();
                } else {
                    line.push(byte);
                }
            }
        }
    }
    // The first line is whole only when the window reaches the file start.
    if complete && !oversized && floor == 0 {
        line.reverse();
        if let Ok(record) = serde_json::from_slice::<Value>(&line) {
            return Ok(select(&record));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::parse_ts;
    #[test]
    fn claude_ignores_metadata_tools_sidechains_and_incomplete_records() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        use std::io::Write;
        writeln!(file, "{}", serde_json::json!({"type":"assistant","timestamp":"2026-09-28T01:00:00Z","message":{"role":"assistant","content":[{"type":"text","text":"reply"}]}})).unwrap();
        for extra in [
            serde_json::json!({"type":"progress","timestamp":"2026-09-28T02:00:00Z","data":"x".repeat(70000)}),
            serde_json::json!({"type":"user","timestamp":"2026-09-28T03:00:00Z","message":{"role":"user","content":[{"type":"tool_result","content":"done"}]}}),
            serde_json::json!({"type":"assistant","isSidechain":true,"timestamp":"2026-09-28T04:00:00Z","message":{"role":"assistant","content":"other"}}),
        ] {
            writeln!(file, "{extra}").unwrap();
        }
        write!(file, "{{\"type\":\"assistant\"").unwrap();
        assert_eq!(
            last_message_time(file.path(), Provider::Claude).unwrap(),
            parse_ts("2026-09-28T01:00:00Z")
        );
    }
}
