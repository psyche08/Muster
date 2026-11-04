//! Runs the bundled l4forwarder addon (ADR 0107) while this machine has a
//! configuration for it. musterd only supervises the process: the rules and
//! every forwarding decision belong to the addon, and who may reach its
//! listeners stays the roster's and the local policy's say.
//!
//! The configuration is `l4forwarder.json` in the state directory. Its
//! appearance starts the addon, a change restarts it, its removal stops it;
//! a file-system watch reports each, so nothing is polled (ADR 0108).
//! The addon's standard input is a pipe from musterd and it runs with
//! `--supervised`, so it exits with musterd even when musterd is killed.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::Notify;

pub const CONFIG: &str = "l4forwarder.json";
const LOG: &str = "l4forwarder.log";
/// A larger file is not a configuration; it is treated as absent.
const MAX_CONFIG: u64 = 1 << 20;
/// A run this long resets the restart backoff.
const STEADY: Duration = Duration::from_secs(60);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
const STOP_GRACE: Duration = Duration::from_secs(2);
/// After a change event, the time a writer gets to finish before the file is
/// read: one save can be several events.
const SETTLE: Duration = Duration::from_millis(100);
/// Only where the platform offers no file watcher.
const FALLBACK_POLL: Duration = Duration::from_secs(5);

/// Where the addon ships: `Contents/Helpers` of the macOS app bundle when
/// musterd runs from `Contents/MacOS`, else beside musterd.
pub fn program(daemon: &Path) -> PathBuf {
    let name = format!("l4forwarder{}", std::env::consts::EXE_SUFFIX);
    let dir = daemon.parent().unwrap_or(Path::new("."));
    match dir.parent() {
        Some(contents) if dir.ends_with("Contents/MacOS") => contents.join("Helpers").join(name),
        _ => dir.join(name),
    }
}

pub struct Supervisor {
    pub program: PathBuf,
    pub config: PathBuf,
    /// Where the addon's output goes; `None` keeps musterd's stderr.
    pub log: Option<PathBuf>,
    /// The first restart delay, doubled per failure up to a minute.
    pub backoff: Duration,
}

impl Supervisor {
    pub fn for_daemon(state_dir: &Path) -> Option<Self> {
        let daemon = std::env::current_exe().ok()?;
        Some(Self {
            program: program(&daemon),
            config: state_dir.join(CONFIG),
            log: muster_paths::log_dir().map(|d| d.join(LOG)),
            backoff: Duration::from_secs(1),
        })
    }

    pub async fn run(self) {
        let changed = Arc::new(Notify::new());
        // Events arrive while this lives; `None` falls back to polling. Asking
        // the OS for the watcher is synchronous and can take long enough to
        // delay everything else sharing this executor, daemon startup
        // included, so it does not run on it.
        let watcher = {
            let config = self.config.clone();
            let changed = changed.clone();
            tokio::task::spawn_blocking(move || watch(&config, changed))
                .await
                .unwrap_or_default()
        };
        let mut seen: Option<Vec<u8>> = None;
        let mut child: Option<Running> = None;
        let mut failures = 0u32;
        // When the addon is due to start; `None` while it runs or is not wanted.
        let mut start_at: Option<Instant> = None;
        let mut last_error = String::new();
        let mut reread = true;
        loop {
            if std::mem::take(&mut reread) {
                let current = read_config(&self.config);
                if current != seen {
                    if child.is_some() {
                        log::info!("l4forwarder: configuration changed; stopping");
                    }
                    stop(&mut child).await;
                    seen = current;
                    failures = 0;
                    start_at = seen.is_some().then(Instant::now);
                }
            }
            if child.is_none() && start_at.is_some_and(|at| at <= Instant::now()) {
                start_at = None;
                match self.spawn() {
                    Ok(started) => {
                        log::info!("l4forwarder: started ({})", self.program.display());
                        last_error.clear();
                        child = Some(started);
                    }
                    Err(error) => {
                        let why = format!("{}: {error}", self.program.display());
                        if why != last_error {
                            log::warn!("l4forwarder: cannot start {why}");
                            last_error = why;
                        }
                        failures += 1;
                        start_at = Some(Instant::now() + self.delay(failures));
                    }
                }
            }
            tokio::select! {
                status = exited(&mut child) => {
                    let ran = child.take().map(|c| c.since.elapsed()).unwrap_or_default();
                    failures = if ran >= STEADY { 1 } else { failures + 1 };
                    log::warn!("l4forwarder: exited ({status:?}) after {}s", ran.as_secs());
                    start_at = Some(Instant::now() + self.delay(failures));
                }
                () = changed.notified() => {
                    tokio::time::sleep(SETTLE).await;
                    reread = true;
                }
                () = until(start_at) => {}
                () = tokio::time::sleep(FALLBACK_POLL), if watcher.is_none() => reread = true,
            }
        }
    }

