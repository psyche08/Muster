//! Bounded, transient diagnostics for explicit local-owner installation only.
use super::*;
use std::sync::Mutex;
use tokio::io::AsyncReadExt;

const LOG_LIMIT: usize = 64 * 1024;
const LINE_LIMIT: usize = 4096;

#[derive(Default)]
pub(super) struct Installation(Mutex<v1::MulticaInstallation>);
pub(super) struct Attempt<'a>(&'a Installation);
impl Drop for Attempt<'_> {
    fn drop(&mut self) {
        if self.0.snapshot().busy {
            self.0.finish(Some("Installation request ended before its outcome was confirmed; check status before retrying"));
        }
    }
}
impl Installation {
    pub fn snapshot(&self) -> v1::MulticaInstallation {
        self.0.lock().unwrap().clone()
    }
    pub fn begin(&self) -> Attempt<'_> {
        *self.0.lock().unwrap() = v1::MulticaInstallation {
            busy: true,
            ..Default::default()
        };
        self.stage("检查安装配置与 Apple 容器环境");
        Attempt(self)
    }
    pub fn stage(&self, stage: &str) {
        self.0.lock().unwrap().stage = stage.into();
        self.append(&format!("\n── {stage} ──"));
    }
    pub fn finish(&self, error: Option<&str>) {
        self.stage(if error.is_some() {
            "安装未完成，请检查日志后重试"
        } else {
            "安装完成，请在终端登录 Multica"
        });
        if let Some(error) = error {
            self.append(error);
        }
        let mut state = self.0.lock().unwrap();
        state.busy = false;
        state.finished = true;
        state.succeeded = error.is_none();
    }
    fn append(&self, line: &str) {
        // Installer output is untrusted. Suppress credential-bearing lines rather
        // than attempting to preserve pieces of a credential or quoted value.
        let mut text = String::new();
        let mut escape = false;
        for ch in line.chars().take(LINE_LIMIT) {
            if ch == '\u{1b}' {
                escape = true;
                continue;
            }
            if escape {
                if ch.is_ascii_alphabetic() {
                    escape = false;
                }
                continue;
            }
            if !ch.is_control() || ch == '\t' {
                text.push(ch);
            }
        }
        let lower = text.to_ascii_lowercase();
        let sensitive = [
            "token",
            "password",
            "secret",
            "authorization",
            "credential",
            "mul_",
        ]
        .iter()
        .any(|word| lower.contains(word));
        let cleaned = if sensitive {
            "[credential-bearing output omitted]".to_string()
        } else {
            text.split_whitespace()
                .map(|word| if word.contains("://") { "[URL]" } else { word })
                .collect::<Vec<_>>()
                .join(" ")
        };
        let mut state = self.0.lock().unwrap();
        state.log.push_str(&cleaned);
        state.log.push('\n');
        if state.log.len() > LOG_LIMIT {
            let mut cut = state.log.len() - LOG_LIMIT;
            while !state.log.is_char_boundary(cut) {
                cut += 1;
            }
            state.log.drain(..cut);
        }
    }
    async fn read(&self, mut pipe: impl tokio::io::AsyncRead + Unpin) -> Result<(), Error> {
        let mut buffer = [0; 1024];
        let mut line = Vec::new();
        let mut overflow = false;
        loop {
            let count = pipe.read(&mut buffer).await.map_err(io)?;
            if count == 0 {
                if !line.is_empty() || overflow {
                    self.line(&line, overflow);
                }
                return Ok(());
            }
            for byte in &buffer[..count] {
                if *byte == b'\n' || *byte == b'\r' {
                    if !line.is_empty() || overflow {
                        self.line(&line, overflow);
                    }
                    line.clear();
                    overflow = false;
                } else if line.len() < LINE_LIMIT {
                    line.push(*byte);
                } else {
                    overflow = true;
                }
            }
        }
    }
    fn line(&self, line: &[u8], overflow: bool) {
        // Never emit a truncated prefix that could hide a sensitive suffix.
        if overflow {
            self.append("[oversized output line omitted]");
        } else {
            self.append(&String::from_utf8_lossy(line));
        }
    }
    pub async fn run(
        &self,
        command: &mut tokio::process::Command,
        seconds: u64,
    ) -> Result<(), Error> {
        use std::process::Stdio;
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(io)?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let result = tokio::time::timeout(Duration::from_secs(seconds), async {
            let (out, err, status) =
                tokio::join!(self.read(stdout), self.read(stderr), child.wait());
            out?;
            err?;
            let status = status.map_err(io)?;
            self.append(&format!("Process exited: {status}"));
            if status.success() {
                Ok(())
            } else {
                Err(Error::Unavailable(format!(
                    "Installation command failed ({status}); see installation log"
                )))
            }
        })
        .await;
        match result {
            Ok(result) => result,
            Err(_) => {
                let _ = child.kill().await;
                Err(Error::Unavailable(
                    "Installation command timed out; check status before retrying".into(),
                ))
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn streams_both_pipes_before_exit_and_reports_failure() {
        let dir = tempfile::tempdir().unwrap();
        let release = dir.path().join("release");
        let log = Installation::default();
        let _attempt = log.begin();
        let mut command = tokio::process::Command::new("/bin/sh");
        command.args(["-c", "printf 'Downloading package\\n'; while [ ! -f \"$1\" ]; do sleep 0.02; done; printf 'Download failed\\n' >&2; exit 7", "fixture"])
            .arg(&release);
        let (result, ()) = tokio::join!(log.run(&mut command, 5), async {
            tokio::time::timeout(Duration::from_secs(3), async {
                while !log.snapshot().log.contains("Downloading package") {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            assert!(log.snapshot().busy);
            std::fs::write(&release, b"ready").unwrap();
        });
        assert!(result.is_err());
        log.finish(Some(&result.unwrap_err().to_string()));
        let state = log.snapshot();
        assert!(state.log.contains("Download failed"));
        assert!(state.log.contains("7"));
        assert!(state.finished && !state.busy && !state.succeeded);
    }

    #[tokio::test]
    async fn successful_command_and_timeout_have_distinct_outcomes() {
        let log = Installation::default();
        let _attempt = log.begin();
        log.run(
            tokio::process::Command::new("/bin/sh").args(["-c", "printf 'done\\n'"]),
            2,
        )
        .await
        .unwrap();
        log.finish(None);
        assert!(log.snapshot().succeeded);
        let _attempt = log.begin();
        assert!(!log.snapshot().log.contains("done"));
        let error = log
            .run(tokio::process::Command::new("/bin/sleep").arg("5"), 0)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("timed out"));
        log.finish(Some(&error.to_string()));
        assert!(!log.snapshot().succeeded);
    }

    #[tokio::test]
    async fn output_is_bounded_and_redacted_across_pipe_chunks() {
        use tokio::io::AsyncWriteExt;
        let log = Installation::default();
        let (mut write, read) = tokio::io::duplex(32);
        let (_, result) = tokio::join!(
            async {
                write.write_all(b"to\x1b[31mken=synthetic-value\nfetch https://example.invalid/a?key=value\n").await.unwrap();
                write.write_all(&vec![b'x'; LINE_LIMIT + 1]).await.unwrap();
                write.write_all(b"token=never-print\n").await.unwrap();
                for _ in 0..100 {
                    write
                        .write_all("安装中".repeat(200).as_bytes())
                        .await
                        .unwrap();
                    write.write_all(b"\n").await.unwrap();
                }
                write.write_all(b"final line").await.unwrap();
                drop(write);
            },
            log.read(read)
        );
        result.unwrap();
        let state = log.snapshot();
        assert!(state.log.len() <= LOG_LIMIT);
        assert!(state.log.ends_with("final line\n"));
        assert!(!state.log.contains("synthetic-value"));
        assert!(!state.log.contains("never-print"));
        // Test redaction independently of rolling-log eviction.
        log.append("to\u{1b}[31mken=synthetic-value");
        log.append("fetch https://example.invalid/a?key=value");
        log.line(&vec![b'x'; LINE_LIMIT], true);
        let text = log.snapshot().log;
        assert!(!text.contains("synthetic-value"));
        assert!(!text.contains("key=value"));
        assert!(text.contains("[URL]"));
        assert!(text.contains("[oversized output line omitted]"));
    }

    #[test]
    fn dropped_attempt_does_not_leave_progress_busy_or_claim_success() {
        let log = Installation::default();
        {
            let _attempt = log.begin();
        }
        let state = log.snapshot();
        assert!(state.finished && !state.busy && !state.succeeded);
        assert!(state.log.contains("before its outcome was confirmed"));
    }
}
