//! Exercise the public RPCs on a disposable Windows node using a real MSYS2 runtime.
#![cfg(windows)]

use muster_core::SoftSigner;
use muster_proto::v1::{self, request::Req, response::Resp};
use muster_rpc::Client;
use musterd::{Daemon, DaemonConfig, DefaultPlatform};
use std::{fs, path::Path, sync::Arc, time::Duration};

const SESSION: &str = "4055d1b1-b7b0-4537-9484-742bc60aa33a";

// The test harness launches this entry in a separate OS process for each generation.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "child of the managed terminal fixture"]
async fn fixture_node_process() {
    let Some(root) = std::env::var_os("MUSTER_FIXTURE_NODE") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let signer =
        Arc::new(SoftSigner::from_bytes(&fs::read(root.join("fixture.key")).unwrap()).unwrap());
    let mut config = DaemonConfig::new(root.join("node"));
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.cloud = false;
    config.agent = Some(muster_agent::AgentConfig {
        codex_home: Some(root.join("codex")),
        claude_home: None,
        claude_desktop: None,
        catdesk_home: None,
        deepseek_home: None,
        ..Default::default()
    });
    let daemon = Daemon::start_with(config, Arc::new(DefaultPlatform), Some(signer))
        .await
        .unwrap();
    println!("fixture ready");
    use std::io::BufRead;
    tokio::task::spawn_blocking(|| std::io::stdin().lock().read_line(&mut String::new()))
        .await
        .unwrap()
        .unwrap();
    daemon.shutdown().await;
}

async fn start(root: &Path) -> (tokio::process::Child, Client) {
    use std::process::Stdio;
    use tokio::io::AsyncBufReadExt;
    let mut child = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "fixture_node_process",
            "--ignored",
            "--nocapture",
        ])
        .env("MUSTER_FIXTURE_NODE", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut lines = tokio::io::BufReader::new(child.stdout.take().unwrap()).lines();
    tokio::time::timeout(Duration::from_secs(20), async {
        while let Some(line) = lines.next_line().await.unwrap() {
            if line.contains("fixture ready") {
                return;
            }
        }
        panic!("fixture exited during startup");
    })
    .await
    .expect("node startup");
    // Continue draining the harness output until the child exits.
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
    let client = Client::new(
        muster_local::connect(&root.join("node/musterd.sock"))
            .await
            .unwrap(),
    );
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let Resp::AgentSessions(page) = client
                .call(Req::AgentList(Default::default()))
                .await
                .unwrap()
            else {
                panic!("expected catalog")
            };
            if page.sessions.iter().any(|s| s.id == SESSION) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("fixture session discovery");
    (child, client)
}

async fn action(client: &Client, action: i32, op: &str) -> v1::OpResult {
    let Resp::OpResult(r) = client
        .call(Req::AgentTerminal(v1::AgentTerminalRequest {
            session: SESSION.into(),
            op_id: op.into(),
            action,
        }))
        .await
        .unwrap()
    else {
        panic!("expected terminal action receipt")
    };
    assert!(r.status == 0 || r.status == 1, "{op}: {}", r.message);
    r
}

fn history(id: &str) -> Req {
    Req::TerminalHistory(v1::TerminalHistoryRequest {
        session: id.into(),
        lines: 100,
    })
}

