#![cfg(unix)]
use muster_browser::*;
use serde_json::json;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

/// The window's operation (ADR 0026). The browser says who the request is, so
/// a caller cannot name the host or hand over a session of its own, and
/// reaching a site waits longer than driving a tab already on screen.
#[test]
fn a_window_fetch_leaves_identity_to_the_browser() {
    let fetch = |method: &str, headers: Vec<(String, String)>| Request::Fetch {
        url: "https://example.test/issues".into(),
        method: method.into(),
        headers,
        body: String::new(),
    };
    let header = |name: &str| vec![(name.to_string(), "x".to_string())];

    assert!(fetch("GET", header("accept")).validate().is_ok());
    for refused in ["cookie", "host", "connection", "content-length"] {
        assert!(
            fetch("GET", header(refused)).validate().is_err(),
            "{refused} is the browser's to set"
        );
    }
    assert!(
        fetch("GET", vec![("x".into(), "a\r\nevil: 1".into())])
            .validate()
            .is_err(),
        "a header value spans one line"
    );
    for refused in ["CONNECT", "TRACE", "get"] {
        assert!(fetch(refused, Vec::new()).validate().is_err(), "{refused}");
    }
    for refused in [
        "file:///tmp/private",
        "javascript:alert(1)",
        "chrome://settings",
        "https://user:password@example.com",
    ] {
        assert!(
            Request::Fetch {
                url: refused.into(),
                method: String::new(),
                headers: Vec::new(),
                body: String::new(),
            }
            .validate()
            .is_err(),
            "{refused}"
        );
    }

    // Reading a page replays safely; anything else may have changed the site.
    assert!(!fetch("GET", Vec::new()).is_mutation());
    assert!(!fetch("", Vec::new()).is_mutation());
    assert!(fetch("POST", Vec::new()).is_mutation());

    assert_eq!(fetch("GET", Vec::new()).deadline(), FETCH_DEADLINE);
    assert_eq!(Request::Ping {}.deadline(), DEADLINE);
    assert_eq!(fetch("GET", Vec::new()).max_request(), MAX_FETCH_REQUEST);
    assert_eq!(Request::Ping {}.max_request(), MAX_REQUEST);
}

#[test]
fn schema_rejects_arbitrary_cdp_and_requires_document_bound_explicit_targets() {
    for value in [
        json!({"op":"evaluate", "code":"x"}),
        json!({"op":"ping", "method":"Runtime.evaluate"}),
        json!({"op":"click", "ref":2}),
        json!({"op":"click", "tabId":1, "ref":2}),
        json!({"op":"detachAll"}),
    ] {
        assert!(serde_json::from_value::<Request>(value).is_err());
    }
    for url in [
        "file:///tmp/private",
        "javascript:alert(1)",
        "chrome://settings",
        "https://user:password@example.com",
    ] {
        assert!(Request::Open { url: url.into() }.validate().is_err());
    }
    assert!(
        Request::Open {
            url: "https://example.com/".into()
        }
        .validate()
        .is_ok()
    );
    assert!(
        Request::Click {
            tab_id: 0,
            document: 1,
            node: 2
        }
        .validate()
        .is_err()
    );
}

#[tokio::test]
async fn framing_handles_fragmented_utf8_and_rejects_oversize_before_payload() {
    let (mut a, mut b) = tokio::io::duplex(128);
    let value = json!({"text":"你好🌍"});
    let data = serde_json::to_vec(&value).unwrap();
    let expected = value.clone();
    let task = tokio::spawn(async move {
        for byte in (data.len() as u32).to_ne_bytes().iter().chain(data.iter()) {
            a.write_all(&[*byte]).await.unwrap();
        }
    });
    assert_eq!(read_frame(&mut b, 128).await.unwrap(), expected);
    task.await.unwrap();
    let (mut a, mut b) = tokio::io::duplex(8);
    a.write_all(&(MAX_REQUEST as u32 + 1).to_ne_bytes())
        .await
        .unwrap();
    assert!(read_frame(&mut b, MAX_REQUEST).await.is_err());
}

