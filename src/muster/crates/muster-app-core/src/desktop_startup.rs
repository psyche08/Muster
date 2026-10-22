//! One bounded startup path for initial launch and the user's retry. Native
//! service APIs are narrow callbacks; readiness and recovery policy live here.
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::time::Instant;

use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ServiceAction {
    Register = 0,
    RecoverStopped = 1,
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
    pub fn prepare_blocking(
        &self,
        socket: &Path,
        managed: bool,
        service: impl Fn(ServiceAction) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| Error::Unavailable(format!("startup runtime: {e}")))?;
        runtime.block_on(self.prepare(socket, managed, service))
    }

    pub async fn prepare(
        &self,
        socket: &Path,
        managed: bool,
        service: impl Fn(ServiceAction) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.prepare_with(socket, managed, service, Limits::default())
            .await
    }

    async fn prepare_with(
        &self,
        socket: &Path,
        managed: bool,
        service: impl Fn(ServiceAction) -> Result<(), Error>,
        limits: Limits,
    ) -> Result<(), Error> {
        if self
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(Error::Busy("Desktop startup is already in progress".into()));
        }
        let _admission = Admission(&self.busy);
        let started = Instant::now();
        log::info!("desktop startup: phase=probe managed={managed}");
        if ready(socket).await {
            return Ok(());
        }
        if managed {
            log::info!("desktop startup: phase=register");
            service(ServiceAction::Register)?;
        }
        if wait_ready(socket, limits.initial, limits.interval).await {
            return Ok(());
        }
        if managed {
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
        }
        log::warn!(
            "desktop startup: phase=not_ready elapsed_ms={}",
            started.elapsed().as_millis()
        );
        Err(Error::Unavailable(format!(
            "Muster background service is not ready at {}. Try again runs service preparation again; a running service was not stopped",
            socket.display()
        )))
    }
}

async fn ready(socket: &Path) -> bool {
    matches!(
        tokio::time::timeout(
            Duration::from_millis(250),
            tokio::net::UnixStream::connect(socket)
        )
        .await,
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
    fn limits() -> Limits {
        Limits {
            initial: Duration::from_millis(20),
            recovery: Duration::from_millis(60),
            interval: Duration::from_millis(2),
        }
    }
    #[tokio::test]
    async fn healthy_socket_never_touches_registration() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s");
        let _listener = tokio::net::UnixListener::bind(&path).unwrap();
        Coordinator::default()
            .prepare_with(
                &path,
                true,
                |_| panic!("healthy service must not restart"),
                limits(),
            )
            .await
            .unwrap();
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
                                .push(tokio::net::UnixListener::bind(path).unwrap());
                        });
                    }
                    Ok(())
                },
                limits(),
            )
            .await
            .unwrap();
        assert_eq!(
            *actions.lock().unwrap(),
            vec![ServiceAction::Register, ServiceAction::RecoverStopped]
        );
        coordinator
            .prepare_with(
                &path,
                true,
                |_| panic!("retry of healthy service"),
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
        let actions = Mutex::new(Vec::new());
        assert!(
            coordinator
                .prepare_with(
                    &path,
                    true,
                    |a| {
                        actions.lock().unwrap().push(a);
                        Err(Error::Unavailable("approval required".into()))
                    },
                    limits()
                )
                .await
                .is_err()
        );
        assert_eq!(*actions.lock().unwrap(), vec![ServiceAction::Register]);
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
        actions.lock().unwrap().clear();
        assert!(
            coordinator
                .prepare_with(
                    &path,
                    true,
                    |a| {
                        actions.lock().unwrap().push(a);
                        Ok(())
                    },
                    limits()
                )
                .await
                .is_err()
        );
        assert_eq!(
            *actions.lock().unwrap(),
            vec![ServiceAction::Register, ServiceAction::RecoverStopped]
        );
    }
    #[tokio::test]
    async fn concurrent_start_is_rejected_and_cancellation_releases_admission() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s");
        let coordinator = Coordinator::default();
        let mut first =
            Box::pin(coordinator.prepare_with(&path, false, |_| Ok(()), Limits::default()));
        tokio::select! { _ = &mut first => panic!("should await socket"), _ = tokio::time::sleep(Duration::from_millis(5)) => {} }
        assert!(matches!(
            coordinator
                .prepare_with(&path, true, |_| panic!("duplicate native action"), limits())
                .await,
            Err(Error::Busy(_))
        ));
        drop(first);
        let _listener = tokio::net::UnixListener::bind(&path).unwrap();
        coordinator
            .prepare_with(&path, true, |_| panic!("healthy"), limits())
            .await
            .unwrap();
    }
}
