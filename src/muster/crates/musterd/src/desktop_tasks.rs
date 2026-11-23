//! A head-of-line turn for agent UI operations on one host. Provider file
//! locks additionally coordinate with other processes. A signed lease keeps
//! a copy of its turn until the owner finishes relocking.
use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::sync::{Mutex, OwnedMutexGuard};

#[derive(Default)]
pub(crate) struct DesktopTasks {
    queue: Arc<Mutex<()>>,
    waiting: AtomicUsize,
}

struct Waiting<'a>(&'a AtomicUsize);
impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

tokio::task_local! {
    static TURN: Arc<OwnedMutexGuard<()>>;
}

pub(crate) fn current() -> Option<Arc<OwnedMutexGuard<()>>> {
    TURN.try_with(Arc::clone).ok()
}

pub(crate) struct Turn {
    started_at: std::time::Instant,
    permit: Arc<OwnedMutexGuard<()>>,
    work: Option<muster_agent::control_guard::WorkQueue>,
}

impl Drop for Turn {
    fn drop(&mut self) {
        if let Some(work) = &self.work {
            work.retain_until_idle(self.permit.clone());
        }
    }
}

impl DesktopTasks {
    pub(crate) fn has_waiters(&self) -> bool {
        self.waiting.load(Ordering::SeqCst) != 0
    }

    pub(crate) async fn enter(&self) -> Turn {
        let (permit, started_at) = match current() {
            Some(turn) if Arc::ptr_eq(OwnedMutexGuard::mutex(&turn), &self.queue) => {
                (turn, muster_agent::control_guard::execution_started())
            }
            _ => {
                self.waiting.fetch_add(1, Ordering::SeqCst);
                let _waiting = Waiting(&self.waiting);
                let permit = self.queue.clone().lock_owned().await;
                (Arc::new(permit), std::time::Instant::now())
            }
        };
        Turn {
            started_at,
            permit,
            work: muster_agent::control_guard::current_work_queue(),
        }
    }

    pub(crate) async fn run<F: Future>(&self, task: F) -> F::Output {
        self.enter().await.run(task).await
    }
}

impl Turn {
    pub(crate) async fn run<F: Future>(&self, task: F) -> F::Output {
        muster_agent::control_guard::execution(
            self.started_at,
            TURN.scope(self.permit.clone(), task),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancellation_drops_a_waiter_and_the_next_budget_starts_at_dequeue() {
        let queue = DesktopTasks::default();
        let first = queue.enter().await;
        let mut cancelled = Box::pin(queue.run(async { panic!("cancelled waiter ran") }));
        assert!(futures_util::poll!(&mut cancelled).is_pending());
        let mut next = Box::pin(queue.enter());
        assert!(futures_util::poll!(&mut next).is_pending());
        drop(cancelled);
        let released = std::time::Instant::now();
        drop(first);
        let turn = next.await;
        assert!(turn.started_at >= released);
        turn.run(queue.run(async {
            assert_eq!(
                muster_agent::control_guard::execution_started(),
                turn.started_at
            );
        }))
        .await;
    }

    #[tokio::test]
    async fn another_provider_cannot_start_until_cancelled_os_work_drains() {
        let queue = DesktopTasks::default();
        let work = muster_agent::control_guard::WorkQueue::default();
        let (finish, wait) = std::sync::mpsc::channel();
        let turn = work.scope(queue.enter()).await;
        let worker = work
            .scope(async {
                muster_agent::control_guard::spawn_blocking(move || wait.recv().unwrap())
            })
            .await;
        drop(turn);
        let mut next = Box::pin(queue.enter());
        assert!(futures_util::poll!(&mut next).is_pending());
        finish.send(()).unwrap();
        worker.await.unwrap();
        next.await;
    }

    #[tokio::test]
    async fn tasks_from_different_providers_wait_in_arrival_order() {
        let queue = DesktopTasks::default();
        let first = queue.queue.lock().await;
        let order = std::cell::RefCell::new(vec![]);
        let claude = queue.run(async { order.borrow_mut().push("claude") });
        let codex = queue.run(async { order.borrow_mut().push("codex") });
        tokio::pin!(claude, codex);
        assert!(futures_util::poll!(&mut claude).is_pending());
        assert!(futures_util::poll!(&mut codex).is_pending());
        let other_host = DesktopTasks::default();
        other_host
            .run(async { assert!(order.borrow().is_empty()) })
            .await;
        drop(first);
        tokio::join!(codex, claude);
        assert_eq!(*order.borrow(), ["claude", "codex"]);
    }
}
