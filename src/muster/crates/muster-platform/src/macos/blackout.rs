//! OS adapter for the daemon's supervised visual blackout.
pub const FLAG: &str = "--display-blackout";

unsafe extern "C" {
    fn muster_blackout_run() -> bool;
}

/// Called only on the panel executable's main thread.
pub fn run() -> anyhow::Result<()> {
    // SAFETY: owns the helper's AppKit main thread, stdin and stdout.
    anyhow::ensure!(
        unsafe { muster_blackout_run() },
        "could not start display blackout"
    );
    Ok(())
}
