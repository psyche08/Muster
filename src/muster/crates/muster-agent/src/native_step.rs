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
            Err(Error::Unavailable(format!(
                "Accessibility step '{}' timed out after {} ms (budget {} ms)",
                self.name,
                self.elapsed().as_millis(),
                self.budget.as_millis()
            )))
        } else {
            Ok(())
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