    fn delay(&self, failures: u32) -> Duration {
        let factor = 1u32 << failures.saturating_sub(1).min(16);
        self.backoff.saturating_mul(factor).min(MAX_BACKOFF)
    }

    fn spawn(&self) -> std::io::Result<Running> {
        let stderr = match &self.log {
            Some(path) => {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                let file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)?;
                Stdio::from(file)
            }
            None => Stdio::inherit(),
        };
        let mut child = Command::new(&self.program)
            .arg("--config")
            .arg(&self.config)
            .arg("--supervised")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(stderr)
            .kill_on_drop(true)
            .spawn()?;
        Ok(Running {
            // Held apart from the child: `Child::wait` would close it.
            input: child.stdin.take(),
            child,
            since: Instant::now(),
        })
    }
}

struct Running {
    child: Child,
    /// Open for as long as the addon should run.
    input: Option<ChildStdin>,
    since: Instant,
}

/// Wakes `changed` on every event about the configuration file. It watches
/// the directory rather than the file: an atomic save replaces the file,
/// which would end a watch on the old one. `None` when the platform offers
/// no watcher.
fn watch(config: &Path, changed: Arc<Notify>) -> Option<notify::RecommendedWatcher> {
    use notify::{RecursiveMode, Watcher};
    let dir = config.parent()?.to_path_buf();
    let name = config.file_name()?.to_owned();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        // An error or an overflow may hide an event about the file.
        let relevant = result.map_or(true, |event| {
            event.need_rescan() || event.paths.iter().any(|p| p.file_name() == Some(&*name))
        });
        if relevant {
            changed.notify_one();
        }
    })
    .map_err(|error| log::info!("l4forwarder: no file watcher, polling: {error}"))
    .ok()?;
    watcher
        .watch(&dir, RecursiveMode::NonRecursive)
        .map_err(|error| log::info!("l4forwarder: watch {}: {error}; polling", dir.display()))
        .ok()?;
    Some(watcher)
}

/// Completes at `at`; never without one.
async fn until(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at.into()).await,
        None => std::future::pending().await,
    }
}

fn read_config(path: &Path) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_CONFIG {
        return None;
    }
    std::fs::read(path).ok()
}

/// The running addon's exit; never completes while none runs.
async fn exited(child: &mut Option<Running>) -> std::io::Result<std::process::ExitStatus> {
    match child {
        Some(running) => running.child.wait().await,
        None => std::future::pending().await,
    }
}

