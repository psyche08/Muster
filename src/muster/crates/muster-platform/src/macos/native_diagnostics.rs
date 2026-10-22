//! Bounded metadata logging; never dump a composer value or transcript tree.
use std::fmt::Arguments;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

static NEXT_TRACE: AtomicU64 = AtomicU64::new(1);

pub(super) struct Trace {
    id: u64,
    started: Instant,
    stage: &'static str,
    complete: bool,
}

impl Trace {
    pub(super) fn new(operation: &str, provider: &str, session: &str) -> Self {
        let trace = Self {
            id: NEXT_TRACE.fetch_add(1, Ordering::Relaxed),
            started: Instant::now(),
            stage: "identity",
            complete: false,
        };
        trace.record(format_args!(
            "begin operation={operation} provider={provider} session={:?}",
            label(session)
        ));
        trace
    }

    pub(super) fn record(&self, details: Arguments<'_>) {
        log::info!(
            "AX diagnostic trace={} stage={} {}",
            self.id,
            self.stage,
            details
        );
    }

    pub(super) fn stage(&mut self, stage: &'static str) {
        self.stage = stage;
        self.record(format_args!("entered"));
    }

    pub(super) fn finish(&mut self) {
        self.complete = true;
    }
}

impl Drop for Trace {
    fn drop(&mut self) {
        self.record(format_args!(
            "end completed={} elapsed_ms={}",
            self.complete,
            self.started.elapsed().as_millis()
        ));
    }
}

/// Sidebar metadata is deliberately bounded. Callers format with Debug to
/// escape newlines and control characters, so it cannot forge log records.
pub(super) fn label(value: &str) -> String {
    muster_agent::text::clip(value, 96)
}

/// Arbitrary main-pane button names may contain answers or message content.
/// Only known control labels are emitted verbatim; other names are redacted.
pub(super) fn control_label(value: &str) -> String {
    match value {
        ""
        | "Send"
        | "Send message"
        | "Send prompt"
        | "Submit"
        | "Queue"
        | "Steer"
        | "Stop"
        | "Stop generating"
        | "Stop streaming"
        | "Stop response"
        | "Interrupt"
        | "Prompt"
        | "Do anything"
        | "Ask anything"
        | "Work with Codex"
        | "与 Codex 协作"
        | "與 Codex 協作"
        | "随心输入"
        | "輸入任何任務"
        | "想做什麼都可以" => value.to_owned(),
        // CatDesk's own control names (zh-CN / en).
        "聊天输入框"
        | "Chat input"
        | "发送"
        | "请输入内容后再发送"
        | "Enter a message before sending"
        | "停止"
        | "发送中..."
        | "Sending..." => value.to_owned(),
        _ => format!("<redacted:{} chars>", value.chars().count()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_control_labels_never_emit_message_content() {
        for secret in [
            "my private reply",
            "Send my private reply",
            "secret\nforged log",
        ] {
            let logged = control_label(secret);
            assert!(logged.starts_with("<redacted:"));
            assert!(!logged.contains(secret));
        }
        assert_eq!(control_label("Send"), "Send");
        assert_eq!(control_label("Stop generating"), "Stop generating");
        assert_eq!(control_label("聊天输入框"), "聊天输入框");
        assert_eq!(control_label("Work with Codex"), "Work with Codex");
        assert!(control_label("Work with Codex on a private task").starts_with("<redacted:"));
    }

    #[test]
    fn sidebar_metadata_is_bounded_and_debug_escaped() {
        assert!(label(&"界".repeat(200)).chars().count() <= 97);
        assert!(!format!("{:?}", label("title\nforged\rlog")).contains('\n'));
    }
}
