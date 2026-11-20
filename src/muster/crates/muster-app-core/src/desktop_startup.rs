//! One bounded startup path for the app's first look, its Start button and
//! the user's retry. Native service APIs are narrow callbacks; what the app
//! shows before its main page, readiness and recovery policy live here
//! (ADR 0021, ADR 0227).
//!
//! On a Mac, launchd runs musterd from the system LaunchAgent
//! `/Library/LaunchAgents/dev.linsheng.musterd.plist`. Writing that file is
//! the one step that needs the administrator's password; loading, starting
//! and stopping the job are the console user's own launchd operations.
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::time::Instant;

use crate::Error;

/// What Rust asks the platform's service adapter to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ServiceAction {
    /// Make launchd run musterd from the system LaunchAgent: load it when no
    /// job is loaded, start it when it is stopped, and replace a job that
    /// was registered any other way (the SMAppService agent of older
    /// installations). A running system job is never stopped.
    Register = 0,
    /// Start a loaded job that is not running, without killing anything.
    RecoverStopped = 1,
    /// Write the system LaunchAgent. This is the step that asks for the
    /// administrator's password.
    Install = 2,
    /// Report [`Facts`]; changes nothing.
    Inspect = 3,
    /// Replace the loaded system job with the LaunchAgent just written,
    /// stopping it if it runs. Only follows an [`ServiceAction::Install`]
    /// that changed the definition of a loaded job.
    Reload = 4,
}

/// The system LaunchAgent file, against the one this app bundle ships.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Agent {
    Missing,
    Current,
    /// Present but different, or not owned by root: launchd would refuse it
    /// or run an older definition.
    Outdated,
}

/// The launchd job labelled `dev.linsheng.musterd` in the user's session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Job {
    Absent,
    Stopped,
    Running,
}

/// Who loaded that job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// No job, or one the adapter could not attribute.
    None,
    /// The system LaunchAgent.
    System,
    /// Anything else: an SMAppService registration or `launchctl submit`.
    Other,
}

/// What the platform observed. Encoded for the C ABI as bits 0-1 the
/// agent (0 missing, 1 current, 2 outdated), bits 2-3 the job (0 absent,
/// 1 stopped, 2 running) and bits 4-5 its origin (0 none, 1 system, 2 other).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Facts {
    pub agent: Agent,
    pub job: Job,
    pub origin: Origin,
}

impl Facts {
    pub fn decode(code: u32) -> Result<Self, Error> {
        let invalid = || Error::Invalid(format!("service facts {code:#x}"));
        let agent = match code & 0b11 {
            0 => Agent::Missing,
            1 => Agent::Current,
            2 => Agent::Outdated,
            _ => return Err(invalid()),
        };
        let job = match (code >> 2) & 0b11 {
            0 => Job::Absent,
            1 => Job::Stopped,
            2 => Job::Running,
            _ => return Err(invalid()),
        };
        let origin = match (code >> 4) & 0b11 {
            0 => Origin::None,
            1 => Origin::System,
            2 => Origin::Other,
            _ => return Err(invalid()),
        };
        if code >> 6 != 0 {
            return Err(invalid());
        }
        Ok(Self { agent, job, origin })
    }

    pub fn encode(self) -> u32 {
        let agent = match self.agent {
            Agent::Missing => 0,
            Agent::Current => 1,
            Agent::Outdated => 2,
        };
        let job = match self.job {
            Job::Absent => 0,
            Job::Stopped => 1,
            Job::Running => 2,
        };
        let origin = match self.origin {
            Origin::None => 0,
            Origin::System => 1,
            Origin::Other => 2,
        };
        agent | job << 2 | origin << 4
    }
}

/// What the app shows before its main page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Startup {
    /// musterd answers, run by the system LaunchAgent: open the main page.
    Ready,
    /// musterd does not answer. The app asks before starting it, unless
    /// `automatic`: Muster itself stopped it a moment ago (a restart or an
    /// update) and starting it needs no password.
    Start { needs_admin: bool, automatic: bool },
    /// musterd answers, but launchd does not run it from the system
    /// LaunchAgent. The app offers the move and may go on without it.
    Migrate { needs_admin: bool },
}

impl Startup {
    /// For the C ABI: 0 ready, 1 start, 2 migrate; plus 16 when the step
    /// needs the administrator's password, plus 32 when it starts by itself.
    pub fn code(self) -> i32 {
        match self {
            Startup::Ready => 0,
            Startup::Start {
                needs_admin,
                automatic,
            } => 1 | i32::from(needs_admin) << 4 | i32::from(automatic) << 5,
            Startup::Migrate { needs_admin } => 2 | i32::from(needs_admin) << 4,
        }
    }
}

