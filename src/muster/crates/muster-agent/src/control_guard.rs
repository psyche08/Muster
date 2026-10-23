//! A scoped, live admission check. Controllers call it immediately before
//! each side effect; cancellation invalidates authority before cleanup starts.
use std::{future::Future, sync::Arc};
tokio::task_local! { static CHECK: Arc<dyn Fn() -> bool + Send + Sync>; }
tokio::task_local! { static WORK: WorkQueue; }
tokio::task_local! { static DELIVERY: Delivery; }

/// Persists provider evidence for exactly the operation captured by the caller.
/// It must be idempotent: a receipt can race the synchronous completion path.
pub type Confirmation = Arc<dyn Fn() -> Result<(), crate::Error> + Send + Sync>;

/// Where one ledgered operation stands relative to its provider submission.
/// Nothing can be submitted before a controller method starts. A controller
/// that reports its submit boundary calls [`delivery_attempt`] immediately
/// before every press that may deliver; any other controller leaves the
/// outcome unknown once entered.
#[derive(Clone, Default)]
pub struct Delivery(Arc<std::sync::Mutex<DeliveryState>>);
#[derive(Default)]
struct DeliveryState {
    entered: bool,
    tracked: bool,
    attempted: bool,
    /// The ledger operation this delivery belongs to, for diagnostics.
    operation: Option<String>,
    confirmation: Option<Confirmation>,
    receipt_budget: Option<std::time::Duration>,
}
impl Delivery {
    /// True only when this operation provably submitted nothing.
    pub fn not_started(&self) -> bool {
        let state = self.0.lock().unwrap();
        !state.attempted && (!state.entered || state.tracked)
    }
    pub async fn scope<T>(&self, future: impl Future<Output = T>) -> T {
        DELIVERY.scope(self.clone(), future).await
    }
    /// Captured by the write guard before submission and retained for late receipts.
    pub fn set_confirmation(&self, confirmation: Confirmation) {
        self.0.lock().unwrap().confirmation = Some(confirmation);
    }
    pub fn confirmation(&self) -> Option<Confirmation> {
        self.0.lock().unwrap().confirmation.clone()
    }
    /// Bound only the immediate evidence wait inside a console lease. This
    /// does not change submission authority, delivery evidence or lease duration.
    pub fn set_receipt_budget(&self, budget: std::time::Duration) {
        self.0.lock().unwrap().receipt_budget = Some(budget);
    }
    /// Names the ledger operation; never affects what was or was not sent.
    pub fn set_operation(&self, operation: &str) {
        let mut state = self.0.lock().unwrap();
        state.operation = Some(operation.to_owned());
        state.confirmation = None;
    }
}
pub fn receipt_budget() -> std::time::Duration {
    current_delivery()
        .and_then(|delivery| delivery.0.lock().unwrap().receipt_budget)
        .unwrap_or(std::time::Duration::from_secs(10))
}

/// The ledger operation of the current delivery scope, if it has one.
pub fn current_operation() -> Option<String> {
    DELIVERY
        .try_with(|delivery| delivery.0.lock().unwrap().operation.clone())
        .ok()
        .flatten()
}
/// The delivery state of the current operation, if it is tracked at all.
pub fn current_delivery() -> Option<Delivery> {
    DELIVERY.try_with(Clone::clone).ok()
}
/// A controller method that may submit begins. Every controller entered by
/// the operation must report its boundary for "not started" to stay provable.
pub fn delivery_enter(tracked: bool) {
    let _ = DELIVERY.try_with(|delivery| {
        let mut state = delivery.0.lock().unwrap();
        state.tracked = tracked && (state.tracked || !state.entered);
        state.entered = true;
    });
}
/// Call before the final live check and the press: once set, an abort can no
/// longer be reported as "not started", even if the press never happens.
pub fn delivery_attempt() {
    let _ = DELIVERY.try_with(|delivery| delivery.0.lock().unwrap().attempted = true);
}

