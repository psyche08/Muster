//! Where a user's daemon keeps its files: the platform's standard
//! per-user locations, or `$MUSTER_DIR` for everything (development, or a
//! second daemon on one machine). A leaf crate so the command line can
//! find the socket without linking the daemon.
//!
//! | | state (store, socket) | log |
//! |---|---|---|
//! | macOS | `~/Library/Application Support/Muster` | `~/Library/Logs/Muster` |
//! | Linux | `$XDG_STATE_HOME/muster` or `~/.local/state/muster` | the state directory |
//! | Windows | `%LOCALAPPDATA%\\Muster` | the state directory |
//!
//! A system daemon (Linux server role) gets `--dir /var/lib/muster` from its
//! unit file instead.

use std::path::PathBuf;

/// The state directory: `$MUSTER_DIR`, else the platform's default; `None`
/// when neither the environment nor a home directory says.
pub fn state_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("MUSTER_DIR") {
        return Some(PathBuf::from(dir));
    }
    default_state_dir()
}

fn default_state_dir() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Application Support/Muster"))
    } else if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(|d| PathBuf::from(d).join("Muster"))
            .or_else(|| home().map(|h| h.join("AppData/Local/Muster")))
    } else if let Some(dir) = std::env::var_os("XDG_STATE_HOME") {
        Some(PathBuf::from(dir).join("muster"))
    } else {
        home().map(|h| h.join(".local/state/muster"))
    }
}

/// Where the log file goes when the daemon logs to a file (`--log`): the
/// platform's log directory, or the state directory when `$MUSTER_DIR`
/// is set.
pub fn log_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("MUSTER_DIR") {
        return Some(PathBuf::from(dir));
    }
    if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Logs/Muster"))
    } else {
        default_state_dir()
    }
}

/// Where the locks on the user's desktop apps live. Always the platform's
/// default state directory, never `$MUSTER_DIR`: a second daemon (a
/// development one) drives the same apps as the installed one, so both must
/// see the same locks.
pub fn desktop_lock_dir() -> Option<PathBuf> {
    default_state_dir().map(|d| d.join("desktop-locks"))
}

/// The daemon's Unix socket for the state directory.
pub fn socket_path(state_dir: &std::path::Path) -> PathBuf {
    state_dir.join("musterd.sock")
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}
