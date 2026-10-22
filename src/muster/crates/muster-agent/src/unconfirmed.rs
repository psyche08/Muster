//! A native write whose delivery is unconfirmed blocks every later write to
//! the same session. Only new provider evidence clears it: a newly appended
//! user record with the same text, or a newly persisted queue item. Elapsed
//! time, a new request, a lost AX surface or a locked screen never do, and
//! reading the evidence needs neither: it is a read of provider files.
//!
//! A turn ending without the prompt is not proof of loss: a provider may
//! persist its receipt much later. Only evidence or an explicit user release
//! after checking the desktop app ends the latch.
use crate::{Error, queue::Admission, receipt::Receipt};
use std::sync::Mutex;
use std::time::SystemTime;

/// The write that is still unconfirmed, for logs and the refusal message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
    /// The ledger operation, when the write had one.
    pub operation: Option<String>,
    /// What was submitted: `send` or `queued send`.
    pub kind: &'static str,
    pub at: SystemTime,
}

impl Origin {
    /// The current ledgered operation, if any, submitting `kind` now.
    pub fn now(kind: &'static str) -> Self {
        Self {
            operation: crate::control_guard::current_operation(),
            kind,
            at: SystemTime::now(),
        }
    }
    /// What a release names: the ledger operation, else the press time.
    pub fn token(&self) -> String {
        self.operation.clone().unwrap_or_else(|| {
            let at = self
                .at
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default();
            format!("write-{}", at.as_nanos())
        })
    }
    fn describe(&self) -> String {
        let at = time::OffsetDateTime::from(self.at)
            .replace_nanosecond(0)
            .ok()
            .and_then(|t| {
                t.format(&time::format_description::well_known::Rfc3339)
                    .ok()
            })
            .unwrap_or_else(|| "an unknown time".into());
        match &self.operation {
            Some(op) => format!("{} {op} at {at}", self.kind),
            None => format!("{} at {at}", self.kind),
        }
    }
}

/// What confirmed an unconfirmed write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Evidence {
    /// A new provider user record with the submitted text.
    Receipt,
    /// A new provider queue item with the submitted text.
    Admission,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Reconciled {
    /// Nothing is unconfirmed.
    Clear,
    /// This read found the evidence and cleared the write.
    Confirmed(Origin, Evidence),
    /// Still no evidence.
    Unconfirmed(Origin),
}

impl Reconciled {
    /// Writes are admitted: nothing is unconfirmed, or this read confirmed it.
    pub fn cleared(&self) -> bool {
        matches!(self, Self::Clear | Self::Confirmed(..))
    }
}

struct Pending {
    origin: Origin,
    receipt: Option<Receipt>,
    admission: Option<Admission>,
    rewrite_logged: bool,
    evidence: Option<Evidence>,
    confirmation: Option<crate::control_guard::Confirmation>,
}

#[derive(Default)]
struct State {
    pending: Option<Pending>,
    /// The token of the write most recently confirmed by evidence: a release
    /// naming it arrives after the write did, and must not send again.
    confirmed: Option<String>,
}

/// One per controller. The lock is held while evidence is read: the
/// transcript tail is stateful, and a concurrent reader must never observe
/// the write as cleared before the evidence was actually seen.
#[derive(Default)]
pub struct WriteGuard {
    state: Mutex<State>,
}

impl WriteGuard {
    /// Marks a write unconfirmed immediately before its submit press. The
    /// evidence sources must already be positioned before that press.
    pub fn arm(
        &self,
        origin: Origin,
        receipt: Option<Receipt>,
        admission: Option<Admission>,
    ) -> Result<(), Error> {
        let mut state = self.state.lock().unwrap();
        if let Some(current) = state.pending.as_ref() {
            return Err(refused(&current.origin));
        }
        state.pending = Some(Pending {
            origin,
            receipt,
            admission,
            rewrite_logged: false,
            evidence: None,
            confirmation: crate::control_guard::current_delivery().and_then(|d| d.confirmation()),
        });
        Ok(())
    }

    pub fn is_unconfirmed(&self) -> bool {
        self.state.lock().unwrap().pending.is_some()
    }

    /// Reads the provider evidence once. Never clears without evidence.
    pub fn reconcile(&self) -> Reconciled {
        reconcile(&mut self.state.lock().unwrap())
    }

