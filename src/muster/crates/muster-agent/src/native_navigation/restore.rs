//! Records only operations that actually dispatched native navigation.
use crate::{Error, controller::ProcessIdentity};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Default)]
pub struct Navigation(Arc<Record>);
struct Record {
    window: std::sync::OnceLock<Box<dyn std::any::Any + Send + Sync>>,
    dispatched: AtomicBool,
    may_activate: AtomicBool,
    foreground: Mutex<Option<ProcessIdentity>>,
    created: Mutex<Option<String>>,
    workers: tokio::sync::watch::Sender<usize>,
}
impl Default for Record {
    fn default() -> Self {
        Self {
            window: Default::default(),
            dispatched: Default::default(),
            may_activate: Default::default(),
            foreground: Default::default(),
            created: Default::default(),
            workers: tokio::sync::watch::channel(0).0,
        }
    }
}
pub(crate) struct Worker(Navigation);
impl Drop for Worker {
    fn drop(&mut self) {
        self.0.0.workers.send_modify(|count| *count -= 1);
    }
}
tokio::task_local! { pub(crate) static NAVIGATION: Navigation; }

impl Navigation {
    /// Nested preparation and execution must inherit the same window, even in
    /// callers that do not request restoration (e.g. model options).
    pub async fn with_window_scope<T>(operation: impl Future<Output = T>) -> T {
        if NAVIGATION.try_with(|_| ()).is_ok() {
            operation.await
        } else {
            Self::default().scope(operation).await
        }
    }
    pub fn bind_window<W: 'static + Send + Sync + Clone + PartialEq>(
        current: W,
    ) -> Result<W, Error> {
        if let Ok(scope) = NAVIGATION.try_with(Clone::clone) {
            let bound = scope.0.window.get_or_init(|| Box::new(current.clone()));
            if bound.downcast_ref::<W>() != Some(&current) {
                return Err(Error::Conflict(
                    "The selected desktop window changed; operation stopped".into(),
                ));
            }
        }
        Ok(current)
    }

    pub(crate) fn worker(&self) -> Worker {
        self.0.workers.send_modify(|count| *count += 1);
        Worker(self.clone())
    }
    pub fn busy(&self) -> bool {
        *self.0.workers.borrow() > 0
    }
    pub async fn drain(&self) {
        let mut rx = self.0.workers.subscribe();
        let _ = rx.wait_for(|n| *n == 0).await;
    }
    pub async fn scope<T>(&self, f: impl Future<Output = T>) -> T {
        NAVIGATION.scope(self.clone(), f).await
    }
    pub fn dispatched(&self) -> bool {
        self.0.dispatched.load(Ordering::Acquire)
    }
    pub fn may_activate(&self) -> bool {
        self.0.may_activate.load(Ordering::Acquire)
    }
    pub fn foreground(&self) -> Option<ProcessIdentity> {
        *self.0.foreground.lock().unwrap()
    }
    pub fn created(&self) -> Option<String> {
        self.0.created.lock().unwrap().clone()
    }
}
pub fn observed_foreground(process: Option<ProcessIdentity>) {
    let _ = NAVIGATION.try_with(|n| *n.0.foreground.lock().unwrap() = process);
}

pub fn dispatch(may_activate: bool) {
    let _ = NAVIGATION.try_with(|n| {
        n.0.dispatched.store(true, Ordering::Release);
        if may_activate {
            n.0.may_activate.store(true, Ordering::Release);
        }
    });
}
pub fn created(session: &str) {
    let _ = NAVIGATION.try_with(|n| *n.0.created.lock().unwrap() = Some(session.to_owned()));
}

#[derive(Clone, PartialEq, Eq)]
pub enum Page {
    Code(String),
    Chat(String),
    Creation(String),
}

