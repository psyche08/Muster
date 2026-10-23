//! Native sources share a namespace only at the RPC boundary. Adapter IDs stay opaque.
use crate::{Daemon, Error};
use muster_proto::v1;
use muster_terminal::Terminal;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::sync::mpsc;

pub(crate) fn source<'a>(daemon: &'a Daemon, source: &str) -> Result<&'a Arc<Terminal>, Error> {
    if source.is_empty() {
        return daemon.terminal().ok_or_else(|| {
            Error::Unavailable("No native terminal is configured on this device".into())
        });
    }
    daemon
        .terminals()
        .find(|t| t.source() == source)
        .ok_or_else(|| {
            Error::Invalid(format!(
                "Terminal source {source:?} is not configured on this device"
            ))
        })
}

pub(crate) fn resolve<'a>(
    daemon: &'a Daemon,
    id: &'a str,
) -> Result<(&'a Arc<Terminal>, &'a str), Error> {
    if let Some((host, raw)) = id.split_once(':')
        && matches!(host, "iterm2" | "ghostty" | "pty")
    {
        if raw.is_empty() {
            return Err(Error::Invalid("Empty terminal ID".into()));
        }
        return Ok((source(daemon, host)?, raw));
    }
    Ok((source(daemon, "")?, id))
}

pub(crate) fn qualify(mut session: v1::TerminalSession, source: &str) -> v1::TerminalSession {
    session.id = format!("{source}:{}", session.id);
    session.source = source.into();
    session
}

pub(crate) fn sources(daemon: &Daemon) -> Vec<v1::TerminalSource> {
    daemon
        .terminals()
        .map(|host| {
            let source = host.source();
            v1::TerminalSource {
                source: source.into(),
                label: match source {
                    "iterm2" => "iTerm2",
                    "ghostty" => "Ghostty",
                    "pty" => "Built-in terminal",
                    _ => source,
                }
                .into(),
            }
        })
        .collect()
}

fn snapshot(
    sources: &[v1::TerminalSource],
    lists: &BTreeMap<String, Result<Vec<v1::TerminalSession>, String>>,
) -> v1::TerminalSessionList {
    let mut result = v1::TerminalSessionList {
        sources: sources.to_vec(),
        ..Default::default()
    };
    for (source, list) in lists {
        match list {
            Ok(list) => result
                .sessions
                .extend(list.iter().cloned().map(|s| qualify(s, source))),
            Err(error) => result.errors.push(format!("{source}: {error}")),
        }
    }
    result
}

pub(crate) async fn list(daemon: &Daemon) -> v1::TerminalSessionList {
    let mut tasks = tokio::task::JoinSet::new();
    for terminal in daemon.terminals() {
        let terminal = terminal.clone();
        tasks.spawn(async move {
            let result = tokio::time::timeout(Duration::from_secs(2), terminal.sessions())
                .await
                .map_err(|_| {
                    "Terminal inventory timed out; check the application's API/Automation setup"
                        .to_string()
                })
                .and_then(|r| r.map_err(|e| e.to_string()));
            (terminal.source().to_string(), result)
        });
    }
    let mut lists = BTreeMap::new();
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok((source, list)) => {
                lists.insert(source, list);
            }
            Err(error) => {
                lists.insert("terminal".into(), Err(error.to_string()));
            }
        }
    }
    snapshot(&sources(daemon), &lists)
}

/// Each source retains its event/poll implementation. A failed source never
/// hides the other source; dropping this future aborts all owned watchers.
pub(crate) async fn watch(daemon: &Daemon, out: mpsc::Sender<v1::TerminalSessionList>) {
    let sources = sources(daemon);
    let (tx, mut rx) = mpsc::channel(16);
    let mut tasks = tokio::task::JoinSet::new();
    for terminal in daemon.terminals() {
        let terminal = terminal.clone();
        let tx = tx.clone();
        tasks.spawn(async move {
            let source = terminal.source().to_string();
            let (updates, mut incoming) = mpsc::channel(1);
            let watch = terminal.watch_sessions(updates);
            tokio::pin!(watch);
            loop {
                tokio::select! {
                    _ = tx.closed() => break,
                    result = &mut watch => {
                        if let Err(error) = result { let _ = tx.send((source.clone(), Err(error.to_string()))).await; }
                        break;
                    }
                    Some(list) = incoming.recv() => {
                        if tx.send((source.clone(), Ok(list))).await.is_err() { break; }
                    }
                }
            }
        });
    }
    drop(tx);
    let mut lists = BTreeMap::new();
    if out.send(snapshot(&sources, &lists)).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            _ = out.closed() => break,
            next = rx.recv() => match next {
                Some((source, list)) => {
                    lists.insert(source, list);
                    if out.send(snapshot(&sources, &lists)).await.is_err() { break; }
                }
                None => break,
            }
        }
    }
}

