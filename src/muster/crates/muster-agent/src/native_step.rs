//! Independent readiness budgets for native Accessibility steps. Each next
//! step starts after the preceding action completes; polling the same step
//! never extends it. OS IPC and tree walks keep their own bounded timeouts.
//! These clocks cannot renew the operation's live authorization.
use crate::{Error, control_guard};
use std::time::Duration;
use tokio::time::Instant;

pub struct Step {
    name: String,
    started: Instant,
    budget: Duration,
}

impl Step {
    pub fn new(name: impl Into<String>, budget: Duration) -> Self {
        Self::at(name.into(), budget, Instant::now())
    }

    fn at(name: String, budget: Duration, started: Instant) -> Self {
        Self {
            name,
            started,
            budget,
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    pub fn remaining(&self) -> Duration {
        self.budget.saturating_sub(self.elapsed())
    }

    pub fn deadline(&self) -> Instant {
        self.started + self.budget
    }

    pub fn expired(&self) -> bool {
        self.expired_at(Instant::now())
    }

    fn expired_at(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.started) >= self.budget
    }

    pub fn check(&self) -> Result<(), Error> {
        // A new step does not grant a new lease or excuse cancellation.
        control_guard::check()?;
        if self.expired() {
            Err(self.timeout(""))
        } else {
            Ok(())
        }
    }

    fn timeout(&self, detail: &str) -> Error {
        Error::Unavailable(format!(
            "Accessibility step '{}' timed out after {} ms (budget {} ms{detail})",
            self.name,
            self.elapsed().as_millis(),
            self.budget.as_millis()
        ))
    }

    /// Reads until `read` returns a value, `interval` apart. The budget
    /// decides whether another read may start, not whether a finished read
    /// counts: a read that started in time decides even when it ends after
    /// the deadline. A read that re-proves its scope can take a large part
    /// of the budget, and throwing its answer away turned a menu that had
    /// closed into a timeout (ADR 0194). `read` re-checks its own scope;
    /// authorization is checked before every read and after every miss.
    pub async fn until<T, F>(
        &self,
        interval: Duration,
        mut read: impl FnMut() -> F,
    ) -> Result<T, Error>
    where
        F: Future<Output = Result<Option<T>, Error>>,
    {
        let mut reads = 0u32;
        loop {
            self.check()?;
            reads += 1;
            if let Some(value) = read().await? {
                return Ok(value);
            }
            control_guard::check()?;
            if self.expired() {
                // How many reads saw the old state tells a slow app from
                // slow reads.
                return Err(self.timeout(&format!("; {reads} reads")));
            }
            tokio::time::sleep(interval).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_chains_give_each_next_step_its_full_budget() {
        let start = Instant::now();
        let budget = Duration::from_secs(3);
        // Locating, opening and reading all finish within their own budget,
        // while the full chain lasts much longer than any individual budget.
        for index in 0..40 {
            let now = start + Duration::from_millis(index * 2900);
            let step = Step::at(format!("step {index}"), budget, now);
            assert!(!step.expired_at(now + Duration::from_millis(2900)));
            assert!(step.expired_at(now + budget));
        }
    }

    #[test]
    fn polling_cannot_refresh_a_stalled_step() {
        let start = Instant::now();
        let step = Step::at("model submenu".into(), Duration::from_secs(2), start);
        for millis in [0, 500, 1500, 1999] {
            assert!(!step.expired_at(start + Duration::from_millis(millis)));
        }
        assert!(step.expired_at(start + Duration::from_secs(2)));
        assert!(step.expired_at(start + Duration::from_secs(20)));
    }

    #[tokio::test]
    async fn a_new_step_cannot_revive_revoked_authorization() {
        control_guard::scope(std::sync::Arc::new(|| false), async {
            let step = Step::new("next question", Duration::from_secs(3));
            let error = step.check().unwrap_err().to_string();
            assert!(error.contains("authorization ended"));
        })
        .await;
    }

    /// The 2026-11-12 failure: each read takes about 260 ms, the panel is
    /// gone from 900 ms on, and the read that shows it ends at 1190 ms.
    #[tokio::test(start_paused = true)]
    async fn a_read_that_ends_after_the_deadline_still_decides() {
        let start = Instant::now();
        let closed_at = start + Duration::from_millis(900);
        let step = Step::new("close model/effort menu", Duration::from_secs(1));
        let mut reads = 0;
        let result = step
            .until(Duration::from_millis(50), || {
                reads += 1;
                async move {
                    let begun = Instant::now();
                    tokio::time::sleep(Duration::from_millis(260)).await;
                    Ok((begun >= closed_at).then_some(()))
                }
            })
            .await;
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(reads, 4);
        assert!(start.elapsed() > Duration::from_secs(1));
    }

    #[tokio::test(start_paused = true)]
    async fn no_read_starts_after_the_deadline() {
        let step = Step::new("close model/effort menu", Duration::from_secs(1));
        let mut reads = 0;
        let error = step
            .until(Duration::from_millis(50), || {
                reads += 1;
                async {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    Ok(None::<()>)
                }
            })
            .await
            .unwrap_err()
            .to_string();
        // Reads start at 0, 350 and 700 ms; the third ends on the deadline.
        assert_eq!(reads, 3);
        assert!(
            error.contains("'close model/effort menu' timed out"),
            "{error}"
        );
        assert!(error.contains("(budget 1000 ms; 3 reads)"), "{error}");
    }

    #[tokio::test(start_paused = true)]
    async fn until_reads_nothing_without_authorization() {
        control_guard::scope(std::sync::Arc::new(|| false), async {
            let step = Step::new("close model/effort menu", Duration::from_secs(1));
            let mut reads = 0;
            let error = step
                .until(Duration::from_millis(50), || {
                    reads += 1;
                    async { Ok(Some(())) }
                })
                .await
                .unwrap_err()
                .to_string();
            assert_eq!(reads, 0);
            assert!(error.contains("authorization ended"));
        })
        .await;
    }

    #[test]
    fn timeout_names_the_failed_step() {
        let step = Step::at(
            "question options".into(),
            Duration::from_secs(3),
            Instant::now() - Duration::from_secs(4),
        );
        let error = step.check().unwrap_err().to_string();
        assert!(error.contains("question options"));
        assert!(error.contains("budget 3000 ms"));
    }
}
