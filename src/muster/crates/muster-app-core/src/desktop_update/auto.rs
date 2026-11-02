//! Unattended update policy. The shell only stores the owner's on/off choice;
//! this decides when to check, download and install, reusing the same signed
//! feed, verified package and daemon lease as the manual buttons.
use std::time::{Duration, Instant};

/// Delay before the first check, so startup and reconnects settle first.
pub const FIRST_CHECK: Duration = Duration::from_secs(120);
/// Between feed checks.
pub const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);
/// How often a downloaded update looks for a quiet moment to install.
pub const TICK: Duration = Duration::from_secs(300);
/// Nobody has touched this Mac's keyboard or pointer for this long.
pub const IDLE_BEFORE_INSTALL: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Default)]
pub struct Schedule {
    /// Bumped on every on/off change; ticks from an older loop are ignored.
    pub generation: u64,
    pub next_check: Option<Instant>,
    /// An unattended install failed after stopping the service. Wait for the
    /// next check instead of stopping the daemon again every tick.
    pub install_failed: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Check,
    Download,
    Install,
    Wait,
}

pub struct Observed {
    pub can_check: bool,
    pub can_download: bool,
    pub can_install: bool,
    /// The core's own guards: no send, terminal, Locked Use or cover.
    pub quiet: bool,
    pub idle: Duration,
}

pub fn step(schedule: &Schedule, now: Instant, seen: &Observed) -> Step {
    if seen.can_install && !schedule.install_failed {
        return if seen.quiet && seen.idle >= IDLE_BEFORE_INSTALL {
            Step::Install
        } else {
            Step::Wait
        };
    }
    if seen.can_download {
        return Step::Download;
    }
    if seen.can_check && schedule.next_check.is_some_and(|at| now >= at) {
        return Step::Check;
    }
    Step::Wait
}

/// Time since the last local keyboard, pointer or scroll event. Remote
/// control through Accessibility does not count as local input.
#[cfg(target_os = "macos")]
pub fn user_idle() -> Duration {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(state_id: i32, event_type: u32) -> f64;
    }
    const HID_SYSTEM_STATE: i32 = 1;
    // kCGAnyInputEventType
    const ANY_INPUT: u32 = u32::MAX;
    // SAFETY: CoreGraphics scalar query with constant arguments.
    let seconds = unsafe { CGEventSourceSecondsSinceLastEventType(HID_SYSTEM_STATE, ANY_INPUT) };
    if seconds.is_finite() && seconds >= 0.0 {
        Duration::from_secs_f64(seconds.min(86400.0))
    } else {
        Duration::ZERO
    }
}
/// No idle observation yet: never install unattended.
#[cfg(not(target_os = "macos"))]
pub fn user_idle() -> Duration {
    Duration::ZERO
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen() -> Observed {
        Observed {
            can_check: true,
            can_download: false,
            can_install: false,
            quiet: true,
            idle: Duration::ZERO,
        }
    }

    #[test]
    fn checks_only_when_due() {
        let now = Instant::now();
        let mut schedule = Schedule::default();
        assert_eq!(step(&schedule, now, &seen()), Step::Wait);
        schedule.next_check = Some(now + TICK);
        assert_eq!(step(&schedule, now, &seen()), Step::Wait);
        schedule.next_check = Some(now);
        assert_eq!(step(&schedule, now, &seen()), Step::Check);
        let busy = Observed {
            can_check: false,
            ..seen()
        };
        assert_eq!(step(&schedule, now, &busy), Step::Wait);
    }

    #[test]
    fn downloads_an_available_update_at_once() {
        let schedule = Schedule::default();
        let available = Observed {
            can_download: true,
            ..seen()
        };
        assert_eq!(step(&schedule, Instant::now(), &available), Step::Download);
    }

    #[test]
    fn installs_only_when_quiet_and_idle() {
        let schedule = Schedule::default();
        let now = Instant::now();
        let ready = Observed {
            can_install: true,
            idle: IDLE_BEFORE_INSTALL,
            ..seen()
        };
        assert_eq!(step(&schedule, now, &ready), Step::Install);
        let touched = Observed {
            idle: IDLE_BEFORE_INSTALL - Duration::from_secs(1),
            ..ready
        };
        assert_eq!(step(&schedule, now, &touched), Step::Wait);
        let busy = Observed {
            can_install: true,
            idle: IDLE_BEFORE_INSTALL,
            quiet: false,
            ..seen()
        };
        assert_eq!(step(&schedule, now, &busy), Step::Wait);
    }

    #[test]
    fn a_failed_install_waits_for_the_next_check() {
        let now = Instant::now();
        let schedule = Schedule {
            next_check: Some(now + CHECK_EVERY),
            install_failed: true,
            ..Default::default()
        };
        let ready = Observed {
            can_install: true,
            idle: IDLE_BEFORE_INSTALL,
            ..seen()
        };
        assert_eq!(step(&schedule, now, &ready), Step::Wait);
        let later = Schedule {
            next_check: Some(now),
            ..schedule
        };
        assert_eq!(step(&later, now, &ready), Step::Check);
    }
}
