//! Default plugin maintenance belongs to the daemon, not the Swift view.
//! Only the installed production daemon may request this bounded OS effect.
use std::{path::Path, sync::Weak, time::Duration};

pub async fn run(daemon: Weak<crate::Daemon>) {
    let mut delay = 2;
    loop {
        if daemon.upgrade().is_none() {
            return;
        }
        let result = tokio::task::spawn_blocking(
            muster_platform::macos::nethelper::NetHelper::install_locked_use,
        )
        .await;
        match result {
            Ok(Ok(())) => {
                if let Some(d) = daemon.upgrade() {
                    d.plugin_setup
                        .store(2, std::sync::atomic::Ordering::Relaxed);
                    let _ = d.changed.send(());
                }
                log::info!("Locked Use plugin installed and verified by root helper");
                return;
            }
            Ok(Err(reason)) => log::warn!("Locked Use plugin installation pending: {reason}"),
            Err(error) => log::warn!("Locked Use plugin installer task failed: {error}"),
        }
        if let Some(d) = daemon.upgrade() {
            d.plugin_setup
                .store(3, std::sync::atomic::Ordering::Relaxed);
            let _ = d.changed.send(());
        }
        tokio::time::sleep(Duration::from_secs(delay)).await;
        delay = (delay * 2).min(60);
    }
}

pub fn eligible(executable: &Path, dir: &Path, home: &Path, development_override: bool) -> bool {
    !development_override
        && executable == Path::new("/Applications/Muster.app/Contents/MacOS/musterd")
        && dir == home.join("Library/Application Support/Muster")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_installed_default_daemon_requests_system_installation() {
        let installed = Path::new("/Applications/Muster.app/Contents/MacOS/musterd");
        let home = Path::new("/Users/test");
        let dir = home.join("Library/Application Support/Muster");
        assert!(eligible(installed, &dir, home, false));
        assert!(!eligible(installed, &dir, home, true));
        assert!(!eligible(Path::new("/tmp/musterd"), &dir, home, false));
        assert!(!eligible(installed, Path::new("/tmp/muster"), home, false));
    }
}