    /// Admission for a new write: re-read the evidence first, then refuse
    /// while the earlier write is still unconfirmed. The refusal happens
    /// before any input, so it is a definite "not sent" for this request.
    pub fn admit(&self) -> Result<(), Error> {
        let mut state = self.state.lock().unwrap();
        match reconcile(&mut state) {
            Reconciled::Unconfirmed(origin) => Err(refused(&origin)),
            Reconciled::Clear | Reconciled::Confirmed(..) => Ok(()),
        }
    }

    /// The user checked the desktop app and found the write `token` names
    /// not delivered. Releases that write only, after reading its evidence
    /// once more: one that did arrive meanwhile is reported, not released.
    pub fn release(&self, token: &str) -> Result<(), Error> {
        let mut state = self.state.lock().unwrap();
        match reconcile(&mut state) {
            Reconciled::Unconfirmed(origin)
                if origin.token() == token =>
            {
                if state.pending.as_ref().is_some_and(|p| p.evidence.is_some()) {
                    return Err(Error::Conflict(
                        "The provider confirmed this write, but its ledger update is pending; do not resend".into()
                    ));
                }
                state.pending = None;
                log::warn!(
                    "unconfirmed {} released by the user as not delivered; writes to this session are admitted again",
                    origin.describe()
                );
                Ok(())
            }
            Reconciled::Unconfirmed(origin) => Err(refused(&origin)),
            Reconciled::Confirmed(..) | Reconciled::Clear
                if state.confirmed.as_deref() == Some(token) =>
            {
                Err(Error::Conflict(
                    "Not sent: the earlier send did reach the provider after all; check the conversation before sending again".into(),
                ))
            }
            // Nothing is latched (the daemon restarted since): nothing to release.
            Reconciled::Confirmed(..) | Reconciled::Clear => Ok(()),
        }
    }
}

fn reconcile(state: &mut State) -> Reconciled {
    let Some(pending) = state.pending.as_mut() else {
        return Reconciled::Clear;
    };
    let evidence = if pending.evidence.is_some() {
        pending.evidence
    } else if pending.receipt.as_mut().is_some_and(Receipt::confirmed) {
        Some(Evidence::Receipt)
    } else if pending.admission.as_ref().is_some_and(Admission::confirmed) {
        Some(Evidence::Admission)
    } else {
        None
    };
    if let Some(evidence) = evidence {
        // Transcript reads consume their tail. Retain evidence across a store
        // failure, and keep refusing writes until the original result is durable.
        pending.evidence = Some(evidence);
        if let Some(confirm) = &pending.confirmation
            && let Err(error) = confirm()
        {
            log::warn!(
                "provider confirmed {}, but ledger update failed: {error}",
                pending.origin.describe()
            );
            return Reconciled::Unconfirmed(pending.origin.clone());
        }
        let origin = state.pending.take().expect("checked").origin;
        log::info!(
            "unconfirmed {} confirmed by {evidence:?} after {} s; writes to this session are admitted again",
            origin.describe(),
            origin.at.elapsed().map(|d| d.as_secs()).unwrap_or(0)
        );
        state.confirmed = Some(origin.token());
        return Reconciled::Confirmed(origin, evidence);
    }
    if !pending.rewrite_logged && pending.receipt.as_ref().is_some_and(Receipt::invalidated) {
        pending.rewrite_logged = true;
        log::warn!(
            "unconfirmed {}: the transcript was rewritten after the press; it can no longer confirm this write",
            pending.origin.describe()
        );
    }
    Reconciled::Unconfirmed(pending.origin.clone())
}