#[tokio::test]
#[ignore = "requires MSYS2 with tmux next to the test executable or in MUSTER_MSYS2_ROOT"]
async fn managed_shell_through_windows_daemon_restart() {
    let root = tempfile::tempdir().unwrap();
    let day = root.path().join("codex/sessions/2026/01/01");
    fs::create_dir_all(&day).unwrap();
    let meta =
        serde_json::json!({"type":"session_meta","payload":{"id":SESSION,"cwd":root.path()}});
    fs::write(
        day.join(format!("rollout-2026-01-01T00-00-00-{SESSION}.jsonl")),
        format!("{meta}\n"),
    )
    .unwrap();
    // On panic, end only the tmux server belonging to this fixture.
    struct Cleanup(std::path::PathBuf, std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::process::Command::new(&self.0)
                .arg("-S")
                .arg(&self.1)
                .arg("kill-server")
                .output();
        }
    }
    let runtime = std::env::var_os("MUSTER_MSYS2_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .join("msys64")
        });
    assert!(runtime.is_absolute());
    let _cleanup = Cleanup(
        runtime.join("usr/bin/tmux.exe"),
        root.path().join("node/tmux.sock"),
    );
    fs::write(
        root.path().join("fixture.key"),
        SoftSigner::generate().to_bytes(),
    )
    .unwrap();
    let (mut child, client) = start(root.path()).await;
    let id = action(&client, 0, "first").await.result_id;
    assert!(id.starts_with("tmux-v1-"));
    assert_eq!(action(&client, 0, "first").await.status, 1);
    let mut stream = client
        .start(Req::TerminalAttach(v1::TerminalAttachRequest {
            session: id.clone(),
            since_rev: 0,
        }))
        .await
        .unwrap();
    let Some(v1::event::Ev::TerminalFrame(frame)) =
        tokio::time::timeout(Duration::from_secs(5), stream.events.recv())
            .await
            .unwrap()
    else {
        panic!("missing frame")
    };
    assert!(
        frame.full
            && frame.session == id
            && frame.desktop_attach_command.is_empty()
            && frame.iterm2_attach_command.is_empty()
    );
    stream.cancel().await;
    drop(stream);
    async fn input(client: &Client, id: &str, op: &str, bytes: &[u8]) {
        let Resp::OpResult(r) = client
            .call(Req::TerminalInput(v1::TerminalInputRequest {
                session: id.into(),
                op_id: op.into(),
                bytes: bytes.to_vec(),
            }))
            .await
            .unwrap()
        else {
            panic!("expected input receipt")
        };
        assert_eq!(r.status, 0, "{}", r.message);
    }
    input(&client, &id, "set", b"fixture_value=41\r").await;
    client
        .call(Req::TerminalClose(v1::TerminalCloseRequest {
            session: id.clone(),
        }))
        .await
        .unwrap();
    client.close();
    drop(client);
    child.stdin.take();
    assert!(
        tokio::time::timeout(Duration::from_secs(20), child.wait())
            .await
            .expect("first daemon shutdown")
            .unwrap()
            .success()
    );

    let (mut child, client) = start(root.path()).await;
    assert_eq!(action(&client, 0, "resume").await.result_id, id);
    input(
        &client,
        &id,
        "read",
        b"printf 'result=%s\\n' \"$((fixture_value+1))\" > terminal-result.txt && cat terminal-result.txt\r",
    )
    .await;
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let Resp::TerminalLines(lines) = client.call(history(&id)).await.unwrap() else {
                panic!("missing history")
            };
            if lines.lines.iter().any(|l| l.text.contains("result=42")) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("same shell after daemon restart");
    assert_ne!(action(&client, 2, "replace").await.result_id, id);
    assert!(client.call(history(&id)).await.is_err());
    action(&client, 1, "end").await;
    client.close();
    child.stdin.take();
    assert!(
        tokio::time::timeout(Duration::from_secs(20), child.wait())
            .await
            .expect("second daemon shutdown")
            .unwrap()
            .success()
    );
}

#[test]
#[ignore = "requires the portable ZIP's msys64 next to the test executable"]
fn packaged_shell_shares_the_windows_user_temp_directory() {
    let bin = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("msys64/usr/bin");
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("from-windows"), "windows").unwrap();
    let script = temp.path().join("check.sh");
    fs::write(&script, r#"cd "/tmp/$MUSTER_FIXTURE_BASENAME" && test "$(/usr/bin/cat from-windows)" = windows && printf shell > from-shell"#).unwrap();
    let output = std::process::Command::new(bin.join("sh.exe"))
        .args(["--login", "-s"])
        .stdin(fs::File::open(script).unwrap())
        .env("MUSTER_FIXTURE_BASENAME", temp.path().file_name().unwrap())
        .env("HOME", temp.path())
        .env("MSYS", "noglob")
        .output()
        .unwrap();
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{diagnostics}");
    assert!(
        !diagnostics.contains("could not find /tmp"),
        "{diagnostics}"
    );
    assert_eq!(fs::read(temp.path().join("from-shell")).unwrap(), b"shell");
}