/// Outstanding OS work, including blocking AX calls that cannot be aborted
/// with their async caller. The console owner retains this until cleanup.
#[derive(Clone, Default)]
pub struct WorkQueue(Arc<std::sync::Mutex<WorkState>>);
#[derive(Default)]
struct WorkState {
    active: usize,
    retained: Vec<Box<dyn Send>>,
}
impl WorkQueue {
    pub fn is_idle(&self) -> bool {
        self.0.lock().unwrap().active == 0
    }
    pub async fn scope<T>(&self, future: impl Future<Output = T>) -> T {
        WORK.scope(self.clone(), future).await
    }
    /// Release an operation's exclusion permits only after its last OS worker.
    /// Registration and deferred release share the same mutex, including when
    /// cancellation races worker completion. Drop resources outside that mutex.
    pub fn retain_until_idle(&self, resource: impl Send + 'static) {
        let mut state = self.0.lock().unwrap();
        if state.active == 0 {
            drop(state);
            drop(resource);
        } else {
            state.retained.push(Box::new(resource));
        }
    }
    fn enter(&self) -> WorkPermit {
        self.0.lock().unwrap().active += 1;
        WorkPermit(self.clone())
    }
}
/// The operation's OS-work lifetime, inherited by blocking workers.
pub fn current_work_queue() -> Option<WorkQueue> {
    WORK.try_with(Clone::clone).ok()
}
struct WorkPermit(WorkQueue);
impl Drop for WorkPermit {
    fn drop(&mut self) {
        let retained = {
            let mut state = self.0.0.lock().unwrap();
            state.active -= 1;
            if state.active == 0 {
                std::mem::take(&mut state.retained)
            } else {
                Vec::new()
            }
        };
        drop(retained);
    }
}

/// Inherit the live check and register before enqueueing, not when the OS
/// worker starts. Dropping/aborting its JoinHandle cannot release the permit.
pub fn spawn_blocking<F, T>(work: F) -> tokio::task::JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let queue = WORK.try_with(Clone::clone).ok();
    let permit = queue.as_ref().map(WorkQueue::enter);
    let check = CHECK.try_with(Clone::clone).ok();
    let delivery = current_delivery();
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let checked = || match check {
            Some(check) => CHECK.sync_scope(check, work),
            None => work(),
        };
        let run = || match delivery {
            Some(delivery) => DELIVERY.sync_scope(delivery, checked),
            None => checked(),
        };
        match queue {
            Some(queue) => WORK.sync_scope(queue, run),
            None => run(),
        }
    })
}

/// AX workflows mix synchronous OS IPC with async readiness waits. Poll the
/// entire workflow on a blocking worker so an IPC stall cannot starve the
/// console owner's heartbeat. Preserve admission checks and queue lifetime,
/// including when the async caller is aborted during an uninterruptible call.
pub fn spawn_blocking_async<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let runtime = tokio::runtime::Handle::current();
    spawn_blocking(move || runtime.block_on(future))
}
pub async fn scope<T>(
    check: Arc<dyn Fn() -> bool + Send + Sync>,
    future: impl Future<Output = T>,
) -> T {
    CHECK.scope(check, future).await
}
pub fn check() -> Result<(), crate::Error> {
    if CHECK.try_with(|check| check()).unwrap_or(true) {
        Ok(())
    } else {
        Err(crate::Error::Unavailable(
            "console authorization ended".into(),
        ))
    }
}

/// Prepare a write using read-only observations. If local input changes during
/// preparation, wait for a short quiet interval and revalidate from scratch.
/// Never put a side effect in `prepare`: once a write begins its original input
/// generation must remain fixed. Live authorization is checked across every wait.
pub async fn prepare_stable<T>(
    generation: impl Fn() -> u64,
    mut prepare: impl FnMut() -> Result<T, crate::Error>,
) -> Result<(T, u64), crate::Error> {
    prepare_readonly(
        "write preparation",
        std::time::Duration::from_secs(1),
        generation,
        || std::future::ready(prepare().map(Some)),
    )
    .await
}

