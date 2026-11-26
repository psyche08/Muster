//! Negative startup probes must be safe on an already-enrolled Windows development machine.
#![cfg(windows)]

use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt, path::Path, process::Command};

fn refused(profile: &Path, args: &[&str]) -> String {
    let result = Command::new(env!("CARGO_BIN_EXE_musterd"))
        .env("LOCALAPPDATA", profile)
        .env_remove("MUSTER_DIR")
        .args(args)
        .output()
        .unwrap();
    assert!(!result.status.success());
    String::from_utf8(result.stderr).unwrap()
}

#[test]
fn invalid_id_and_conflicting_profile_are_rejected_before_startup() {
    let profile = tempfile::tempdir().unwrap();
    assert!(
        refused(profile.path(), &["--resume-existing-device", "invalid"]).contains("invalid value")
    );
    let id = "00".repeat(32);
    assert!(
        refused(
            profile.path(),
            &["--resume-existing-device", &id, "--dir", "."]
        )
        .contains("cannot be used with")
    );
    assert_eq!(profile.path().read_dir().unwrap().count(), 0);
}

#[test]
fn resume_never_provisions_a_missing_or_busy_profile() {
    let profile = tempfile::tempdir().unwrap();
    let id = "00".repeat(32);
    let args = ["--resume-existing-device", &id];
    let missing = refused(profile.path(), &args);
    assert_eq!(profile.path().read_dir().unwrap().count(), 0);
    if muster_platform::require_ordinary_user().is_err() {
        assert!(
            missing.contains("non-administrative user token"),
            "{missing}"
        );
        return;
    }
    assert!(
        missing.contains("cannot acquire Windows device identity"),
        "{missing}"
    );
    let vnet = profile.path().join("Muster").join("Vnet");
    std::fs::create_dir_all(&vnet).unwrap();
    let _owner = OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(0)
        .open(vnet.join("owner.lock"))
        .unwrap();
    let busy = refused(profile.path(), &args);
    assert!(busy.contains("os error 32"), "{busy}");
    assert!(!vnet.join("node.sqlite3").exists());
}
