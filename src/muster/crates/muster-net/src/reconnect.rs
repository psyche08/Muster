//! Shared connection deadlines and bounded retry policy.
use std::time::{Duration, Instant};

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, Default)]
pub struct Retry {
    started: Option<Instant>,
    next: Option<Instant>,
    failures: u32,
}

impl Retry {
    pub fn due(&self, now: Instant) -> bool {
        self.started.is_none() && self.next.is_none_or(|at| now >= at)
    }
    pub fn begin(&mut self, now: Instant) {
        self.started = Some(now);
        self.next = None;
    }
    pub fn expired(&self, now: Instant) -> bool {
        self.started
            .is_some_and(|at| now.saturating_duration_since(at) >= CONNECT_TIMEOUT)
    }
    pub fn fail(&mut self, now: Instant) {
        self.started = None;
        self.next = Some(now + Duration::from_secs(1 << self.failures.min(3)));
        self.failures = self.failures.saturating_add(1);
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn status(&self, now: Instant) -> &'static str {
        if self.started.is_some() && !self.expired(now) {
            "connecting"
        } else if self.failures > 0 || self.expired(now) {
            "failed"
        } else {
            "not connected"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deadline_failure_backoff_and_network_recovery() {
        let start = Instant::now();
        let mut retry = Retry::default();
        retry.begin(start);
        assert_eq!(
            retry.status(start + CONNECT_TIMEOUT - Duration::from_millis(1)),
            "connecting"
        );
        assert_eq!(retry.status(start + CONNECT_TIMEOUT), "failed");
        let mut now = start + CONNECT_TIMEOUT;
        for delay in [1, 2, 4, 8, 8] {
            retry.fail(now);
            assert_eq!(retry.status(now), "failed");
            assert!(!retry.due(now + Duration::from_secs(delay) - Duration::from_millis(1)));
            now += Duration::from_secs(delay);
            assert!(retry.due(now));
            retry.begin(now);
            assert_eq!(retry.status(now), "connecting");
            now += CONNECT_TIMEOUT;
        }
        retry.reset();
        assert!(retry.due(now));
    }
}
