//! Bounded Accessibility scans. A walk that stops early cannot prove what it
//! did not see, so the adapter refuses on it; this decides what the early stop
//! means, whether walking again can help, and which subtrees a session scan
//! never needs to enter. Nothing here reads or drives the OS.
use std::time::Duration;

/// Why a bounded walk stopped before covering its whole subtree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanStop {
    /// The node ceiling. The same tree reaches it again.
    NodeLimit,
    /// The time budget, which is already spent.
    TimeLimit,
    /// A node failed its read, with the AX status when there was one:
    /// destroyed while the walk ran, or the app did not answer in time.
    NodeRead(Option<i32>),
}

impl ScanStop {
    /// Content-free wording for logs and refusals.
    pub fn describe(self) -> String {
        match self {
            Self::NodeLimit => "node limit".into(),
            Self::TimeLimit => "time limit".into(),
            Self::NodeRead(Some(code)) => format!("node read failed ({code})"),
            Self::NodeRead(None) => "node read failed".into(),
        }
    }
}

/// Walks for one scan, the first included.
pub const SCAN_ATTEMPTS: usize = 3;
/// The pause before walking again, for the app to finish its rebuild.
pub const SCAN_RETRY_PAUSE: Duration = Duration::from_millis(100);

/// Whether to walk again after `attempts` walks ended with `stop`.
///
/// Electron rebuilds the composer and its controls around a write and
/// re-renders a streaming transcript. A node it destroys mid-walk fails its
/// read, and the scan was refused as incomplete within half a second of a
/// successful write; a fresh walk a moment later sees a consistent tree. A
/// partial walk is never patched by skipping the failed node: its subtree may
/// have held the composer or a second submit control. The node ceiling and the
/// time budget are not transient, and repeating them only spends the
/// operation's time. Walking is read-only, so a repeat costs time, never
/// safety.
pub fn repeat_scan(stop: ScanStop, attempts: usize) -> bool {
    attempts < SCAN_ATTEMPTS && matches!(stop, ScanStop::NodeRead(_))
}

/// Whether a session scan descends into a web area whose own key `resolves`
/// to a conversation.
///
/// A web area is resolved from its own URL and title, never from what it
/// contains, and a resolved one is that conversation's view: its subtree is
/// the transcript and composer, which the control scan walks separately.
/// Descending into every transcript made the app-wide scan grow with each
/// open conversation until it hit the node ceiling — after it had already
/// found the target — and refused the send as incomplete. A web area that
/// resolves to nothing (the app's shell, a home page) may hold conversation
/// views and is entered.
///
/// This never lets a send reach another conversation: the target is still
/// matched only by its own key, and the control scan still requires exactly
/// one composer inside it. A view nested inside another conversation's view,
/// which no supported app has, would go unseen and refuse as not found.
pub fn descend_web_area(resolves: bool) -> bool {
    !resolves
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_failed_node_read_is_walked_again_and_only_twice() {
        for code in [Some(-25202), Some(-25204), None] {
            let stop = ScanStop::NodeRead(code);
            assert!(repeat_scan(stop, 1));
            assert!(repeat_scan(stop, 2));
            assert!(!repeat_scan(stop, 3));
        }
        // The same tree reaches the ceiling again; the budget is spent.
        for stop in [ScanStop::NodeLimit, ScanStop::TimeLimit] {
            assert!(!repeat_scan(stop, 1));
        }
    }

    #[test]
    fn a_session_scan_enters_only_web_areas_that_resolve_to_nothing() {
        assert!(!descend_web_area(true));
        assert!(descend_web_area(false));
    }

    #[test]
    fn stops_are_described_without_content() {
        assert_eq!(ScanStop::NodeLimit.describe(), "node limit");
        assert_eq!(ScanStop::TimeLimit.describe(), "time limit");
        assert_eq!(
            ScanStop::NodeRead(Some(-25202)).describe(),
            "node read failed (-25202)"
        );
        assert_eq!(ScanStop::NodeRead(None).describe(), "node read failed");
    }
}
