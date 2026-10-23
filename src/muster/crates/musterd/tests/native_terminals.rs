//! A native-source fixture drives real local RPC without touching user windows.
use muster_proto::v1::{self, request::Req, response::Resp};
use muster_rpc::Client;
use muster_terminal::{
    Error, GhosttyPlatform, TerminalConfig, TerminalKind,
    ghostty::{GhosttyOs, Listed, Surface},
};
use musterd::{Daemon, DaemonConfig, DefaultPlatform};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct NativeFixture {
    inputs: Mutex<Vec<(String, String)>>,
    commands: Mutex<Vec<String>>,
    created: Mutex<usize>,
    closed: Mutex<Vec<String>>,
    create_outcome: AtomicU8,
    deny: AtomicBool,
    inventory_delay_ms: AtomicU64,
}
impl NativeFixture {
    fn allowed(&self) -> Result<(), Error> {
        if self.deny.load(Ordering::SeqCst) {
            Err(Error::Unavailable(
                "fixture Automation permission denied".into(),
            ))
        } else {
            Ok(())
        }
    }
}
struct Screen;
impl Surface for Screen {
    fn text(&mut self) -> Result<Option<String>, Error> {
        Ok(Some("fixture screen".into()))
    }
}
impl GhosttyOs for NativeFixture {
    fn running(&self) -> bool {
        true
    }
    fn terminals(&self) -> Result<Vec<Listed>, Error> {
        self.allowed()?;
        std::thread::sleep(std::time::Duration::from_millis(
            self.inventory_delay_ms.load(Ordering::SeqCst),
        ));
        Ok(vec![Listed {
            id: "opaque:existing".into(),
            title: "Fixture shell".into(),
            cwd: "/tmp".into(),
            window: "native-window".into(),
            tab: "native-tab".into(),
        }])
    }
    fn perform(&self, id: &str, action: &str) -> Result<(), Error> {
        self.allowed()?;
        self.inputs.lock().unwrap().push((id.into(), action.into()));
        Ok(())
    }
    fn open(&self, id: &str) -> Result<Box<dyn Surface>, Error> {
        self.allowed()?;
        assert_eq!(id, "opaque:existing");
        Ok(Box::new(Screen))
    }
    fn create(&self) -> Result<String, Error> {
        self.allowed()?;
        let mut n = self.created.lock().unwrap();
        *n += 1;
        match self.create_outcome.load(Ordering::SeqCst) {
            1 => Err(Error::Unavailable("fixture lost creation receipt".into())),
            2 => Ok(String::new()),
            _ => Ok(format!("created-{n}")),
        }
    }
    fn create_command(&self, command: &str) -> Result<String, Error> {
        self.commands.lock().unwrap().push(command.into());
        self.create()
    }
    fn close(&self, id: &str) -> Result<(), Error> {
        self.allowed()?;
        self.closed.lock().unwrap().push(id.into());
        Ok(())
    }
}

