//! Rust port of Orca's host responsibilities: durable journal, process ownership,
//! generation fencing, and exact approval responses. The installed native CLI
//! provides inference; no Electron, JavaScript, Orca service or SDK is launched.
mod process;
pub(crate) mod program;
mod translate;
use super::{Error, Reply, Rpc, diagnostic, fingerprint, string};
use process::Process;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

#[derive(Clone, Serialize, Deserialize)]
struct Session {
    id: String,
    agent: String,
    cwd: String,
    provider_id: String,
    #[serde(default)]
    stream_message: String,
    generation: String,
    turn: String,
    state: String,
    #[serde(default)]
    created_at: u64,
}
struct Store {
    db: Connection,
    sessions: HashMap<String, Session>,
}
pub struct NativeRuntime {
    store: Mutex<Store>,
    running: tokio::sync::Mutex<HashMap<String, Arc<Process>>>,
    runtime: String,
    fence: u64,
    stopped: AtomicBool,
    owner: Weak<Self>,
    programs: HashMap<String, PathBuf>,
    /// Per-session environment for the provider child, by session id.
    ///
    /// Deliberately not part of `Session`: that is serialized into the
    /// journal, and these values carry a caller's credential. Keeping them
    /// here means a session resumed after a restart starts without them —
    /// the caller supplies them again — instead of launching a provider with
    /// a credential that expired while the daemon was down.
    session_env: Mutex<HashMap<String, Vec<(String, String)>>>,
}
impl NativeRuntime {
    pub fn open(dir: &Path) -> Result<Arc<Self>, Error> {
        Self::with_programs(dir, HashMap::new())
    }
    /// Dependency injection for protocol tests; production resolves trusted absolute CLI paths.
    pub fn with_programs(
        dir: &Path,
        programs: HashMap<String, PathBuf>,
    ) -> Result<Arc<Self>, Error> {
        std::fs::create_dir_all(dir)
            .map_err(|_| unavailable("CLI session directory unavailable"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| unavailable("CLI session permissions unavailable"))?;
        }
        let db = Connection::open(dir.join("sessions.db")).map_err(db_error)?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA locking_mode=EXCLUSIVE;
            CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS sessions (id TEXT PRIMARY KEY, data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS items (session TEXT, id TEXT, seq INTEGER PRIMARY KEY AUTOINCREMENT, revision INTEGER, observed INTEGER, body TEXT, UNIQUE(session,id));
            CREATE TABLE IF NOT EXISTS creates (operation TEXT PRIMARY KEY, directory TEXT, agent TEXT, session TEXT);").map_err(db_error)?;
        db.execute(
            "INSERT OR IGNORE INTO meta VALUES ('runtime', ?)",
            [random()],
        )
        .map_err(db_error)?;
        let runtime = db
            .query_row("SELECT value FROM meta WHERE key='runtime'", [], |r| {
                r.get(0)
            })
            .map_err(db_error)?;
        let fence = (rand::random::<u64>() & ((1_u64 << 53) - 1)).max(1);
        let sessions = {
            let mut query = db.prepare("SELECT data FROM sessions").map_err(db_error)?;
            query
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(db_error)?
                .map(|row| {
                    let mut s: Session = serde_json::from_str(&row.map_err(db_error)?)
                        .map_err(|_| unavailable("CLI session store is corrupt"))?;
                    // A persisted state never proves that an old child is alive.
                    s.generation.clear();
                    s.turn.clear();
                    s.state = "unknown".into();
                    Ok((s.id.clone(), s))
                })
                .collect::<Result<HashMap<_, _>, Error>>()?
        };
        let runtime = Arc::new_cyclic(|owner| Self {
            store: Mutex::new(Store { db, sessions }),
            running: tokio::sync::Mutex::new(HashMap::new()),
            runtime,
            fence,
            stopped: AtomicBool::new(false),
            owner: owner.clone(),
            programs,
            session_env: Mutex::new(HashMap::new()),
        });
        // Pending callbacks belonged to the previous process generation.
        let ids: Vec<_> = runtime
            .store
            .lock()
            .unwrap()
            .sessions
            .keys()
            .cloned()
            .collect();
        for id in ids {
            runtime.cancel_prompts(&id)?;
        }
        Ok(runtime)
    }
    fn program(&self, agent: &str) -> Result<PathBuf, Error> {
        if let Some(path) = self.programs.get(agent) {
            return Ok(path.clone());
        }
        if !matches!(agent, "codex" | "claude") {
            return Err(Error::Invalid("Unsupported CLI agent".into()));
        }
        if let Some(path) = program::resolve(agent) {
            return Ok(path);
        }
        Err(unavailable(
            "Install the native Codex or Claude CLI and sign in on this device; Node wrappers are not used",
        ))
    }
    fn session(&self, id: &str) -> Result<Session, Error> {
        self.store
            .lock()
            .unwrap()
            .sessions
            .get(id)
            .cloned()
            .ok_or_else(|| Error::NotFound("CLI session".into()))
    }
    fn update(&self, id: &str, change: impl FnOnce(&mut Session)) -> Result<(), Error> {
        let mut store = self.store.lock().unwrap();
        let mut s = store
            .sessions
            .get(id)
            .cloned()
            .ok_or_else(|| Error::NotFound("CLI session".into()))?;
        change(&mut s);
        store
            .db
            .execute(
                "UPDATE sessions SET data=? WHERE id=?",
                params![serde_json::to_string(&s).map_err(|_| unknown())?, id],
            )
            .map_err(db_error)?;
        store.sessions.insert(id.into(), s);
        Ok(())
    }
    fn item(&self, session: &str, id: &str, body: Value) -> Result<(), Error> {
        let store = self.store.lock().unwrap();
        let data = serde_json::to_string(&body).map_err(|_| unknown())?;
        if data.len() > 4 * 1024 * 1024 {
            return Err(unavailable("CLI journal item exceeds 4 MiB"));
        }
        store.db.execute("INSERT INTO items(session,id,revision,observed,body) VALUES (?,?,1,?,?) ON CONFLICT(session,id) DO UPDATE SET revision=revision+1,observed=excluded.observed,body=excluded.body", params![session,id,now() as i64,data]).map_err(db_error)?;
        Ok(())
    }
    fn items(&self, session: &str) -> Result<Vec<Value>, Error> {
        let store = self.store.lock().unwrap();
        let mut q = store.db.prepare("SELECT id,seq,revision,observed,body FROM items WHERE session=? ORDER BY seq LIMIT 10001").map_err(db_error)?;
        let rows = q
            .query_map([session], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })
            .map_err(db_error)?;
        let items = rows.map(|row| { let (id,seq,revision,observed,body) = row.map_err(db_error)?;
            let body: Value = serde_json::from_str(&body).map_err(|_| unavailable("CLI journal is corrupt"))?;
            Ok(json!({"itemId":id,"sequence":seq,"revision":revision,"observedAt":observed,"body":body}))
        }).collect::<Result<Vec<_>,Error>>()?;
        if items.len() > 10000 {
            return Err(unavailable("CLI history exceeds the 10000-item limit"));
        }
        Ok(items)
    }
    fn cancel_prompts(&self, id: &str) -> Result<(), Error> {
        for item in self.items(id)? {
            if item["body"]["resolution"]["state"] == "pending" {
                let mut body = item["body"].clone();
                body["resolution"]["state"] = json!("cancelled");
                self.item(id, item["itemId"].as_str().unwrap_or_default(), body)?;
            }
        }
        Ok(())
    }
    fn create(&self, params: &Value) -> Result<Value, Error> {
        let agent = string(params, "agent")?;
        self.program(&agent)?;
        let directory = string(params, "directory")?;
        let operation = string(params, "operation")?;
        let env = session_env(&params["env"])?;
        let path = Path::new(&directory);
        if !path.is_absolute() || !path.is_dir() {
            return Err(Error::Invalid(
                "Choose an existing absolute directory on the target device".into(),
            ));
        }
        let cwd = std::fs::canonicalize(path)
            .map_err(|_| unavailable("Directory unavailable"))?
            .to_str()
            .ok_or_else(|| Error::Invalid("Directory must be UTF-8".into()))?
            .to_owned();
        let mut store = self.store.lock().unwrap();
        let prior: Option<(String, String, String)> = store
            .db
            .query_row(
                "SELECT directory,agent,session FROM creates WHERE operation=?",
                [&operation],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(db_error)?;
        if let Some((dir, provider, id)) = prior {
            return if dir == cwd && provider == agent {
                // A replayed create answers with the session it already made.
                // Its environment lives only in memory, so take this call's:
                // after a restart this is the only chance to restore it, and
                // the ledger has already refused a replay with a different
                // body.
                if !env.is_empty() {
                    self.session_env
                        .lock()
                        .unwrap()
                        .insert(id.clone(), env.clone());
                }
                Ok(json!({"sessionId":id}))
            } else {
                Err(Error::Conflict("Creation operation was reused".into()))
            };
        }
        let id = random();
        let s = Session {
            id: id.clone(),
            agent: agent.clone(),
            cwd: cwd.clone(),
            provider_id: String::new(),
            stream_message: String::new(),
            generation: String::new(),
            turn: String::new(),
            state: "completed".into(),
            created_at: now(),
        };
        let tx = store.db.transaction().map_err(db_error)?;
        tx.execute(
            "INSERT INTO sessions VALUES (?,?)",
            params![id, serde_json::to_string(&s).map_err(|_| unknown())?],
        )
        .map_err(db_error)?;
        tx.execute(
            "INSERT INTO creates VALUES (?,?,?,?)",
            params![operation, cwd, agent, id],
        )
        .map_err(db_error)?;
        tx.commit().map_err(db_error)?;
        store.sessions.insert(id.clone(), s);
        if !env.is_empty() {
            self.session_env.lock().unwrap().insert(id.clone(), env);
        }
        Ok(json!({"sessionId":id}))
    }
    async fn process(&self, id: &str) -> Result<Arc<Process>, Error> {
        let mut running = self.running.lock().await;
        if let Some(p) = running.get(id).filter(|p| !p.dead.load(Ordering::Acquire)) {
            return Ok(p.clone());
        }
        if self.stopped.load(Ordering::Acquire) {
            return Err(unavailable("CLI session manager stopped"));
        }
        let s = self.session(id)?;
        let program = self.program(&s.agent)?;
        let mut args: Vec<String> = if s.agent == "codex" {
            vec!["app-server".into(), "--listen".into(), "stdio://".into()]
        } else {
            [
                "--print",
                "--verbose",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--include-partial-messages",
                "--replay-user-messages",
                "--permission-prompt-tool",
                "stdio",
                "--permission-mode",
                "default",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect()
        };
        if s.agent == "claude" && !s.provider_id.is_empty() {
            args.extend(["--resume".into(), s.provider_id.clone()]);
        }
        let env = self
            .session_env
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .unwrap_or_default();
        let process = Process::spawn(
            &program,
            &args,
            &s.cwd,
            s.agent == "claude",
            self.owner.clone(),
            id.into(),
            &env,
        )?;
        self.update(id, |s| s.generation = process.generation.clone())?;
        let result = async {
            if s.agent == "codex" {
                process.request("initialize",json!({"clientInfo":{"name":"muster","title":"Muster","version":"0.1.0"},"capabilities":{"experimentalApi":true}})).await?;
                process.write(json!({"method":"initialized","params":{}})).await?;
                let mut params=json!({"cwd":s.cwd,"approvalPolicy":"on-request","sandbox":"workspace-write"});
                let method=if s.provider_id.is_empty(){"thread/start"}else{params["threadId"]=json!(s.provider_id);"thread/resume"};
                let reply=process.request(method,params).await?;
                let thread=string(&reply["thread"],"id")?;
                self.update(id,|s|s.provider_id=thread)?;
            } else {
                process.request("initialize",json!({"subtype":"initialize","hooks":{}})).await?;
            }
            Ok::<_,Error>(())
        }.await;
        if let Err(error) = result {
            process.stop();
            return Err(error);
        }
        running.insert(id.into(), process.clone());
        Ok(process)
    }
    /// Drop every session environment. A session can be prompted again, and
    /// its child respawned, for as long as this runtime lives, so entries are
    /// kept until it shuts down rather than cleared when a child exits.
    fn forget_env(&self) {
        self.session_env.lock().unwrap().clear();
    }
    fn exited(&self, id: &str, generation: &str) {
        if self.session(id).is_ok_and(|s| s.generation == generation) {
            let _ = self.update(id, |s| {
                s.generation.clear();
                s.state = "unknown".into();
                s.turn.clear();
            });
            let _ = self.cancel_prompts(id);
        }
    }
}
use rusqlite::OptionalExtension;
/// Longest environment this session layer accepts, and the longest value.
/// A provider child needs a handful of task variables; these bound what a
/// caller can push into another process's environment.
const MAX_ENV_ENTRIES: usize = 32;
const MAX_ENV_VALUE: usize = 4096;

/// Validate a session's requested environment.
///
/// Refusals are by name so a caller learns what was wrong. Dynamic-linker
/// variables are refused outright: they change which code *every* binary the
/// child starts will load, which is a different and larger power than running
/// a command inside the session the caller already controls.
fn session_env(value: &Value) -> Result<Vec<(String, String)>, Error> {
    let map = match value {
        Value::Null => return Ok(Vec::new()),
        Value::Object(map) => map,
        _ => {
            return Err(Error::Invalid(
                "Session environment must be an object".into(),
            ));
        }
    };
    if map.len() > MAX_ENV_ENTRIES {
        return Err(Error::Invalid(format!(
            "Session environment holds at most {MAX_ENV_ENTRIES} entries"
        )));
    }
    let mut env = Vec::with_capacity(map.len());
    for (name, value) in map {
        let valid_name = !name.is_empty()
            && !name.starts_with(|c: char| c.is_ascii_digit())
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid_name {
            return Err(Error::Invalid(format!(
                "Environment name {name:?} must match [A-Za-z_][A-Za-z0-9_]*"
            )));
        }
        if name.starts_with("DYLD_") || name.starts_with("LD_") {
            return Err(Error::Invalid(format!(
                "Environment name {name:?} would redirect dynamic linking and is refused"
            )));
        }
        let text = value.as_str().ok_or_else(|| {
            Error::Invalid(format!("Environment value for {name:?} must be text"))
        })?;
        if text.len() > MAX_ENV_VALUE {
            return Err(Error::Invalid(format!(
                "Environment value for {name:?} exceeds {MAX_ENV_VALUE} bytes"
            )));
        }
        if text.contains('\0') {
            return Err(Error::Invalid(format!(
                "Environment value for {name:?} must not contain a NUL"
            )));
        }
        env.push((name.clone(), text.to_owned()));
    }
    // One order, so a replay and a restart build the same child environment.
    env.sort();
    Ok(env)
}

#[cfg(test)]
mod env_tests {
    use super::*;

    #[test]
    fn an_absent_environment_is_empty_not_an_error() {
        assert_eq!(session_env(&Value::Null).unwrap(), Vec::new());
        assert_eq!(session_env(&json!({})).unwrap(), Vec::new());
    }

    #[test]
    fn entries_are_returned_in_one_order() {
        // A replay and a restart must build the same child environment.
        let env = session_env(&json!({"MULTICA_TOKEN": "t", "MULTICA_AGENT_ID": "a"})).unwrap();
        assert_eq!(
            env,
            vec![
                ("MULTICA_AGENT_ID".to_owned(), "a".to_owned()),
                ("MULTICA_TOKEN".to_owned(), "t".to_owned()),
            ]
        );
    }

    #[test]
    fn dynamic_linker_names_are_refused_by_name() {
        // These change what every binary the child starts will load, which is
        // a larger power than running a command in the caller's own session.
        for name in ["DYLD_INSERT_LIBRARIES", "LD_PRELOAD", "LD_LIBRARY_PATH"] {
            let error = session_env(&json!({name: "/tmp/x"})).unwrap_err();
            assert!(
                format!("{error:?}").contains("dynamic linking"),
                "{name} should be refused for redirecting dynamic linking, got {error:?}"
            );
        }
        // PATH is allowed: a caller's own session command has to be findable.
        assert!(session_env(&json!({"PATH": "/usr/bin"})).is_ok());
    }

    #[test]
    fn malformed_names_and_values_are_refused() {
        assert!(session_env(&json!({"": "v"})).is_err());
        assert!(session_env(&json!({"2COOL": "v"})).is_err());
        assert!(session_env(&json!({"HAS-DASH": "v"})).is_err());
        assert!(session_env(&json!({"OK": 7})).is_err());
        assert!(session_env(&json!({"OK": "a\0b"})).is_err());
        assert!(session_env(&json!("not an object")).is_err());
    }

    #[test]
    fn the_map_and_its_values_are_bounded() {
        let mut many = serde_json::Map::new();
        for index in 0..=MAX_ENV_ENTRIES {
            many.insert(format!("V{index}"), json!("x"));
        }
        assert!(session_env(&Value::Object(many)).is_err());
        let long = "x".repeat(MAX_ENV_VALUE + 1);
        assert!(session_env(&json!({"OK": long})).is_err());
    }
}

fn random() -> String {
    hex::encode(rand::random::<[u8; 16]>())
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn unavailable(message: &str) -> Error {
    Error::Unavailable(message.into())
}
fn unknown() -> Error {
    Error::UnknownOutcome(
        "CLI operation has no verified receipt; do not resend automatically".into(),
    )
}
fn db_error(_: rusqlite::Error) -> Error {
    unavailable("CLI session store unavailable")
}

#[async_trait::async_trait]
impl Rpc for NativeRuntime {
    fn shutdown(&self) {
        self.stopped.store(true, Ordering::Release);
        // Nothing will be spawned again, so the callers' credentials have no
        // further use here.
        self.forget_env();
        if let Some(owner) = self.owner.upgrade() {
            tokio::spawn(async move {
                for (_, p) in owner.running.lock().await.drain() {
                    p.stop();
                }
            });
        }
    }
    async fn call(
        &self,
        runtime: Option<&str>,
        method: &str,
        params: Value,
        _mutation: bool,
    ) -> Result<Reply, Error> {
        if runtime.is_some_and(|r| r != self.runtime) {
            return Err(unavailable("CLI runtime identity changed"));
        }
        let result = match method {
            "muster.session.create" => self.create(&params)?,
            "session.tabs.listAll" => {
                let store = self.store.lock().unwrap();
                let mut query = store
                    .db
                    .prepare("SELECT session, MAX(observed) FROM items GROUP BY session")
                    .map_err(db_error)?;
                let updated: HashMap<String, u64> = query
                    .query_map([], |row| {
                        Ok((row.get(0)?, row.get::<_, i64>(1)?.max(0) as u64))
                    })
                    .map_err(db_error)?
                    .collect::<Result<_, _>>()
                    .map_err(db_error)?;
                let snapshots: Vec<_> = store.sessions.values().map(|s| json!({"worktree":s.cwd,"tabs":[{"type":"agent-session","sessionId":s.id,"agent":s.agent,"updatedAt":updated.get(&s.id).copied().unwrap_or(s.created_at),"title":Path::new(&s.cwd).file_name().and_then(|s|s.to_str()).unwrap_or("CLI session") }]})).collect();
                json!({"snapshots":snapshots})
            }
            "agentSession.history" => {
                let id = string(&params, "sessionId")?;
                let s = self.session(&id)?;
                // Revisions can change before the newest sequence: replay all bounded rows.
                let items = self.items(&id)?;
                json!({"ok":true,"page":{"sessionId":id,"epoch":self.runtime,"fence":self.fence,"items":items,"removedItemIds":[],"window":{"oldest":{"epoch":self.runtime,"sequence":0},"nextCursor":{"epoch":self.runtime,"sequence":now()}},"hasOlder":false,"hasNewer":false,"latestTurn":{"turn":{"state":s.state}}}})
            }
            "agentSession.send"
            | "agentSession.cancel"
            | "agentSession.respondToApproval"
            | "agentSession.respondToQuestion" => self.mutate(method, params).await?,
            _ => return Err(unavailable("Unsupported Rust CLI adapter method")),
        };
        Ok(Reply {
            runtime: self.runtime.clone(),
            result,
        })
    }
}
impl NativeRuntime {
    async fn mutate(&self, method: &str, params: Value) -> Result<Value, Error> {
        let envelope = &params["envelope"];
        let id = string(envelope, "sessionId")?;
        let operation = string(envelope, "clientOperationId")?;
        let mut fields = params.clone();
        fields
            .as_object_mut()
            .ok_or_else(unknown)?
            .remove("envelope");
        if envelope["expectedRuntimeFence"].as_u64() != Some(self.fence)
            || envelope["payloadFingerprint"] != fingerprint(method, &id, &fields)
        {
            return Err(Error::Conflict("Stale CLI operation".into()));
        }
        let value = match method {
            "agentSession.send" => {
                if self.session(&id)?.state == "running"
                    || self
                        .items(&id)?
                        .iter()
                        .any(|i| i["body"]["resolution"]["state"] == "pending")
                {
                    return Err(Error::Conflict(
                        "Wait for the current turn, answer its prompt, or interrupt it".into(),
                    ));
                }
                let process = self.process(&id).await?;
                let s = self.session(&id)?;
                let text = params["body"]["blocks"][0]["text"]
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 256 * 1024)
                    .ok_or_else(|| Error::Invalid("Invalid CLI message".into()))?;
                self.item(&id, &format!("user:{operation}"), params["body"].clone())?;
                self.update(&id, |s| s.state = "running".into())?;
                let sent = if s.agent == "codex" {
                    process
                        .request(
                            "turn/start",
                            json!({"threadId":s.provider_id,"input":[{"type":"text","text":text}]}),
                        )
                        .await
                        .map(|r| r["turn"]["id"].as_str().is_some())
                } else {
                    let token = random();
                    let uuid = format!(
                        "{}-{}-{}-{}-{}",
                        &token[..8],
                        &token[8..12],
                        &token[12..16],
                        &token[16..20],
                        &token[20..]
                    );
                    process.exchange(format!("echo:{uuid}"), json!({"type":"user","uuid":uuid,"session_id":s.provider_id,"message":{"role":"user","content":[{"type":"text","text":text}]},"parent_tool_use_id":null})).await.map(|_| true)
                };
                if !matches!(sent, Ok(true)) {
                    process.stop();
                    self.update(&id, |s| s.state = "unknown".into())?;
                    return match sent {
                        Err(Error::Conflict(e)) => Err(Error::Conflict(e)),
                        _ => Err(unknown()),
                    };
                }
                json!({"clientMessageId":operation,"submission":{"dispatchState":"accepted"}})
            }
            "agentSession.cancel" => {
                let process = self
                    .running
                    .lock()
                    .await
                    .get(&id)
                    .cloned()
                    .ok_or_else(|| unavailable("No live CLI turn"))?;
                let s = self.session(&id)?;
                if s.agent == "codex" {
                    if s.turn.is_empty() {
                        return Err(unavailable("The CLI has not identified an active turn"));
                    }
                    process
                        .request(
                            "turn/interrupt",
                            json!({"threadId":s.provider_id,"turnId":s.turn}),
                        )
                        .await?;
                } else {
                    process
                        .request("interrupt", json!({"subtype":"interrupt"}))
                        .await?;
                }
                json!({"cancelled":true})
            }
            _ => self.respond(&id, &params).await?,
        };
        Ok(json!({"ok":true,"fence":self.fence,"value":value}))
    }
    async fn respond(&self, id: &str, params: &Value) -> Result<Value, Error> {
        let item_id = string(params, "itemId")?;
        let item = self
            .items(id)?
            .into_iter()
            .find(|i| i["itemId"] == item_id)
            .ok_or_else(|| Error::Conflict("Prompt is gone".into()))?;
        let mut body = item["body"].clone();
        let s = self.session(id)?;
        if item["revision"] != params["expectedRevision"]
            || body["resolution"]["state"] != "pending"
            || body["generation"] != s.generation
        {
            return Err(Error::Conflict(
                "Prompt changed or its process ended".into(),
            ));
        }
        let process = self
            .running
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| unavailable("Prompt process exited"))?;
        let result = translate::response(&s, &body, params)?;
        // Consume the callback before writing; an ambiguous write cannot enable a second answer.
        body["resolution"]["state"] = json!("resolved");
        body["resolution"]["selectedOptionId"] = params["optionId"].clone();
        {
            let store = self.store.lock().unwrap();
            let changed = store.db.execute(
                "UPDATE items SET body=?, revision=revision+1, observed=? WHERE session=? AND id=? AND revision=? AND json_extract(body, '$.resolution.state')='pending'",
                params![body.to_string(),now() as i64,id,item_id,item["revision"].as_i64().ok_or_else(unknown)?]
            ).map_err(db_error)?;
            if changed != 1 {
                return Err(Error::Conflict("Prompt was resolved concurrently".into()));
            }
        }
        process.write(result).await.map_err(|_| unknown())?;
        Ok(
            json!({"itemId":item_id,"revision":item["revision"].as_u64().unwrap_or(0)+1,"resolution":body["resolution"]}),
        )
    }
    fn event(&self, id: &str, generation: &str, frame: &Value) -> Result<(), Error> {
        let s = self.session(id)?;
        if s.generation != generation {
            return Ok(());
        }
        translate::event(self, &s, frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_owner_per_store() {
        let dir = tempfile::tempdir().unwrap();
        let first = NativeRuntime::open(dir.path()).unwrap();
        assert!(NativeRuntime::open(dir.path()).is_err());
        drop(first);
        assert!(NativeRuntime::open(dir.path()).is_ok());
    }
    #[tokio::test]
    #[ignore = "requires installed native CLIs; run with isolated CODEX_HOME and CLAUDE_CONFIG_DIR"]
    async fn installed_native_cli_handshakes_without_inference() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let rt = NativeRuntime::open(dir.path()).unwrap();
        for agent in ["codex", "claude"] {
            let reply=rt.create(&json!({"agent":agent,"directory":cwd.path().to_str().unwrap(),"operation":agent})).unwrap();
            let id = reply["sessionId"].as_str().unwrap();
            let p = rt
                .process(id)
                .await
                .unwrap_or_else(|e| panic!("{agent} handshake: {e}"));
            assert!(!p.dead.load(Ordering::Acquire));
            p.stop();
        }
        rt.shutdown();
    }
}
