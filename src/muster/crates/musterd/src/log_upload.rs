//! Hourly log upload (ADR 0228): the daemon hands `muster_logs::upload` its
//! log files, its state file, and who it is at each upload (device key,
//! network, the names redaction must replace). Scanning, redaction,
//! scheduling and the cursors are muster-logs'.

use std::sync::{Arc, Weak};

use muster_core::DeviceKind;
use muster_logs::collect::default_sources;
use muster_logs::upload::{self, Identity};

use crate::{Daemon, build_info, hostname, kind_str};

/// The state file beside the store: cursors, the redaction salt, and the
/// owner's `"enabled": false`.
pub const STATE: &str = "log-upload.json";

/// Starts the uploader for a desktop or server daemon that writes log
/// files; `None` when there is nothing to upload or the URL is refused.
pub(crate) fn spawn(daemon: &Arc<Daemon>, url: String) -> Option<tokio::task::JoinHandle<()>> {
    if daemon.kind() == DeviceKind::Phone {
        return None;
    }
    let log_dir = muster_paths::log_dir()?;
    let config = match upload::Config::new(
        url,
        daemon.config.dir.join(STATE),
        default_sources(&log_dir),
    ) {
        Ok(config) => config,
        Err(error) => {
            log::warn!("log upload off: {error}");
            return None;
        }
    };
    log::info!(
        "log upload to {} every {} s",
        config.url,
        config.interval.as_secs()
    );
    let weak = Arc::downgrade(daemon);
    Some(tokio::spawn(upload::run(config, move || identity(&weak))))
}

fn identity(daemon: &Weak<Daemon>) -> Option<Identity> {
    let daemon = daemon.upgrade()?;
    if daemon.closing.load(std::sync::atomic::Ordering::Relaxed) {
        return None;
    }
    let (network, mut names) = {
        let roster = daemon.roster().read().unwrap();
        let names: Vec<String> = roster
            .state()
            .members
            .values()
            .map(|member| member.record.name.clone())
            .collect();
        (roster.network_id().map(|id| id.0), names)
    };
    names.push(daemon.name.read().unwrap().clone());
    names.push(hostname());
    Some(Identity {
        signer: daemon.signer().clone(),
        network,
        kind: kind_str(daemon.kind()).into(),
        version: daemon.config.version.clone(),
        commit: build_info::COMMIT.into(),
        names,
    })
}
