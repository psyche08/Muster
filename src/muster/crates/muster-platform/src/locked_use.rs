//! OS-only boundary for temporary console access. The daemon owns the policy.
use async_trait::async_trait;
use muster_core::locked_use::{Console, Screen};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
pub struct ConsoleSample {
    pub at: Instant,
    pub console: Option<Console>,
    pub screen: Screen,
    pub input_generation: Option<u64>,
}

pub struct GuardSample {
    pub covered: bool,
    pub alive: bool,
    /// Terminal watchdog failure, distinct from startup not yet being ready.
    pub failed: bool,
    /// Historical OS unlock seen by this guard, not authority to execute.
    pub observed_unlock: Option<ConsoleSample>,
    /// Diagnostic only: how old the newest watchdog status was at this pulse.
    /// Never relaxes `covered`, which still requires a fresh ready status.
    pub status_age: Option<Duration>,
}

pub trait ConsoleGuard: Send {
    /// Independent process owns the displays and locks on parent/heartbeat loss.
    fn pulse(&mut self) -> GuardSample;
    fn lock(&mut self);
    /// Only after Rust has confirmed cleanup. Drop alone must never remove it.
    fn release(&mut self) -> Result<(), String>;
    /// Moves the independent deadline to `ttl` from now (at most the Locked
    /// Use maximum), for a terminal session's signed renewal. It never
    /// shortens it and never revives a guard that is already stopping.
    fn extend(&mut self, _ttl: Duration) -> Result<(), String> {
        Err("this console guard cannot be extended".into())
    }
}

/// Rust-owned permission to continue the current OS authorization submission.
pub type UnlockCheck = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;

#[async_trait]
pub trait LockedUseOs: Send + Sync {
    fn available(&self) -> Result<(), String>;
    /// Refuse replacement while an orphaned protection transaction remains.
    fn update_ready(&self) -> Result<(), String> {
        Ok(())
    }
    fn observe(&self) -> ConsoleSample;
    /// `initial` is the fresh screen state admitted by the signed Rust gate.
    /// The independent process must observe that same state at startup.
    fn guard(
        &self,
        nonce: [u8; 32],
        ttl: Duration,
        initial: Screen,
    ) -> Result<Box<dyn ConsoleGuard>, String>;
    /// Starts the OS authorization flow; never supplies/reads credentials.
    async fn request_unlock(&self, check: UnlockCheck) -> Result<(), String>;
    fn lock(&self);
    /// Authenticates an accepted plugin connection. UID alone is insufficient.
    fn plugin_peer(&self, stream: &std::os::unix::net::UnixStream) -> bool;
}
