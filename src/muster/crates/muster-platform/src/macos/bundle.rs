//! Where the app bundle keeps its own executables, resolved from the running
//! one.
//!
//! `Contents/MacOS` is the application's own directory: a process started
//! from there takes `Muster.app` as its main bundle, and an AppKit process
//! that does so registers with Launch Services under the app's bundle id and
//! name. A second record under `dev.linsheng.muster` is what hid the app's
//! windows from the Accessibility API, because an AX client resolving the app
//! by identity takes the first record it is handed (ADR 0127, ADR 0128).
//! Helpers therefore live in `Contents/Helpers`, where there is no
//! `Info.plist` above them.

use std::path::{Path, PathBuf};

/// The AppKit panel helper. Both the idle-compaction banner (ADR 0087) and
/// the Locked Use console cover draw their panel in it.
pub const PANEL: &str = "muster-panel";

/// Where a bundled helper sits, given the executable it is spawned from:
/// `Contents/Helpers` of the app bundle when the caller runs from
/// `Contents/MacOS`, else beside the caller (a build tree).
pub fn beside(running: &Path, name: &str) -> PathBuf {
    let dir = running.parent().unwrap_or(Path::new("."));
    match dir.parent() {
        Some(contents) if dir.ends_with("Contents/MacOS") => contents.join("Helpers").join(name),
        _ => dir.join(name),
    }
}

/// This process's panel helper. An installation without it draws no banner
/// and starts no Locked Use transaction, rather than falling back to an
/// executable in `Contents/MacOS` that would take the app's identity.
pub fn panel() -> Result<PathBuf, String> {
    let running = std::env::current_exe().map_err(|e| e.to_string())?;
    let path = beside(&running, PANEL);
    if !path.is_file() {
        return Err(format!("no panel helper at {}", path.display()));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_daemon_in_the_bundle_finds_the_helper_beside_the_other_helpers() {
        assert_eq!(
            beside(
                Path::new("/Applications/Muster.app/Contents/MacOS/musterd"),
                PANEL
            ),
            Path::new("/Applications/Muster.app/Contents/Helpers/muster-panel")
        );
    }

    #[test]
    fn a_build_tree_finds_the_helper_beside_the_daemon() {
        assert_eq!(
            beside(Path::new("/src/muster/target/release/musterd"), PANEL),
            Path::new("/src/muster/target/release/muster-panel")
        );
    }

    #[test]
    fn only_contents_macos_reaches_out_of_the_executable_directory() {
        // A helper spawning another helper must not climb out of
        // `Contents/Helpers` and name a sibling of `Contents`.
        assert_eq!(
            beside(
                Path::new("/Applications/Muster.app/Contents/Helpers/muster-panel"),
                PANEL
            ),
            Path::new("/Applications/Muster.app/Contents/Helpers/muster-panel")
        );
    }
}
