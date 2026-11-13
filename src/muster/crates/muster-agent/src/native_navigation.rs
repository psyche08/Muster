//! Shared AX navigation workflow. Host handles are opaque observations, never
//! authority to submit. A navigation is dispatched once and verified by identity.
pub mod restore;

use crate::{Error, control_guard, native_step::Step};
use std::time::Duration;

pub struct Sidebar<C> {
    pub candidates: Vec<C>,
    pub more: Option<C>,
    pub rows: usize,
    pub navigation: usize,
    pub complete: bool,
}

#[async_trait::async_trait]
pub trait Host: Send + Sync {
    type Surface: Send;
    type Control: Clone + Send + Sync;
    fn check(&self) -> Result<(), Error>;
    fn generation(&self) -> u64;
    fn surface(&self) -> Result<Option<Self::Surface>, Error>;
    /// The visible target needs no navigation. Check before planning any other route
    /// and again at the OS action boundary.
    fn check_navigation(&self) -> Result<(), Error>;
    fn sidebar_only(&self) -> bool;
    fn inspection_idle(&self) -> Option<Duration> {
        None
    }
    fn sidebar(&self) -> Result<Sidebar<Self::Control>, Error>;
    fn exact_route(&self) -> Option<String>;
    fn requires_exact_route(&self) -> bool {
        false
    }
    fn press(&self, control: &Self::Control, permit: &ActionPermit) -> Result<(), Error>;
    async fn navigate(&self, route: &str, permit: &ActionPermit) -> Result<(), Error>;
    fn refresh_renderer(&self, budget: Duration) -> Result<(), Error>;
}

pub fn admit_inspection(idle: Option<Duration>) -> Result<(), Error> {
    if idle.is_some_and(|d| d < Duration::from_secs(3)) {
        return Err(Error::Unavailable(muster_proto::INSPECT_LOCAL_INPUT.into()));
    }
    Ok(())
}

/// Limits belong to one navigation, not to the length of the whole user task.
/// The live console guard remains authoritative even before these clocks expire.
#[derive(Clone, Copy)]
pub struct Budget {
    pub total: Duration,
    pub ready: Duration,
    pub expansion: Duration,
    pub verify: Duration,
    pub poll: Duration,
    pub expansions: usize,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            total: Duration::from_secs(10),
            ready: Duration::from_secs(3),
            expansion: Duration::from_secs(2),
            verify: Duration::from_secs(3),
            poll: Duration::from_millis(100),
            expansions: 5,
        }
    }
}

enum Ready<S, C> {
    Visible(S),
    Exact(String),
    Sidebar(Sidebar<C>),
}

/// Recheck immediately before the OS action, after potentially slow identity IPC.
pub fn check_generation(expected: u64, current: u64) -> Result<(), Error> {
    control_guard::check()?;
    if expected != current {
        return Err(Error::Conflict(
            "local input changed during session navigation; nothing more was pressed".into(),
        ));
    }
    Ok(())
}

/// A non-serializable deadline/input binding. OS adapters must recheck it after
/// identity IPC and immediately before dispatch; it cannot renew authorization.
pub struct ActionPermit {
    generation: u64,
    deadline: tokio::time::Instant,
}
impl ActionPermit {
    pub fn check(&self, current: u64) -> Result<(), Error> {
        check_generation(self.generation, current)?;
        if self.remaining().is_zero() {
            return Err(Error::Unavailable(
                "session navigation budget expired before OS action".into(),
            ));
        }
        Ok(())
    }
    pub fn remaining(&self) -> Duration {
        self.deadline
            .saturating_duration_since(tokio::time::Instant::now())
    }
    pub fn for_restoration(generation: u64) -> Self {
        Self {
            generation,
            deadline: tokio::time::Instant::now() + Duration::from_secs(3),
        }
    }
    fn bounded(&self, budget: Duration) -> Self {
        Self {
            generation: self.generation,
            deadline: self.deadline.min(tokio::time::Instant::now() + budget),
        }
    }
}