#[derive(Default)]
pub struct Coordinator {
    busy: AtomicBool,
}
struct Admission<'a>(&'a AtomicBool);
impl Drop for Admission<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
#[derive(Clone, Copy)]
struct Limits {
    initial: Duration,
    recovery: Duration,
    interval: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            initial: Duration::from_secs(5),
            recovery: Duration::from_secs(10),
            interval: Duration::from_millis(250),
        }
    }
}

impl Coordinator {
    /// Called from a platform worker thread, never an existing async runtime.
    pub fn check_blocking(
        &self,
        socket: &Path,
        managed: bool,
        resumed: bool,
        service: impl Fn(ServiceAction) -> Result<u32, Error>,
    ) -> Result<Startup, Error> {
        runtime()?.block_on(self.check(socket, managed, resumed, service))
    }

    /// Called from a platform worker thread, never an existing async runtime.
    pub fn prepare_blocking(
        &self,
        socket: &Path,
        managed: bool,
        service: impl Fn(ServiceAction) -> Result<u32, Error>,
    ) -> Result<(), Error> {
        runtime()?.block_on(self.prepare(socket, managed, service))
    }

    /// Decides what the app shows first. Changes nothing: the only native
    /// call is [`ServiceAction::Inspect`].
    pub async fn check(
        &self,
        socket: &Path,
        managed: bool,
        resumed: bool,
        service: impl Fn(ServiceAction) -> Result<u32, Error>,
    ) -> Result<Startup, Error> {
        self.check_with(socket, managed, resumed, service, Limits::default())
            .await
    }

    pub async fn prepare(
        &self,
        socket: &Path,
        managed: bool,
        service: impl Fn(ServiceAction) -> Result<u32, Error>,
    ) -> Result<(), Error> {
        self.prepare_with(socket, managed, service, Limits::default())
            .await
    }

    fn admit(&self) -> Result<Admission<'_>, Error> {
        self.busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::Busy("Desktop startup is already in progress".into()))?;
        Ok(Admission(&self.busy))
    }

    async fn check_with(
        &self,
        socket: &Path,
        managed: bool,
        resumed: bool,
        service: impl Fn(ServiceAction) -> Result<u32, Error>,
        limits: Limits,
    ) -> Result<Startup, Error> {
        let _admission = self.admit()?;
        if !managed {
            // A daemon someone runs by hand: there is nothing to install,
            // and starting it is theirs to do.
            return Ok(if ready(socket).await {
                Startup::Ready
            } else {
                Startup::Start {
                    needs_admin: false,
                    automatic: false,
                }
            });
        }
        let facts = inspect(&service)?;
        let mut answering = ready(socket).await;
        if !answering && facts.job == Job::Running {
            // launchd has started it (a login, a restart, an update) and it
            // is still coming up: that is not a daemon to ask about.
            answering = wait_ready(socket, limits.initial, limits.interval).await;
        }
        let needs_admin = facts.agent != Agent::Current;
        let startup = if !answering {
            Startup::Start {
                needs_admin,
                automatic: resumed && !needs_admin,
            }
        } else if needs_admin || facts.origin == Origin::Other {
            Startup::Migrate { needs_admin }
        } else {
            Startup::Ready
        };
        log::info!("desktop startup: phase=check facts={facts:?} startup={startup:?}");
        Ok(startup)
    }

    async fn prepare_with(
        &self,
        socket: &Path,
        managed: bool,
        service: impl Fn(ServiceAction) -> Result<u32, Error>,
        limits: Limits,
    ) -> Result<(), Error> {
        let _admission = self.admit()?;
        let started = Instant::now();
        log::info!("desktop startup: phase=probe managed={managed}");
        if !managed {
            if ready(socket).await || wait_ready(socket, limits.initial, limits.interval).await {
                return Ok(());
            }
            return Err(not_ready(socket, started));
        }
        let facts = inspect(&service)?;
        if facts.agent != Agent::Current {
            log::info!("desktop startup: phase=install agent={:?}", facts.agent);
            service(ServiceAction::Install)?;
        }
        if ready(socket).await {
            // A healthy system job, or a daemon nobody registered (someone
            // ran it by hand): leave both alone. The system job takes over
            // at the next login.
            let healthy = facts.origin == Origin::System && facts.agent == Agent::Current;
            if healthy || facts.origin == Origin::None {
                return Ok(());
            }
        }
        let action = if facts.origin == Origin::System
            && facts.agent != Agent::Current
            && facts.job != Job::Absent
        {
            ServiceAction::Reload
        } else {
            ServiceAction::Register
        };
        log::info!("desktop startup: phase={action:?}");
        service(action)?;
        if wait_ready(socket, limits.initial, limits.interval).await {
            return Ok(());
        }
        // A final probe prevents repairing a service that became ready at
        // the boundary. The OS adapter also refuses to stop a running job.
        if ready(socket).await {
            return Ok(());
        }
        log::info!(
            "desktop startup: phase=recover_stopped elapsed_ms={}",
            started.elapsed().as_millis()
        );
        service(ServiceAction::RecoverStopped)?;
        if wait_ready(socket, limits.recovery, limits.interval).await {
            return Ok(());
        }
        Err(not_ready(socket, started))
    }
}