#[tokio::test]
async fn native_inventory_routes_existing_sessions_and_creation_is_deduplicated() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = Arc::new(NativeFixture::default());
    let mut config = DaemonConfig::new(dir.path());
    config.agent = None;
    config.cloud = false;
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.terminal = Some(TerminalKind::Iterm2(TerminalConfig {
        socket: dir.path().join("absent-iterm.sock"),
        ..Default::default()
    }));
    config.additional_terminals = vec![TerminalKind::Ghostty(GhosttyPlatform(fixture.clone()))];
    let daemon = Daemon::start_with(config, Arc::new(DefaultPlatform), None)
        .await
        .unwrap();
    let client = Client::new(
        tokio::net::UnixStream::connect(daemon.socket_path())
            .await
            .unwrap(),
    );
    let Resp::TerminalSessions(list) = client
        .call(Req::TerminalSessions(v1::TerminalSessionsRequest::default()))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        list.sources
            .iter()
            .map(|s| s.source.as_str())
            .collect::<Vec<_>>(),
        ["iterm2", "ghostty"]
    );
    assert_eq!(list.sessions[0].id, "ghostty:opaque:existing");
    assert_eq!(list.errors.len(), 1);
    assert!(list.errors[0].starts_with("iterm2:"));
    client
        .call(Req::TerminalInput(v1::TerminalInputRequest {
            op_id: "fixture-input".into(),
            session: list.sessions[0].id.clone(),
            bytes: b"hello".to_vec(),
        }))
        .await
        .unwrap();
    assert_eq!(fixture.inputs.lock().unwrap()[0].0, "opaque:existing");
    let Resp::TerminalLines(lines) = client
        .call(Req::TerminalHistory(v1::TerminalHistoryRequest {
            session: list.sessions[0].id.clone(),
            lines: 10,
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(lines.lines[0].text, "fixture screen");
    let request = Req::TerminalCreate(v1::TerminalCreateRequest {
        source: "ghostty".into(),
        op_id: "fixture-create".into(),
        ..Default::default()
    });
    for _ in 0..2 {
        let Resp::TerminalSession(created) = client.call(request.clone()).await.unwrap() else {
            panic!()
        };
        assert_eq!(created.id, "ghostty:created-1");
        assert_eq!(created.source, "ghostty");
    }
    assert_eq!(*fixture.created.lock().unwrap(), 1);
    assert!(
        client
            .call(Req::TerminalCreate(v1::TerminalCreateRequest {
                source: "unknown".into(),
                op_id: "fixture-unknown".into(),
                ..Default::default()
            }))
            .await
            .is_err()
    );
    assert!(
        client
            .call(Req::TerminalCreate(v1::TerminalCreateRequest {
                source: "ghostty".into(),
                managed_session: "tmux-invalid".into(),
                op_id: "fixture-stale".into(),
                ..Default::default()
            }))
            .await
            .is_err()
    );
    assert_eq!(*fixture.created.lock().unwrap(), 1);
    assert!(fixture.commands.lock().unwrap().is_empty());
    client
        .call(Req::TerminalClose(v1::TerminalCloseRequest {
            session: list.sessions[0].id.clone(),
        }))
        .await
        .unwrap();
    assert_eq!(*fixture.closed.lock().unwrap(), ["opaque:existing"]);
    daemon.shutdown().await;
}

#[tokio::test]
async fn a_finished_inventory_watch_delivers_its_final_source_error() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = DaemonConfig::new(dir.path());
    config.agent = None;
    config.cloud = false;
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.terminal = Some(TerminalKind::Iterm2(TerminalConfig {
        socket: dir.path().join("missing.sock"),
        ..Default::default()
    }));
    let daemon = Daemon::start_with(config, Arc::new(DefaultPlatform), None)
        .await
        .unwrap();
    let client = Client::new(
        tokio::net::UnixStream::connect(daemon.socket_path())
            .await
            .unwrap(),
    );
    let mut call = client
        .start(Req::TerminalSessions(v1::TerminalSessionsRequest {
            watch: true,
        }))
        .await
        .unwrap();
    let mut saw_error = false;
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while let Some(event) = call.events.recv().await {
            if let v1::event::Ev::TerminalSessions(list) = event {
                saw_error |= !list.errors.is_empty();
            }
        }
    })
    .await
    .unwrap();
    call.finish().await.unwrap();
    assert!(saw_error);
    daemon.shutdown().await;
}

async fn fixture_client(
    dir: &std::path::Path,
    fixture: Arc<NativeFixture>,
) -> (Arc<Daemon>, Client) {
    let mut config = DaemonConfig::new(dir);
    config.agent = None;
    config.cloud = false;
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.terminal = Some(TerminalKind::Ghostty(GhosttyPlatform(fixture)));
    let daemon = Daemon::start_with(config, Arc::new(DefaultPlatform), None)
        .await
        .unwrap();
    let client = Client::new(
        tokio::net::UnixStream::connect(daemon.socket_path())
            .await
            .unwrap(),
    );
    (daemon, client)
}
fn status(response: Resp) -> v1::op_result::Status {
    let Resp::OpResult(result) = response else {
        panic!("{response:?}")
    };
    v1::op_result::Status::try_from(result.status).unwrap()
}

