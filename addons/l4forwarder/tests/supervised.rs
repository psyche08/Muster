//! Under musterd (ADR 0106) the forwarder's standard input is a pipe from
//! its supervisor; when that closes, the forwarder exits rather than outlive
//! a supervisor that could not stop it.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn a_supervised_forwarder_exits_when_its_input_closes() {
    let dir = std::env::temp_dir().join(format!("l4forwarder-supervised-{}", std::process::id()));
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
        assert!(started.elapsed() < Duration::from_secs(5), "never listened");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(child.try_wait().unwrap().is_none());

    drop(child.stdin.take());
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "outlived its supervisor"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(status.success());
    let _ = std::fs::remove_dir_all(&dir);
}