fn runtime() -> Result<tokio::runtime::Runtime, Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::Unavailable(format!("startup runtime: {e}")))
}

fn inspect(service: &impl Fn(ServiceAction) -> Result<u32, Error>) -> Result<Facts, Error> {
    Facts::decode(service(ServiceAction::Inspect)?)
}

fn not_ready(socket: &Path, started: Instant) -> Error {
    log::warn!(
        "desktop startup: phase=not_ready elapsed_ms={}",
        started.elapsed().as_millis()
    );
    Error::Unavailable(format!(
        "Muster background service is not ready at {}. Try again runs service preparation again; a running service was not stopped",
        socket.display()
    ))
}

async fn ready(socket: &Path) -> bool {
    matches!(
        tokio::time::timeout(Duration::from_millis(250), muster_local::connect(socket)).await,
        Ok(Ok(_))
    )
}
async fn wait_ready(socket: &Path, budget: Duration, interval: Duration) -> bool {
    let deadline = Instant::now() + budget;
    loop {
        if tokio::time::timeout_at(deadline, ready(socket))
            .await
            .unwrap_or(false)
        {
            log::info!("desktop startup: phase=ready");
            return true;
        }
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        tokio::time::sleep(interval.min(deadline - now)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    // A Windows pipe has one client per instance. Accept each readiness
    // probe so the fixture keeps serving subsequent checks like musterd.
    fn listening(path: impl AsRef<Path>) -> tokio::task::JoinHandle<()> {
        let listener = muster_local::Listener::bind(path).unwrap();
        tokio::spawn(async move { while listener.accept().await.is_ok() {} })
    }

    fn limits() -> Limits {
        Limits {
            initial: Duration::from_millis(20),
            recovery: Duration::from_millis(60),
            interval: Duration::from_millis(2),
        }
    }
    fn facts(agent: Agent, job: Job, origin: Origin) -> u32 {
        Facts { agent, job, origin }.encode()
    }
    /// The system job, installed and running: what a migrated Mac reports.
    fn healthy() -> u32 {
        facts(Agent::Current, Job::Running, Origin::System)
    }

    #[test]
    fn facts_round_trip_and_reject_unknown_bits() {
        for agent in [Agent::Missing, Agent::Current, Agent::Outdated] {
            for job in [Job::Absent, Job::Stopped, Job::Running] {
                for origin in [Origin::None, Origin::System, Origin::Other] {
                    let f = Facts { agent, job, origin };
                    assert_eq!(Facts::decode(f.encode()).unwrap(), f);
                }
            }
        }
        assert!(Facts::decode(0b11).is_err());
        assert!(Facts::decode(0b11 << 2).is_err());
        assert!(Facts::decode(0b11 << 4).is_err());
        assert!(Facts::decode(1 << 6).is_err());
    }

    #[test]
    fn startup_codes_are_stable() {
        assert_eq!(Startup::Ready.code(), 0);
        let start = |needs_admin, automatic| Startup::Start {
            needs_admin,
            automatic,
        };
        assert_eq!(start(false, false).code(), 1);
        assert_eq!(start(true, false).code(), 17);
        assert_eq!(start(false, true).code(), 33);
        assert_eq!(Startup::Migrate { needs_admin: false }.code(), 2);
        assert_eq!(Startup::Migrate { needs_admin: true }.code(), 18);
    }

    #[tokio::test]
    async fn a_healthy_system_job_opens_the_main_page_without_asking() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s");
        let _listener = listening(&path);
        let only_inspect = |a| match a {
            ServiceAction::Inspect => Ok(healthy()),
            other => panic!("a healthy service must not be touched: {other:?}"),
        };
        let coordinator = Coordinator::default();
        assert_eq!(
            coordinator
                .check_with(&path, true, false, only_inspect, limits())
                .await
                .unwrap(),
            Startup::Ready
        );
        coordinator
            .prepare_with(&path, true, only_inspect, limits())
            .await
            .unwrap();
    }

    /// The app asks before starting musterd, and says whether that needs
    /// the administrator's password; only Muster's own restart or update
    /// starts it without asking, and never when a password is involved.
    #[tokio::test]
    async fn a_stopped_daemon_is_asked_about_unless_muster_stopped_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing");
        let check = |code: u32, resumed: bool| {
            let path = path.clone();
            async move {
                Coordinator::default()
                    .check_with(
                        &path,
                        true,
                        resumed,
                        move |a| {
                            assert_eq!(a, ServiceAction::Inspect, "check changes nothing");
                            Ok(code)
                        },
                        limits(),
                    )
                    .await
                    .unwrap()
            }
        };
        let installed = facts(Agent::Current, Job::Stopped, Origin::System);
        let absent = facts(Agent::Missing, Job::Absent, Origin::None);
        let start = |needs_admin, automatic| Startup::Start {
            needs_admin,
            automatic,
        };
        assert_eq!(check(installed, false).await, start(false, false));
        assert_eq!(check(installed, true).await, start(false, true));
        assert_eq!(check(absent, true).await, start(true, false));
        // A launchd job that is running but never answers is asked about
        // once its grace period runs out.
        let running = facts(Agent::Current, Job::Running, Origin::System);
        assert_eq!(check(running, false).await, start(false, false));
    }

    #[tokio::test]
    async fn a_daemon_still_coming_up_is_waited_for() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s");
        let late = path.clone();
        let listener = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(5)).await;
            listening(late)
        });
        let startup = Coordinator::default()
            .check_with(&path, true, false, |_| Ok(healthy()), limits())
            .await
            .unwrap();
        let _listener = listener.await.unwrap();
        assert_eq!(startup, Startup::Ready);
    }

    #[tokio::test]
    async fn a_running_daemon_from_an_older_registration_is_offered_the_move() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s");
        let _listener = listening(&path);
        let check = |code: u32| {
            let path = path.clone();
            async move {
                Coordinator::default()
                    .check_with(&path, true, false, move |_| Ok(code), limits())
                    .await
                    .unwrap()
            }
        };
        assert_eq!(
            check(facts(Agent::Missing, Job::Running, Origin::Other)).await,
            Startup::Migrate { needs_admin: true }
        );
        assert_eq!(
            check(facts(Agent::Current, Job::Running, Origin::Other)).await,
            Startup::Migrate { needs_admin: false }
        );
        assert_eq!(
            check(facts(Agent::Outdated, Job::Running, Origin::System)).await,
            Startup::Migrate { needs_admin: true }
        );
        // Someone runs it by hand: nothing to move.
        assert_eq!(
            check(facts(Agent::Current, Job::Absent, Origin::None)).await,
            Startup::Ready
        );
    }

    /// Migration: the agent is written first (the password), then the old
    /// registration is replaced by the system job.
    #[tokio::test]
    async fn migration_installs_then_replaces_the_old_registration() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s");
        let _listener = listening(&path);
        let run = |code: u32| {
            let path = path.clone();
            async move {
                let actions = Mutex::new(Vec::new());
                Coordinator::default()
                    .prepare_with(
                        &path,
                        true,
                        |a| {
                            actions.lock().unwrap().push(a);
                            Ok(code)
                        },
                        limits(),
                    )
                    .await
                    .unwrap();
                actions.into_inner().unwrap()
            }
        };
        use ServiceAction::*;
        assert_eq!(
            run(facts(Agent::Missing, Job::Running, Origin::Other)).await,
            vec![Inspect, Install, Register]
        );
        // The agent is already in place: moving needs no password.
        assert_eq!(
            run(facts(Agent::Current, Job::Running, Origin::Other)).await,
            vec![Inspect, Register]
        );
        // A changed definition of a loaded system job is reloaded; nothing
        // else ever stops a running system job.
        assert_eq!(
            run(facts(Agent::Outdated, Job::Running, Origin::System)).await,
            vec![Inspect, Install, Reload]
        );
    }

    #[tokio::test]
    async fn a_cancelled_password_stops_before_launchd_is_touched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing");
        let actions = Mutex::new(Vec::new());
        let result = Coordinator::default()
            .prepare_with(
                &path,
                true,
                |a| {
                    actions.lock().unwrap().push(a);
                    match a {
                        ServiceAction::Inspect => {
                            Ok(facts(Agent::Missing, Job::Absent, Origin::None))
                        }
                        ServiceAction::Install => Err(Error::Unavailable("cancelled".into())),
                        other => panic!("nothing after a cancelled install: {other:?}"),
                    }
                },
                limits(),
            )
            .await;
        assert!(result.is_err());
        assert_eq!(
            *actions.lock().unwrap(),
            vec![ServiceAction::Inspect, ServiceAction::Install]
        );
    }

    #[tokio::test]
    async fn missing_socket_recovers_once_and_observes_delayed_readiness() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s");
        let actions = Mutex::new(Vec::new());
        let listeners = Arc::new(Mutex::new(Vec::new()));
        let coordinator = Coordinator::default();
        coordinator
            .prepare_with(
                &path,
                true,
                |action| {
                    actions.lock().unwrap().push(action);
                    if action == ServiceAction::RecoverStopped {
                        let path = path.clone();
                        let listeners = listeners.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_millis(5)).await;
                            listeners
                                .lock()
                                .unwrap()
                                .push(listening(path));
                        });
                    }
                    Ok(facts(Agent::Current, Job::Stopped, Origin::System))
                },
                limits(),
            )
            .await
            .unwrap();
        assert_eq!(
            *actions.lock().unwrap(),
            vec![
                ServiceAction::Inspect,
                ServiceAction::Register,
                ServiceAction::RecoverStopped
            ]
        );
        coordinator
            .prepare_with(
                &path,
                true,
                |a| match a {
                    ServiceAction::Inspect => Ok(healthy()),
                    other => panic!("retry of healthy service: {other:?}"),
                },
                limits(),
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn failed_registration_custom_paths_and_exhaustion_do_not_loop() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing");
        let coordinator = Coordinator::default();
        let installed = facts(Agent::Current, Job::Absent, Origin::None);
        let actions = Mutex::new(Vec::new());
        assert!(
            coordinator
                .prepare_with(
                    &path,
                    true,
                    |a| {
                        actions.lock().unwrap().push(a);
                        match a {
                            ServiceAction::Inspect => Ok(installed),
                            _ => Err(Error::Unavailable("approval required".into())),
                        }
                    },
                    limits()
                )
                .await
                .is_err()
        );
        assert_eq!(
            *actions.lock().unwrap(),
            vec![ServiceAction::Inspect, ServiceAction::Register]
        );
        assert!(
            coordinator
                .prepare_with(
                    &path,
                    false,
                    |_| panic!("custom socket cannot manage services"),
                    limits()
                )
                .await
                .is_err()
        );
        assert_eq!(
            coordinator
                .check_with(
                    &path,
                    false,
                    true,
                    |_| panic!("custom socket cannot manage services"),
                    limits()
                )
                .await
                .unwrap(),
            Startup::Start {
                needs_admin: false,
                automatic: false
            }
        );
        actions.lock().unwrap().clear();
        assert!(
            coordinator
                .prepare_with(
                    &path,
                    true,
                    |a| {
                        actions.lock().unwrap().push(a);
                        Ok(installed)
                    },
                    limits()
                )
                .await
                .is_err()
        );
        assert_eq!(
            *actions.lock().unwrap(),
            vec![
                ServiceAction::Inspect,
                ServiceAction::Register,
                ServiceAction::RecoverStopped
            ]
        );
    }
    #[tokio::test]
    async fn concurrent_start_is_rejected_and_cancellation_releases_admission() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s");
        let coordinator = Coordinator::default();
        let mut first =
            Box::pin(coordinator.prepare_with(&path, false, |_| Ok(0), Limits::default()));
        tokio::select! { _ = &mut first => panic!("should await socket"), _ = tokio::time::sleep(Duration::from_millis(5)) => {} }
        assert!(matches!(
            coordinator
                .prepare_with(&path, true, |_| panic!("duplicate native action"), limits())
                .await,
            Err(Error::Busy(_))
        ));
        assert!(matches!(
            coordinator
                .check_with(&path, true, false, |_| panic!("duplicate check"), limits())
                .await,
            Err(Error::Busy(_))
        ));
        drop(first);
        let _listener = listening(&path);
        coordinator
            .prepare_with(&path, true, |_| Ok(healthy()), limits())
            .await
            .unwrap();
    }
}
