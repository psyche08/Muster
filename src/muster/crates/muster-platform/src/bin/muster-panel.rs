//! The app bundle's AppKit panel helper: one borderless, non-activating panel
//! per run, on this process's main thread.
//!
//! Two callers, both in the daemon, which has no main thread of its own to
//! give AppKit: the idle-compaction banner (ADR 0087) and the Locked Use
//! console cover. They used to run as `musterd` subcommands, spawned from
//! `Contents/MacOS/musterd`, where each one registered with Launch Services
//! as a second running application named "Muster" under the app's own bundle
//! id and hid the app from the Accessibility API. This helper ships in
//! `Contents/Helpers` instead, so no `Info.plist` sits above it and the app's
//! identity stays the app's (ADR 0128, ADR 0127).

#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    use muster_platform::macos::{locked_use, notice};
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some(locked_use::WATCHDOG_FLAG) => {
            locked_use::watchdog(&args[2..]).map_err(anyhow::Error::msg)
        }
        Some(notice::FLAG) => notice::run().map_err(anyhow::Error::msg),
        _ => anyhow::bail!(
            "musterd spawns this helper: muster-panel {} | {} <dir> <nonce> <deadline> <locked|unlocked>",
            notice::FLAG,
            locked_use::WATCHDOG_FLAG
        ),
    }
}

#[cfg(not(target_os = "macos"))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("muster-panel draws AppKit panels; it has no non-macOS build")
}
