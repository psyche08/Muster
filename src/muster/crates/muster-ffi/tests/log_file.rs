//! `muster_log_init_file` through the C ABI: its own test binary, since a
//! process has one logger.

#[test]
fn the_app_log_lands_in_the_log_directory() {
    let dir = tempfile::tempdir().unwrap();
    // SAFETY: the only thread in this binary that reads the environment.
    unsafe { std::env::set_var("MUSTER_DIR", dir.path()) };
    muster_ffi::muster_log_init_file(3);
    log::info!("session live updates interrupted; retrying in 1s");
    log::debug!("below the level");
    let log = std::fs::read_to_string(dir.path().join("app.log")).unwrap();
    let lines: Vec<_> = log.lines().collect();
    assert_eq!(lines.len(), 2, "{log}");
    assert!(lines[0].ends_with(&format!("app log opened, pid {}", std::process::id())));
    assert!(
        lines[1].starts_with('[') && lines[1].contains(" INFO "),
        "{log}"
    );
    assert!(lines[1].ends_with("session live updates interrupted; retrying in 1s"));
    assert!(!log.contains('\u{1b}'), "no terminal colours in a file");
}
