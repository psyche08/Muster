//! A native-source fixture drives real local RPC without touching user windows.
use muster_proto::v1::{self, request::Req, response::Resp};
use muster_rpc::Client;
use muster_terminal::{
    Error, GhosttyPlatform, TerminalConfig, TerminalKind,
    ghostty::{GhosttyOs, Listed, Surface},
};
use musterd::{Daemon, DaemonConfig, DefaultPlatform};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct NativeFixture {
    inputs: Mutex<Vec<(String, String)>>,
    commands: Mutex<Vec<String>>,
    created: Mutex<usize>,
    closed: Mutex<Vec<String>>,
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
        Ok(vec![Listed {
            id: "opaque:existing".into(),
            title: "Fixture shell".into(),
            cwd: "/tmp".into(),
            window: "native-window".into(),
            tab: "native-tab".into(),
        }])
    }
    fn perform(&self, id: &str, action: &str) -> Result<(), Error> {
        self.inputs.lock().unwrap().push((id.into(), action.into()));
        Ok(())
    }
    fn open(&self, id: &str) -> Result<Box<dyn Surface>, Error> {
        assert_eq!(id, "opaque:existing");
        Ok(Box::new(Screen))
    }
    fn create(&self) -> Result<String, Error> {
        let mut n = self.created.lock().unwrap();
        *n += 1;
        Ok(format!("created-{n}"))
    }
    fn create_command(&self, command: &str) -> Result<String, Error> {
        self.commands.lock().unwrap().push(command.into());
        self.create()
    }
    fn close(&self, id: &str) -> Result<(), Error> {
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
