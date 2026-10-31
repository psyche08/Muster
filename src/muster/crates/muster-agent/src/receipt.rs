//! Confirm sends from newly appended provider records of matching text (user
//! records, or a Claude prompt-queue acceptance/delivery), never from AX clicks.
//! Codex equal-label URL wrappers are formatting; other edits are not equivalent.
//! Turn-end records are diagnostic only: they cannot prove a prompt was lost.
use crate::{
    model::{Provider, now_unix, parse_ts},
    transcript::Tail,
    viewer::Discovered,
};
use serde_json::Value;
use std::path::Path;

pub type Source = std::sync::Arc<dyn Fn(&str) -> Result<Receipt, crate::Error> + Send + Sync>;

pub struct Receipt(Inner);

enum Inner {
    Transcript {
        tail: Tail,
        provider: Provider,
        text: String,
        invalidated: bool,
        /// Provider time (Unix seconds) of the first turn end appended after
        /// the press without the text before it.
        turn_ended: Option<u64>,
    },
    /// CatDesk keeps no transcript file: its store is read instead.
    Catdesk(crate::catdesk::Receipt),
}

impl Receipt {
    pub fn new(path: &Path, provider: Provider, text: &str) -> std::io::Result<Self> {
        Ok(Self(Inner::Transcript {
            tail: Tail::from_end(path)?,
            provider,
            text: text.trim_end().into(),
            invalidated: false,
            turn_ended: None,
        }))
    }
    /// The receipt for a discovered session, from whatever its provider keeps.
    pub fn for_session(session: &Discovered, text: &str) -> std::io::Result<Self> {
        if session.provider == Provider::Catdesk {
            return crate::catdesk::Receipt::new(&session.path, &session.id, text)
                .map(|r| Self(Inner::Catdesk(r)));
        }
        if session.provider == Provider::Deepseek {
            // Its profile declares no send; its log is not a raw transcript.
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "DeepSeek Harness sessions take no prompts",
            ));
        }
        Self::new(&session.path, session.provider, text)
    }
    /// The transcript was rewritten after the press: its offsets, and so any
    /// "newly appended" record, can no longer be trusted.
    pub fn invalidated(&self) -> bool {
        match &self.0 {
            Inner::Transcript { invalidated, .. } => *invalidated,
            Inner::Catdesk(receipt) => receipt.invalidated(),
        }
    }
    /// When a turn first ended after the press with the text still absent
    /// (provider time, Unix seconds). `None` until then, after a rewrite, and
    /// for CatDesk, whose store has no turn records.
    pub fn turn_ended(&self) -> Option<u64> {
        match &self.0 {
            Inner::Transcript {
                invalidated: false,
                turn_ended,
                ..
            } => *turn_ended,
            _ => None,
        }
    }
    pub fn confirmed(&mut self) -> bool {
        let (tail, provider, text, invalidated, turn_ended) = match &mut self.0 {
            Inner::Transcript {
                tail,
                provider,
                text,
                invalidated,
                turn_ended,
            } => (tail, *provider, text, invalidated, turn_ended),
            Inner::Catdesk(receipt) => return receipt.confirmed(),
        };
        let Ok(read) = tail.read() else {
            return false;
        };
        *invalidated |= read.truncated;
        if *invalidated {
            return false;
        }
        for (_, record) in &read.records {
            if user_text(record, provider).is_some_and(|t| text_matches(provider, text, &t)) {
                return true;
            }
            if turn_ended.is_none() && ends_turn(record, provider) {
                *turn_ended = Some(
                    record["timestamp"]
                        .as_str()
                        .map(parse_ts)
                        .filter(|&t| t > 0)
                        .unwrap_or_else(now_unix),
                );
            }
        }
        false
    }
}

/// Only Codex's equal-label URL wrapping is equivalent to the submitted text.
/// Normalize the observed side only: literal Markdown in a prompt stays literal.
pub fn text_matches(provider: Provider, submitted: &str, observed: &str) -> bool {
    let submitted = submitted.trim_end();
    let observed = observed.trim_end();
    submitted == observed
        || (provider == Provider::Codex && unwrap_url_links(observed) == submitted)
}

