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

async fn observed(mut check: impl AsyncFnMut() -> bool) -> bool {
    tokio::time::timeout(Duration::from_secs(15), async {
        while !check().await {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .is_ok()
}

#[tokio::test]
#[ignore = "requires the packaged MSYS2 tmux runtime"]
async fn reopen_after_ending_the_last_shell() {
    let root = tempfile::tempdir().unwrap();
    let service = TmuxService::new(root.path().join("s"));
    let program = service.program.as_ref().expect("set MUSTER_MSYS2_ROOT");
    let _cleanup = FixtureServer(program.clone(), service.args().await.unwrap());
    let cwd = root.path().to_str().unwrap();
    let old = service.action("fixture", cwd, 0).await.unwrap();
    service.action("fixture", cwd, 1).await.unwrap();

    let current = service.action("fixture", cwd, 0).await.unwrap();
    assert_ne!(current, old);
    assert!(service.input(&old, b"stale").await.is_err());
    service
        .input(&current, b"echo $((37 + 42))\r")
        .await
        .unwrap();
    assert!(
        observed(async || service
            .history(&current, 80)
            .await
            .unwrap()
            .iter()
            .any(|row| row.text.trim() == "79"))
        .await
    );
}

#[tokio::test]
#[ignore = "requires the packaged MSYS2 runtime and Windows PowerShell in ConPTY"]
async fn powershell_desktop_client_shares_only_the_selected_generation() {
    let root = tempfile::Builder::new()
        .prefix("共享 'x’ $ ")
        .tempdir()
        .unwrap();
    let service = TmuxService::new(root.path().join("s"));
    let program = service.program.as_ref().expect("set MUSTER_MSYS2_ROOT");
    let _cleanup = FixtureServer(program.clone(), service.args().await.unwrap());
    let cwd = root.path().to_str().unwrap();
    let handle = service.action("handoff", cwd, 0).await.unwrap();

    // Leave a real unfinished command on this disposable shell before opening another client.
    service
        .input(&handle, b"printf '%s%s\\n' DESKTOP")
        .await
        .unwrap();
    assert!(
        observed(async || service
            .history(&handle, 100)
            .await
            .unwrap()
            .iter()
            .any(|line| line.text.trim_end().ends_with("DESKTOP")))
        .await
    );

    let (command, iterm) = service.desktop_commands(&handle).await.unwrap();
    assert!(iterm.is_empty() && !command.is_empty());
    let shell = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let desktop = PtyService::new(PtyConfig::default());
    let args = [
        "-NoProfile".into(),
        "-NoLogo".into(),
        "-Command".into(),
        command,
    ];
    let client = desktop
        .create_command(&shell, &args, None, "test-", &[])
        .await
        .unwrap();
    let target = service.target(&handle).await.unwrap();
    let attached = observed(async || {
        service
            .checked(&[
                "list-clients",
                "-t",
                &target,
                "-F",
                "#{client_flags} #{client_termfeatures}",
            ])
            .await
            .unwrap()
            .lines()
            .any(|line| line.contains("ignore-size") && line.contains("bpaste"))
    })
    .await;
    service.input(&handle, b" -SHARED\r").await.unwrap();
    let visible = attached
        && observed(async || {
            desktop
                .history(&client.id, 100)
                .await
                .is_ok_and(|rows| rows.iter().any(|line| line.text.contains("DESKTOP-SHARED")))
        })
        .await;
    if visible {
        desktop
            .input(&client.id, b"printf '%s%s\\n' RETURN -PATH\r")
            .await
            .unwrap();
    }
    let returned = visible
        && observed(async || {
            service
                .history(&handle, 100)
                .await
                .unwrap()
                .iter()
                .any(|line| line.text.contains("RETURN-PATH"))
        })
        .await;
    if desktop.contains(&client.id) {
        desktop.close(&client.id).await.unwrap();
    }
    assert!(
        attached,
        "second client has ignore-size and bracketed paste"
    );
    assert!(
        visible,
        "opening a second client must preserve unfinished input"
    );
    assert!(
        returned,
        "input on the second client reaches the original shell"
    );

    assert_eq!(service.action("handoff", cwd, 0).await.unwrap(), handle);
    let overrides = service
        .checked(&["show-options", "-sv", "terminal-overrides"])
        .await
        .unwrap();
    assert_ne!(service.action("handoff", cwd, 2).await.unwrap(), handle);
    assert_eq!(
        service
            .checked(&["show-options", "-sv", "terminal-overrides"])
            .await
            .unwrap(),
        overrides,
        "reconnecting must preserve existing capability overrides"
    );
    let stale = desktop
        .create_command(&shell, &args, None, "stale-", &[])
        .await
        .unwrap();
    let exited = observed(async || !desktop.contains(&stale.id)).await;
    if !exited {
        desktop.close(&stale.id).await.unwrap();
    }
    assert!(
        exited,
        "previous command cannot attach to replacement shell"
    );
    service.action("handoff", cwd, 1).await.unwrap();
}
