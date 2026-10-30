//! Durable pending native sessions. A scheduler ID is never a guessed provider ID.
use crate::{Host, StopReason, TurnEvent};
use async_trait::async_trait;
use muster_proto::v1::{self, request::Req, response::Resp};
use muster_rpc::Client;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tokio::sync::mpsc;

#[derive(Serialize, Deserialize)]
struct Binding {
    provider: String,
    cwd: String,
    session: String,
    phase: String,
}
#[derive(Clone)]
pub struct NativeHost {
    client: Client,
    provider: String,
    root: PathBuf,
    brief: String,
    _credential: Option<std::sync::Arc<TaskCredential>>,
}
impl NativeHost {
    pub fn new(
        client: Client,
        provider: &str,
        root: PathBuf,
        credential: Option<std::sync::Arc<TaskCredential>>,
    ) -> Self {
        let brief=credential.as_ref().map(|c|format!("For this Multica task, use {} instead of the bare multica command. This task-scoped launcher supplies your existing task credentials; do not read or display its contents.",shell_quote(&c.path.to_string_lossy()))).unwrap_or_default();
        Self {
            client,
            provider: provider.into(),
            root,
            brief,
            _credential: credential,
        }
    }
    fn path(&self, id: &str) -> Result<PathBuf, String> {
        let key = id
            .strip_prefix("muster-native-")
            .ok_or("Not a native scheduler session")?;
        if key.len() != 32 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Invalid native scheduler session".into());
        }
        Ok(self.root.join(format!("{key}.json")))
    }
    fn load(&self, id: &str) -> Result<Binding, String> {
        let b: Binding = serde_json::from_slice(
            &std::fs::read(self.path(id)?)
                .map_err(|_| "Native binding is missing; refusing a fresh conversation")?,
        )
        .map_err(|_| "Native binding is unreadable")?;
        if b.provider != self.provider {
            return Err("Native provider does not match its binding".into());
        }
        Ok(b)
    }
    fn save(&self, id: &str, b: &Binding) -> Result<(), String> {
        let path = self.path(id)?;
        let tmp = path.with_extension(format!("{}.tmp", nonce()));
        write_private(&tmp, &serde_json::to_vec(b).map_err(|e| e.to_string())?)?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
    async fn messages(&self, id: &str) -> Result<Vec<v1::AgentMessage>, String> {
        match self
            .client
            .call(Req::AgentMessages(v1::AgentMessagesRequest {
                session: id.into(),
                tail: 2000,
                ..Default::default()
            }))
            .await
            .map_err(|e| e.to_string())?
        {
            Resp::AgentMessages(page) => Ok(page.messages),
            Resp::Error(e) => Err(e.message),
            _ => Err("Unexpected native history response".into()),
        }
    }
    async fn control(&self, request: Req) -> Result<String, String> {
        match self.client.call(request).await.map_err(|e| e.to_string())? {
            Resp::OpResult(op) => {
                let id = op.result_id.clone();
                crate::host_rpc::accept_op(op)?;
                Ok(id)
            }
            Resp::Error(e) => Err(e.message),
            _ => Err("Unexpected native operation response".into()),
        }
    }
}
pub(crate) fn nonce() -> String {
    use rand::RngCore;
    let mut b = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut b);
    b.iter().map(|v| format!("{v:02x}")).collect()
}
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .and_then(|mut f| {
            f.write_all(bytes)?;
            f.sync_all()
        })
        .map_err(|e| e.to_string())
}
struct Fence(PathBuf);
impl Drop for Fence {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[async_trait]
impl Host for NativeHost {
    async fn create(&self, cwd: &str) -> Result<String, String> {
        let path = std::fs::canonicalize(cwd).map_err(|_| "Scheduler directory does not exist")?;
        if !path.is_dir() {
            return Err("Scheduler directory is not a directory".into());
        }
        std::fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        let current = self.root.join("current");
        if let Ok(id) = std::fs::read_to_string(&current) {
            let b = self.load(&id)?;
            if b.cwd != path.to_string_lossy() {
                return Err("Task binding directory changed".into());
            }
            return Ok(id);
        }
        let id = format!("muster-native-{}", nonce());
        self.save(
            &id,
            &Binding {
                provider: self.provider.clone(),
                cwd: path.to_string_lossy().into_owned(),
                session: String::new(),
                phase: "pending".into(),
            },
        )?;
        if write_private(&current, id.as_bytes()).is_err() {
            return std::fs::read_to_string(current).map_err(|e| e.to_string());
        }
        Ok(id)
    }
    async fn resume(&self, id: &str) -> Result<String, String> {
        let b = self.load(id)?;
        if b.phase != "ready" || b.session.is_empty() {
            return Err(
                "Native turn is pending or unconfirmed; inspect it before continuing".into(),
            );
        }
        Ok(id.into())
    }
    async fn prompt(&self, id: &str, text: &str) -> Result<mpsc::Receiver<TurnEvent>, String> {
        let path = self.path(id)?.with_extension("busy");
        write_private(&path, b"native turn ownership")?;
        let fence = Fence(path);
        let mut b = self.load(id)?;
        if !matches!(b.phase.as_str(), "pending" | "ready") {
            return Err("A prior native submission is unconfirmed; never replay it".into());
        }
        let baseline = if b.session.is_empty() {
            Vec::new()
        } else {
            self.messages(&b.session).await?
        };
        let before: std::collections::HashSet<_> = baseline.iter().map(|m| m.id.clone()).collect();
        // A task-local launcher path is nonsecret. Credentials are never in this text.
        let prompt = if self.brief.is_empty() {
            text.to_owned()
        } else {
            format!("{text}\n\n{}", self.brief)
        };
        b.phase = "attempted".into();
        self.save(id, &b)?;
        if b.session.is_empty() {
            b.session = self
                .control(Req::AgentNativeCreate(v1::AgentNativeCreateRequest {
                    op_id: format!("native-create-{id}"),
                    provider: b.provider.clone(),
                    directory: b.cwd.clone(),
                    prompt: prompt.clone(),
                }))
                .await?;
            if b.session.is_empty() {
                return Err("Native creation returned no provider identity".into());
            }
            self.save(id, &b)?;
        } else {
            self.control(Req::AgentSend(v1::AgentSendRequest {
                op_id: format!("native-turn-{}", nonce()),
                session: b.session.clone(),
                text: prompt.clone(),
                ..Default::default()
            }))
            .await?;
        }
        let host = self.clone();
        let alias = id.to_owned();
        let (tx, rx) = mpsc::channel(256);
        tokio::spawn(async move {
            let _fence = fence;
            let mut seen = HashMap::<String, String>::new();
            let mut started = false;
            loop {
                if tx.is_closed() {
                    break;
                }
                let rows = match host.messages(&b.session).await {
                    Ok(rows) => rows,
                    Err(e) => {
                        let _ = tx.send(TurnEvent::Settled(StopReason::Error(e))).await;
                        break;
                    }
                };
                let mut settled = false;
                for row in rows {
                    if before.contains(&row.id) {
                        continue;
                    }
                    if row.role == "user"
                        && row.kind == "text"
                        && row.text.trim_end() == prompt.trim_end()
                    {
                        started = true;
                        continue;
                    }
                    if !started {
                        continue;
                    }
                    if row.role == "assistant" && row.kind == "text" {
                        let old = seen.entry(row.id.clone()).or_default();
                        if let Some(delta) = row.text.strip_prefix(old.as_str())
                            && !delta.is_empty()
                        {
                            let _ = tx.send(TurnEvent::Text(delta.into())).await;
                        }
                        *old = row.text;
                        if row.phase == "final_answer" {
                            settled = true;
                        }
                    } else if row.kind == "tool_call" && !seen.contains_key(&row.id) {
                        seen.insert(row.id, String::new());
                        let _ = tx
                            .send(TurnEvent::Tool(
                                row.text.lines().next().unwrap_or_default().into(),
                            ))
                            .await;
                    }
                }
                if settled {
                    b.phase = "ready".into();
                    let result = host
                        .save(&alias, &b)
                        .map(|_| StopReason::EndTurn)
                        .unwrap_or_else(StopReason::Error);
                    let _ = tx.send(TurnEvent::Settled(result)).await;
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }
        });
        Ok(rx)
    }
    async fn interject(&self, _: &str, _: &str) -> Result<(), String> {
        Err("Native mid-turn steering is not supported".into())
    }
    async fn interrupt(&self, id: &str) -> Result<(), String> {
        let b = self.load(id)?;
        if b.session.is_empty() {
            return Err("Native creation has no confirmed identity; do not replay".into());
        }
        self.control(Req::AgentInterrupt(v1::AgentInterruptRequest {
            op_id: format!("native-stop-{}", nonce()),
            session: b.session,
        }))
        .await
        .map(|_| ())
    }
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
/// The only secret-bearing file is private and lives as long as its active turn.
pub struct TaskCredential {
    path: PathBuf,
}
impl TaskCredential {
    /// Restart revokes any task context left by the previous bridge process.
    /// Only files with our exact random name grammar are candidates.
    pub fn revoke_stale(root: &Path) -> Result<(), String> {
        let entries = match std::fs::read_dir(root) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.to_string()),
        };
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(key) = name.strip_prefix("multica-")
                && key.len() == 32
                && key.bytes().all(|b| b.is_ascii_hexdigit())
                && entry.file_type().map_err(|e| e.to_string())?.is_file()
            {
                std::fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
    pub fn create(
        root: &Path,
        cli: &Path,
        env: &HashMap<String, String>,
    ) -> Result<Option<std::sync::Arc<Self>>, String> {
        if env.is_empty() {
            return Ok(None);
        }
        if !cli.is_file() {
            return Err("Host Multica CLI is missing".into());
        }
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        let mut script = String::from("#!/bin/sh\n");
        for (k, v) in env {
            if !k.starts_with("MULTICA_")
                || !k
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            {
                return Err("Invalid task environment name".into());
            }
            script.push_str(&format!("export {k}={}\n", shell_quote(v)));
        }
        script.push_str("exec ");
        script.push_str(&shell_quote(&cli.to_string_lossy()));
        script.push_str(" \"$@\"\n");
        let path = root.join(format!("multica-{}", nonce()));
        write_private(&path, script.as_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        Ok(Some(std::sync::Arc::new(Self { path })))
    }
}
impl Drop for TaskCredential {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn unconfirmed_first_turn_survives_reconnect_and_new_session_request() {
        let root = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let (pipe, _peer) = tokio::io::duplex(1024);
        let host = NativeHost::new(Client::new(pipe), "codex", root.path().into(), None);
        let id = host.create(cwd.path().to_str().unwrap()).await.unwrap();
        let mut b = host.load(&id).unwrap();
        b.phase = "attempted".into();
        host.save(&id, &b).unwrap();
        let (pipe, _peer) = tokio::io::duplex(1024);
        let reconnected = NativeHost::new(Client::new(pipe), "codex", root.path().into(), None);
        assert_eq!(
            reconnected
                .create(cwd.path().to_str().unwrap())
                .await
                .unwrap(),
            id
        );
        assert!(reconnected.resume(&id).await.is_err());
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            reconnected.prompt(&id, "hello"),
        )
        .await
        .unwrap();
        assert!(result.is_err());
        assert_eq!(reconnected.load(&id).unwrap().phase, "attempted");
        assert!(reconnected.load("muster-native-../../escape").is_err());
    }
    #[test]
    #[cfg(unix)]
    fn task_credential_is_private_child_scoped_and_removed_on_last_owner_drop() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let cli = root.path().join("cli");
        std::fs::write(&cli,b"#!/bin/sh\n[ \"$MULTICA_TOKEN\" = \"fixture-token\" ] && [ \"$1\" = \"literal argument\" ]\n").unwrap();
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
        let credential = TaskCredential::create(
            &root.path().join("contexts"),
            &cli,
            &HashMap::from([("MULTICA_TOKEN".into(), "fixture-token".into())]),
        )
        .unwrap()
        .unwrap();
        let path = credential.path.clone();
        let held = credential.clone();
        assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o700);
        assert!(
            std::process::Command::new(&path)
                .arg("literal argument")
                .status()
                .unwrap()
                .success()
        );
        drop(credential);
        assert!(path.exists());
        drop(held);
        assert!(!path.exists());
        assert!(
            TaskCredential::create(
                root.path(),
                &cli,
                &HashMap::from([("MULTICA_X;invalid".into(), "value".into())])
            )
            .is_err()
        );
    }
}
