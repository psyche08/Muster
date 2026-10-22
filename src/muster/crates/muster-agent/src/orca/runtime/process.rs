//! Bounded JSONL transport and owned child lifetime. No shell and no Node shim.
use super::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{oneshot, watch};

pub(super) struct Process {
    stdin: tokio::sync::Mutex<tokio::process::ChildStdin>,
    waiting: Mutex<HashMap<String, oneshot::Sender<Result<Value, Error>>>>,
    next: AtomicU64,
    stop: watch::Sender<bool>,
    pub dead: AtomicBool,
    pub generation: String,
    claude: bool,
}
impl Process {
    pub fn spawn(
        program: &Path,
        args: &[String],
        cwd: &str,
        claude: bool,
        owner: Weak<NativeRuntime>,
        session: String,
        env: &[(String, String)],
    ) -> Result<Arc<Self>, Error> {
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(args)
            .current_dir(cwd)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        // A launchd PATH is often minimal. Resolve the executable ourselves and
        // keep project-local node_modules and cwd out of executable lookup.
        cmd.env_remove("CLAUDECODE")
            .env_remove("CLAUDE_CODE_ENTRYPOINT");
        // The session's own environment, already validated by the caller.
        // It is applied last so it wins over the daemon's, which is the
        // point: these are the variables this task needs and the daemon
        // cannot hold, such as a per-task credential. Values are never
        // logged here or anywhere else in this layer.
        for (name, value) in env {
            cmd.env(name, value);
        }
        #[cfg(unix)]
        cmd.process_group(0);
        let mut child = cmd
            .spawn()
            .map_err(|_| unavailable("CLI agent could not start; check its installation"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| unavailable("CLI output unavailable"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| unavailable("CLI input unavailable"))?;
        let (stop, mut stopped) = watch::channel(false);
        let process = Arc::new(Self {
            stdin: tokio::sync::Mutex::new(stdin),
            waiting: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            stop,
            dead: AtomicBool::new(false),
            generation: random(),
            claude,
        });
        let weak = Arc::downgrade(&process);
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                let read = async {
                    (&mut reader)
                        .take(8 * 1024 * 1024 + 1)
                        .read_line(&mut line)
                        .await
                };
                tokio::select! {
                    _ = stopped.changed() => break,
                    _ = child.wait() => break,
                    result = read => {
                        if !matches!(result, Ok(n) if n > 0 && n <= 8 * 1024 * 1024) { break; }
                        let Ok(frame) = serde_json::from_str::<Value>(&line) else { break; };
                        let Some(p) = weak.upgrade() else { break; };
                        if p.deliver(&frame) { continue; }
                        if let Some(runtime) = owner.upgrade() {
                            if runtime.event(&session, &p.generation, &frame).is_err() { break; }
                        } else { break; }
                    }
                }
            }
            if let Some(p) = weak.upgrade() {
                p.dead.store(true, Ordering::Release);
                for (_, sender) in p.waiting.lock().unwrap().drain() {
                    let _ = sender.send(Err(unknown()));
                }
                if let Some(runtime) = owner.upgrade() {
                    runtime.exited(&session, &p.generation);
                }
            }
            // The child remains unreaped until its process group is stopped,
            // avoiding signalling a reused PID. No PID from a state file is used.
            #[cfg(unix)]
            if let Some(pid) = child.id() {
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGTERM);
                }
            }
            if tokio::time::timeout(Duration::from_secs(3), child.wait())
                .await
                .is_err()
            {
                #[cfg(unix)]
                if let Some(pid) = child.id() {
                    unsafe {
                        libc::kill(-(pid as i32), libc::SIGKILL);
                    }
                }
                let _ = child.kill().await;
                let _ = child.wait().await;
            }
        });
        Ok(process)
    }
    fn deliver(&self, frame: &Value) -> bool {
        let (key, value, failed) = if self.claude && frame["type"] == "control_response" {
            (
                frame["response"]["request_id"].as_str().map(str::to_owned),
                frame["response"]["response"].clone(),
                frame["response"]["subtype"] == "error",
            )
        } else if !self.claude && frame.get("method").is_none() && frame.get("id").is_some() {
            (
                Some(frame["id"].to_string()),
                frame["result"].clone(),
                frame.get("error").is_some(),
            )
        } else if self.claude && frame["type"] == "user" {
            // --replay-user-messages acknowledges the exact submitted UUID.
            (
                frame["uuid"].as_str().map(|s| format!("echo:{s}")),
                frame.clone(),
                false,
            )
        } else {
            return false;
        };
        if let Some(key) = key
            && let Some(sender) = self.waiting.lock().unwrap().remove(&key)
        {
            let _ = sender.send(if failed {
                Err(Error::Conflict("CLI agent refused the request".into()))
            } else {
                Ok(value)
            });
            true
        } else {
            false
        }
    }
    pub async fn write(&self, frame: Value) -> Result<(), Error> {
        if self.dead.load(Ordering::Acquire) {
            return Err(unavailable("CLI process exited"));
        }
        let mut bytes = serde_json::to_vec(&frame).map_err(|_| unknown())?;
        bytes.push(b'\n');
        tokio::time::timeout(Duration::from_secs(10), async {
            self.stdin
                .lock()
                .await
                .write_all(&bytes)
                .await
                .map_err(|_| unknown())
        })
        .await
        .map_err(|_| unknown())?
    }
    pub async fn request(&self, method: &str, params: Value) -> Result<Value, Error> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let key = id.to_string();
        let frame = if self.claude {
            json!({"type":"control_request","request_id":key,"request":params})
        } else {
            json!({"id":id,"method":method,"params":params})
        };
        self.exchange(key, frame).await
    }
    pub async fn exchange(&self, key: String, frame: Value) -> Result<Value, Error> {
        let (sender, receiver) = oneshot::channel();
        self.waiting.lock().unwrap().insert(key.clone(), sender);
        let result = async {
            self.write(frame).await?;
            tokio::time::timeout(Duration::from_secs(30), receiver)
                .await
                .map_err(|_| unknown())?
                .map_err(|_| unknown())?
        }
        .await;
        self.waiting.lock().unwrap().remove(&key);
        result
    }
    pub fn stop(&self) {
        let _ = self.stop.send(true);
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}
