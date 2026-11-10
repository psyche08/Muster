//! The phone sets a durable preference. Only the panel process can report
//! that the Mac has acquired its assertions and covered the current screens.
use crate::Error;
use muster_proto::v1::DisplayBlackout;
use muster_store::Store;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

const KEY: &str = "display.blackout";
pub(crate) struct Blackout {
    // Serialize persistence with publishing the corresponding desired state.
    desired: Mutex<watch::Sender<bool>>,
    observed: Arc<Mutex<String>>,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Blackout {
    #[cfg(target_os = "macos")]
    pub fn start(store: &Store) -> Result<Self, Error> {
        Self::with_panel(store, muster_platform::macos::bundle::panel)
    }

    #[cfg(target_os = "macos")]
    fn with_panel(
        store: &Store,
        path: impl Fn() -> Result<std::path::PathBuf, String> + Send + 'static,
    ) -> Result<Self, Error> {
        let (desired, rx) = watch::channel(store.kv_get(KEY)?.as_deref() == Some(b"on"));
        let observed = Arc::new(Mutex::new(String::new()));
        let task = tokio::spawn(supervise(rx, observed.clone(), path));
        Ok(Self {
            desired: Mutex::new(desired),
            observed,
            task: Mutex::new(Some(task)),
        })
    }

    pub fn set(&self, store: &Store, on: bool, locked: bool) -> Result<(), Error> {
        let desired = self.desired.lock().unwrap();
        if *desired.borrow() == on {
            return Ok(());
        }
        if on && locked {
            return Err(Error::Conflict(
                "Unlock this Mac before enabling display blackout".into(),
            ));
        }
        store.kv_set(KEY, if on { b"on" } else { b"off" })?;
        desired.send_replace(on);
        Ok(())
    }

    pub fn view(&self) -> DisplayBlackout {
        let enabled = *self.desired.lock().unwrap().borrow();
        let observation = self.observed.lock().unwrap();
        let status = match (enabled, observation.is_empty()) {
            (true, true) => "正在开启黑屏…",
            (true, false) => observation.as_str(),
            (false, false) => "正在解除黑屏…",
            (false, true) => "已关闭",
        };
        DisplayBlackout {
            enabled,
            status: status.into(),
        }
    }

    pub async fn stop(&self) {
        let task = self.task.lock().unwrap().take();
        if let Some(task) = task {
            task.abort();
            let _ = task.await;
        }
    }
}

#[cfg(target_os = "macos")]
async fn supervise(
    mut desired: watch::Receiver<bool>,
    observed: Arc<Mutex<String>>,
    path: impl Fn() -> Result<std::path::PathBuf, String>,
) {
    use std::time::Duration;
    loop {
        if desired.has_changed().is_err() || desired.wait_for(|on| *on).await.is_err() {
            return;
        }
        let result = run_panel(&mut desired, &observed, path()).await;
        match result {
            Ok(()) => observed.lock().unwrap().clear(),
            Err(error) => {
                *observed.lock().unwrap() = format!("黑屏未就绪，正在重试：{error}");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(3)) => {}
                    changed = desired.changed() => if changed.is_err() { return; }
                }
                observed.lock().unwrap().clear();
            }
        }
    }
}

#[cfg(target_os = "macos")]
async fn run_panel(
    desired: &mut watch::Receiver<bool>,
    observed: &Mutex<String>,
    path: Result<std::path::PathBuf, String>,
) -> anyhow::Result<()> {
    use muster_platform::macos::blackout::FLAG;
    use std::{process::Stdio, time::Duration};
    use tokio::io::AsyncReadExt;
    let path = path.map_err(anyhow::Error::msg)?;
    let mut child = tokio::process::Command::new(path)
        .arg(FLAG)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let mut reports = child.stdout.take().expect("piped stdout");
    let mut first = true;
    loop {
        tokio::select! {
            _ = async { drop(desired.wait_for(|on| !*on).await); } => {
                drop(child.stdin.take());
                if tokio::time::timeout(Duration::from_secs(2), child.wait()).await.is_err() {
                    child.kill().await?;
                }
                return Ok(());
            }
            report = reports.read_u8() => {
                *observed.lock().unwrap() = match report? {
                    b'1' => "所有屏幕已遮盖 · Mac 保持唤醒".into(),
                    b'0' => "正在等待可遮盖的屏幕…".into(),
                    _ => anyhow::bail!("invalid panel status"),
                };
                first = false;
            }
            _ = tokio::time::sleep(Duration::from_secs(5)), if first => {
                anyhow::bail!("display helper did not report readiness");
            }
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

    async fn await_status(mode: &Blackout, expected: &str) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !mode.view().status.contains(expected) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("expected {expected}, got {:?}", mode.view()));
    }

    #[tokio::test]
    async fn blackout_recovers_persisted_intent_and_releases_the_panel() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("blackout.db")).unwrap();
        // A fake helper reports the same readiness byte and waits for EOF.
        // No AppKit or power changes run on the test machine.
        let helper = dir.path().join("panel");
        fs::write(&helper, "#!/bin/sh\nprintf 1\nexec /bin/cat\n").unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        let start = || {
            let path = helper.clone();
            Blackout::with_panel(&store, move || Ok(path.clone())).unwrap()
        };
        let first = start();
        assert!(first.set(&store, true, true).is_err());
        assert!(!first.view().enabled);
        first.set(&store, true, false).unwrap();
        first.set(&store, true, true).unwrap();
        await_status(&first, "所有屏幕已遮盖").await;
        first.stop().await;

        let restarted = start();
        assert!(restarted.view().enabled);
        await_status(&restarted, "所有屏幕已遮盖").await;
        restarted.set(&store, false, true).unwrap();
        await_status(&restarted, "已关闭").await;
        assert_eq!(store.kv_get(KEY).unwrap().unwrap(), b"off");
        restarted.stop().await;
    }
}