#[tokio::test]
async fn ambiguous_creation_and_conflicting_replay_never_open_a_second_window() {
    use v1::op_result::Status;
    for outcome in [0, 1, 2] {
        let dir = tempfile::tempdir().unwrap();
        let fixture = Arc::new(NativeFixture::default());
        fixture.create_outcome.store(outcome, Ordering::SeqCst);
        let (daemon, client) = fixture_client(dir.path(), fixture.clone()).await;
        let request = v1::TerminalCreateRequest {
            source: "ghostty".into(),
            op_id: "same-create".into(),
            ..Default::default()
        };
        let (a, b) = tokio::join!(
            client.call(Req::TerminalCreate(request.clone())),
            client.call(Req::TerminalCreate(request.clone()))
        );
        for response in [a.unwrap(), b.unwrap()] {
            if outcome == 0 {
                // A concurrent in-flight duplicate can be Unknown; a completed
                // duplicate returns the exact receipt. Neither may resubmit.
                match response {
                    Resp::TerminalSession(s) => assert_eq!(s.id, "ghostty:created-1"),
                    other => assert_eq!(status(other), Status::Unknown),
                }
            } else {
                assert_eq!(status(response), Status::Unknown);
            }
        }
        let replay = client
            .call(Req::TerminalCreate(request.clone()))
            .await
            .unwrap();
        if outcome == 0 {
            assert!(matches!(replay, Resp::TerminalSession(s) if s.id == "ghostty:created-1"));
        } else {
            assert_eq!(status(replay), Status::Unknown);
        }
        let mut conflict = request;
        conflict.profile = "different-profile".into();
        assert_eq!(
            status(client.call(Req::TerminalCreate(conflict)).await.unwrap()),
            Status::Rejected
        );
        assert_eq!(*fixture.created.lock().unwrap(), 1);
        assert!(fixture.inputs.lock().unwrap().is_empty());
        daemon.shutdown().await;
    }
}

#[tokio::test]
async fn input_replay_id_conflicts_and_invalid_routes_have_no_extra_side_effects() {
    use v1::op_result::Status;
    let dir = tempfile::tempdir().unwrap();
    let fixture = Arc::new(NativeFixture::default());
    let (daemon, client) = fixture_client(dir.path(), fixture.clone()).await;
    let input = v1::TerminalInputRequest {
        session: "ghostty:opaque:existing".into(),
        bytes: b"hello\r".to_vec(),
        op_id: "same-input".into(),
    };
    assert_eq!(
        status(
            client
                .call(Req::TerminalInput(input.clone()))
                .await
                .unwrap()
        ),
        Status::Applied
    );
    assert_eq!(
        status(
            client
                .call(Req::TerminalInput(input.clone()))
                .await
                .unwrap()
        ),
        Status::Duplicate
    );
    let mut conflict = input;
    conflict.bytes = b"different".to_vec();
    assert_eq!(
        status(client.call(Req::TerminalInput(conflict)).await.unwrap()),
        Status::Rejected
    );
    assert_eq!(
        fixture.inputs.lock().unwrap().as_slice(),
        [("opaque:existing".into(), "text:hello\\x0d".into())]
    );
    for id in ["ghostty:", "iterm2:fixture", "pty:fixture"] {
        assert!(
            client
                .call(Req::TerminalInput(v1::TerminalInputRequest {
                    session: id.into(),
                    bytes: b"never".to_vec(),
                    op_id: format!("invalid-{id}")
                }))
                .await
                .is_err()
        );
    }
    assert!(
        client
            .call(Req::TerminalCreate(v1::TerminalCreateRequest {
                source: "ghostty".into(),
                ..Default::default()
            }))
            .await
            .is_err(),
        "explicit host requires operation ID"
    );
    assert!(
        client
            .call(Req::TerminalResize(v1::TerminalResizeRequest {
                session: "ghostty:opaque:existing".into(),
                cols: 80,
                rows: 24,
            }))
            .await
            .is_err(),
        "unsupported Ghostty resize must not be acknowledged"
    );
    assert_eq!(fixture.inputs.lock().unwrap().len(), 1);
    assert_eq!(*fixture.created.lock().unwrap(), 0);
    assert!(fixture.closed.lock().unwrap().is_empty());
    // Old clients may still use an unqualified opaque ID on the primary host.
    client
        .call(Req::TerminalInput(v1::TerminalInputRequest {
            session: "opaque:existing".into(),
            bytes: b"legacy".to_vec(),
            op_id: "legacy".into(),
        }))
        .await
        .unwrap();
    assert_eq!(fixture.inputs.lock().unwrap()[1].0, "opaque:existing");
    daemon.shutdown().await;
}

