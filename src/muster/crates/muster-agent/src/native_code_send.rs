//! Submit one ordinary desktop prompt only after verifying the live thread.
use crate::{
    Error, control_guard, native_sidebar, receipt,
    unconfirmed::{Origin, WriteGuard},
};
use std::{
    thread,
    time::{Duration, Instant},
};

/// Each observation and OS operation belongs to the same signed process,
/// window and physical input generation.
pub trait Window {
    type Editor;
    type Submit;
    fn check(&self) -> Result<(), Error>;
    fn navigate(&self, route: &str) -> Result<(), Error>;
    fn thread_link(&self) -> Result<String, Error>;
    fn editor(&self) -> Result<(Self::Editor, String), Error>;
    fn write(&self, editor: &Self::Editor, text: &str) -> Result<(), Error>;
    fn submit_control(&self, editor: &Self::Editor, text: &str) -> Result<Self::Submit, Error>;
    fn press(&self, control: &Self::Submit) -> Result<(), Error>;
}

fn exact(window: &impl Window, session: &str) -> Result<(), Error> {
    window.check()?;
    if native_sidebar::codex_thread_id(&window.thread_link()?).as_deref() != Some(session) {
        return Err(Error::Conflict(
            "Codex did not open the selected thread; nothing submitted".into(),
        ));
    }
    window.check()
}

pub fn send(
    window: &impl Window,
    session: &str,
    text: &str,
    source: &receipt::Source,
    pending: &WriteGuard,
) -> Result<(), Error> {
    pending.admit()?;
    let route = native_sidebar::catalog_session_route("codex", session)
        .ok_or_else(|| Error::Unavailable("Codex did not publish a desktop thread route".into()))?;
    window.check()?;
    window.navigate(&route)?;
    exact(window, session)?;
    let (editor, draft) = window.editor()?;
    if !draft.is_empty() {
        return Err(Error::Conflict(
            "Codex contains an existing draft; it was preserved".into(),
        ));
    }
    window.check()?;
    window.write(&editor, text)?;
    exact(window, session)?;
    let submit = window.submit_control(&editor, text)?;
    let receipt = source(text)?;
    window.check()?;
    control_guard::delivery_attempt();
    control_guard::check()?;
    let origin = Origin::now("send");
    let token = origin.token();
    pending.arm(origin, Some(receipt), None)?;
    let _ = window.press(&submit);
    let deadline = Instant::now() + control_guard::receipt_budget();
    loop {
        if pending.confirmed_operation(&token) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(Error::UnknownOutcome(
                "Codex has not recorded the submitted user message in this thread".into(),
            ));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model::Provider, receipt::Receipt};
    use std::{
        cell::{Cell, RefCell},
        fs::OpenOptions,
        io::Write,
        path::PathBuf,
        sync::Arc,
    };
    const ID: &str = "10000000-0000-4000-8000-000000000001";
    const OTHER: &str = "20000000-0000-4000-8000-000000000002";

    struct Fixture {
        _dir: tempfile::TempDir,
        path: PathBuf,
        draft: RefCell<String>,
        links: RefCell<std::collections::VecDeque<&'static str>>,
        submits: Cell<usize>,
        ambiguous: bool,
        acknowledge: bool,
        interrupted: bool,
    }
    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("thread.jsonl");
            std::fs::write(&path, b"").unwrap();
            Self {
                _dir: dir,
                path,
                draft: RefCell::default(),
                links: RefCell::new([ID, ID].into()),
                submits: Cell::new(0),
                ambiguous: false,
                acknowledge: true,
                interrupted: false,
            }
        }
        fn record(&self) {
            let record = serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"sample prompt"}]}});
            writeln!(
                OpenOptions::new().append(true).open(&self.path).unwrap(),
                "{record}"
            )
            .unwrap();
        }
        fn run(&self, pending: &WriteGuard) -> Result<(), Error> {
            let path = self.path.clone();
            let source: receipt::Source = Arc::new(move |text| {
                Receipt::new(&path, Provider::Codex, text)
                    .map_err(|e| Error::Unavailable(e.to_string()))
            });
            send(self, ID, "sample prompt", &source, pending)
        }
    }
    impl Window for Fixture {
        type Editor = ();
        type Submit = ();
        fn check(&self) -> Result<(), Error> {
            if self.interrupted && !self.draft.borrow().is_empty() {
                Err(Error::Conflict("local input".into()))
            } else {
                Ok(())
            }
        }
        fn navigate(&self, route: &str) -> Result<(), Error> {
            assert_eq!(route, format!("codex://threads/{ID}"));
            Ok(())
        }
        fn thread_link(&self) -> Result<String, Error> {
            Ok(format!(
                "codex://threads/{}",
                self.links.borrow_mut().pop_front().unwrap()
            ))
        }
        fn editor(&self) -> Result<((), String), Error> {
            if self.ambiguous {
                return Err(Error::Conflict("multiple composers".into()));
            }
            Ok(((), self.draft.borrow().clone()))
        }
        fn write(&self, _: &(), text: &str) -> Result<(), Error> {
            *self.draft.borrow_mut() = text.into();
            Ok(())
        }
        fn submit_control(&self, _: &(), text: &str) -> Result<(), Error> {
            assert_eq!(&*self.draft.borrow(), text);
            Ok(())
        }
        fn press(&self, _: &()) -> Result<(), Error> {
            self.submits.set(self.submits.get() + 1);
            if self.acknowledge {
                self.record();
            }
            Ok(())
        }
    }
    #[test]
    fn exact_thread_requires_new_provider_record() {
        let f = Fixture::new();
        let pending = WriteGuard::default();
        f.run(&pending).unwrap();
        assert_eq!(f.submits.get(), 1);
        assert!(!pending.is_unconfirmed());
    }
    #[test]
    fn wrong_thread_existing_draft_and_ambiguous_editor_stop_before_writing() {
        let mut f = Fixture::new();
        let pending = WriteGuard::default();
        *f.links.borrow_mut() = [OTHER].into();
        assert!(f.run(&pending).is_err());
        assert!(f.draft.borrow().is_empty());
        *f.links.borrow_mut() = [ID, ID].into();
        f.ambiguous = true;
        assert!(f.run(&pending).is_err());
        assert!(f.draft.borrow().is_empty());
        f.ambiguous = false;
        *f.draft.borrow_mut() = "unfinished".into();
        assert!(f.run(&pending).is_err());
        assert_eq!(&*f.draft.borrow(), "unfinished");
        assert_eq!(f.submits.get(), 0);
    }
    #[test]
    fn target_change_or_user_input_stops_submission() {
        for interrupted in [false, true] {
            let mut f = Fixture::new();
            f.interrupted = interrupted;
            *f.links.borrow_mut() = [ID, OTHER].into();
            assert!(f.run(&WriteGuard::default()).is_err());
            assert_eq!(f.submits.get(), 0);
        }
    }
    #[tokio::test]
    async fn click_without_receipt_blocks_replay_until_late_evidence() {
        let mut f = Fixture::new();
        f.acknowledge = false;
        f.record(); // An identical record from before the attempt cannot confirm it.
        let pending = WriteGuard::default();
        let scope = control_guard::Delivery::default();
        scope.set_receipt_budget(Duration::ZERO);
        scope
            .scope(async {
                assert!(matches!(f.run(&pending), Err(Error::UnknownOutcome(_))));
                assert!(f.run(&pending).is_err());
                assert_eq!(f.submits.get(), 1);
                f.record();
                assert!(matches!(
                    pending.reconcile(),
                    crate::unconfirmed::Reconciled::Confirmed(..)
                ));
            })
            .await;
    }
}
