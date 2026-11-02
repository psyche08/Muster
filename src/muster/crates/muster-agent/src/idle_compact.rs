//! Idle compaction (ADR 0083): once a desktop Code conversation has been idle
//! for [`IDLE`], send its provider's `/compact` once, while the provider's
//! one-hour prompt cache is still warm, so the next turn starts from a short
//! summary instead of re-reading the whole conversation. Opt-in per Mac.
//!
//! This module only decides. The daemon adds this Mac's facts (opt-in, screen
//! lock, local input) and drives the same native path as a send, which keeps a
//! person's draft and is confirmed only by the provider's own record.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use crate::model::{AgentSession, Provider};

/// What is typed into the native prompt.
pub const COMMAND: &str = "/compact";
/// Quiet time after the newest conversation message before compacting.
pub const IDLE: Duration = Duration::from_secs(30 * 60);
/// Past this the prompt cache has expired: compacting then costs a full
/// re-read for nothing, and an old conversation is left as the person left it.
pub const LATEST: Duration = Duration::from_secs(60 * 60);
/// How long this Mac's own keyboard and mouse must have been idle. Selecting
/// another conversation in a desktop app is visible; never do it to a person
/// who is working at the Mac.
pub const LOCAL_IDLE: Duration = Duration::from_secs(2 * 60);
/// How often the daemon looks; at most one conversation per look.
pub const TICK: Duration = Duration::from_secs(60);

/// Providers whose desktop prompt runs a typed `/compact` when sent. Claude
/// Desktop hands the text to Claude Code, which records the command. The Codex
/// desktop composer runs slash commands only from its suggestion menu; sending
/// the same text would reach the model as an ordinary prompt.
pub fn typed_command(provider: &str) -> bool {
    provider == Provider::Claude.as_str()
}

/// Whether `session` is a writable desktop Code conversation that has been
/// idle for at least [`IDLE`] and at most [`LATEST`] at `now` (Unix seconds).
pub fn due(session: &AgentSession, now: u64) -> bool {
    let idle = now.saturating_sub(session.updated_unix);
    typed_command(&session.provider)
        && session.mode == "code"
        && !session.archived
        && !session.read_only
        && session.state == "idle"
        && session.process_state == "running"
        && session.descendant_activity.is_empty()
        && session.updated_unix > 0
        && idle >= IDLE.as_secs()
        && idle <= LATEST.as_secs()
}

/// One attempt per idle period. A period is named by the session's newest
/// message time, so new activity opens a new one; a failed or unconfirmed
/// attempt is never repeated within the same period.
#[derive(Default)]
pub struct Tracker {
    tried: HashMap<String, u64>,
}

impl Tracker {
    pub fn untried(&self, session: &AgentSession) -> bool {
        self.tried.get(&session.id) != Some(&session.updated_unix)
    }

    pub fn tried(&mut self, session: &AgentSession) {
        self.tried.insert(session.id.clone(), session.updated_unix);
    }

    /// Forget periods that can no longer be due.
    pub fn prune(&mut self, now: u64) {
        self.tried
            .retain(|_, updated| now.saturating_sub(*updated) <= LATEST.as_secs());
    }
}

/// How far back to look for the newest conversation record. A compaction
/// summary is large, but a few MiB hold it and the records around it.
const WINDOW: u64 = 8 << 20;

/// Whether the newest conversation record is a compaction, so compacting again
/// would only summarize a summary. Survives a daemon restart, unlike [`Tracker`].
pub fn compacted(path: &Path, provider: Provider) -> bool {
    crate::recency::last_record_within(path, WINDOW, |r| compaction(r, provider))
        .ok()
        .flatten()
        .unwrap_or(false)
}

