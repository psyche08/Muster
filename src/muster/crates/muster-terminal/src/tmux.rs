//! Agent-bound tmux sessions on a dedicated socket. They outlive UI attachments.
use crate::{
    Error, TerminalSession,
    pty::{PtyConfig, PtyService},
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

pub struct TmuxService {
    pty: Arc<PtyService>,
    pub(crate) socket: PathBuf,
    pub(crate) runs: Mutex<()>,
    program: Option<PathBuf>,
    sessions: Mutex<HashMap<String, String>>,
    prefix: String,
}
impl TmuxService {
    pub fn new(socket: PathBuf) -> Arc<Self> {
        let mut candidates = vec![
            PathBuf::from("/opt/homebrew/bin/tmux"),
            PathBuf::from("/usr/local/bin/tmux"),
            PathBuf::from("/usr/bin/tmux"),
        ];
        if let Some(path) = std::env::var_os("PATH") {
            candidates.extend(
                std::env::split_paths(&path)
                    .filter(|p| p.is_absolute())
                    .map(|p| p.join("tmux")),
            );
        }
        Arc::new(Self {
            pty: PtyService::new(PtyConfig::default()),
            socket,
            runs: Mutex::new(()),
            program: candidates.into_iter().find(|p| p.is_file()),
            sessions: Mutex::new(HashMap::new()),
            prefix: format!(
                "tmux-{}-{}-",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ),
        })
    }
    pub fn available(&self) -> bool {
        self.program.is_some()
    }
    fn args(&self) -> Vec<String> {
        vec![
            "-u".into(),
            "-f".into(),
            "/dev/null".into(),
            "-S".into(),
            self.socket.to_string_lossy().into_owned(),
        ]
    }
    pub(crate) async fn run(&self, args: &[&str]) -> Result<std::process::Output, Error> {
        let program = self.program.as_ref().ok_or_else(|| {
            Error::Unavailable("Install tmux on the session's device first".into())
        })?;
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(self.args())
            .args(args)
            .env_remove("TMUX")
            .kill_on_drop(true);
        tokio::time::timeout(Duration::from_secs(5), cmd.output())
            .await
            .map_err(|_| {
                Error::Unavailable(
                    "tmux command timed out; check the session before retrying".into(),
                )
            })?
            .map_err(|e| Error::Unavailable(format!("tmux: {e}")))
    }
    pub(crate) fn name(agent: &str) -> Result<String, Error> {
        if agent.is_empty() || agent.len() > 256 {
            return Err(Error::Protocol("invalid agent session id".into()));
        }
        Ok(format!(
            "muster-{}",
            agent
                .as_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        ))
    }
    /// Serialized start/restart/end, scoped to exactly one provider session.
    /// An existing shell can be resumed even after its original cwd is removed.
    pub async fn action(&self, agent: &str, cwd: &str, action: i32) -> Result<String, Error> {
        if !(0..=2).contains(&action) {
            return Err(Error::Protocol("invalid terminal action".into()));
        }
        let name = Self::name(agent)?;
        let target = format!("={name}");
        let mut sessions = self.sessions.lock().await;
        sessions.retain(|_, id| self.pty.contains(id));
        let exists = self
            .run(&["has-session", "-t", &target])
            .await?
            .status
            .success();
        // Validate before a destructive restart, but not before resume/end.
        if action != 1
            && (!exists || action == 2)
            && (!Path::new(cwd).is_absolute() || !Path::new(cwd).is_dir())
        {
            return Err(Error::Protocol(
                "Session working directory is missing or unavailable".into(),
            ));
        }
        if action != 0 {
            if exists {
                let old = self.handle(&target).await?;
                self.checked(&["kill-session", "-t", &target]).await?;
                if let Some(id) = sessions.remove(&old) {
                    let _ = self.pty.close(&id).await;
                }
            }
            if action == 1 {
                return Ok(String::new());
            }
        }
        if !exists || action == 2 {
            self.checked(&[
                "new-session",
                "-d",
                "-s",
                &name,
                "-c",
                cwd,
                "-x",
                "120",
                "-y",
                "40",
            ])
            .await?;
        }
        // Some tmux commands parse -t as a pane/window, others as a session.
        // Resolve once to its native ID instead of reinterpreting a name.
        let target = self
            .checked(&["display-message", "-p", "-t", &target, "#{session_id}"])
            .await?;
        let mut handle = self.handle(&target).await?;
        if handle.is_empty() {
            handle = format!(
                "tmux-v1-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            );
            self.checked(&["set-option", "-t", &target, "@muster_terminal_id", &handle])
                .await?;
        }
        Self::validate_handle(&handle)?;
        self.connect(&mut sessions, &handle, &target).await?;
        Ok(handle)
    }

    async fn checked(&self, args: &[&str]) -> Result<String, Error> {
        let result = self.run(args).await?;
        if !result.status.success() {
            return Err(Error::Unavailable(
                String::from_utf8_lossy(&result.stderr).trim().into(),
            ));
        }
        Ok(String::from_utf8_lossy(&result.stdout).trim().into())
    }

    async fn handle(&self, target: &str) -> Result<String, Error> {
        self.checked(&[
            "display-message",
            "-p",
            "-t",
            target,
            "#{@muster_terminal_id}",
        ])
        .await
    }

    fn validate_handle(handle: &str) -> Result<(), Error> {
        if !handle.starts_with("tmux-v1-")
            || handle.len() > 96
            || !handle
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(Error::NotFound(
                "managed terminal handle is invalid or obsolete; reconnect from its conversation"
                    .into(),
            ));
        }
        Ok(())
    }

    /// Lookup never creates a shell and never falls back to a title or cwd.
    async fn target(&self, handle: &str) -> Result<String, Error> {
        Self::validate_handle(handle)?;
        let result = self
            .run(&[
                "list-sessions",
                "-F",
                "#{@muster_terminal_id}\t#{session_name}\t#{session_id}",
            ])
            .await?;
        if result.status.success() {
            for row in String::from_utf8_lossy(&result.stdout).lines() {
                let parts: Vec<_> = row.split('\t').collect();
                if let [id, name, target] = parts.as_slice()
                    && *id == handle
                    && name.strip_prefix("muster-").is_some_and(|suffix| {
                        !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_hexdigit())
                    })
                    && target.strip_prefix('$').is_some_and(|suffix| {
                        !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit())
                    })
                {
                    return Ok((*target).into());
                }
            }
        }
        Err(Error::NotFound(
            "managed terminal ended; reconnect from its conversation".into(),
        ))
    }

    async fn connect(
        &self,
        sessions: &mut HashMap<String, String>,
        handle: &str,
        target: &str,
    ) -> Result<String, Error> {
        if let Some(id) = sessions.get(handle)
            && self.pty.contains(id)
        {
            return Ok(id.clone());
        }
        // The native client is permanently bound to this session instance. Do
        // not let tmux switch it to another session when this one is destroyed.
        let mut args = self.args();
        args.extend([
            "if-shell".into(),
            "-F".into(),
            "-t".into(),
            target.into(),
            format!("#{{==:#{{@muster_terminal_id}},{handle}}}"),
            format!("attach-session -t {}", shell_quote(target)),
            "display-message 'Muster session ended; reconnect from its conversation'".into(),
        ]);
        // detach-on-destroy is a session option: detach instead of switching.
        self.checked(&["set-option", "-t", target, "detach-on-destroy", "on"])
            .await?;
        let TerminalSession { id, .. } = self
            .pty
            .create_command(self.program.as_ref().unwrap(), &args, None, &self.prefix)
            .await?;
        sessions.insert(handle.into(), id.clone());
        Ok(id)
    }

    async fn resolve(&self, handle: &str) -> Result<String, Error> {
        Self::validate_handle(handle)?;
        let mut sessions = self.sessions.lock().await;
        if let Some(id) = sessions.get(handle)
            && self.pty.contains(id)
        {
            return Ok(id.clone());
        }
        sessions.remove(handle);
        let target = self.target(handle).await?;
        self.connect(&mut sessions, handle, &target).await
    }

    /// Commands are display-only and bound to the server-side generation. The
    /// conditional and attach execute on the same tmux command queue.
    async fn desktop_commands(&self, handle: &str) -> Result<(String, String), Error> {
        let target = self.target(handle).await?;
        let program = self
            .program
            .as_ref()
            .ok_or_else(|| Error::Unavailable("tmux is unavailable".into()))?;
        let mut args = vec![program.to_string_lossy().into_owned()];
        args.extend(self.args());
        let condition = format!("#{{==:#{{@muster_terminal_id}},{handle}}}");
        let attach = format!("attach-session -f ignore-size -t {}", shell_quote(&target));
        let tail = [
            "if-shell",
            "-F",
            "-t",
            &target,
            &condition,
            &attach,
            "display-message 'Muster session ended; reconnect from its conversation'",
        ]
        .iter()
        .map(|s| shell_quote(s))
        .collect::<Vec<_>>()
        .join(" ");
        let base = args
            .iter()
            .map(|s| shell_quote(s))
            .collect::<Vec<_>>()
            .join(" ");
        Ok((
            format!("env -u TMUX {base} {tail}"),
            format!("env -u TMUX {base} -CC {tail}"),
        ))
    }

    async fn public_sessions(&self, mut list: Vec<TerminalSession>) -> Vec<TerminalSession> {
        let bindings = self.sessions.lock().await;
        list.retain_mut(|session| {
            if let Some((handle, _)) = bindings.iter().find(|(_, id)| **id == session.id) {
                session.id = handle.clone();
                true
            } else {
                false
            }
        });
        list
    }

    pub async fn sessions(&self) -> Result<Vec<TerminalSession>, Error> {
        Ok(self.public_sessions(self.pty.sessions().await?).await)
    }

    pub async fn watch_sessions(
        &self,
        out: tokio::sync::mpsc::Sender<Vec<TerminalSession>>,
    ) -> Result<(), Error> {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let watcher = self.pty.watch_sessions(tx);
        tokio::pin!(watcher);
        loop {
            tokio::select! {
                _ = out.closed() => return Ok(()),
                result = &mut watcher => return result,
                Some(list) = rx.recv() => {
                    if out.send(self.public_sessions(list).await).await.is_err() { return Ok(()); }
                }
            }
        }
    }

    pub async fn attach(
        &self,
        handle: &str,
        since_rev: u64,
        out: tokio::sync::mpsc::Sender<muster_proto::v1::TerminalFrame>,
    ) -> Result<(), Error> {
        let id = self.resolve(handle).await?;
        let (desktop, iterm2) = self.desktop_commands(handle).await?;
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let reader = self.pty.attach(&id, since_rev, tx);
        tokio::pin!(reader);
        let decorate = |mut frame: muster_proto::v1::TerminalFrame| {
            frame.session = handle.into();
            if frame.full {
                frame.desktop_attach_command = desktop.clone();
                frame.iterm2_attach_command = iterm2.clone();
            }
            frame
        };
        loop {
            tokio::select! {
                _ = out.closed() => return Ok(()),
                result = &mut reader => {
                    while let Ok(frame) = rx.try_recv() {
                        if out.send(decorate(frame)).await.is_err() { break; }
                    }
                    return result;
                }
                Some(frame) = rx.recv() => {
                    if out.send(decorate(frame)).await.is_err() { return Ok(()); }
                }
            }
        }
    }

    pub async fn input(&self, handle: &str, bytes: &[u8]) -> Result<(), Error> {
        self.pty.input(&self.resolve(handle).await?, bytes).await
    }
    pub async fn resize(&self, handle: &str, cols: u32, rows: u32) -> Result<(), Error> {
        self.pty
            .resize(&self.resolve(handle).await?, cols, rows)
            .await
    }
    pub async fn history(
        &self,
        handle: &str,
        lines: u32,
    ) -> Result<Vec<muster_proto::v1::TerminalLine>, Error> {
        self.pty.history(&self.resolve(handle).await?, lines).await
    }
    /// A view close is detach only. AgentTerminal END is the explicit kill.
    pub async fn close(&self, handle: &str) -> Result<(), Error> {
        self.target(handle).await.map(|_| ())
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_are_exact_bounded_and_do_not_alias_shell_metacharacters() {
        assert_ne!(
            TmuxService::name("one:two").unwrap(),
            TmuxService::name("one/two").unwrap()
        );
        assert!(
            TmuxService::name("a; $(touch bad)")
                .unwrap()
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        );
        assert!(TmuxService::name("").is_err());
        assert!(TmuxService::name(&"a".repeat(257)).is_err());
    }
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let t = TmuxService::new(self.0.join("s"));
            if let Some(p) = &t.program {
                let _ = std::process::Command::new(p)
                    .args(t.args())
                    .arg("kill-server")
                    .output();
            }
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    async fn wait_text(service: &TmuxService, id: &str, expected: &str) {
        for _ in 0..100 {
            let text = service
                .history(id, 100)
                .await
                .unwrap()
                .into_iter()
                .map(|l| l.text)
                .collect::<Vec<_>>()
                .join("\n");
            if text.contains(expected) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        panic!(
            "terminal did not produce expected text: {expected}; {:?}",
            service
                .history(id, 100)
                .await
                .unwrap()
                .iter()
                .map(|l| &l.text)
                .collect::<Vec<_>>()
        );
    }
    #[tokio::test]
    #[ignore = "requires installed tmux; isolated socket and PTYs, no GUI automation"]
    async fn real_desktop_attach_commands_share_shell_and_reject_stale_generation() {
        let root = PathBuf::from(format!(
            "/tmp/muster-desktop-{} 'x",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let _cleanup = Cleanup(root.clone());
        let service = TmuxService::new(root.join("s"));
        let handle = service
            .action("desktop-test", root.to_str().unwrap(), 0)
            .await
            .unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let svc = service.clone();
        let id = handle.clone();
        let stream = tokio::spawn(async move { svc.attach(&id, 0, tx).await });
        let first = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(first.full);
        assert_eq!(first.session, handle);
        assert!(!first.desktop_attach_command.is_empty());
        assert!(!first.iterm2_attach_command.is_empty());
        let desktop = PtyService::new(PtyConfig::default());
        let mut clients = Vec::new();
        for command in [&first.desktop_attach_command, &first.iterm2_attach_command] {
            // Execute exactly the command the UI offers, including shell quoting.
            let client = desktop
                .create_command(
                    Path::new("/bin/sh"),
                    &["-c".into(), command.clone()],
                    None,
                    "desktop-",
                )
                .await
                .unwrap();
            clients.push(client.id);
        }
        let target = format!("={}", TmuxService::name("desktop-test").unwrap());
        let mut attached = false;
        for _ in 0..100 {
            let flags = service
                .checked(&["list-clients", "-t", &target, "-F", "#{client_flags}"])
                .await
                .unwrap();
            if flags
                .lines()
                .filter(|line| line.contains("ignore-size"))
                .count()
                == 2
            {
                attached = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        assert!(
            attached,
            "both normal and iTerm2 control-mode commands must attach"
        );
        service
            .input(&handle, b"printf '%s%s\\n' DESKTOP -SHARED\r")
            .await
            .unwrap();
        wait_text(&service, &handle, "DESKTOP-SHARED").await;
        for id in &clients {
            let mut seen = false;
            for _ in 0..100 {
                let output = desktop.history(id, 100).await.unwrap();
                if output
                    .iter()
                    .any(|line| line.text.contains("DESKTOP-SHARED"))
                {
                    seen = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
            assert!(seen, "desktop client must see shared output");
            desktop.close(id).await.unwrap();
        }
        drop(rx);
        tokio::time::timeout(Duration::from_secs(2), stream)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        service.close(&handle).await.unwrap();
        service
            .input(&handle, b"printf '%s%s\\n' STILL -RUNNING\r")
            .await
            .unwrap();
        wait_text(&service, &handle, "STILL-RUNNING").await;
        let new_handle = service
            .action("desktop-test", root.to_str().unwrap(), 2)
            .await
            .unwrap();
        assert_ne!(handle, new_handle);
        // A copied command from the old generation must exit without attaching.
        let stale = desktop
            .create_command(
                Path::new("/bin/sh"),
                &["-c".into(), first.desktop_attach_command],
                None,
                "stale-",
            )
            .await
            .unwrap();
        for _ in 0..100 {
            if !desktop.contains(&stale.id) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        assert!(
            !desktop.contains(&stale.id),
            "stale command must not attach to replacement"
        );
        service.action("desktop-test", "", 1).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires installed tmux; uses an isolated socket and temporary workspace"]
    async fn real_code_runs_preserve_cwd_exit_output_and_never_repeat() {
        use muster_proto::v1::TerminalRunRequest;
        let root = PathBuf::from(format!(
            "/tmp/muster-run-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let _cleanup = Cleanup(root.clone());
        let cwd = root.join("work ' $(literal)");
        std::fs::create_dir(&cwd).unwrap();
        let service = TmuxService::new(root.join("s"));
        assert!(service.available());
        let dir = cwd.to_str().unwrap();
        let terminal = service.action("original", dir, 0).await.unwrap();
        service.input(&terminal, b"sleep 30\r").await.unwrap();
        let request = TerminalRunRequest {
            session: "original".into(),
            op_id: "once".into(),
            auto_return: true,
            cwd: dir.into(),
            language: "sh".into(),
            code: "printf 'once\\n' >> count; pwd; printf 'RESULT-OK\\n'; exit 7".into(),
            ..Default::default()
        };
        assert!(
            service
                .start_run("/tmp", &request, &[], "phone")
                .await
                .is_err()
        );
        let id = service
            .start_run(dir, &request, &[], "phone")
            .await
            .unwrap();
        let resumed = TmuxService::new(root.join("s"));
        let mut completed = None;
        for _ in 0..100 {
            let records = resumed.run_list("original").await.unwrap();
            if let Some(r) = records
                .into_iter()
                .find(|r| r.id == id && r.state == "completed")
            {
                completed = Some(r);
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        let result = completed.expect("run should finish without disturbing busy original shell");
        assert_eq!(result.return_owner, "phone");
        assert!(result.exit_known);
        assert_eq!(result.exit_code, 7);
        assert!(result.output.contains(dir), "{}", result.output);
        assert!(result.output.contains("RESULT-OK"));
        assert_eq!(
            result.output.lines().count(),
            2,
            "Capture must contain command output only: {}",
            result.output
        );
        assert!(!result.truncated);
        assert!(
            resumed
                .start_run(dir, &request, &[], "phone")
                .await
                .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(cwd.join("count")).unwrap(),
            "once\n"
        );
        assert!(resumed.run_list("other").await.unwrap().is_empty());
        resumed.action("original", "", 1).await.unwrap();
        assert_eq!(
            resumed.run_list("original").await.unwrap()[0].output,
            result.output
        );
        let large=TerminalRunRequest {op_id:"large".into(),code:"i=0; while [ $i -lt 4000 ]; do printf 'line-%s-abcdefghijklmnopqrstuvwxyz\\n' \"$i\"; i=$((i+1)); done".into(),..request.clone()};
        let large_id = resumed.start_run(dir, &large, &[], "phone").await.unwrap();
        for _ in 0..100 {
            let runs = resumed.run_list("original").await.unwrap();
            let r = runs.iter().find(|r| r.id == large_id).unwrap();
            if r.state == "completed" {
                assert!(r.truncated);
                assert!(r.output.len() <= 32768);
                assert!(r.output.contains("line-3999"));
                return;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        panic!("large output run did not finish");
    }
    #[tokio::test]
    #[ignore = "requires an installed tmux; uses an isolated test socket"]
    async fn real_tmux_cwd_reconnect_restart_and_end() {
        let root = PathBuf::from(format!(
            "/tmp/muster-tmux-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let _cleanup = Cleanup(root.clone());
        let cwd = root.join("work ' $(literal)");
        std::fs::create_dir(&cwd).unwrap();
        let service = TmuxService::new(root.join("s"));
        assert!(service.available(), "install tmux before running this test");
        let dir = cwd.to_str().unwrap();
        let first = service.action("agent-one", dir, 0).await.unwrap();
        assert_eq!(service.action("agent-one", dir, 0).await.unwrap(), first);
        service.input(&first, b"pwd\r").await.unwrap();
        wait_text(&service, &first, "work ' $(literal)").await;
        service
            .input(
                &first,
                b"export MUSTER_TMUX_PROBE=preserved; printf '%s%s\\n' READY -ONE\r",
            )
            .await
            .unwrap();
        wait_text(&service, &first, "READY-ONE").await;
        // Recreating the Rust service attaches to the existing tmux shell.
        let resumed = TmuxService::new(root.join("s"));
        // A saved handle reconnects without an AgentTerminal start mutation.
        resumed
            .input(&first, b"printf 'RECOVERED=%s\\n' \"$MUSTER_TMUX_PROBE\"\r")
            .await
            .unwrap();
        wait_text(&resumed, &first, "RECOVERED=preserved").await;
        let second = resumed
            .action("agent-one", "/missing/old-cwd", 0)
            .await
            .unwrap();
        assert_eq!(first, second);
        resumed.close(&second).await.unwrap();
        resumed
            .input(&second, b"printf '%s%s\\n' DETACH -ALIVE\r")
            .await
            .unwrap();
        wait_text(&resumed, &second, "DETACH-ALIVE").await;
        wait_text(&resumed, &second, "READY-ONE").await;
        resumed
            .input(&second, b"printf 'VALUE=%s\\n' \"$MUSTER_TMUX_PROBE\"\r")
            .await
            .unwrap();
        wait_text(&resumed, &second, "VALUE=preserved").await;
        // Invalid restart must preserve the existing shell.
        assert!(
            resumed
                .action("agent-one", "/missing/muster-test-dir", 2)
                .await
                .is_err()
        );
        assert_eq!(resumed.action("agent-one", dir, 0).await.unwrap(), second);
        let restarted = resumed.action("agent-one", dir, 2).await.unwrap();
        assert_ne!(restarted, second);
        // A fresh daemon cannot turn an obsolete handle into the new shell.
        let fresh = TmuxService::new(root.join("s"));
        assert!(fresh.input(&second, b"echo MUST-NOT-RUN\r").await.is_err());
        wait_text(&resumed, &restarted, "work").await;
        resumed
            .input(
                &restarted,
                b"printf 'AFTER=%s\\n' \"${MUSTER_TMUX_PROBE:-fresh}\"\r",
            )
            .await
            .unwrap();
        wait_text(&resumed, &restarted, "AFTER=fresh").await;
        let other = resumed.action("agent-two", dir, 0).await.unwrap();
        wait_text(&resumed, &other, "work").await;
        resumed.action("agent-one", "", 1).await.unwrap();
        let target = format!("={}", TmuxService::name("agent-one").unwrap());
        assert!(
            !resumed
                .run(&["has-session", "-t", &target])
                .await
                .unwrap()
                .status
                .success()
        );
        resumed
            .input(&other, b"printf 'OTHER-ALIVE\\n'\r")
            .await
            .unwrap();
        wait_text(&resumed, &other, "OTHER-ALIVE").await;
        resumed.action("agent-two", "", 1).await.unwrap();
    }
}
