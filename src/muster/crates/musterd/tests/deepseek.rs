//! DeepSeek Harness sessions are driven through Muster's plugin inside the
//! app (ADR 0074), yet stay under the same rules as Accessibility-driven
//! sessions: a locked Mac demands a signed Locked Use operation, and nothing
//! is sent while the plugin is not running.
use muster_core::{Author, DeviceKind, DeviceRecord, HwSigner, Scope, SoftSigner};
use muster_proto::v1::{self, error::Code, request::Req};
use muster_rpc::{Client, Principal};
use musterd::{Daemon, DaemonConfig};
use std::sync::Arc;
use std::time::{Duration, Instant};

const SESSION: &str = "session-4fa08e96-7b5c-4d2e-9f6a-0c1d2e3f4a5b";

struct Locked;
impl muster_platform::Platform for Locked {
    fn locked(&self) -> bool {
        true
    }
}

struct Unlocked;
impl muster_platform::Platform for Unlocked {}

fn connect(daemon: &Arc<Daemon>, peer: muster_core::DeviceId) -> Client {
    let (a, b) = tokio::io::duplex(1024 * 1024);
    tokio::spawn(muster_rpc::serve(
        b,
        Principal::Remote(peer),
        Arc::new(musterd::MusterHandler::new(Arc::downgrade(daemon))),
    ));
    Client::new(a)
}

/// A daemon reading one DeepSeek Harness session (an uncompressed log), and
/// a phone allowed to drive DeepSeek Harness sessions.
async fn daemon(
    platform: Arc<dyn muster_platform::Platform>,
) -> (tempfile::TempDir, Arc<Daemon>, Client) {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("dsh");
    let log = home.join("sessions/--tmp-project--").join(SESSION);
    std::fs::create_dir_all(&log).unwrap();
    std::fs::write(
        log.join("session.v4.jsonl"),
        format!(
            "{}\n",
            serde_json::json!({"type":"session","version":4,"id":SESSION,"createdAt":1,
                "cwd":dir.path(),"isSeeded":false})
        ),
    )
    .unwrap();
    daemon_with_home(dir, platform, home, SESSION).await
}