/// Closes the addon's input, which ends a supervised addon, and kills it if
/// it is still there after a grace period.
async fn stop(child: &mut Option<Running>) {
    let Some(mut running) = child.take() else {
        return;
    };
    drop(running.input.take());
    if tokio::time::timeout(STOP_GRACE, running.child.wait())
        .await
        .is_err()
    {
        let _ = running.child.kill().await;
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn the_addon_ships_beside_the_daemon() {
        assert_eq!(
            program(Path::new("/Applications/Muster.app/Contents/MacOS/musterd")),
            Path::new("/Applications/Muster.app/Contents/Helpers/l4forwarder")
        );
        assert_eq!(
            program(Path::new("/usr/local/bin/musterd")),
            Path::new("/usr/local/bin/l4forwarder")
        );
    }

    /// A stand-in addon that records each start with its arguments, runs
    /// until its input closes, then records its exit.
    fn fake_addon(dir: &Path) -> (PathBuf, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let record = dir.join("record");
        let program = dir.join("l4forwarder");
        std::fs::write(
            &program,
            format!(
                "#!/bin/sh\necho \"start $$ $*\" >> '{0}'\ncat > /dev/null\necho \"exit $$\" >> '{0}'\n",
                record.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        (program, record)
    }

    fn lines(record: &Path, prefix: &str) -> Vec<String> {
        std::fs::read_to_string(record)
            .unwrap_or_default()
            .lines()
            .filter(|l| l.starts_with(prefix))
            .map(str::to_owned)
            .collect()
    }

    async fn until(what: &str, mut done: impl FnMut() -> bool) {
        let started = Instant::now();
        while !done() {
            assert!(started.elapsed() < Duration::from_secs(5), "{what}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Writes as an editor's atomic save does; a plain write can be seen
    /// half done, which costs one extra restart.
    fn save(path: &Path, text: &str) {
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, text).unwrap();
        std::fs::rename(&tmp, path).unwrap();
    }

    fn supervisor(dir: &Path, program: PathBuf) -> Supervisor {
        Supervisor {
            program,
            config: dir.join(CONFIG),
            log: Some(dir.join(LOG)),
            backoff: Duration::from_millis(100),
        }
    }

    #[tokio::test]
    async fn the_configuration_starts_restarts_and_stops_the_addon() {
        let dir = tempfile::tempdir().unwrap();
        let (program, record) = fake_addon(dir.path());
        let config = dir.path().join(CONFIG);
        let task = tokio::spawn(supervisor(dir.path(), program).run());

        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            lines(&record, "start").is_empty(),
            "started without a configuration"
        );

        save(&config, "{}");
        until("not started", || lines(&record, "start").len() == 1).await;
        let start = &lines(&record, "start")[0];
        assert!(
            start.ends_with(&format!("--config {} --supervised", config.display())),
            "{start}"
        );
        // It keeps running across polls: its input stays open.
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(lines(&record, "exit").is_empty(), "stopped on its own");

        save(&config, r#"{"rules": []}"#);
        until("not restarted", || {
            lines(&record, "exit").len() == 1 && lines(&record, "start").len() == 2
        })
        .await;

        std::fs::remove_file(&config).unwrap();
        until("not stopped", || lines(&record, "exit").len() == 2).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            lines(&record, "start").len(),
            2,
            "restarted without a configuration"
        );
        task.abort();
    }

    /// The watch, not the fallback poll, reports a change: well within the
    /// poll's 5 seconds, and a removal as much as a write.
    #[tokio::test]
    async fn changes_are_seen_as_they_happen() {
        let dir = tempfile::tempdir().unwrap();
        let (program, record) = fake_addon(dir.path());
        let config = dir.path().join(CONFIG);
        let task = tokio::spawn(supervisor(dir.path(), program).run());
        // Asking the OS for the watch is itself slow, and the first look at the
        // file is what covers a change made while it is still being set up. So
        // reach the watched state first and time what the watch reports, not
        // how long arming it took.
        save(&config, "{}");
        until("not started", || lines(&record, "start").len() == 1).await;

        let removed = Instant::now();
        std::fs::remove_file(&config).unwrap();
        until("not stopped", || lines(&record, "exit").len() == 1).await;
        assert!(
            removed.elapsed() < Duration::from_secs(1),
            "{:?}",
            removed.elapsed()
        );

        let rewritten = Instant::now();
        save(&config, "{}");
        until("not restarted", || lines(&record, "start").len() == 2).await;
        assert!(
            rewritten.elapsed() < Duration::from_secs(1),
            "{:?}",
            rewritten.elapsed()
        );
        task.abort();
    }

    #[tokio::test]
    async fn an_addon_that_exits_is_started_again() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let record = dir.path().join("record");
        let program = dir.path().join("l4forwarder");
        std::fs::write(
            &program,
            format!("#!/bin/sh\necho start >> '{}'\nexit 3\n", record.display()),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        save(&dir.path().join(CONFIG), "{}");
        let task = tokio::spawn(supervisor(dir.path(), program).run());
        until("not started again", || lines(&record, "start").len() >= 3).await;
        task.abort();
    }

    /// Stopping musterd drops the supervisor; the addon must not outlive it
    /// and keep its ports from the next daemon.
    #[tokio::test]
    async fn the_addon_ends_with_its_supervisor() {
        let dir = tempfile::tempdir().unwrap();
        let (program, record) = fake_addon(dir.path());
        save(&dir.path().join(CONFIG), "{}");
        let task = tokio::spawn(supervisor(dir.path(), program).run());
        until("not started", || lines(&record, "start").len() == 1).await;
        let pid = lines(&record, "start")[0]
            .split_whitespace()
            .nth(1)
            .unwrap()
            .to_owned();
        task.abort();
        let _ = task.await;
        until("outlived its supervisor", || {
            !std::process::Command::new("kill")
                .args(["-0", &pid])
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success()
        })
        .await;
    }

    #[tokio::test]
    async fn a_missing_addon_is_retried_without_failing_the_daemon() {
        let dir = tempfile::tempdir().unwrap();
        let (program, record) = fake_addon(dir.path());
        save(&dir.path().join(CONFIG), "{}");
        let absent = dir.path().join("absent");
        let task = tokio::spawn(supervisor(dir.path(), absent.clone()).run());
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!task.is_finished());
        // The app update that brings the addon arrives later.
        std::fs::rename(&program, &absent).unwrap();
        until("not started once installed", || {
            lines(&record, "start").len() == 1
        })
        .await;
        task.abort();
    }
}
