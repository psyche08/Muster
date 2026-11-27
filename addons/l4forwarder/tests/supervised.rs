//! Under musterd (ADR 0107) the forwarder's standard input is a pipe from
//! its supervisor; when that closes, the forwarder exits rather than outlive
//! a supervisor that could not stop it. A signal still ends it while that
//! input stays open (ADR 0328), so musterd sees the exit and restarts it.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// A `--supervised` forwarder with its input held open, listening on one
/// TCP rule.
struct Supervised {
    child: Child,
    dir: PathBuf,
}

impl Supervised {
    fn start(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "l4forwarder-supervised-{name}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let config = dir.join("config.json");
        let mut file = std::fs::File::create(&config).unwrap();
        write!(
            file,
            r#"{{"rules": [{{"proto": "tcp", "listen": "127.0.0.1:{port}", "upstream": "127.0.0.1:9"}}]}}"#
        )
        .unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_l4forwarder"))
            .arg("--config")
            .arg(&config)
            .arg("--supervised")
            .stdin(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let started = Instant::now();
        while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
            if started.elapsed() > Duration::from_secs(5) {
                let _ = child.kill();
                panic!("never listened");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(child.try_wait().unwrap().is_none());
        Self { child, dir }
    }

    fn signal(&self, name: &str) {
        let sent = Command::new("kill")
            .arg(format!("-{name}"))
            .arg(self.child.id().to_string())
            .status()
            .unwrap();
        assert!(sent.success(), "kill -{name} failed");
    }

    /// The exit status, if the forwarder exits within `limit`.
    fn exit_within(&mut self, limit: Duration) -> Option<ExitStatus> {
        let started = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return Some(status);
            }
            if started.elapsed() > limit {
                return None;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Supervised {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn a_supervised_forwarder_exits_when_its_input_closes() {
    let mut forwarder = Supervised::start("eof");
    drop(forwarder.child.stdin.take());
    let status = forwarder
        .exit_within(Duration::from_secs(5))
        .expect("outlived its supervisor");
    assert!(status.success());
}

/// A plain `kill` once dropped every listener but left the process running:
/// the runtime waited for the blocked read of the still-open input.
#[test]
fn a_supervised_forwarder_exits_on_sigterm_with_its_input_open() {
    let mut forwarder = Supervised::start("term");
    forwarder.signal("TERM");
    let status = forwarder
        .exit_within(Duration::from_secs(1))
        .expect("still running 1 s after SIGTERM");
    assert!(status.success());
    assert!(forwarder.child.stdin.is_some(), "input was closed");
}

#[test]
fn a_supervised_forwarder_exits_on_sigint_with_its_input_open() {
    let mut forwarder = Supervised::start("int");
    forwarder.signal("INT");
    let status = forwarder
        .exit_within(Duration::from_secs(1))
        .expect("still running 1 s after SIGINT");
    assert!(status.success());
    assert!(forwarder.child.stdin.is_some(), "input was closed");
}
