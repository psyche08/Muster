//! `authorized_keys` from the roster (§3.6.3, P6): every active member
//! whose scopes reach tcp/22 on this device gets its device key as an
//! OpenSSH line, so the phone's in-app SSH (and any member over vnet) logs
//! in with the same key it pairs with. Point sshd at the file with
//! `AuthorizedKeysFile .ssh/authorized_keys /etc/muster/authorized_keys`.

use std::path::{Path, PathBuf};
use std::sync::Weak;

use muster_core::{Proto, Scope};

use crate::Daemon;

/// The file's content for the current roster.
pub fn render(daemon: &Daemon) -> String {
    let me = daemon.device_id();
    let wanted = Scope::Net {
        proto: Proto::Tcp,
        port: Some(22),
    };
    let roster = daemon.roster().read().unwrap();
    let mut lines = vec!["# managed by musterd: members that may reach tcp/22 here".to_string()];
    for member in roster.state().active_members() {
        if member.record.device_id == me || !member.record.scopes.iter().any(|s| s.covers(&wanted))
        {
            continue;
        }
        let comment = format!(
            "muster:{}:{}",
            member.record.name.replace(' ', "_"),
            member.record.device_id.fingerprint()
        );
        match muster_terminal::ssh::authorized_key_line(&member.record.public_key, &comment) {
            Ok(line) => lines.push(line),
            Err(error) => log::warn!("authorized_keys: {}: {error}", member.record.name),
        }
    }
    lines.join("\n") + "\n"
}

fn write_atomically(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644))?;
    }
    std::fs::rename(&tmp, path)
}

/// Rewrites the file whenever the roster changes (and once at start).
pub async fn run(daemon: Weak<Daemon>, path: PathBuf) {
    let mut changes = match daemon.upgrade() {
        Some(d) => d.subscribe_changes(),
        None => return,
    };
    let mut last = String::new();
    loop {
        if let Some(d) = daemon.upgrade() {
            let content = render(&d);
            if content != last {
                match write_atomically(&path, &content) {
                    Ok(()) => {
                        log::info!(
                            "authorized_keys: {} keys in {}",
                            content.lines().count() - 1,
                            path.display()
                        );
                        last = content;
                    }
                    Err(error) => log::warn!("authorized_keys {}: {error}", path.display()),
                }
            }
        } else {
            return;
        }
        match changes.recv().await {
            Ok(()) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
            Err(_) => return,
        }
    }
}
