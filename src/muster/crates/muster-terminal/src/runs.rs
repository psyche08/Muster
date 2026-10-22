//! Bounded execution records for user-selected shell blocks. Each run owns one
//! tmux window; it never types into an interactive pane that may be busy.
use crate::{Error, tmux::TmuxService};
use muster_proto::{
    Message,
    v1::{AgentMessage, TerminalRun, TerminalRunRequest},
};
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
const OUTPUT_LIMIT: usize = 32 * 1024;

use crate::commands::shell;
fn id(text: &str) -> String {
    muster_proto::history::prefix_hash(&[AgentMessage {
        text: text.into(),
        ..Default::default()
    }])
    .iter()
    .map(|b| format!("{b:02x}"))
    .collect()
}
fn io(e: std::io::Error) -> Error {
    Error::Unavailable(e.to_string())
}
fn write(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let tmp = path.with_extension("tmp");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(io)?;
    file.write_all(bytes).map_err(io)?;
    file.sync_all().map_err(io)?;
    std::fs::rename(tmp, path).map_err(io)
}
impl TmuxService {
    fn run_root(&self, agent: &str) -> PathBuf {
        self.socket
            .parent()
            .unwrap_or(Path::new("."))
            .join("terminal-runs")
            .join(id(agent))
    }
    pub async fn start_run(
        &self,
        cwd: &str,
        request: &TerminalRunRequest,
        delivered: &[String],
        return_owner: &str,
    ) -> Result<String, Error> {
        let interpreter = shell(&request.language)
            .ok_or_else(|| Error::Protocol("Choose a sh, bash, shell or zsh code block".into()))?;
        if request.op_id.is_empty()
            || request.op_id.len() > 256
            || request.code.trim().is_empty()
            || request.code.len() > 16384
            || request.code.contains('\0')
        {
            return Err(Error::Protocol("Invalid or oversized shell command".into()));
        }
        if request.cwd != cwd {
            return Err(Error::Protocol(
                "Working directory changed; review the command again".into(),
            ));
        }
        let run_id = id(&format!("{}:{}", request.session, request.op_id));
        let _lock = self.runs.lock().await;
        let dir = self.run_root(&request.session).join(&run_id);
        // A retry must never execute again after a partial start or lost reply.
        if dir.exists() {
            return Err(Error::Unavailable(
                "This run already exists; refresh its recorded outcome".into(),
            ));
        }
        let previous = self.run_list_unlocked(&request.session).await?;
        if previous.len() >= 32 {
            // Rotate only an older result with a confirmed provider receipt.
            // Pending/unknown outcomes remain available for reconciliation.
            let old=previous.iter().rev().find(|r|r.state=="completed" && delivered.contains(&r.id))
                .ok_or_else(||Error::Unavailable("32 results are retained; return completed results before starting another run".into()))?;
            let old_dir = self.run_root(&request.session).join(&old.id);
            if let Ok(pane) = std::fs::read_to_string(old_dir.join("pane")) {
                let identity = self
                    .run(&[
                        "display-message",
                        "-p",
                        "-t",
                        pane.trim(),
                        "#{@muster-run-id}",
                    ])
                    .await?;
                if String::from_utf8_lossy(&identity.stdout).trim() == old.id {
                    let _ = self.run(&["kill-pane", "-t", pane.trim()]).await;
                }
            }
            std::fs::remove_dir_all(old_dir).map_err(io)?;
        }
        self.action(&request.session, cwd, 0).await?;
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)
            .map_err(io)?;
        let record = TerminalRun {
            id: run_id.clone(),
            session: request.session.clone(),
            code: request.code.clone(),
            language: request.language.clone(),
            source_message: request.source_message.clone(),
            cwd: cwd.into(),
            state: "starting".into(),
            return_owner: if request.auto_return {
                return_owner.into()
            } else {
                String::new()
            },
            started_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            ..Default::default()
        };
        write(&dir.join("run.pb"), &record.encode_to_vec())?;
        write(&dir.join("command"), request.code.as_bytes())?;
        write(&dir.join("runner"), b"n=0; while [ ! -f \"$1/go\" ]; do n=$((n+1)); [ \"$n\" -lt 1200 ] || exit 125; sleep 0.05; done\nexec \"$2\" \"$1/command\"\n")?;
        let target = format!("={}", Self::name(&request.session)?);
        let output = self
            .run(&[
                "new-window",
                "-P",
                "-F",
                "#{pane_id}",
                "-t",
                &target,
                "-n",
                &format!("run-{}", &run_id[..8]),
                "-c",
                cwd,
                "/bin/sh",
                &dir.join("runner").to_string_lossy(),
                &dir.to_string_lossy(),
                interpreter,
            ])
            .await?;
        if !output.status.success() {
            return Err(Error::Unavailable(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ));
        }
        let pane = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !pane.starts_with('%') || !pane[1..].bytes().all(|b| b.is_ascii_digit()) {
            return Err(Error::Protocol("Invalid tmux pane response".into()));
        }
        let setup = async {
            write(&dir.join("pane"), pane.as_bytes())?;
            let identity = self
                .run(&["set-option", "-p", "-t", &pane, "@muster-run-id", &run_id])
                .await?;
            if !identity.status.success() {
                return Err(Error::Unavailable("Could not bind terminal run".into()));
            }
            let set = self
                .run(&["set-option", "-w", "-t", &pane, "remain-on-exit", "on"])
                .await?;
            if !set.status.success() {
                return Err(Error::Unavailable(
                    "Could not retain terminal result".into(),
                ));
            }
            // Retain the pane without adding tmux's synthetic "Pane is dead"
            // notification to captured command output.
            let format = self
                .run(&["set-option", "-w", "-t", &pane, "remain-on-exit-format", ""])
                .await?;
            if !format.status.success() {
                return Err(Error::Unavailable(
                    "Could not configure terminal result capture".into(),
                ));
            }
            // The script starts only after its pane and recovery record are ready.
            write(&dir.join("go"), b"run")
        }
        .await;
        if let Err(error) = setup {
            let _ = self.run(&["kill-pane", "-t", &pane]).await;
            return Err(error);
        }
        Ok(run_id)
    }
    pub async fn run_list(&self, agent: &str) -> Result<Vec<TerminalRun>, Error> {
        let _lock = self.runs.lock().await;
        self.run_list_unlocked(agent).await
    }
    async fn run_list_unlocked(&self, agent: &str) -> Result<Vec<TerminalRun>, Error> {
        let root = self.run_root(agent);
        let entries = match std::fs::read_dir(&root) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(io(e)),
        };
        let mut results = vec![];
        for entry in entries.take(64) {
            let entry = entry.map_err(io)?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.len() != 64 || !name.bytes().all(|b| b.is_ascii_hexdigit()) {
                continue;
            }
            let dir = entry.path();
            let path = dir.join("run.pb");
            if std::fs::metadata(&path).map_err(io)?.len() > 128 * 1024 {
                continue;
            }
            let mut record = TerminalRun::decode(std::fs::read(&path).map_err(io)?.as_slice())
                .map_err(|e| Error::Protocol(e.to_string()))?;
            if record.session != agent || record.id != name {
                continue;
            }
            if !matches!(record.state.as_str(), "completed" | "unavailable") {
                if let Ok(pane) = std::fs::read_to_string(dir.join("pane")) {
                    let status = self
                        .run(&[
                            "display-message",
                            "-p",
                            "-t",
                            pane.trim(),
                            "#{pane_dead}:#{pane_dead_status}:#{history_size}:#{@muster-run-id}",
                        ])
                        .await?;
                    if status.status.success()
                        && String::from_utf8_lossy(&status.stdout)
                            .trim()
                            .ends_with(&format!(":{}", record.id))
                    {
                        let info = String::from_utf8_lossy(&status.stdout);
                        let fields: Vec<_> = info.trim().split(':').collect();
                        record.state = if fields.first() == Some(&"1") {
                            "completed"
                        } else {
                            "running"
                        }
                        .into();
                        record.exit_known = record.state == "completed"
                            && fields.get(1).and_then(|s| s.parse::<i32>().ok()).is_some();
                        record.exit_code = fields
                            .get(1)
                            .and_then(|s| s.parse().ok())
                            .unwrap_or_default();
                        let capture = self
                            .run(&["capture-pane", "-p", "-J", "-S", "-2000", "-t", pane.trim()])
                            .await?;
                        if capture.status.success() {
                            let text = String::from_utf8_lossy(&capture.stdout);
                            record.truncated = text.len() > OUTPUT_LIMIT
                                || fields
                                    .get(2)
                                    .and_then(|s| s.parse::<usize>().ok())
                                    .unwrap_or(0)
                                    >= 2000;
                            let mut start = text.len().saturating_sub(OUTPUT_LIMIT);
                            while !text.is_char_boundary(start) {
                                start += 1;
                            }
                            record.output = text[start..].trim_end().into();
                        } else {
                            record.state = "unavailable".into();
                            record.exit_known = false;
                        }
                    } else {
                        record.state = "unavailable".into();
                    }
                } else {
                    record.state = "unavailable".into();
                }
                if matches!(record.state.as_str(), "completed" | "unavailable") {
                    write(&path, &record.encode_to_vec())?;
                }
            }
            results.push(record);
        }
        results.sort_by(|a, b| (b.started_unix, &b.id).cmp(&(a.started_unix, &a.id)));
        Ok(results)
    }
}