/// Only an explicit create may launch a native application. Inventory/attach
/// do not launch apps or silently substitute a different host.
pub(crate) async fn prepare(terminal: &Terminal) -> Result<(), Error> {
    if terminal.available() {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        let bundle = match terminal.source() {
            "iterm2" => "com.googlecode.iterm2",
            "ghostty" => "com.mitchellh.ghostty",
            _ => return Ok(()),
        };
        // Launch Services reuses a running app; no app preferences are changed.
        let status = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::process::Command::new("/usr/bin/open")
                .args(["-g", "-b", bundle])
                .kill_on_drop(true)
                .status(),
        )
        .await
        .map_err(|_| Error::Unavailable("Terminal application launch timed out".into()))?
        .map_err(|e| Error::Unavailable(e.to_string()))?;
        if !status.success() {
            return Err(Error::Unavailable(format!(
                "Could not open {}",
                terminal.source()
            )));
        }
        for _ in 0..30 {
            if terminal.available() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err(Error::Unavailable(format!(
            "{} opened but its terminal API is unavailable; complete its local setup first",
            terminal.source()
        )))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = terminal;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    #[ignore = "requires installed tmux; isolated native fixture and socket"]
    async fn native_managed_window_uses_exact_handle_and_replay_opens_only_once() {
        use muster_proto::v1::{request::Req, response::Resp};
        use muster_terminal::ghostty::{GhosttyOs, Listed, Surface};
        use std::sync::Mutex;
        struct Host(Arc<Mutex<Vec<String>>>);
        impl GhosttyOs for Host {
            fn running(&self) -> bool {
                true
            }
            fn terminals(&self) -> Result<Vec<Listed>, muster_terminal::Error> {
                Ok(vec![])
            }
            fn perform(&self, _: &str, _: &str) -> Result<(), muster_terminal::Error> {
                panic!("Never type the command into an existing window")
            }
            fn open(&self, _: &str) -> Result<Box<dyn Surface>, muster_terminal::Error> {
                unreachable!()
            }
            fn create(&self) -> Result<String, muster_terminal::Error> {
                panic!("Managed creation requires an initial command")
            }
            fn create_command(&self, command: &str) -> Result<String, muster_terminal::Error> {
                self.0.lock().unwrap().push(command.into());
                Ok("native-fixture".into())
            }
            fn close(&self, _: &str) -> Result<(), muster_terminal::Error> {
                Ok(())
            }
        }
        let commands = Arc::new(Mutex::new(Vec::new()));
        let dir = tempfile::tempdir_in("/tmp").unwrap();
        let mut config = crate::DaemonConfig::new(dir.path());
        config.agent = None;
        config.cloud = false;
        config.bind = "127.0.0.1:0".parse().unwrap();
        config.terminal = Some(muster_terminal::TerminalKind::Ghostty(
            muster_terminal::GhosttyPlatform(Arc::new(Host(commands.clone()))),
        ));
        let daemon = Daemon::start_with(config, Arc::new(crate::DefaultPlatform), None)
            .await
            .unwrap();
        let Terminal::Tmux(tmux) = daemon.tmux.as_ref() else {
            unreachable!()
        };
        let handle = tmux
            .action("fixture", dir.path().to_str().unwrap(), 0)
            .await
            .unwrap();
        let client = muster_rpc::Client::new(
            tokio::net::UnixStream::connect(daemon.socket_path())
                .await
                .unwrap(),
        );
        let create = Req::TerminalCreate(v1::TerminalCreateRequest {
            source: "ghostty".into(),
            managed_session: handle.clone(),
            op_id: "native-fixture-create".into(),
            ..Default::default()
        });
        for _ in 0..2 {
            let Resp::TerminalSession(session) = client.call(create.clone()).await.unwrap() else {
                panic!()
            };
            assert_eq!(session.id, "ghostty:native-fixture");
        }
        assert_eq!(commands.lock().unwrap().len(), 1);
        assert!(commands.lock().unwrap()[0].starts_with("/bin/sh -c "));
        assert!(commands.lock().unwrap()[0].contains(&handle));
        let mut attach = client
            .start(Req::TerminalAttach(v1::TerminalAttachRequest {
                session: handle.clone(),
                since_rev: 0,
            }))
            .await
            .unwrap();
        let frame = tokio::time::timeout(Duration::from_secs(3), attach.events.recv())
            .await
            .unwrap()
            .unwrap();
        let v1::event::Ev::TerminalFrame(frame) = frame else {
            panic!()
        };
        assert_eq!(frame.session, handle);
        assert_eq!(frame.native_sources[0].source, "ghostty");
        attach.cancel().await;
        client
            .call(Req::TerminalClose(v1::TerminalCloseRequest {
                session: "ghostty:native-fixture".into(),
            }))
            .await
            .unwrap();
        assert!(tmux.native_command(&handle).await.is_ok());
        let replacement = tmux
            .action("fixture", dir.path().to_str().unwrap(), 2)
            .await
            .unwrap();
        assert_ne!(replacement, handle);
        let stale = Req::TerminalCreate(v1::TerminalCreateRequest {
            source: "ghostty".into(),
            managed_session: handle,
            op_id: "native-fixture-stale".into(),
            ..Default::default()
        });
        assert!(client.call(stale).await.is_err());
        assert_eq!(commands.lock().unwrap().len(), 1);
        tmux.action("fixture", "", 1).await.unwrap();
        daemon.shutdown().await;
    }
    #[test]
    fn sources_with_colliding_ids_are_distinct_and_partial_errors_are_visible() {
        let list = vec![v1::TerminalSession {
            id: "same:id".into(),
            title: "same title".into(),
            ..Default::default()
        }];
        let mut lists = BTreeMap::from([
            ("iterm2".into(), Ok(list.clone())),
            ("ghostty".into(), Ok(list)),
        ]);
        let result = snapshot(&[], &lists);
        assert_eq!(result.sessions[0].id, "ghostty:same:id");
        assert_eq!(result.sessions[1].id, "iterm2:same:id");
        lists.insert("iterm2".into(), Err("API disabled".into()));
        let result = snapshot(&[], &lists);
        assert_eq!(result.sessions.len(), 1);
        assert_eq!(result.errors, ["iterm2: API disabled"]);
    }
}