/// `Some(true)` for a compaction record, `Some(false)` for a conversation
/// message, `None` for anything else (metadata, the command's own records).
fn compaction(record: &Value, provider: Provider) -> Option<bool> {
    match provider {
        Provider::Claude => {
            if (record["type"] == "system" && record["subtype"] == "compact_boundary")
                || record["isCompactSummary"] == true
            {
                return Some(true);
            }
            if record["isMeta"] == true || record["isSidechain"] == true {
                return None;
            }
            let message = &record["message"];
            match message["role"].as_str()? {
                "assistant" => Some(false),
                "user" => {
                    let content = &message["content"];
                    let text = match content {
                        Value::String(text) => text.clone(),
                        Value::Array(parts) => {
                            if parts.iter().any(|p| p["type"] == "tool_result") {
                                return Some(false);
                            }
                            parts
                                .iter()
                                .filter(|p| p["type"] == "text")
                                .filter_map(|p| p["text"].as_str())
                                .collect::<Vec<_>>()
                                .join("\n")
                        }
                        _ => return None,
                    };
                    // `/compact` and its output are recorded as command tags.
                    (!crate::claude::user_text(&text).is_empty()).then_some(false)
                }
                _ => None,
            }
        }
        Provider::Codex => {
            if record["type"] == "compacted" {
                return Some(true);
            }
            let payload = &record["payload"];
            (record["type"] == "response_item"
                && payload["type"] == "message"
                && matches!(payload["role"].as_str(), Some("user" | "assistant")))
            .then_some(false)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const NOW: u64 = 1_800_000_000;

    fn idle_claude(minutes: u64) -> AgentSession {
        AgentSession {
            id: "c1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d".into(),
            provider: "claude".into(),
            mode: "code".into(),
            state: "idle".into(),
            process_state: "running".into(),
            updated_unix: NOW - minutes * 60,
            ..Default::default()
        }
    }

    #[test]
    fn due_only_inside_the_warm_cache_window() {
        assert!(!due(&idle_claude(29), NOW));
        assert!(due(&idle_claude(30), NOW));
        assert!(due(&idle_claude(60), NOW));
        assert!(!due(&idle_claude(61), NOW));
        // A conversation idle for days is left alone, even after a restart.
        assert!(!due(&idle_claude(3 * 24 * 60), NOW));
    }

    #[test]
    fn only_idle_writable_claude_code_sessions() {
        let base = idle_claude(45);
        let changed = |f: fn(&mut AgentSession)| {
            let mut s = base.clone();
            f(&mut s);
            due(&s, NOW)
        };
        assert!(due(&base, NOW));
        assert!(!changed(|s| s.provider = "codex".into()));
        assert!(!changed(|s| s.provider = "deepseek".into()));
        assert!(!changed(|s| s.mode = "chat".into()));
        assert!(!changed(|s| s.archived = true));
        assert!(!changed(|s| s.read_only = true));
        assert!(!changed(|s| s.state = "running".into()));
        assert!(!changed(|s| s.state = "waiting_approval".into()));
        assert!(!changed(|s| s.state = "unknown".into()));
        assert!(!changed(|s| s.process_state = "unknown".into()));
        assert!(!changed(|s| s.process_state = "ended".into()));
        assert!(!changed(|s| s.descendant_activity = "running".into()));
        assert!(!changed(|s| s.updated_unix = 0));
    }

    #[test]
    fn one_attempt_per_idle_period() {
        let mut tracker = Tracker::default();
        let session = idle_claude(40);
        assert!(tracker.untried(&session));
        tracker.tried(&session);
        assert!(!tracker.untried(&session));
        // New activity opens a new period.
        let later = AgentSession {
            updated_unix: session.updated_unix + 600,
            ..session.clone()
        };
        assert!(tracker.untried(&later));
        tracker.prune(NOW + LATEST.as_secs());
        assert!(tracker.tried.is_empty());
    }

    fn transcript(lines: &[&str]) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
        file
    }

    // The order Claude Desktop writes a manual `/compact`: the boundary and
    // summary first, then the command (stamped with its start time) and output.
    const CLAUDE_COMPACTED: &[&str] = &[
        r#"{"type":"user","message":{"role":"user","content":"Fix the build."}}"#,
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Done."}]}}"#,
        r#"{"type":"system","subtype":"compact_boundary","content":"Conversation compacted","compactMetadata":{"trigger":"manual"}}"#,
        r#"{"type":"user","isCompactSummary":true,"message":{"role":"user","content":"This session is being continued from a previous conversation."}}"#,
        r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"<local-command-caveat>Caveat.</local-command-caveat>"}}"#,
        r#"{"type":"user","message":{"role":"user","content":"<command-name>/compact</command-name>\n            <command-message>compact</command-message>\n            <command-args></command-args>"}}"#,
        r#"{"type":"user","message":{"role":"user","content":"<local-command-stdout>Compacted </local-command-stdout>"}}"#,
        r#"{"type":"attachment"}"#,
    ];

    #[test]
    fn a_claude_compaction_is_the_newest_record_until_the_conversation_continues() {
        let file = transcript(CLAUDE_COMPACTED);
        assert!(compacted(file.path(), Provider::Claude));

        let mut continued = CLAUDE_COMPACTED.to_vec();
        continued.push(r#"{"type":"user","message":{"role":"user","content":"Next step?"}}"#);
        assert!(!compacted(transcript(&continued).path(), Provider::Claude));

        let mut answered = CLAUDE_COMPACTED.to_vec();
        answered.push(r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Continuing."}]}}"#);
        assert!(!compacted(transcript(&answered).path(), Provider::Claude));

        let plain = transcript(&CLAUDE_COMPACTED[..2]);
        assert!(!compacted(plain.path(), Provider::Claude));
    }

    #[test]
    fn a_codex_compaction_record_counts_until_a_new_message() {
        let compaction = r#"{"type":"compacted","payload":{"message":""}}"#;
        let user =
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[]}}"#;
        let tokens = r#"{"type":"event_msg","payload":{"type":"token_count"}}"#;
        assert!(compacted(
            transcript(&[user, compaction, tokens]).path(),
            Provider::Codex
        ));
        assert!(!compacted(
            transcript(&[compaction, user]).path(),
            Provider::Codex
        ));
    }

    #[test]
    fn a_missing_transcript_is_not_compacted() {
        assert!(!compacted(
            Path::new("/nonexistent/transcript.jsonl"),
            Provider::Claude
        ));
    }
}