/// All three observations are required again immediately before a restore action.
pub fn ownership(
    generation: (u64, u64),
    front: (ProcessIdentity, Option<ProcessIdentity>),
    destination: (&Page, Option<&Page>),
) -> Result<(), Error> {
    if generation.0 != generation.1 {
        return Err(Error::Conflict("local HID input changed".into()));
    }
    if Some(front.0) != front.1 {
        return Err(Error::Conflict("foreground application changed".into()));
    }
    if Some(destination.0) != destination.1 {
        return Err(Error::Conflict(
            "selected session changed or is unobservable".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn one_window_is_pinned_across_workers_but_not_across_operations() {
        use crate::{control_guard, native_scan::preferred_window};
        // Both sidebars contain the same target; window 1 has a duplicate too.
        let windows = [0usize, 1];
        let rows = [vec!["target"], vec!["target", "target", "other"]];
        for preferred in windows {
            Navigation::with_window_scope(async {
                let selected =
                    *preferred_window(&windows, Some(&preferred), None, |_| true).unwrap();
                let root = Navigation::bind_window(selected).unwrap();
                let candidates = rows[root].iter().filter(|r| **r == "target").count();
                assert_eq!(candidates, if preferred == 0 { 1 } else { 2 });
                if preferred == 0 {
                    assert!(!rows[root].contains(&"other"));
                }
                control_guard::spawn_blocking(move || {
                    assert!(Navigation::bind_window(1 - root).is_err());
                    assert_eq!(Navigation::bind_window(root).unwrap(), root);
                })
                .await
                .unwrap();
            })
            .await;
        }
    }

    struct FakeHost {
        hid: u64,
        front: ProcessIdentity,
        shown: Page,
        session_returns: usize,
        focus_returns: usize,
    }
    impl FakeHost {
        fn borrowed() -> Self {
            Self {
                hid: 5,
                front: ProcessIdentity { pid: 2, start: 1 },
                shown: Page::Code("B".into()),
                session_returns: 0,
                focus_returns: 0,
            }
        }
        fn finish(&mut self, original: Option<Page>) {
            let target = Self::borrowed();
            if ownership(
                (target.hid, self.hid),
                (target.front, Some(self.front)),
                (&target.shown, Some(&self.shown)),
            )
            .is_err()
            {
                return;
            }
            if let Some(page) = original {
                self.shown = page;
                self.session_returns += 1;
            }
            self.focus_returns += 1;
        }
    }
    #[tokio::test]
    async fn dispatch_marker_survives_failure_timeout_and_worker_cancellation() {
        for mode in 0..3 {
            let log = Navigation::default();
            let run = log.scope(async {
                crate::control_guard::spawn_blocking(|| dispatch(true))
                    .await
                    .unwrap();
                match mode {
                    0 => Ok(()),
                    1 => Err("failed"),
                    _ => std::future::pending().await,
                }
            });
            let _ = tokio::time::timeout(std::time::Duration::from_millis(200), run).await;
            assert!(log.dispatched() && log.may_activate());
            let mut host = FakeHost::borrowed();
            host.finish(Some(Page::Code("A".into())));
            assert_eq!((host.session_returns, host.focus_returns), (1, 1));
        }
    }
    #[tokio::test]
    async fn cancellation_waits_for_a_dispatched_os_worker() {
        let journal = Navigation::default();
        let (release, wait) = std::sync::mpsc::channel();
        journal
            .scope(async {
                drop(crate::control_guard::spawn_blocking(move || {
                    wait.recv_timeout(std::time::Duration::from_secs(2))
                        .unwrap();
                    dispatch(true);
                }));
            })
            .await;
        assert!(journal.busy());
        assert!(!journal.dispatched());
        release.send(()).unwrap();
        journal.drain().await;
        assert!(journal.dispatched());
    }
    #[test]
    fn user_takeover_cancels_all_cleanup_and_unknown_original_only_restores_focus() {
        let mut host = FakeHost::borrowed();
        host.hid += 1;
        host.finish(None);
        assert_eq!(host.focus_returns, 0);
        let mut host = FakeHost::borrowed();
        host.shown = Page::Chat("C".into());
        host.finish(None);
        assert_eq!(host.focus_returns, 0);
        let mut host = FakeHost::borrowed();
        host.front.start += 1;
        host.finish(None);
        assert_eq!(host.focus_returns, 0);
        let mut host = FakeHost::borrowed();
        host.finish(None);
        assert_eq!((host.session_returns, host.focus_returns), (0, 1));
    }
}

/// The timer may consume only the exact Arc issued at this completion.
pub fn take_ticket<K: std::hash::Hash + Eq, T>(
    slots: &mut std::collections::HashMap<K, Arc<T>>,
    key: &K,
    ticket: &Arc<T>,
) -> Option<Arc<T>> {
    if slots.get(key).is_some_and(|s| Arc::ptr_eq(s, ticket)) {
        slots.remove(key)
    } else {
        None
    }
}
#[cfg(test)]
mod debounce_tests {
    use super::*;
    #[test]
    fn two_consecutive_operations_only_finish_the_last_ticket() {
        let first = Arc::new("A");
        let second = Arc::new("A");
        let mut slots = std::collections::HashMap::from([("provider", second.clone())]);
        assert!(take_ticket(&mut slots, &"provider", &first).is_none());
        assert!(take_ticket(&mut slots, &"provider", &second).is_some());
        assert!(take_ticket(&mut slots, &"provider", &second).is_none());
    }
}