pub async fn select<H: Host>(host: &H, budget: Budget) -> Result<(H::Surface, u64), Error> {
    let chain = Step::new("session navigation", budget.total);
    let (ready, generation) = control_guard::prepare_readonly(
        "session selection",
        budget.ready.min(chain.remaining()),
        || host.generation(),
        || async {
            chain.check()?;
            host.check()?;
            if let Some(surface) = host.surface()? {
                return Ok(Some(Ready::Visible(surface)));
            }
            admit_inspection(host.inspection_idle())?;
            host.check_navigation()?;
            // A native ID makes sidebar titles and pagination irrelevant. It
            // does not make the future destination or its composer trusted.
            if let Some(route) = host.exact_route() {
                if !host.sidebar_only() {
                    return Ok(Some(Ready::Exact(route)));
                }
                // UUID mapping is required even when the title only locates a row.
                // Launch Services may activate a provider despite its background flag.
            } else if host.requires_exact_route() {
                return Err(Error::Unavailable("local session UUID mapping is missing or ambiguous; no title lookup or navigation attempted".into()));
            }
            let sidebar = host.sidebar()?;
            Ok((sidebar.navigation > 0).then_some(Ready::Sidebar(sidebar)))
        },
    )
    .await?;
    let permit = ActionPermit {
        generation,
        deadline: chain.deadline(),
    };
    let check = || {
        chain.check()?;
        host.check()?;
        check_generation(generation, host.generation())
    };
    check()?;
    match ready {
        Ready::Visible(surface) => return Ok((surface, generation)),
        Ready::Exact(route) => {
            log::info!("AX navigation phase=dispatch route=exact_id");
            check()?;
            host.navigate(&route, &permit.bounded(Duration::from_secs(2)))
                .await?;
            check()?;
        }
        Ready::Sidebar(mut sidebar) => {
            for _ in 0..budget.expansions {
                if !sidebar.complete || !sidebar.candidates.is_empty() {
                    break;
                }
                let Some(more) = sidebar.more.take() else {
                    break;
                };
                check()?;
                host.press(&more, &permit)?;
                let before = sidebar.rows;
                let step = Step::new(
                    "expanded sidebar rows",
                    budget.expansion.min(chain.remaining()),
                );
                loop {
                    tokio::time::sleep(budget.poll.min(step.remaining())).await;
                    check()?;
                    // Expiration is final even if the OS read returns late.
                    step.check()?;
                    sidebar = host.sidebar()?;
                    check()?;
                    step.check()?;
                    if !sidebar.complete || sidebar.rows > before || !sidebar.candidates.is_empty()
                    {
                        break;
                    }
                }
                if sidebar.rows <= before {
                    break;
                }
            }
            if host.sidebar_only() && sidebar.complete && sidebar.candidates.is_empty() {
                return Err(Error::Unavailable(
                    "The desktop app did not expose this session in its sidebar; nothing was submitted".into(),
                ));
            }
            if !sidebar.complete || sidebar.candidates.len() != 1 {
                return Err(Error::Unavailable(format!(
                    "cannot uniquely select the session ({} matches; complete={})",
                    sidebar.candidates.len(),
                    sidebar.complete
                )));
            }
            check()?;
            log::info!("AX navigation phase=dispatch route=sidebar");
            host.press(&sidebar.candidates[0], &permit)?;
            check()?;
        }
    }
    let verify = Step::new(
        "selected session surface",
        budget.verify.min(chain.remaining()),
    );
    let mut refreshes = 0;
    loop {
        check()?;
        verify.check()?;
        let surface = host.surface()?;
        check()?;
        verify.check()?;
        if let Some(surface) = surface {
            log::info!(
                "AX navigation phase=verified elapsed_ms={}",
                chain.elapsed().as_millis()
            );
            return Ok((surface, generation));
        }
        if refreshes < 3 && verify.elapsed() >= Duration::from_secs(refreshes) {
            host.refresh_renderer(verify.remaining().min(chain.remaining()))?;
            check()?;
            verify.check()?;
            refreshes += 1;
        }
        tokio::time::sleep(budget.poll.min(verify.remaining()).min(chain.remaining())).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::SeqCst};

    #[derive(Default)]
    struct Fixture {
        visible: AtomicBool,
        generation: AtomicU64,
        dispatches: AtomicUsize,
        presses: AtomicUsize,
        reads: AtomicUsize,
        exact: bool,
        require_exact: bool,
        missing: bool,
        ambiguous: bool,
        background: bool,
        no_rows: bool,
        inspect: bool,
        recent_input: bool,
        ambiguous_rows: bool,
        incomplete: bool,
        input_on_dispatch: bool,
        slow_surface: bool,
        slow_verification: bool,
        slow_expansion: bool,
        action_delay: Duration,
        pages: usize,
    }
    #[async_trait::async_trait]
    impl Host for Fixture {
        type Surface = &'static str;
        type Control = bool; // true = exact target row; false = expand
        fn check(&self) -> Result<(), Error> {
            control_guard::check()
        }
        fn generation(&self) -> u64 {
            self.generation.load(SeqCst)
        }
        fn surface(&self) -> Result<Option<Self::Surface>, Error> {
            if self.slow_surface || (self.slow_verification && self.visible.load(SeqCst)) {
                std::thread::sleep(Duration::from_millis(25));
            }
            if self.ambiguous {
                return Err(Error::Conflict("ambiguous views".into()));
            }
            Ok(self.visible.load(SeqCst).then_some("verified-target"))
        }
        fn check_navigation(&self) -> Result<(), Error> {
            Ok(())
        }
        fn inspection_idle(&self) -> Option<Duration> {
            (self.inspect && self.recent_input).then_some(Duration::from_millis(10))
        }
        fn sidebar_only(&self) -> bool {
            self.background
        }
        fn sidebar(&self) -> Result<Sidebar<bool>, Error> {
            self.reads.fetch_add(1, SeqCst);
            let page = self.presses.load(SeqCst);
            if self.slow_expansion && page > 0 {
                std::thread::sleep(Duration::from_millis(25));
            }
            Ok(Sidebar {
                candidates: if self.ambiguous_rows {
                    vec![true, true]
                } else if page >= self.pages && !self.no_rows {
                    vec![true]
                } else {
                    vec![]
                },
                more: (page < self.pages).then_some(false),
                rows: page + 1,
                navigation: 1,
                complete: !self.incomplete,
            })
        }
        fn requires_exact_route(&self) -> bool {
            self.require_exact
        }
        fn exact_route(&self) -> Option<String> {
            self.exact.then(|| "exact-test-route".into())
        }
        fn press(&self, target: &bool, permit: &ActionPermit) -> Result<(), Error> {
            permit.check(self.generation())?;
            self.presses.fetch_add(1, SeqCst);
            if *target {
                self.visible.store(!self.missing, SeqCst);
            }
            Ok(())
        }
        async fn navigate(&self, _: &str, permit: &ActionPermit) -> Result<(), Error> {
            tokio::time::sleep(self.action_delay).await;
            permit.check(self.generation())?;
            self.dispatches.fetch_add(1, SeqCst);
            if self.input_on_dispatch {
                self.generation.fetch_add(1, SeqCst);
            }
            self.visible.store(!self.missing, SeqCst);
            Ok(())
        }
        fn refresh_renderer(&self, _: Duration) -> Result<(), Error> {
            Ok(())
        }
    }
    fn budget() -> Budget {
        Budget {
            total: Duration::from_secs(1),
            ready: Duration::from_millis(200),
            expansion: Duration::from_millis(100),
            verify: Duration::from_millis(20),
            poll: Duration::from_millis(1),
            expansions: 5,
        }
    }
    #[tokio::test]
    async fn missing_local_uuid_mapping_never_queries_or_presses_a_title() {
        let host = Fixture {
            require_exact: true,
            ..Default::default()
        };
        assert!(select(&host, budget()).await.is_err());
        assert_eq!(host.reads.load(SeqCst), 0);
        assert_eq!(host.presses.load(SeqCst), 0);
        assert_eq!(host.dispatches.load(SeqCst), 0);
    }

    #[tokio::test]
    async fn busy_inspection_can_read_visible_surface_but_never_navigates() {
        let host = Fixture {
            inspect: true,
            exact: true,
            recent_input: true,
            ..Default::default()
        };
        assert!(
            matches!(select(&host,budget()).await,Err(Error::Unavailable(reason)) if reason==muster_proto::INSPECT_LOCAL_INPUT)
        );
        assert_eq!(host.dispatches.load(SeqCst) + host.presses.load(SeqCst), 0);
        host.visible.store(true, SeqCst);
        assert!(select(&host, budget()).await.is_ok());
    }
    #[tokio::test]
    async fn hidden_background_write_uses_in_app_rows_with_or_without_a_url_route() {
        for exact in [true, false] {
            let h = Fixture {
                background: true,
                exact,
                require_exact: exact,
                recent_input: true, // input in the unrelated foreground app
                ..Default::default()
            };
            assert_eq!(select(&h, budget()).await.unwrap().0, "verified-target");
            assert_eq!(h.presses.load(SeqCst), 1);
            assert_eq!(h.dispatches.load(SeqCst), 0);
        }
    }
    #[tokio::test]
    async fn background_inspection_selects_and_verifies_only_through_the_sidebar() {
        let inspect = || Fixture {
            background: true,
            inspect: true,
            exact: true,
            require_exact: true,
            ..Default::default()
        };
        let h = Fixture {
            pages: 1,
            ..inspect()
        };
        assert_eq!(select(&h, budget()).await.unwrap().0, "verified-target");
        assert_eq!(h.presses.load(SeqCst), 2);
        assert_eq!(h.dispatches.load(SeqCst), 0);
        let unmapped = Fixture {
            exact: false,
            ..inspect()
        };
        assert!(select(&unmapped, budget()).await.is_err());
        assert_eq!(unmapped.reads.load(SeqCst), 0);
        for h in [
            Fixture {
                ambiguous_rows: true,
                ..inspect()
            },
            Fixture {
                incomplete: true,
                ..inspect()
            },
        ] {
            assert!(select(&h, budget()).await.is_err());
            assert_eq!(h.presses.load(SeqCst) + h.dispatches.load(SeqCst), 0);
        }
        let wrong = Fixture {
            missing: true,
            ..inspect()
        };
        assert!(select(&wrong, budget()).await.is_err());
        assert_eq!(wrong.presses.load(SeqCst), 1);
        assert_eq!(wrong.dispatches.load(SeqCst), 0);
    }
    #[tokio::test]
    async fn a_missing_window_row_never_dispatches_a_process_wide_url() {
        let h = Fixture {
            background: true,
            exact: true,
            no_rows: true,
            pages: 1,
            ..Default::default()
        };
        assert!(select(&h, budget()).await.is_err());
        assert_eq!(h.presses.load(SeqCst), 1); // one in-window expansion
        assert_eq!(h.dispatches.load(SeqCst), 0);
    }
    #[tokio::test]
    async fn visible_target_needs_no_navigation_or_sidebar() {
        let h = Fixture {
            visible: AtomicBool::new(true),
            background: true,
            exact: true,
            ..Default::default()
        };
        assert_eq!(select(&h, budget()).await.unwrap().0, "verified-target");
        assert_eq!(
            h.dispatches.load(SeqCst) + h.reads.load(SeqCst) + h.presses.load(SeqCst),
            0
        );
    }
    #[tokio::test]
    async fn exact_identity_skips_a_long_sidebar_and_requires_destination_evidence() {
        let h = Fixture {
            exact: true,
            pages: 1000,
            ..Default::default()
        };
        assert_eq!(select(&h, budget()).await.unwrap().0, "verified-target");
        assert_eq!(h.dispatches.load(SeqCst), 1);
        assert_eq!(h.reads.load(SeqCst), 0);
    }
    #[tokio::test]
    async fn missing_destination_never_replays_or_falls_back_to_sidebar() {
        let h = Fixture {
            exact: true,
            missing: true,
            ..Default::default()
        };
        assert!(select(&h, budget()).await.is_err());
        assert_eq!(h.dispatches.load(SeqCst), 1);
        assert_eq!(h.reads.load(SeqCst) + h.presses.load(SeqCst), 0);
    }
    #[tokio::test]
    async fn ambiguous_surface_is_not_permission_to_navigate() {
        let h = Fixture {
            exact: true,
            ambiguous: true,
            ..Default::default()
        };
        assert!(matches!(
            select(&h, budget()).await,
            Err(Error::Conflict(_))
        ));
        assert_eq!(h.dispatches.load(SeqCst), 0);
    }
    #[tokio::test]
    async fn input_change_after_dispatch_stops_the_chain() {
        let h = Fixture {
            exact: true,
            input_on_dispatch: true,
            ..Default::default()
        };
        assert!(matches!(
            select(&h, budget()).await,
            Err(Error::Conflict(_))
        ));
        assert_eq!(h.dispatches.load(SeqCst), 1);
    }
    #[tokio::test]
    async fn known_sidebar_progress_continues_but_incomplete_reads_never_press() {
        let h = Fixture {
            pages: 4,
            ..Default::default()
        };
        assert!(select(&h, budget()).await.is_ok());
        assert_eq!(h.presses.load(SeqCst), 5);
        let h = Fixture {
            incomplete: true,
            ..Default::default()
        };
        assert!(select(&h, budget()).await.is_err());
        assert_eq!(h.presses.load(SeqCst), 0);
    }
    #[tokio::test]
    async fn late_read_cannot_admit_a_dispatch() {
        let h = Fixture {
            exact: true,
            slow_surface: true,
            ..Default::default()
        };
        let b = Budget {
            ready: Duration::from_millis(5),
            ..budget()
        };
        assert!(
            select(&h, b)
                .await
                .unwrap_err()
                .to_string()
                .contains("preparation budget")
        );
        assert_eq!(h.dispatches.load(SeqCst), 0);
    }
    #[tokio::test]
    async fn late_destination_evidence_does_not_complete_or_replay_navigation() {
        let h = Fixture {
            exact: true,
            slow_verification: true,
            ..Default::default()
        };
        assert!(select(&h, budget()).await.is_err());
        assert_eq!(h.dispatches.load(SeqCst), 1);
        assert_eq!(h.reads.load(SeqCst) + h.presses.load(SeqCst), 0);
    }
    #[tokio::test]
    async fn slow_identity_check_cannot_dispatch_after_total_budget() {
        let h = Fixture {
            exact: true,
            action_delay: Duration::from_millis(25),
            ..Default::default()
        };
        assert!(
            select(
                &h,
                Budget {
                    total: Duration::from_millis(10),
                    ..budget()
                }
            )
            .await
            .is_err()
        );
        assert_eq!(h.dispatches.load(SeqCst), 0);
    }
    #[tokio::test]
    async fn sidebar_progress_does_not_extend_total_budget() {
        let h = Fixture {
            pages: 4,
            slow_expansion: true,
            ..Default::default()
        };
        assert!(
            select(
                &h,
                Budget {
                    total: Duration::from_millis(10),
                    ..budget()
                }
            )
            .await
            .is_err()
        );
        assert!(h.presses.load(SeqCst) <= 1);
        assert!(!h.visible.load(SeqCst));
    }
    #[tokio::test]
    async fn cancelled_authority_and_zero_total_budget_allow_no_actions() {
        let h = Fixture {
            exact: true,
            ..Default::default()
        };
        control_guard::scope(std::sync::Arc::new(|| false), async {
            assert!(select(&h, budget()).await.is_err());
        })
        .await;
        assert!(
            select(
                &h,
                Budget {
                    total: Duration::ZERO,
                    ..budget()
                }
            )
            .await
            .is_err()
        );
        assert_eq!(h.dispatches.load(SeqCst), 0);
    }
}