/// The refusal of a write behind an unconfirmed one, naming it for a release.
fn refused(origin: &Origin) -> Error {
    Error::Unconfirmed {
        operation: origin.token(),
        message: format!(
            "Not sent: the earlier {} to this session has no provider receipt yet. Check the conversation in the desktop app; Muster will not resend it automatically.",
            origin.describe()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Provider;
    use std::io::Write;
    use std::sync::Arc;

    fn record(text: &str) -> String {
        serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":text}]}}).to_string()
    }
    fn origin() -> Origin {
        Origin {
            operation: Some("app-1-15".into()),
            kind: "send",
            at: SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_790_919_749),
        }
    }
    fn armed(file: &tempfile::NamedTempFile, text: &str) -> WriteGuard {
        let guard = WriteGuard::default();
        let receipt = Receipt::new(file.path(), Provider::Codex, text).unwrap();
        guard.arm(origin(), Some(receipt), None).unwrap();
        guard
    }

    #[test]
    fn late_receipt_clears_without_any_desktop_surface() {
        // The guard reads only the provider transcript: a receipt appended
        // long after the 10 s wait, while the Mac is locked or another
        // conversation is shown, still clears the original write.
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(file, "{}", record("hello")).unwrap();
        let guard = armed(&file, "hello");
        assert_eq!(guard.reconcile(), Reconciled::Unconfirmed(origin()));
        writeln!(file, "{}", record("hello")).unwrap();
        assert_eq!(
            guard.reconcile(),
            Reconciled::Confirmed(origin(), Evidence::Receipt)
        );
        assert_eq!(guard.reconcile(), Reconciled::Clear);
        assert!(guard.admit().is_ok());
    }

    #[test]
    fn claude_prompt_absorbed_into_a_running_turn_clears_the_latch() {
        // Mac-B example-session at 18:57 CST: Claude was busy, so the submitted prompt
        // was queued and then absorbed into the turn; no `user` record followed.
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let guard = WriteGuard::default();
        let receipt = Receipt::new(file.path(), Provider::Claude, "hello").unwrap();
        guard.arm(origin(), Some(receipt), None).unwrap();
        writeln!(file, r#"{{"type":"attachment","attachment":{{"type":"queued_command","commandMode":"task-notification","prompt":"hello"}}}}"#).unwrap();
        assert!(guard.admit().is_err());
        writeln!(file, r#"{{"type":"attachment","isSidechain":false,"attachment":{{"type":"queued_command","commandMode":"prompt","origin":{{"kind":"human"}},"prompt":"hello"}}}}"#).unwrap();
        assert_eq!(
            guard.reconcile(),
            Reconciled::Confirmed(origin(), Evidence::Receipt)
        );
    }

    #[test]
    fn unmatched_or_rewritten_transcripts_keep_refusing() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let guard = armed(&file, "hello");
        // Another text, a partial line and the old record never confirm.
        writeln!(file, "{}", record("something else")).unwrap();
        write!(file, "{}", record("hello")).unwrap();
        let refused = guard.admit().unwrap_err();
        assert!(matches!(&refused, Error::Unconfirmed { operation, message }
            if operation == "app-1-15"
                && message.starts_with("Not sent")
                && message.contains("send app-1-15 at 2026-10-02T05:42:29Z")));
        assert!(guard.is_unconfirmed());
        // A rewrite makes byte offsets meaningless: fail closed for good.
        file.as_file().set_len(0).unwrap();
        writeln!(file.as_file(), "{}", record("hello")).unwrap();
        assert!(matches!(guard.reconcile(), Reconciled::Unconfirmed(_)));
        assert!(guard.admit().is_err());
    }

    fn turn_end(at: &str) -> String {
        serde_json::json!({"type":"event_msg","timestamp":at,"payload":{"type":"task_complete"}})
            .to_string()
    }

    #[test]
    fn a_turn_end_never_proves_loss_and_a_late_receipt_still_confirms() {
        // Mac-C 2026-10-10: receipt arrived 1069 s after the press, after
        // the old 120 s heuristic had already reported the prompt as dropped.
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let guard = armed(&file, "hello");
        writeln!(file, "{}", turn_end("2026-10-02T05:43:00Z")).unwrap();
        for _ in 0..3 {
            assert_eq!(guard.reconcile(), Reconciled::Unconfirmed(origin()));
            assert!(matches!(guard.admit(), Err(Error::Unconfirmed { .. })));
        }
        writeln!(file, "{}", record("hello")).unwrap();
        assert_eq!(
            guard.reconcile(),
            Reconciled::Confirmed(origin(), Evidence::Receipt)
        );
        assert!(guard.admit().is_ok());
    }

    #[tokio::test]
    async fn consumed_receipt_is_retained_until_ledger_confirmation_succeeds() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let attempts = Arc::new(AtomicUsize::new(0));
        let calls = attempts.clone();
        let delivery = crate::control_guard::Delivery::default();
        delivery.set_confirmation(Arc::new(move || {
            if calls.fetch_add(1, Ordering::SeqCst) < 2 {
                Err(Error::Io("store temporarily unavailable".into()))
            } else {
                Ok(())
            }
        }));
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let guard = delivery.scope(async { armed(&file, "hello") }).await;
        writeln!(file, "{}", record("hello")).unwrap();
        assert_eq!(guard.reconcile(), Reconciled::Unconfirmed(origin()));
        assert!(guard.is_unconfirmed());
        assert!(
            matches!(guard.release("app-1-15"), Err(Error::Conflict(message)) if message.contains("ledger update is pending"))
        );
        assert!(guard.is_unconfirmed());
        assert_eq!(
            guard.reconcile(),
            Reconciled::Confirmed(origin(), Evidence::Receipt)
        );
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
        assert!(guard.reconcile().cleared());
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn the_user_releases_only_the_write_they_checked() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let guard = armed(&file, "hello");
        assert!(matches!(
            guard.release("app-1-14"),
            Err(Error::Unconfirmed { operation, .. }) if operation == "app-1-15"
        ));
        assert!(guard.is_unconfirmed());
        guard.release("app-1-15").unwrap();
        assert!(guard.admit().is_ok());
        // Nothing latched (the daemon restarted): nothing to release.
        assert!(WriteGuard::default().release("app-1-15").is_ok());
        // A write without an operation is named by its press time.
        let anonymous = Origin {
            operation: None,
            ..origin()
        };
        assert_eq!(anonymous.token(), "write-1790919749000000000");
    }

    #[test]
    fn a_release_after_the_write_arrived_does_not_send_again() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let guard = armed(&file, "hello");
        writeln!(file, "{}", record("hello")).unwrap();
        assert!(matches!(guard.release("app-1-15"), Err(Error::Conflict(m))
            if m.contains("did reach the provider")));
        // Confirmed in the background before the release came in.
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let guard = armed(&file, "hello");
        writeln!(file, "{}", record("hello")).unwrap();
        assert!(guard.reconcile().cleared());
        assert!(guard.release("app-1-15").is_err());
        assert!(guard.admit().is_ok());
    }

    #[test]
    fn a_second_write_cannot_replace_the_unconfirmed_one() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let guard = armed(&file, "hello");
        let other = Receipt::new(file.path(), Provider::Codex, "again").unwrap();
        assert!(matches!(
            guard.arm(origin(), Some(other), None),
            Err(Error::Unconfirmed { .. })
        ));
        assert!(guard.is_unconfirmed());
    }

    #[test]
    fn queue_admission_confirms_a_codex_write() {
        let items = Arc::new(Mutex::new(Vec::new()));
        let state = items.clone();
        let source: crate::queue::Source = Arc::new(move || Ok(state.lock().unwrap().clone()));
        let guard = WriteGuard::default();
        let admission = Admission::new(source, "hello").unwrap();
        guard.arm(origin(), None, Some(admission)).unwrap();
        assert!(guard.admit().is_err());
        items
            .lock()
            .unwrap()
            .push(crate::queue::QueuedMessage::test("server:1", "hello"));
        assert_eq!(
            guard.reconcile(),
            Reconciled::Confirmed(origin(), Evidence::Admission)
        );
    }

    #[test]
    fn concurrent_readers_confirm_once_and_never_before_the_evidence() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let guard = Arc::new(armed(&file, "hello"));
        let read = |guard: &Arc<WriteGuard>| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    let guard = guard.clone();
                    std::thread::spawn(move || {
                        (0..25).map(|_| guard.reconcile()).collect::<Vec<_>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().unwrap())
                .collect::<Vec<_>>()
        };
        assert!(
            read(&guard)
                .iter()
                .all(|r| matches!(r, Reconciled::Unconfirmed(_)))
        );
        writeln!(file, "{}", record("hello")).unwrap();
        let results = read(&guard);
        assert_eq!(
            results
                .iter()
                .filter(|r| matches!(r, Reconciled::Confirmed(..)))
                .count(),
            1
        );
        assert!(
            !results
                .iter()
                .any(|r| matches!(r, Reconciled::Unconfirmed(_)))
        );
        assert!(!guard.is_unconfirmed());
    }
}