#[tokio::test]
async fn native_attach_qualifies_frames_and_cancellation_does_not_close_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = Arc::new(NativeFixture::default());
    let (daemon, client) = fixture_client(dir.path(), fixture.clone()).await;
    let mut call = client
        .start(Req::TerminalAttach(v1::TerminalAttachRequest {
            session: "ghostty:opaque:existing".into(),
            since_rev: 9,
        }))
        .await
        .unwrap();
    let event = tokio::time::timeout(std::time::Duration::from_secs(2), call.events.recv())
        .await
        .unwrap()
        .unwrap();
    let v1::event::Ev::TerminalFrame(frame) = event else {
        panic!()
    };
    assert_eq!(frame.session, "ghostty:opaque:existing");
    assert!(frame.full);
    assert_eq!(frame.rev, 10);
    call.cancel().await;
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), call.finish())
        .await
        .unwrap();
    assert!(fixture.closed.lock().unwrap().is_empty());
    assert!(fixture.inputs.lock().unwrap().is_empty());
    daemon.shutdown().await;
}

#[tokio::test]
async fn simulated_permission_denials_are_visible_and_never_synthesize_success() {
    use v1::op_result::Status;
    let dir = tempfile::tempdir().unwrap();
    let fixture = Arc::new(NativeFixture::default());
    fixture.deny.store(true, Ordering::SeqCst);
    let (daemon, client) = fixture_client(dir.path(), fixture.clone()).await;
    let Resp::TerminalSessions(list) = client
        .call(Req::TerminalSessions(v1::TerminalSessionsRequest::default()))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert!(list.sessions.is_empty());
    assert_eq!(list.sources[0].source, "ghostty");
    assert!(list.errors[0].contains("permission denied"));
    assert_eq!(
        status(
            client
                .call(Req::TerminalInput(v1::TerminalInputRequest {
                    session: "ghostty:opaque:existing".into(),
                    bytes: b"never".to_vec(),
                    op_id: "denied-input".into(),
                }))
                .await
                .unwrap()
        ),
        Status::Rejected
    );
    for req in [
        Req::TerminalHistory(v1::TerminalHistoryRequest {
            session: "ghostty:opaque:existing".into(),
            lines: 10,
        }),
        Req::TerminalAttach(v1::TerminalAttachRequest {
            session: "ghostty:opaque:existing".into(),
            since_rev: 0,
        }),
        Req::TerminalClose(v1::TerminalCloseRequest {
            session: "ghostty:opaque:existing".into(),
        }),
    ] {
        assert!(client.call(req).await.is_err());
    }
    assert!(fixture.inputs.lock().unwrap().is_empty());
    assert!(fixture.closed.lock().unwrap().is_empty());
    // A fresh read after the OS becomes available recovers without new identity.
    fixture.deny.store(false, Ordering::SeqCst);
    let Resp::TerminalSessions(list) = client
        .call(Req::TerminalSessions(v1::TerminalSessionsRequest::default()))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert!(list.errors.is_empty());
    assert_eq!(list.sessions[0].id, "ghostty:opaque:existing");
    daemon.shutdown().await;
}

#[tokio::test]
async fn a_slow_inventory_is_bounded_and_reports_its_source() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = Arc::new(NativeFixture::default());
    fixture.inventory_delay_ms.store(2200, Ordering::SeqCst);
    let (daemon, client) = fixture_client(dir.path(), fixture).await;
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(4),
        client.call(Req::TerminalSessions(v1::TerminalSessionsRequest::default())),
    )
    .await
    .unwrap()
    .unwrap();
    let Resp::TerminalSessions(list) = response else {
        panic!()
    };
    assert!(list.sessions.is_empty());
    assert_eq!(list.errors.len(), 1);
    assert!(list.errors[0].starts_with("ghostty:"));
    assert!(list.errors[0].contains("timed out"));
    daemon.shutdown().await;
}