/// Wait for a usable, stable read-only snapshot before the first side effect.
/// `None` means the provider has not exposed its UI yet; it never authorizes a
/// fallback target. Input changes discard the whole snapshot and require a
/// quiet interval. The single deadline and live authority survive every retry.
/// Never include a navigation press, draft write or submission in `observe`.
/// The returned generation must stay fixed throughout subsequent side effects.
pub async fn prepare_readonly<T, F>(
    name: &str,
    budget: std::time::Duration,
    generation: impl Fn() -> u64,
    mut observe: impl FnMut() -> F,
) -> Result<(T, u64), crate::Error>
where
    F: Future<Output = Result<Option<T>, crate::Error>>,
{
    use std::time::Duration;
    let deadline = tokio::time::Instant::now() + budget;
    let mut wait_for_quiet = false;
    loop {
        check()?;
        if tokio::time::Instant::now() >= deadline {
            return Err(if wait_for_quiet {
                crate::Error::Conflict(
                    "local input is still active; nothing was written or sent".into(),
                )
            } else {
                crate::Error::Unavailable(format!("{name}: desktop content is not ready"))
            });
        }
        let before = generation();
        if wait_for_quiet {
            tokio::time::sleep_until(
                deadline.min(tokio::time::Instant::now() + Duration::from_millis(150)),
            )
            .await;
            check()?;
            if generation() != before || tokio::time::Instant::now() >= deadline {
                continue;
            }
        }
        let prepared = observe().await?;
        check()?;
        let after = generation();
        if after != before {
            log::info!(
                "local input changed during read-only {name} ({before} -> {after}); discarding snapshot and waiting for quiet"
            );
            wait_for_quiet = true;
            continue;
        }
        if let Some(prepared) = prepared {
            return Ok((prepared, after));
        }
        wait_for_quiet = false;
        tokio::time::sleep_until(
            deadline.min(tokio::time::Instant::now() + Duration::from_millis(100)),
        )
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn aborted_caller_keeps_blocking_work_registered_and_live_check_inherited() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let queue = WorkQueue::default();
        let live = Arc::new(AtomicBool::new(true));
        let authority = live.clone();
        let (started, ready) = tokio::sync::oneshot::channel();
        let (finish, wait) = std::sync::mpsc::channel();
        let (verified, verification) = tokio::sync::oneshot::channel();
        let worker_queue = queue.clone();
        let job = tokio::spawn(async move {
            worker_queue
                .scope(scope(
                    Arc::new(move || authority.load(Ordering::SeqCst)),
                    async move {
                        spawn_blocking(move || {
                            let initially_live = check().is_ok();
                            started.send(()).unwrap();
                            wait.recv().unwrap();
                            verified.send((initially_live, check().is_err())).unwrap();
                        })
                        .await
                        .unwrap();
                    },
                ))
                .await;
        });
        ready.await.unwrap();
        assert!(!queue.is_idle());
        job.abort();
        let _ = job.await;
        live.store(false, Ordering::SeqCst);
        assert!(
            !queue.is_idle(),
            "aborting the caller cannot end an OS call"
        );
        finish.send(()).unwrap();
        assert_eq!(verification.await.unwrap(), (true, true));
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while !queue.is_idle() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    #[tokio::test(flavor = "current_thread")]
    async fn blocking_async_work_keeps_heartbeat_and_revocation_live_across_awaits() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let queue = WorkQueue::default();
        let live = Arc::new(AtomicBool::new(true));
        let authority = live.clone();
        let (started, ready) = tokio::sync::oneshot::channel();
        let (finish, wait) = std::sync::mpsc::channel();
        let (verified, verification) = tokio::sync::oneshot::channel();
        let worker_queue = queue.clone();
        let caller = tokio::spawn(async move {
            worker_queue
                .scope(scope(
                    Arc::new(move || authority.load(Ordering::SeqCst)),
                    async move {
                        spawn_blocking_async(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                            let initially_live = check().is_ok();
                            started.send(()).unwrap();
                            // Simulates AX IPC; the timer/owner must keep running on
                            // even a single-thread runtime while this call is blocked.
                            wait.recv_timeout(std::time::Duration::from_secs(2))
                                .unwrap();
                            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                            verified.send((initially_live, check().is_err())).unwrap();
                        })
                        .await
                        .unwrap();
                    },
                ))
                .await;
        });
        ready.await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(!queue.is_idle());
        live.store(false, Ordering::SeqCst);
        caller.abort();
        let _ = caller.await;
        assert!(!queue.is_idle(), "cleanup must wait for the AX worker");
        finish.send(()).unwrap();
        assert_eq!(verification.await.unwrap(), (true, true));
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while !queue.is_idle() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn authority_is_rechecked_and_does_not_escape_scope() {
        let live = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let read = live.clone();
        scope(
            Arc::new(move || read.load(std::sync::atomic::Ordering::SeqCst)),
            async {
                assert!(check().is_ok());
                live.store(false, std::sync::atomic::Ordering::SeqCst);
                assert!(check().is_err());
            },
        )
        .await;
        assert!(check().is_ok());
    }

    #[tokio::test]
    async fn input_change_discards_prepared_controls_and_prepares_again() {
        use std::cell::Cell;
        let input = Cell::new(0);
        let calls = Cell::new(0);
        let (prepared, generation) = prepare_stable(
            || input.get(),
            || {
                calls.set(calls.get() + 1);
                if calls.get() == 1 {
                    input.set(1);
                }
                Ok(calls.get())
            },
        )
        .await
        .unwrap();
        assert_eq!((prepared, generation, calls.get()), (2, 1, 2));
    }

    #[tokio::test]
    async fn ongoing_input_times_out_without_reusing_stale_preparation() {
        use std::cell::Cell;
        let input = Cell::new(0);
        let calls = Cell::new(0);
        let result = prepare_stable(
            || {
                input.set(input.get() + 1);
                input.get()
            },
            || {
                calls.set(calls.get() + 1);
                Ok(())
            },
        )
        .await;
        assert!(matches!(result, Err(crate::Error::Conflict(_))));
        assert_eq!(calls.get(), 1);
    }

    #[tokio::test]
    async fn revoked_authority_during_quiet_wait_cannot_prepare_again() {
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        let live = Arc::new(AtomicBool::new(true));
        let input = AtomicU64::new(0);
        let calls = AtomicU64::new(0);
        let cancel = live.clone();
        let revoke = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            cancel.store(false, Ordering::SeqCst);
        });
        let check_live = live.clone();
        let result = scope(
            Arc::new(move || check_live.load(Ordering::SeqCst)),
            prepare_stable(
                || input.load(Ordering::SeqCst),
                || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    input.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
            ),
        )
        .await;
        revoke.await.unwrap();
        assert!(matches!(result, Err(crate::Error::Unavailable(_))));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn preparation_errors_are_not_retried() {
        let mut calls = 0;
        let result: Result<((), u64), _> = prepare_stable(
            || 0,
            || {
                calls += 1;
                Err(crate::Error::UnknownOutcome("prior press".into()))
            },
        )
        .await;
        assert!(matches!(result, Err(crate::Error::UnknownOutcome(_))));
        assert_eq!(calls, 1);
    }

    #[tokio::test]
    async fn unready_renderer_and_changed_async_snapshot_are_reobserved_before_use() {
        use std::{cell::Cell, time::Duration};
        let input = Cell::new(10);
        let reads = Cell::new(0);
        let (snapshot, generation) = prepare_readonly(
            "navigation",
            Duration::from_secs(1),
            || input.get(),
            || async {
                let call = reads.get() + 1;
                reads.set(call);
                tokio::task::yield_now().await;
                match call {
                    1 => Ok(None), // menu-only tree, no content yet
                    2 => {
                        input.set(11); // input arrived while reading old controls
                        Ok(Some("stale session"))
                    }
                    _ => Ok(Some("fresh session")),
                }
            },
        )
        .await
        .unwrap();
        assert_eq!(
            (snapshot, generation, reads.get()),
            ("fresh session", 11, 3)
        );
    }

    #[tokio::test]
    async fn missing_renderer_has_one_bounded_deadline_and_no_fallback_target() {
        let start = tokio::time::Instant::now();
        let result = prepare_readonly::<(), _>(
            "session selection",
            std::time::Duration::from_millis(180),
            || 1,
            || async { Ok(None) },
        )
        .await;
        assert!(
            matches!(result, Err(crate::Error::Unavailable(ref message)) if message.contains("session selection") && message.contains("not ready"))
        );
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
    }

    #[tokio::test]
    async fn revoked_during_async_observation_discards_even_a_ready_snapshot() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let live = Arc::new(AtomicBool::new(true));
        let check_live = live.clone();
        let result = scope(
            Arc::new(move || check_live.load(Ordering::SeqCst)),
            prepare_readonly(
                "navigation",
                std::time::Duration::from_secs(1),
                || 1,
                || async {
                    tokio::task::yield_now().await;
                    live.store(false, Ordering::SeqCst);
                    Ok(Some("ready but unauthorized"))
                },
            ),
        )
        .await;
        assert!(matches!(result, Err(crate::Error::Unavailable(_))));
    }

    #[tokio::test]
    async fn only_an_unentered_or_unpressed_tracked_operation_is_not_started() {
        let fresh = Delivery::default();
        assert!(fresh.not_started());
        fresh.scope(async { delivery_attempt() }).await;
        assert!(
            !fresh.not_started(),
            "an attempt is never reported as not started"
        );

        let tracked = Delivery::default();
        tracked.scope(async { delivery_enter(true) }).await;
        assert!(tracked.not_started());
        tracked.scope(async { delivery_attempt() }).await;
        assert!(!tracked.not_started());

        let untracked = Delivery::default();
        untracked.scope(async { delivery_enter(false) }).await;
        assert!(!untracked.not_started());

        // A later untracked controller cannot be hidden by an earlier tracked one,
        // nor can a tracked one restore provability after an untracked one.
        for order in [[true, false], [false, true]] {
            let mixed = Delivery::default();
            mixed
                .scope(async {
                    delivery_enter(order[0]);
                    delivery_enter(order[1]);
                })
                .await;
            assert!(!mixed.not_started(), "{order:?}");
        }
        // Outside any scope the markers are no-ops.
        delivery_enter(true);
        delivery_attempt();
        assert!(current_delivery().is_none());
    }

    #[tokio::test]
    async fn delivery_attempts_on_the_blocking_worker_reach_the_operation() {
        let delivery = Delivery::default();
        delivery
            .scope(async {
                spawn_blocking_async(async {
                    delivery_enter(true);
                    tokio::task::yield_now().await;
                    delivery_attempt();
                })
                .await
                .unwrap();
            })
            .await;
        assert!(!delivery.not_started());
    }
}
