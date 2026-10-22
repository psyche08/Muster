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
    pub pty: Arc<PtyService>,
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
    /// Stop never inspects cwd; a removed workspace must not prevent cleanup.
    pub async fn action(&self, agent: &str, cwd: &str, action: i32) -> Result<String, Error> {
        if !(0..=2).contains(&action) {
            return Err(Error::Protocol("invalid terminal action".into()));
        }
        let name = Self::name(agent)?;
        if action != 1 && (!Path::new(cwd).is_absolute() || !Path::new(cwd).is_dir()) {
            return Err(Error::Protocol(
                "Session working directory is missing or unavailable".into(),
            ));
        }
        let target = format!("={name}");
        let mut sessions = self.sessions.lock().await;
        let exists = self
            .run(&["has-session", "-t", &target])
            .await?
            .status
            .success();
        if action != 0 {
            if exists {
                let result = self.run(&["kill-session", "-t", &target]).await?;
                if !result.status.success() {
                    return Err(Error::Unavailable(
                        String::from_utf8_lossy(&result.stderr).into_owned(),
                    ));
                }
            }
            if let Some(id) = sessions.remove(agent) {
                let _ = self.pty.close(&id).await;
            }
            if action == 1 {
                return Ok(String::new());
            }
        } else if exists
            && let Some(id) = sessions.get(agent)
            && self.pty.sessions().await?.iter().any(|s| &s.id == id)
        {
            return Ok(id.clone());
        }
        let cwd = Path::new(cwd);
        if !cwd.is_absolute() || !cwd.is_dir() {
            return Err(Error::Protocol(
                "Session working directory is missing or unavailable".into(),
            ));
        }
        if !exists || action == 2 {
            let result = self
                .run(&[
                    "new-session",
                    "-d",
                    "-s",
                    &name,
                    "-c",
                    &cwd.to_string_lossy(),
                ])
                .await?;
            if !result.status.success() {
                return Err(Error::Unavailable(
                    String::from_utf8_lossy(&result.stderr).into_owned(),
                ));
            }
        }
        let mut args = self.args();
        args.extend(["attach-session".into(), "-t".into(), target]);
        let TerminalSession { id, .. } = self
            .pty
            .create_command(
                self.program.as_ref().unwrap(),
                &args,
                Some(cwd),
                &self.prefix,
            )
            .await?;
        sessions.insert(agent.into(), id.clone());
        Ok(id)
    }
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
                .pty
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
                .pty
                .history(id, 100)
                .await
                .unwrap()
                .iter()
                .map(|l| &l.text)
                .collect::<Vec<_>>()
        );
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
        service.pty.input(&terminal, b"sleep 30\r").await.unwrap();
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
        service.pty.input(&first, b"pwd\r").await.unwrap();
        wait_text(&service, &first, "work ' $(literal)").await;
        service
            .pty
            .input(
                &first,
                b"export MUSTER_TMUX_PROBE=preserved; printf '%s%s\\n' READY -ONE\r",
            )
            .await
            .unwrap();
        wait_text(&service, &first, "READY-ONE").await;
        // Recreating the Rust service attaches to the existing tmux shell.
        let resumed = TmuxService::new(root.join("s"));
        let second = resumed.action("agent-one", dir, 0).await.unwrap();
        wait_text(&resumed, &second, "READY-ONE").await;
        resumed
            .pty
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
        wait_text(&resumed, &restarted, "work").await;
        resumed
            .pty
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
            .pty
            .input(&other, b"printf 'OTHER-ALIVE\\n'\r")
            .await
            .unwrap();
        wait_text(&resumed, &other, "OTHER-ALIVE").await;
        resumed.action("agent-two", "", 1).await.unwrap();
    }
}