pub(crate) fn unwrap_url_links(text: &str) -> String {
    use std::sync::LazyLock;
    // Be conservative with literal code; this is not Markdown interpretation.
    if text.contains('`') {
        return text.to_owned();
    }
    static LINKS: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"\[(https?://[^\s\[\]\(\)\\`]+)\]\((https?://[^\s\[\]\(\)\\`]+)\)")
            .unwrap()
    });
    LINKS
        .replace_all(text, |caps: &regex::Captures<'_>| {
            let whole = caps.get(0).unwrap();
            let before = &text[..whole.start()];
            if caps[1] == caps[2] && !before.ends_with(['!', '\\']) {
                caps[1].to_owned()
            } else {
                whole.as_str().to_owned()
            }
        })
        .into_owned()
}

/// The provider finished a turn (the grammar's `turn.ended`: a main-thread
/// Claude reply that ended it, Codex's task completion or abort). Any prompt
/// it held for the turn was handed over by then.
fn ends_turn(record: &Value, provider: Provider) -> bool {
    provider
        .jsonl()
        .is_some_and(|(records, _)| crate::jsonl::ends_turn(records, record))
}

/// The exact text of a user message (the grammar's message rule), or of a
/// prompt the app queued (the source's hook). A message answering tools is
/// not a prompt.
fn user_text(record: &Value, provider: Provider) -> Option<String> {
    use crate::profile::{Where, field};
    let (records, hooks) = provider.jsonl()?;
    if let Some(text) = hooks.queued_text(record) {
        return Some(text);
    }
    let message = crate::jsonl::message(records, record)?;
    if field(message, records.message.role) != "user" {
        return None;
    }
    let content = field(message, records.message.content);
    if let Some(text) = content.as_str() {
        return Some(text.into());
    }
    let parts = content.as_array()?;
    if let Where::Block(result) = records.tool_result.at
        && parts.iter().any(|p| p["type"] == result)
    {
        return None;
    }
    let texts = crate::jsonl::block_texts(content, records.message.text_blocks);
    // Claude Desktop 2.1.299 prepends linked-session notifications as separate
    // text blocks to a human prompt. Keep exact matching for the remaining
    // prompt, and do not treat quoted text or an automatic event as a receipt.
    let human_desktop = provider == Provider::Claude
        && record["origin"]["kind"] == "human"
        && record["turnOrigin"] == "human"
        && record["entrypoint"] == "claude-desktop";
    let skipped = if human_desktop {
        texts
            .iter()
            .take_while(|text| {
                let text = text.trim();
                text.starts_with("<system-reminder>\n") && text.ends_with("\n</system-reminder>")
            })
            .count()
    } else {
        0
    };
    Some(texts[skipped..].join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn codex_equal_url_formatting_is_not_general_markdown_equivalence() {
        let prompt = "read https://example.com/article?a=1 then http://example.org/x ";
        let formatted = "read [https://example.com/article?a=1](https://example.com/article?a=1) then [http://example.org/x](http://example.org/x)\n";
        assert!(text_matches(Provider::Codex, prompt, formatted));
        assert!(!text_matches(Provider::Claude, prompt, formatted));
        for changed in [
            "read [https://example.com/article?a=1](https://example.com/other) then http://example.org/x",
            "read [article](https://example.com/article?a=1) then http://example.org/x",
            "read ![https://example.com/article?a=1](https://example.com/article?a=1) then http://example.org/x",
            "Read https://example.com/article?a=1 then http://example.org/x",
        ] {
            assert!(!text_matches(Provider::Codex, prompt, changed));
        }
        assert!(!text_matches(
            Provider::Codex,
            "`https://example.com`",
            "`[https://example.com](https://example.com)`"
        ));
        assert!(
            !text_matches(Provider::Codex, formatted, prompt),
            "do not strip user-authored literal Markdown"
        );
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let record = serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":formatted}]}});
        writeln!(file, "{record}").unwrap();
        let mut receipt = Receipt::new(file.path(), Provider::Codex, prompt).unwrap();
        assert!(
            !receipt.confirmed(),
            "an older equal message is not this send"
        );
        let mut assistant = record.clone();
        assistant["payload"]["role"] = "assistant".into();
        writeln!(file, "{assistant}").unwrap();
        assert!(!receipt.confirmed());
        write!(file, "{record}").unwrap();
        assert!(!receipt.confirmed());
        writeln!(file).unwrap();
        assert!(receipt.confirmed());
    }

    #[test]
    fn claude_desktop_human_receipt_with_separate_notification_blocks() {
        let human = serde_json::json!({
            "type": "user", "origin": {"kind": "human"}, "turnOrigin": "human",
            "entrypoint": "claude-desktop",
            "message": {"role": "user", "content": [
                {"type": "text", "text": "<system-reminder>\nA linked session finished.\n</system-reminder>\n"},
                {"type": "text", "text": "<system-reminder>\nLinked sessions now.\n</system-reminder>\n\n"},
                {"type": "text", "text": "send me"}
            ]}
        });
        assert_eq!(
            user_text(&human, Provider::Claude).as_deref(),
            Some("send me")
        );
        let mut automatic = human.clone();
        automatic["origin"]["kind"] = "automatic".into();
        assert_ne!(
            user_text(&automatic, Provider::Claude).as_deref(),
            Some("send me")
        );
        let mut quoted = human.clone();
        quoted["message"]["content"][0]["text"] = "Quoted notification: send me".into();
        assert_ne!(
            user_text(&quoted, Provider::Claude).as_deref(),
            Some("send me")
        );
        let mut tool = human.clone();
        tool["message"]["content"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"type": "tool_result", "content": "send me"}));
        assert_eq!(user_text(&tool, Provider::Claude), None);
        let mut embedded = human.clone();
        embedded["message"]["content"] = serde_json::json!([
            {"type":"text","text":"<system-reminder>\nnotification\n</system-reminder>\n\nsend me"}
        ]);
        assert_ne!(
            user_text(&embedded, Provider::Claude).as_deref(),
            Some("send me")
        );

        let mut file = tempfile::NamedTempFile::new().unwrap();
        let mut receipt = Receipt::new(file.path(), Provider::Claude, "send me").unwrap();
        writeln!(file, "{human}").unwrap();
        assert!(receipt.confirmed());
    }
    #[test]
    fn receipt_requires_a_new_complete_user_record() {
        for provider in [Provider::Claude, Provider::Codex] {
            let mut file = tempfile::NamedTempFile::new().unwrap();
            let record = match provider {
                Provider::Claude => {
                    serde_json::json!({"type":"user","message":{"role":"user","content":"send me"}})
                }
                _ => {
                    serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"send me"}]}})
                }
            };
            writeln!(file, "{record}").unwrap();
            let mut receipt = Receipt::new(file.path(), provider, "send me").unwrap();
            assert!(
                !receipt.confirmed(),
                "old matching messages are not acknowledgements"
            );
            write!(file, "{record}").unwrap();
            assert!(
                !receipt.confirmed(),
                "partial JSONL is not an acknowledgement"
            );
            writeln!(file).unwrap();
            assert!(receipt.confirmed());
        }
    }
    #[test]
    fn claude_prompts_submitted_during_a_running_turn_confirm_the_send() {
        // Sanitized from Mac-B session example-session, 2026-10-02 18:57 CST: the
        // press was accepted into Claude's prompt queue and absorbed into the
        // running turn, but no `user` record was written for 46 minutes.
        let enqueue = serde_json::json!({"type":"queue-operation","operation":"enqueue","timestamp":"2026-10-02T10:57:01.167Z","sessionId":"s","content":"send me"});
        let absorbed = serde_json::json!({"type":"attachment","isSidechain":false,"attachment":{"type":"queued_command","commandMode":"prompt","origin":{"kind":"human"},"prompt":"send me"}});
        let blocks = serde_json::json!({"type":"attachment","attachment":{"type":"queued_command","commandMode":"prompt","prompt":[{"type":"text","text":"send me"}]}});
        for evidence in [&enqueue, &absorbed, &blocks] {
            let mut file = tempfile::NamedTempFile::new().unwrap();
            // An identical earlier submission is never an acknowledgement.
            writeln!(file, "{evidence}").unwrap();
            let mut receipt = Receipt::new(file.path(), Provider::Claude, "send me").unwrap();
            assert!(!receipt.confirmed());
            writeln!(file, "{evidence}").unwrap();
            assert!(receipt.confirmed(), "{evidence}");
            // Codex transcripts never carry Claude queue records.
            assert!(user_text(evidence, Provider::Codex).is_none());
        }
        for other in [
            serde_json::json!({"type":"queue-operation","operation":"remove","reason":"absorbed_mid_turn","content":"send me"}),
            serde_json::json!({"type":"queue-operation","operation":"dequeue"}),
            serde_json::json!({"type":"attachment","attachment":{"type":"queued_command","commandMode":"task-notification","prompt":"send me"}}),
            serde_json::json!({"type":"attachment","attachment":{"type":"queued_command","commandMode":"prompt","origin":{"kind":"peer"},"prompt":"send me"}}),
            serde_json::json!({"type":"attachment","isSidechain":true,"attachment":{"type":"queued_command","commandMode":"prompt","prompt":"send me"}}),
            serde_json::json!({"type":"attachment","attachment":{"type":"file","prompt":"send me"}}),
        ] {
            assert!(user_text(&other, Provider::Claude).is_none(), "{other}");
        }
    }
    #[test]
    fn a_turn_that_ends_without_the_text_is_recorded() {
        // Sanitized from Mac-B session example-session, 2026-10-04 03:39:14Z: the
        // press landed during a running turn, but Claude Code never queued
        // the prompt; the turn ended at 03:40:28Z and nothing followed.
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let mut receipt = Receipt::new(file.path(), Provider::Claude, "send me").unwrap();
        for record in [
            serde_json::json!({"type":"assistant","timestamp":"2026-10-04T03:39:20Z","message":{"role":"assistant","stop_reason":"tool_use","content":[]}}),
            serde_json::json!({"type":"assistant","isSidechain":true,"timestamp":"2026-10-04T03:39:30Z","message":{"role":"assistant","stop_reason":"end_turn","content":[]}}),
        ] {
            writeln!(file, "{record}").unwrap();
        }
        assert!(!receipt.confirmed());
        assert_eq!(
            receipt.turn_ended(),
            None,
            "tool calls and sidechains go on"
        );
        writeln!(file, "{}", serde_json::json!({"type":"assistant","timestamp":"2026-10-04T03:40:28Z","message":{"role":"assistant","stop_reason":"end_turn","content":[]}})).unwrap();
        writeln!(file, "{}", serde_json::json!({"type":"assistant","timestamp":"2026-10-04T03:50:00Z","message":{"role":"assistant","stop_reason":"end_turn","content":[]}})).unwrap();
        assert!(!receipt.confirmed());
        assert_eq!(
            receipt.turn_ended(),
            Some(1_791_085_228),
            "the first end counts"
        );
        // A late receipt still confirms.
        writeln!(
            file,
            "{}",
            serde_json::json!({"type":"user","message":{"role":"user","content":"send me"}})
        )
        .unwrap();
        assert!(receipt.confirmed());

        let mut file = tempfile::NamedTempFile::new().unwrap();
        let mut receipt = Receipt::new(file.path(), Provider::Codex, "send me").unwrap();
        writeln!(file, "{}", serde_json::json!({"type":"event_msg","timestamp":"2026-10-04T03:40:28Z","payload":{"type":"task_complete"}})).unwrap();
        assert!(!receipt.confirmed());
        assert_eq!(receipt.turn_ended(), Some(1_791_085_228));
        // A rewritten transcript proves nothing, not even the turn end.
        file.as_file().set_len(0).unwrap();
        assert!(!receipt.confirmed());
        assert_eq!(receipt.turn_ended(), None);
    }

    #[test]
    fn tool_results_and_sidechains_cannot_confirm_a_send() {
        for record in [
            serde_json::json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"send me"}]}}),
            serde_json::json!({"type":"user","isSidechain":true,"message":{"role":"user","content":"send me"}}),
            serde_json::json!({"type":"assistant","message":{"role":"assistant","content":"send me"}}),
        ] {
            assert!(user_text(&record, Provider::Claude).is_none());
        }
    }
}
