//! muster-platform (ARCHITECTURE §3.7, §4): the OS-only capabilities musterd
//! needs, each behind a trait so the daemon never links to a platform API
//! directly. macOS is the first target; other systems get honest defaults.
//!
//! The macOS modules compile only on macOS and have not run on this branch's
//! CI host (Linux); they port the rules of the old Swift/Rust adapters and
//! wait for the P0 device checks in docs/P0-validation.md.

pub mod locked_use;
pub mod signer;

#[cfg(target_os = "macos")]
pub mod macos;

use std::sync::Arc;

/// What only the operating system knows about the console session.
pub trait Platform: Send + Sync {
    fn locked_use(&self, _dir: &std::path::Path) -> Option<Arc<dyn locked_use::LockedUseOs>> {
        None
    }
    /// The console session is locked (screen lock / login window).
    fn locked(&self) -> bool {
        false
    }
    /// The daemon runs in the console user's session.
    fn on_console(&self) -> bool {
        true
    }
    /// Whether the OS lets this process drive other apps' controls
    /// (macOS Accessibility); `None` where no such permission exists.
    fn accessibility(&self) -> Option<bool> {
        None
    }
    /// Asks the OS for that permission (a prompt on macOS); returns the
    /// state afterwards.
    fn request_accessibility(&self) -> Option<bool> {
        self.accessibility()
    }
    /// A diagnostic for the desktop's own user (`muster debug <what> [arg]`):
    /// macOS knows `ax-dump <pid|Claude>` and `ax-sessions <pid|Claude>`.
    fn debug(&self, what: &str, _arg: &str) -> Result<String, String> {
        Err(format!("no diagnostic {what:?} on this platform"))
    }
}

/// Right for a Linux server and for phones (no console to lock).
pub struct DefaultPlatform;

impl Platform for DefaultPlatform {}

/// The platform for this build: lock state and console presence.
pub fn platform() -> Arc<dyn Platform> {
    #[cfg(target_os = "macos")]
    {
        Arc::new(macos::lock::MacPlatform)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Arc::new(DefaultPlatform)
    }
}

/// Requirement strings the controllers check before touching a process.
/// The desktop apps' own come from `muster_core::apps::DesktopApp::requirement`.
pub mod requirements {
    /// Claude Code CLI, whose runtime files name the sessions it runs.
    pub const CLAUDE_CLI: &str = "identifier \"com.anthropic.claude-code\" and anchor apple generic and certificate leaf[subject.OU] = \"Q6L2SF6YDW\"";
}