#[test]
fn ax_projection_filters_redundant_text_and_preserves_false_checkbox() {
    let raw = json!({"tabId":1,"document":20,"nodes":[
        {"nodeId":"a", "role":{"value":"textbox"},"name":{"value":"姓名"},"value":{"value":"张三"},"childIds":["b","c"]},
        {"nodeId":"b","parentId":"a","role":{"value":"StaticText"},"name":{"value":"张三"}},
        {"nodeId":"c","parentId":"a","role":{"value":"checkbox"},"properties":[{"name":"checked","value":{"value":false}}],"childIds":["a"]}
    ]});
    let result = project(
        &Request::Snapshot {
            tab_id: 1,
            all: false,
        },
        raw.clone(),
    )
    .unwrap();
    assert_eq!(result["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(result["nodes"][1]["props"]["checked"], false);
    assert_eq!(result["document"], 20);
    assert_eq!(
        project(
            &Request::Snapshot {
                tab_id: 1,
                all: true
            },
            raw
        )
        .unwrap()["nodes"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert!(
        project(
            &Request::Snapshot {
                tab_id: 1,
                all: false
            },
            json!({"nodes":[]})
        )
        .is_ok()
    );
}

struct Host {
    child: Child,
    input: ChildStdin,
    output: ChildStdout,
    temp: tempfile::TempDir,
}
impl Host {
    async fn start() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_muster-browser-host"))
            .arg(ORIGIN)
            .env("MUSTER_DIR", temp.path())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let host = Self {
            child,
            input,
            output,
            temp,
        };
        for _ in 0..100 {
            if host.path().exists() {
                return host;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("host did not bind");
    }
    fn path(&self) -> std::path::PathBuf {
        self.temp.path().join("browser/bridge.sock")
    }
}

#[tokio::test]
async fn real_native_host_routes_response_and_rejects_second_profile() {
    use std::os::unix::fs::PermissionsExt;
    let mut host = Host::start().await;
    let path = host.path();
    let call_task = tokio::spawn(async move { call(&path, &Request::Ping {}).await });
    let command = read_frame(&mut host.output, MAX_REQUEST).await.unwrap();
    assert_eq!(command["op"], "ping");
    // bind creates the path before the host applies chmod. Receiving a routed
    // command proves initialization finished; retain the exact permission gate.
    assert_eq!(
        std::fs::metadata(host.path()).unwrap().permissions().mode() & 0o777,
        0o600
    );
    write_frame(
        &mut host.input,
        &json!({"reqId":command["reqId"],"result":{"version":"test"}}),
        MAX_RESPONSE,
    )
    .await
    .unwrap();
    assert_eq!(call_task.await.unwrap().unwrap()["version"], "test");
    let output = Command::new(env!("CARGO_BIN_EXE_muster-browser-host"))
        .arg(ORIGIN)
        .env("MUSTER_DIR", host.temp.path())
        .output()
        .await
        .unwrap();
    assert!(!output.status.success());
    assert!(host.path().exists());
    // Native EOF closes the owner and removes only its socket.
    let owned_path = host.path();
    drop(host.input);
    assert!(
        !tokio::time::timeout(DEADLINE, host.child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    assert!(!owned_path.exists());
}

#[tokio::test]
async fn timeout_marks_mutation_unknown_and_exits_without_replay() {
    let mut host = Host::start().await;
    let path = host.path();
    let task = tokio::spawn(async move {
        call(
            &path,
            &Request::Type {
                tab_id: 1,
                document: 2,
                node: 3,
                text: "hello".into(),
                clear: false,
            },
        )
        .await
    });
    let command = read_frame(&mut host.output, MAX_REQUEST).await.unwrap();
    assert_eq!(command["op"], "type");
    let error = task.await.unwrap().unwrap_err().to_string();
    assert!(error.contains("outcome unknown"), "{error}");
    assert!(
        !tokio::time::timeout(DEADLINE, host.child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    assert!(read_frame(&mut host.output, MAX_REQUEST).await.is_err());
}

#[tokio::test]
async fn wrong_origin_never_creates_socket() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_muster-browser-host"))
        .arg("chrome-extension://wrong/")
        .env("MUSTER_DIR", temp.path())
        .output()
        .await
        .unwrap();
    assert!(!output.status.success());
    assert!(!temp.path().join("browser").exists());
}

#[tokio::test]
async fn rust_idle_lifecycle_releases_debugger_after_fifteen_seconds() {
    let mut host = Host::start().await;
    let command = tokio::time::timeout(
        std::time::Duration::from_secs(18),
        read_frame(&mut host.output, MAX_REQUEST),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(command["op"], "detachAll");
    write_frame(
        &mut host.input,
        &json!({"reqId":command["reqId"],"result":null}),
        MAX_RESPONSE,
    )
    .await
    .unwrap();
    drop(host.input);
    let _ = tokio::time::timeout(DEADLINE, host.child.wait())
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn timed_out_queued_client_never_sends_mutation_without_host_ready() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("waiting.sock");
    let listener = tokio::net::UnixListener::bind(&path).unwrap();
    let task = tokio::spawn(async move {
        call(
            &path,
            &Request::Open {
                url: "https://example.com/".into(),
            },
        )
        .await
    });
    let (mut client, _) = listener.accept().await.unwrap();
    // Host has not sent ready. The outer deadline must close without a payload.
    assert!(task.await.unwrap().is_err());
    use tokio::io::AsyncReadExt;
    let mut bytes = Vec::new();
    client.read_to_end(&mut bytes).await.unwrap();
    assert!(bytes.is_empty());
}
