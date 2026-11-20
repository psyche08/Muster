use super::*;
use std::process::Command;

fn cygpath(bin: &Path, mode: &str, path: &Path) -> PathBuf {
    let output = Command::new(bin.join("cygpath.exe"))
        .arg(mode)
        .arg(path)
        .output()
        .expect("run cygpath from the test runtime");
    assert!(output.status.success(), "cygpath failed");
    let path = String::from_utf8(output.stdout).unwrap();
    PathBuf::from(path.trim_end_matches(['\n', '\r']))
}

// Shut down only the server under this test's freshly created directory,
// including on assertion failure. ConPTY readers can then finish on their own.
struct FixtureServer(PathBuf, Vec<String>);

impl Drop for FixtureServer {
    fn drop(&mut self) {
        let _ = Command::new(&self.0)
            .args(&self.1)
            .arg("kill-server")
            .output();
    }
}

#[tokio::test]
#[ignore = "requires an explicitly selected MSYS2 tmux runtime on Windows"]
async fn windows_tmux_preserves_each_native_working_directory() {
    let bin = PathBuf::from(
        std::env::var_os("MUSTER_TEST_MSYS2_BIN").expect("set MUSTER_TEST_MSYS2_BIN"),
    );
    let tmux = bin.join("tmux.exe");
    assert!(
        bin.is_absolute() && tmux.is_file(),
        "expected an absolute MSYS2 bin directory with tmux.exe"
    );
    let root = tempfile::tempdir().unwrap();
    let socket = cygpath(&bin, "-u", &root.path().join("s"));
    let mut service = TmuxService::new(socket);
    Arc::get_mut(&mut service).unwrap().program = Some(tmux.clone());
    let _cleanup = FixtureServer(tmux, service.args().await.unwrap());

    for (agent, workspace) in [("first", "first work"), ("second", "第二 #{session_id}")] {
        let directory = root.path().join(workspace);
        std::fs::create_dir(&directory).unwrap();
        // Native CLI sessions persist canonical Windows paths, including the verbatim prefix.
        let canonical = directory.canonicalize().unwrap();
        let cwd = if agent == "first" {
            &directory
        } else {
            &canonical
        };
        let handle = service
            .action(agent, cwd.to_str().unwrap(), 0)
            .await
            .unwrap();
        let target = service.target(&handle).await.unwrap();
        let reported = service
            .checked(&[
                "display-message",
                "-p",
                "-t",
                &target,
                "#{pane_current_path}",
            ])
            .await
            .unwrap();
        assert_eq!(
            cygpath(&bin, "-w", Path::new(&reported))
                .canonicalize()
                .unwrap(),
            canonical
        );
        let missing = directory.join("missing");
        assert!(
            service
                .action(agent, missing.to_str().unwrap(), 2)
                .await
                .is_err()
        );
        assert_eq!(service.action(agent, "", 0).await.unwrap(), handle);
    }
}

#[tokio::test]
async fn missing_tmux_reports_the_host_prerequisite() {
    let root = tempfile::tempdir().unwrap();
    let mut service = TmuxService::new(root.path().join("s"));
    Arc::get_mut(&mut service).unwrap().program = None;
    assert!(matches!(
        service.action("fixture", root.path().to_str().unwrap(), 0).await,
        Err(Error::Unavailable(message)) if message.contains("Install tmux")
    ));
}

#[tokio::test]
#[ignore = "requires MSYS2 with tmux and cygpath"]
async fn oversized_socket_with_no_short_mount_reports_the_limit() {
    let root = tempfile::tempdir().unwrap();
    let socket = root.path().join("x".repeat(120));
    let service = TmuxService::new(socket.clone());
    let result = service.args().await;
    assert!(
        matches!(result, Err(Error::Unavailable(message)) if message.contains("shorter node directory"))
    );
    assert!(!socket.exists());
}