async fn daemon_with_home(
    dir: tempfile::TempDir,
    platform: Arc<dyn muster_platform::Platform>,
    home: std::path::PathBuf,
    session: &str,
) -> (tempfile::TempDir, Arc<Daemon>, Client) {
    let mut config = DaemonConfig::new(dir.path().join("muster"));
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.listen = false;
    config.terminal = None;
    config.agent = Some(muster_agent::AgentConfig {
        codex_home: None,
        claude_home: None,
        catdesk_home: None,
        deepseek_home: Some(home),
        claude_desktop: None,
        rescan: Duration::from_millis(100),
        ..Default::default()
    });
    let daemon = Daemon::start_with(config, platform, None).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while daemon.agent().unwrap().session(session).is_none() {
        assert!(Instant::now() < deadline, "the log was not listed");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let phone = SoftSigner::generate();
    {
        let mut roster = daemon.roster().write().unwrap();
        let entry = Author {
            roster: &roster,
            signer: &**daemon.signer(),
        }
        .genesis(
            "test",
            ("192.168.14.0".parse().unwrap(), 24),
            daemon.own_record(),
        )
        .unwrap();
        roster.insert(entry).unwrap();
        let mut record = DeviceRecord::new(phone.public_key(), "phone", DeviceKind::Phone);
        record
            .scopes
            .extend([Scope::Agents, Scope::AgentProvider("deepseek".into())]);
        let entry = Author {
            roster: &roster,
            signer: &**daemon.signer(),
        }
        .add(record)
        .unwrap();
        roster.insert(entry).unwrap();
    }
    let client = connect(&daemon, phone.public_key().device_id());
    (dir, daemon, client)
}

/// Explicit local acceptance only. The host is the installed, signed App
/// runtime in an isolated profile, not a fake socket or a Muster CLI session.
#[cfg(target_os = "macos")]
#[tokio::test]
#[ignore = "requires an isolated installed App host; see plugin README"]
async fn installed_app_plugin_accepts_and_recovers_over_existing_device_rpc() {
    use muster_agent::controller::{ProcessIdentity, ProcessState};
    use muster_platform::macos::codesign;
    use muster_proto::{
        Message,
        v1::{op_result::Status, response::Resp},
    };
    struct SignedHost;
    impl muster_agent::deepseek_app::Host for SignedHost {
        fn verify(&self, pid: u32) -> Option<ProcessIdentity> {
            let start = codesign::process_start(pid)?;
            (codesign::process_satisfies(pid, &muster_core::apps::DEEPSEEK.requirement())
                && codesign::process_start(pid) == Some(start))
            .then_some(ProcessIdentity { pid, start })
        }
        fn exited(&self, process: ProcessIdentity) -> bool {
            process.observe(codesign::observe_process_start(process.pid).map_err(|_| ()))
                == ProcessState::Ended
        }
    }
    let home = std::path::PathBuf::from(
        std::env::var("MUSTER_DSH_ACCEPTANCE_HOME").expect("explicit isolated host directory"),
    )
    .canonicalize()
    .unwrap();
    assert!(
        home.starts_with("/private/tmp"),
        "never drive a production profile in this test"
    );
    let created: serde_json::Value =
        serde_json::from_slice(&std::fs::read(home.join("acceptance-session.json")).unwrap())
            .unwrap();
    let session = created["sessionId"].as_str().unwrap().to_owned();
    let (_dir, daemon, client) = daemon_with_home(
        tempfile::tempdir().unwrap(),
        Arc::new(Unlocked),
        home,
        &session,
    )
    .await;
    let agent = daemon.agent().unwrap().clone();
    let link = agent.deepseek_link().unwrap();
    let service = agent.clone();
    tokio::task::spawn_blocking(move || {
        muster_agent::deepseek_app::refresh(&service, &link, &SignedHost)
    })
    .await
    .unwrap();
    assert_eq!(
        agent.session(&session).unwrap().controller,
        "plugin",
        "signed App process must bind before writing"
    );
    let request = v1::AgentSendRequest {
        op_id: format!("local-plugin-{}", std::process::id()),
        session: session.clone(),
        text: "Muster isolated plugin acceptance. Reply only: OK.".into(),
        ..Default::default()
    };
    let Resp::OpResult(result) = client.call(Req::AgentSend(request.clone())).await.unwrap() else {
        panic!()
    };
    assert_eq!(result.status, Status::Applied as i32, "{}", result.message);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !agent.lookup_send(&session, &request.op_id).await.unwrap() {
        assert!(
            Instant::now() < deadline,
            "host inbox/event must retain the exact request receipt"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // Acceptance and durable host observation can arrive separately. Neither
    // the evidence wait nor the repeated device request may resend the prompt.
    assert!(matches!(
        daemon
            .store()
            .ledger()
            .outcome(&request.op_id, "agent.send", &request.encode_to_vec())
            .unwrap(),
        muster_store::Outcome::Applied(_)
    ));
    let Resp::OpResult(duplicate) = client.call(Req::AgentSend(request.clone())).await.unwrap()
    else {
        panic!()
    };
    assert_eq!(duplicate.status, Status::Duplicate as i32);
    let Resp::OpResult(status) = client.call(Req::AgentSendStatus(request)).await.unwrap() else {
        panic!()
    };
    assert_eq!(status.status, Status::Applied as i32);
    let Resp::OpResult(cancel) = client
        .call(Req::AgentInterrupt(v1::AgentInterruptRequest {
            session,
            op_id: format!("local-interrupt-{}", std::process::id()),
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(cancel.status, Status::Applied as i32, "{}", cancel.message);
}

fn send() -> Req {
    Req::AgentSend(v1::AgentSendRequest {
        op_id: "op-1".into(),
        session: SESSION.into(),
        text: "run the tests".into(),
        ..Default::default()
    })
}

/// The gates every agent provider meets on a locked Mac: the Mac's own
/// policy must allow locked control, and then only a signed Locked Use
/// operation (presence and `locked.control`) runs.
#[tokio::test]
async fn a_locked_mac_demands_locked_use_as_for_a_native_app() {
    let (_dir, daemon, client) = daemon(Arc::new(Locked)).await;
    match client.call(send()).await {
        Err(muster_rpc::Error::Remote { code, message }) => {
            assert_eq!(code, Code::Unauthorized, "{message}");
            assert!(message.contains("refused by local policy"), "{message}");
        }
        other => panic!("a locked Mac must refuse an unsigned send: {other:?}"),
    }
    daemon.policy().write().unwrap().allow_locked_control = true;
    match client.call(send()).await {
        Err(muster_rpc::Error::Remote { code, message }) => {
            assert_eq!(code, Code::LockedUseRequired, "{message}")
        }
        other => panic!("a locked Mac must refuse an unsigned send: {other:?}"),
    }
    assert!(daemon.store().ledger().status("op-1").unwrap().is_none());
}

#[tokio::test]
async fn nothing_is_sent_while_the_plugin_is_not_running() {
    let (_dir, daemon, client) = daemon(Arc::new(Unlocked)).await;
    let error = client.call(send()).await.unwrap_err().to_string();
    assert!(error.contains("Muster's plugin"), "{error}");
    assert!(error.contains("Nothing was sent"), "{error}");
    assert!(daemon.store().ledger().status("op-1").unwrap().is_none());
    // A model choice stays with the app.
    let options = client
        .call(Req::AgentChatOptions(v1::AgentChatOptionsRequest {
            session: SESSION.into(),
            ..Default::default()
        }))
        .await
        .unwrap_err()
        .to_string();
    assert!(options.contains("model or effort"), "{options}");
}

/// Exercise the existing authenticated device API against the new local plugin
/// contract. The host accepted the prompt but lost its socket reply; only a
/// subsequent receipt lookup may settle the original operation.
#[tokio::test]
async fn unchanged_device_rpc_recovers_plugin_receipt_without_replay() {
    use muster_agent::controller::ProcessIdentity;
    use muster_proto::{
        Message,
        v1::{op_result::Status, response::Resp},
    };
    use serde_json::{Value, json};
    use std::io::{BufRead, BufReader, Write};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    struct Host;
    impl muster_agent::deepseek_app::Host for Host {
        fn verify(&self, pid: u32) -> Option<ProcessIdentity> {
            (pid == std::process::id()).then_some(ProcessIdentity { pid, start: 1 })
        }
        fn exited(&self, _: ProcessIdentity) -> bool {
            false
        }
    }

    let (dir, daemon, client) = daemon(Arc::new(Unlocked)).await;
    let socket = dir.path().join("dsh/muster/bridge.sock");
    std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let accepted = Arc::new(AtomicBool::new(false));
    let prompts = Arc::new(AtomicUsize::new(0));
    let lookups = Arc::new(AtomicUsize::new(0));
    let (visible, sent, queried) = (accepted.clone(), prompts.clone(), lookups.clone());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut line = String::new();
            if BufReader::new(&stream).read_line(&mut line).unwrap_or(0) == 0 {
                continue;
            }
            let r: Value = serde_json::from_str(&line).unwrap();
            let result = match r["op"].as_str().unwrap() {
                "state" => json!({"protocol":1,"sessions":[{"id":SESSION,"running":false}],
                    "control":{"version":1,"domain":"app","provider":"deepseek","epoch":"app-launch",
                        "capabilities":["prompt","prompt.lookup","cancel","approve","answer"]}}),
                "prompt" => {
                    assert_eq!(r["control"]["domain"], "app");
                    assert_eq!(r["control"]["operationId"], "op-1");
                    assert_eq!(
                        r["requestId"],
                        muster_agent::plugin_control::prompt_id(SESSION, "op-1")
                    );
                    sent.fetch_add(1, Ordering::SeqCst);
                    continue; // Lost reply after host acceptance.
                }
                "prompt.lookup" => {
                    queried.fetch_add(1, Ordering::SeqCst);
                    let records = if visible.load(Ordering::SeqCst) {
                        vec![
                            json!({"source":{"kind":"user","rpcId":r["requestId"]},"location":"session-event"}),
                        ]
                    } else {
                        Vec::new()
                    };
                    json!({"sessionId":r["sessionId"],"requestId":r["requestId"],"records":records})
                }
                op => panic!("unexpected mutation {op}"),
            };
            let mut reply = json!({"id":r["id"],"ok":true,"result":result});
            if let Some(control) = r.get("control") {
                reply["control"] = control.clone();
            }
            let _ = writeln!(stream, "{reply}");
        }
    });
    let agent = daemon.agent().unwrap().clone();
    let link = agent.deepseek_link().unwrap();
    let initial = agent.clone();
    tokio::task::spawn_blocking(move || {
        muster_agent::deepseek_app::refresh(&initial, &link, &Host)
    })
    .await
    .unwrap();
    let Resp::OpResult(first) = client.call(send()).await.unwrap() else {
        panic!()
    };
    assert_eq!(first.status, Status::Unknown as i32);
    assert_eq!(prompts.load(Ordering::SeqCst), 1);
    let Req::AgentSend(original) = send() else {
        panic!()
    };
    let mut changed = original.clone();
    changed.text.push('!');
    let before = lookups.load(Ordering::SeqCst);
    assert!(client.call(Req::AgentSendStatus(changed)).await.is_err());
    let mut absent = original.clone();
    absent.op_id = "absent".into();
    client.call(Req::AgentSendStatus(absent)).await.unwrap();
    assert_eq!(
        lookups.load(Ordering::SeqCst),
        before,
        "no plugin read for absent or mismatched rows"
    );

    // Rebuild the plugin link without its in-memory pending send; the native ID
    // is recovered from the original operation, not a second execution.
    accepted.store(true, Ordering::SeqCst);
    let fresh = muster_agent::deepseek_app::Link::new(socket);
    tokio::task::spawn_blocking(move || muster_agent::deepseek_app::refresh(&agent, &fresh, &Host))
        .await
        .unwrap();
    let peer = daemon
        .roster()
        .read()
        .unwrap()
        .state()
        .members
        .keys()
        .find(|id| **id != daemon.device_id())
        .cloned()
        .unwrap();
    drop(client);
    let client = connect(&daemon, peer);
    let Resp::OpResult(result) = client
        .call(Req::AgentSendStatus(original.clone()))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(result.status, Status::Applied as i32);
    assert_eq!(result.result_id, SESSION);
    assert!(matches!(
        daemon
            .store()
            .ledger()
            .outcome("op-1", "agent.send", &original.encode_to_vec())
            .unwrap(),
        muster_store::Outcome::Applied(_)
    ));
    let Resp::OpResult(repeated) = client.call(send()).await.unwrap() else {
        panic!()
    };
    assert_eq!(repeated.status, Status::Duplicate as i32);
    assert_eq!(prompts.load(Ordering::SeqCst), 1);
}
