//! Cloud sessions (ARCHITECTURE §3.6.2, 2026-10-04 revision): the same
//! account's sessions in the provider's cloud, listed, followed and written
//! through the provider's own API with the credentials its CLI keeps, by
//! the strategy the profile's `[cloud]` table names (`profile::Cloud`).
//! No desktop app, Accessibility or Locked Use takes part: a cloud session
//! has no process, no window and no working tree on this machine, so it
//! is a session of its own kind (`mode = "cloud"`, `controller = "cloud"`).
//!
//! Credentials are read, never refreshed or stored: an expired token means
//! the person signs in with the CLI again. The HTTP transport is a trait so
//! the daemon supplies one and tests supply a fake.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::jsonl;
use crate::model::{
    Message, MessageKind, Provider, Role, SessionMeta, SessionState, now_unix, parse_ts,
};
use crate::profile::{Cloud, CloudApi, field};
use crate::text::clip;

/// Cloud session ids carry the provider: `cloud:claude:session_…`.
pub const PREFIX: &str = "cloud:";
/// Sessions per page of the Claude list, and how many pages one listing reads.
const PAGE: usize = 100;
const MAX_PAGES: usize = 5;
/// Event pages one read of a Claude session follows.
const MAX_EVENT_PAGES: usize = 50;
/// The provider rejects list requests above 20 (HTTP 400, 2026-10-20).
const CODEX_PAGE: usize = 20;
/// Preserve the existing bounded listing size across provider pages.
const CODEX_TASKS: usize = 50;
const MAX_DIFF: usize = 32 * 1024;
/// A token this close to its expiry counts as expired, so a request never
/// races the CLI's refresh.
const EXPIRY_MARGIN: u64 = 60;
/// Longest reply body kept in an error message.
const ERROR_SNIPPET: usize = 200;

/// The service id of a cloud session.
pub fn session_id(provider: Provider, remote: &str) -> String {
    format!("{PREFIX}{}:{remote}", provider.as_str())
}

/// The provider and the provider's own id behind a cloud session id.
pub fn parse(id: &str) -> Option<(Provider, &str)> {
    let rest = id.strip_prefix(PREFIX)?;
    let (provider, remote) = rest.split_once(':')?;
    let provider = Provider::parse(provider)?;
    let safe = !remote.is_empty()
        && remote
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    safe.then_some((provider, remote))
}

pub fn is_cloud(id: &str) -> bool {
    parse(id).is_some()
}

/// One HTTP request the engine makes; the transport adds nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

/// The transport: the daemon's HTTPS client, or a fake in tests.
#[async_trait]
pub trait Http: Send + Sync {
    async fn send(&self, request: Request) -> Result<Response, String>;
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CloudError {
    /// No usable credentials: none found, expired, or refused by the API.
    #[error("signed out: {0}")]
    SignedOut(String),
    #[error("{0}")]
    Http(String),
    #[error("{0}")]
    Unavailable(String),
}

impl From<CloudError> for crate::Error {
    fn from(error: CloudError) -> Self {
        crate::Error::Unavailable(error.to_string())
    }
}

/// What the CLI's credentials file yields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Credentials {
    pub token: String,
    /// Sent beside the token when the profile names one (`ChatGPT-Account-Id`).
    pub account: Option<String>,
    /// Seconds since the epoch, when known.
    pub expires_unix: Option<u64>,
}

impl Credentials {
    /// The credentials at the profile's paths into the file's JSON.
    pub fn from_json(cloud: &Cloud, json: &Value) -> Result<Self, CloudError> {
        let paths = &cloud.credentials;
        let token = field(json, paths.token)
            .as_str()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .ok_or_else(|| {
                CloudError::SignedOut(format!(
                    "{} holds no token at {}; sign in with the CLI on this Mac",
                    paths.file, paths.token
                ))
            })?
            .to_string();
        let account = match paths.account {
            Some(path) => Some(
                field(json, path)
                    .as_str()
                    .map(str::trim)
                    .filter(|a| !a.is_empty())
                    .ok_or_else(|| {
                        CloudError::SignedOut(format!(
                            "{} holds no account id at {path}; sign in with the CLI on this Mac",
                            paths.file
                        ))
                    })?
                    .to_string(),
            ),
            None => None,
        };
        let expires_unix = match paths.expires_at {
            Some(path) => {
                let at = field(json, path);
                at.as_u64()
                    .or_else(|| at.as_f64().map(|f| f.max(0.0) as u64))
                    .map(|ms| ms / 1000)
            }
            None => jwt_exp(&token),
        };
        Ok(Self {
            token,
            account,
            expires_unix,
        })
    }

    /// Whether the token is still good for a request made at `now`.
    pub fn valid_at(&self, now: u64) -> bool {
        self.expires_unix
            .is_none_or(|expires| now.saturating_add(EXPIRY_MARGIN) < expires)
    }
}

/// The `exp` claim of a JWT, read without verifying it: the API verifies,
/// this only avoids sending a token known to be stale.
fn jwt_exp(token: &str) -> Option<u64> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64url(payload)?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    claims["exp"].as_u64()
}

fn base64url(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            b'=' => break,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// The credentials file's text: the file under the source's home, else on
/// macOS the Keychain item the CLI keeps instead (the system asks the user
/// once; nothing unlocks a keychain).
pub fn credentials_text(cloud: &Cloud, home: &Path) -> Result<String, CloudError> {
    let path = home.join(cloud.credentials.file);
    match std::fs::read_to_string(&path) {
        Ok(text) => return Ok(text),
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(CloudError::SignedOut(format!(
                "{} is unreadable: {error}",
                path.display()
            )));
        }
        Err(_) => {}
    }
    if let Some(service) = cloud.credentials.keychain
        && let Some(text) = keychain_item(service)
    {
        return Ok(text);
    }
    Err(CloudError::SignedOut(format!(
        "no {} under {}; sign in with the CLI on this Mac",
        cloud.credentials.file,
        home.display()
    )))
}

/// The generic password `service` from the user's keychains, via the
/// system's own tool; a locked keychain or a refused prompt yields nothing.
#[cfg(target_os = "macos")]
fn keychain_item(service: &str) -> Option<String> {
    let output = std::process::Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", service, "-w"])
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

#[cfg(not(target_os = "macos"))]
fn keychain_item(_service: &str) -> Option<String> {
    None
}

pub fn read_credentials(cloud: &Cloud, home: &Path) -> Result<Credentials, CloudError> {
    let text = credentials_text(cloud, home)?;
    let json: Value = serde_json::from_str(&text).map_err(|error| {
        CloudError::SignedOut(format!(
            "{} is not JSON: {error}; sign in with the CLI on this Mac",
            cloud.credentials.file
        ))
    })?;
    Credentials::from_json(cloud, &json)
}

/// A session as the cloud lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteSession {
    pub remote_id: String,
    pub title: String,
    pub state: SessionState,
    pub updated_unix: u64,
    pub archived: bool,
    /// Where it works, as people name it (`owner/repo@branch`, an
    /// environment label); shown where a local session shows its directory.
    pub repo: String,
}

/// What one session's reads keep between polls.
#[derive(Clone)]
pub enum Transcript {
    /// Claude's events are transcript records: the engine reads them over
    /// the grammar, continuing from the last sequence number.
    Claude {
        reader: Box<jsonl::Reader>,
        cursor: Option<String>,
    },
    /// A Codex task is one document, re-read whole; `seen` tells an
    /// unchanged one from a new one.
    Codex { seen: Option<u64> },
}

/// What one read produced.
#[derive(Debug, Default)]
pub struct Fetched {
    pub messages: Vec<Message>,
    /// `messages` are the whole transcript, replacing what was shown.
    pub replace: bool,
    pub state: Option<SessionState>,
    pub title: Option<String>,
}

/// One provider's cloud, as the profile describes it.
pub struct Source {
    provider: Provider,
    cloud: &'static Cloud,
    home: PathBuf,
    http: Arc<dyn Http>,
}

impl Source {
    pub fn new(
        provider: Provider,
        cloud: &'static Cloud,
        home: PathBuf,
        http: Arc<dyn Http>,
    ) -> Self {
        Self {
            provider,
            cloud,
            home,
            http,
        }
    }

    pub fn provider(&self) -> Provider {
        self.provider
    }

    pub fn cloud(&self) -> &'static Cloud {
        self.cloud
    }

    pub fn poll(&self) -> Duration {
        Duration::from_secs(self.cloud.poll_secs)
    }

    /// The state one session's reads keep.
    pub fn transcript(&self, remote: &RemoteSession) -> Transcript {
        match self.cloud.api {
            CloudApi::ClaudeCode => {
                let (records, hooks) = self
                    .provider
                    .jsonl()
                    .expect("a claude-code cloud source has the transcript grammar");
                let meta = SessionMeta {
                    title: remote.title.clone(),
                    cwd: PathBuf::new(),
                    updated_unix: remote.updated_unix,
                    archived: remote.archived,
                    state: remote.state,
                };
                Transcript::Claude {
                    reader: Box::new(jsonl::Reader::new(self.provider, records, hooks, meta)),
                    cursor: None,
                }
            }
            CloudApi::CodexTasks => Transcript::Codex { seen: None },
        }
    }

    async fn credentials(&self) -> Result<Credentials, CloudError> {
        let (cloud, home) = (self.cloud, self.home.clone());
        // A file read, or a Keychain prompt the user answers: off the runtime.
        let credentials = tokio::task::spawn_blocking(move || read_credentials(cloud, &home))
            .await
            .map_err(|error| CloudError::Unavailable(error.to_string()))??;
        if !credentials.valid_at(now_unix()) {
            return Err(CloudError::SignedOut(format!(
                "the {} token has expired; sign in with the CLI on this Mac",
                self.cloud.label
            )));
        }
        Ok(credentials)
    }

    fn headers(&self, credentials: &Credentials) -> Vec<(String, String)> {
        let mut headers = vec![
            (
                "Authorization".into(),
                format!("Bearer {}", credentials.token),
            ),
            ("Content-Type".into(), "application/json".into()),
            ("User-Agent".into(), "Muster".into()),
        ];
        match self.cloud.api {
            CloudApi::ClaudeCode => {
                headers.push(("anthropic-version".into(), "2023-06-01".into()));
                headers.push(("anthropic-beta".into(), "oauth-2025-04-20".into()));
            }
            CloudApi::CodexTasks => {
                if let Some(account) = &credentials.account {
                    headers.push(("ChatGPT-Account-Id".into(), account.clone()));
                }
            }
        }
        headers
    }

    async fn call(
        &self,
        method: &'static str,
        url: String,
        body: Option<Value>,
    ) -> Result<Value, CloudError> {
        let credentials = self.credentials().await?;
        let request = Request {
            method,
            url,
            headers: self.headers(&credentials),
            body: body.map(|b| b.to_string()),
        };
        let response = self.http.send(request).await.map_err(CloudError::Http)?;
        match response.status {
            200..=299 => {
                if response.body.trim().is_empty() {
                    Ok(Value::Null)
                } else {
                    serde_json::from_str(&response.body).map_err(|error| {
                        CloudError::Http(format!(
                            "{} answered with no JSON: {error}",
                            self.cloud.label
                        ))
                    })
                }
            }
            401 | 403 => Err(CloudError::SignedOut(format!(
                "{} refused the token (HTTP {}); sign in with the CLI on this Mac",
                self.cloud.label, response.status
            ))),
            404 => Err(CloudError::Unavailable(format!(
                "{} knows no such session",
                self.cloud.label
            ))),
            status => Err(CloudError::Http(format!(
                "{} answered HTTP {status}: {}",
                self.cloud.label,
                clip(response.body.trim(), ERROR_SNIPPET)
            ))),
        }
    }

    /// The account's sessions, newest activity first.
    pub async fn list(&self) -> Result<Vec<RemoteSession>, CloudError> {
        let mut sessions = match self.cloud.api {
            CloudApi::ClaudeCode => self.list_claude().await?,
            CloudApi::CodexTasks => self.list_codex().await?,
        };
        sessions.sort_by(|a, b| {
            b.updated_unix
                .cmp(&a.updated_unix)
                .then_with(|| a.remote_id.cmp(&b.remote_id))
        });
        Ok(sessions)
    }

    async fn list_codex(&self) -> Result<Vec<RemoteSession>, CloudError> {
        let mut out = Vec::new();
        let mut ids = std::collections::HashSet::new();
        let mut cursors = std::collections::HashSet::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut url = format!(
                "{}/wham/tasks/list?limit={CODEX_PAGE}&task_filter=current",
                self.cloud.base_url
            );
            if let Some(cursor) = &cursor {
                url.push_str("&cursor=");
                url.push_str(&encode(cursor));
            }
            let page = self.call("GET", url, None).await?;
            for task in codex_tasks(&page) {
                if ids.insert(task.remote_id.clone()) {
                    out.push(task);
                }
                if out.len() == CODEX_TASKS {
                    return Ok(out);
                }
            }
            cursor = page["cursor"]
                .as_str()
                .filter(|c| !c.is_empty())
                .map(String::from);
            let Some(next) = &cursor else { break };
            if !cursors.insert(next.clone()) {
                return Err(CloudError::Http(
                    "Codex Cloud repeated its pagination cursor".into(),
                ));
            }
        }
        Ok(out)
    }

    async fn list_claude(&self) -> Result<Vec<RemoteSession>, CloudError> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let url = match &cursor {
                Some(cursor) => format!(
                    "{}/v1/code/sessions?limit={PAGE}&cursor={}",
                    self.cloud.base_url,
                    encode(cursor)
                ),
                None => format!("{}/v1/code/sessions?limit={PAGE}", self.cloud.base_url),
            };
            let page = self.call("GET", url, None).await?;
            out.extend(claude_sessions(&page));
            cursor = page["next_cursor"]
                .as_str()
                .filter(|c| !c.is_empty())
                .map(String::from);
            if cursor.is_none() {
                break;
            }
        }
        Ok(out)
    }

    /// What was said since the last read (Claude), or the task as it
    /// stands when it changed (Codex).
    pub async fn read(
        &self,
        remote: &str,
        transcript: &mut Transcript,
    ) -> Result<Fetched, CloudError> {
        match transcript {
            Transcript::Claude { reader, cursor } => self.read_claude(remote, reader, cursor).await,
            Transcript::Codex { seen } => {
                let url = format!("{}/wham/tasks/{remote}", self.cloud.base_url);
                let details = self.call("GET", url, None).await?;
                let messages = codex_messages(&details);
                let state = details["current_assistant_turn"]["turn_status"]
                    .as_str()
                    .map(codex_state);
                let mut hasher = DefaultHasher::new();
                for message in &messages {
                    (&message.id, message.kind.as_str(), &message.text).hash(&mut hasher);
                }
                let hash = hasher.finish();
                if *seen == Some(hash) {
                    return Ok(Fetched {
                        state,
                        ..Fetched::default()
                    });
                }
                *seen = Some(hash);
                Ok(Fetched {
                    messages,
                    replace: true,
                    state,
                    title: None,
                })
            }
        }
    }

    async fn read_claude(
        &self,
        remote: &str,
        reader: &mut jsonl::Reader,
        cursor: &mut Option<String>,
    ) -> Result<Fetched, CloudError> {
        let mut records: Vec<(u64, Value)> = Vec::new();
        let mut next = cursor.clone();
        for _ in 0..MAX_EVENT_PAGES {
            let base = format!(
                "{}/v1/code/sessions/{remote}/events?sort_order=asc",
                self.cloud.base_url
            );
            let url = match &next {
                Some(cursor) => format!("{base}&cursor={}", encode(cursor)),
                None => base,
            };
            let page = self.call("GET", url, None).await?;
            for event in page["data"].as_array().into_iter().flatten() {
                let Some(sequence) = event["sequence_num"].as_u64() else {
                    continue;
                };
                next = Some(sequence.to_string());
                if event["payload"].is_object() {
                    records.push((sequence, event["payload"].clone()));
                }
            }
            if !page["next_cursor"].as_str().is_some_and(|c| !c.is_empty()) {
                break;
            }
        }
        *cursor = next;
        let before = reader.meta().title.clone();
        let (messages, changed) = reader.read(&records);
        let title = (changed && reader.meta().title != before).then(|| reader.meta().title.clone());
        Ok(Fetched {
            messages,
            replace: false,
            state: Some(reader.state()),
            title,
        })
    }

    /// Posts a prompt into the session, where the profile allows it.
    pub async fn send(&self, remote: &str, text: &str) -> Result<(), CloudError> {
        if !self.cloud.capabilities.send {
            return Err(CloudError::Unavailable(format!(
                "{} does not offer sending prompts",
                self.cloud.label
            )));
        }
        match self.cloud.api {
            CloudApi::ClaudeCode => {
                let body = json!({
                    "events": [{
                        "payload": {
                            "type": "user",
                            "uuid": uuid_v4(),
                            "session_id": remote,
                            "timestamp": rfc3339_now(),
                            "message": { "role": "user", "content": text },
                        }
                    }]
                });
                let url = format!("{}/v1/code/sessions/{remote}/events", self.cloud.base_url);
                self.call("POST", url, Some(body)).await.map(|_| ())
            }
            CloudApi::CodexTasks => Err(CloudError::Unavailable(format!(
                "{} does not offer sending prompts",
                self.cloud.label
            ))),
        }
    }
}

/// The sessions of one page of the Claude list: this account's cloud
/// sessions with a title, leaving out ones bridged from a machine (those
/// are that machine's local sessions) and the service's own pooled ones.
fn claude_sessions(page: &Value) -> Vec<RemoteSession> {
    page["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|session| {
            let id = session["id"].as_str()?;
            if !(id.starts_with("session_") || id.starts_with("cse_")) {
                return None;
            }
            if session["environment_kind"].as_str() == Some("bridge") {
                return None;
            }
            let title = session["title"].as_str().unwrap_or("").trim();
            if title.is_empty()
                || title.contains("__CBU_POOLED__")
                || title == "__warming__"
                || title.starts_with("ditto:")
            {
                return None;
            }
            let archived = session["status"].as_str() == Some("archived");
            let state = if archived {
                SessionState::Ended
            } else {
                claude_state(session["worker_status"].as_str().unwrap_or("idle"))
            };
            let updated_unix = session["last_event_at"]
                .as_str()
                .or_else(|| session["created_at"].as_str())
                .map(parse_ts)
                .unwrap_or(0);
            Some(RemoteSession {
                remote_id: id.to_string(),
                title: title.to_string(),
                state,
                updated_unix,
                archived,
                repo: repo_label(session),
            })
        })
        .collect()
}

fn claude_state(worker_status: &str) -> SessionState {
    match worker_status {
        "running" | "working" => SessionState::Running,
        "requires_action" | "waiting" => SessionState::WaitingApproval,
        "idle" | "completed" => SessionState::Idle,
        "failed" | "stopped" | "ended" | "archived" => SessionState::Ended,
        _ => SessionState::Unknown,
    }
}

/// `owner/repo@branch` from the session's git source, if it has one.
fn repo_label(session: &Value) -> String {
    let Some(source) = session["config"]["sources"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|s| s["type"].as_str() == Some("git_repository"))
    else {
        return String::new();
    };
    let url = source["url"].as_str().unwrap_or("").trim();
    let repo = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("git@")
        .trim_start_matches("github.com/")
        .trim_start_matches("github.com:")
        .trim_end_matches('/')
        .trim_end_matches(".git");
    match source["revision"]
        .as_str()
        .map(str::trim)
        .filter(|r| !r.is_empty())
    {
        Some(revision) if !repo.is_empty() => format!("{repo}@{revision}"),
        _ => repo.to_string(),
    }
}

/// The tasks of the Codex list.
fn codex_tasks(page: &Value) -> Vec<RemoteSession> {
    page["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|task| {
            let id = task["id"].as_str().filter(|id| !id.is_empty())?;
            let title = task["title"].as_str().map(str::trim).unwrap_or("");
            let title = if title.is_empty() {
                format!("task {}", clip(id, 8))
            } else {
                title.to_string()
            };
            let display = &task["task_status_display"];
            let state = display["latest_turn_status_display"]["turn_status"]
                .as_str()
                .map(codex_state)
                .unwrap_or(SessionState::Unknown);
            let repo = display["environment_label"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_string();
            Some(RemoteSession {
                remote_id: id.to_string(),
                title,
                state,
                updated_unix: unix_time(&task["updated_at"]),
                archived: false,
                repo,
            })
        })
        .collect()
}

fn codex_state(turn_status: &str) -> SessionState {
    match turn_status {
        "in_progress" | "pending" => SessionState::Running,
        "completed" => SessionState::Idle,
        "failed" | "cancelled" => SessionState::Ended,
        _ => SessionState::Unknown,
    }
}

/// A time the backend gives as seconds since the epoch or as RFC 3339.
fn unix_time(value: &Value) -> u64 {
    value
        .as_u64()
        .or_else(|| value.as_f64().map(|f| f.max(0.0) as u64))
        .or_else(|| value.as_str().map(parse_ts))
        .unwrap_or(0)
}

/// The text parts of a Codex turn item: `{content_type: text, text}`
/// fragments or plain strings.
fn fragment_texts(item: &Value) -> Vec<String> {
    item["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|fragment| match fragment {
            Value::String(text) => Some(text.clone()),
            other => {
                let typed = other["content_type"]
                    .as_str()
                    .is_none_or(|t| t.eq_ignore_ascii_case("text"));
                typed
                    .then(|| other["text"].as_str())
                    .flatten()
                    .map(String::from)
            }
        })
        .filter(|text| !text.trim().is_empty())
        .collect()
}

/// A Codex task as messages: its prompt, the latest turn's replies, its
/// diff as a tool pair named `diff`, and an error when the turn failed.
fn codex_messages(details: &Value) -> Vec<Message> {
    let mut out = Vec::new();
    let user = &details["current_user_turn"];
    let user_time = unix_time(&user["created_at"]);
    let prompt: Vec<String> = user["input_items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["type"].as_str() == Some("message"))
        .filter(|item| {
            item["role"]
                .as_str()
                .is_none_or(|r| r.eq_ignore_ascii_case("user"))
        })
        .flat_map(fragment_texts)
        .collect();
    if !prompt.is_empty() {
        out.push(message(
            "prompt",
            Role::User,
            MessageKind::Text,
            prompt.join("\n\n"),
            user_time,
        ));
    }
    let turn = &details["current_assistant_turn"];
    let turn_time = unix_time(&turn["created_at"]).max(user_time);
    let completed = turn["turn_status"].as_str() == Some("completed");
    let mut replies = Vec::new();
    for (index, item) in turn["output_items"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        match item["type"].as_str().unwrap_or("") {
            "message" => {
                let texts = fragment_texts(item);
                if !texts.is_empty() {
                    replies.push(message(
                        &format!("reply:{index}"),
                        Role::Assistant,
                        MessageKind::Text,
                        texts.join("\n\n"),
                        turn_time,
                    ));
                }
            }
            "output_diff" | "pr" => {
                let diff = item["diff"]
                    .as_str()
                    .or_else(|| item["output_diff"]["diff"].as_str())
                    .unwrap_or("");
                if diff.is_empty() {
                    continue;
                }
                let call_id = format!("change:{index}");
                let mut call = message(
                    &call_id,
                    Role::Assistant,
                    MessageKind::ToolCall,
                    "diff".into(),
                    turn_time,
                );
                call.tool_name = "diff".into();
                call.tool_call_id = call_id.clone();
                let mut result = message(
                    &format!("diff:{index}"),
                    Role::Tool,
                    MessageKind::ToolResult,
                    format!("diff: {}", clip(diff, MAX_DIFF)),
                    turn_time,
                );
                result.tool_name = "diff".into();
                result.tool_call_id = call_id;
                replies.push(call);
                replies.push(result);
            }
            _ => {}
        }
    }
    if !replies.iter().any(|m| m.kind == MessageKind::Text) {
        // Older tasks keep their replies in the worklog only.
        for (index, entry) in turn["worklog"]["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            if entry["author"]["role"].as_str() != Some("assistant") {
                continue;
            }
            let texts: Vec<String> = entry["content"]["parts"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|part| match part {
                    Value::String(text) => Some(text.clone()),
                    other => other["text"].as_str().map(String::from),
                })
                .filter(|t| !t.trim().is_empty())
                .collect();
            if !texts.is_empty() {
                replies.push(message(
                    &format!("worklog:{index}"),
                    Role::Assistant,
                    MessageKind::Text,
                    texts.join("\n\n"),
                    turn_time,
                ));
            }
        }
    }
    if completed
        && let Some(last) = replies
            .iter_mut()
            .rev()
            .find(|m| m.kind == MessageKind::Text)
    {
        last.phase = "final_answer".into();
    }
    out.extend(replies);
    if let Some(error) = turn["error"]["message"]
        .as_str()
        .filter(|m| !m.trim().is_empty())
    {
        out.push(message(
            "error",
            Role::Assistant,
            MessageKind::Text,
            format!("Task failed: {error}"),
            turn_time,
        ));
    }
    out
}

fn message(id: &str, role: Role, kind: MessageKind, text: String, ts_unix: u64) -> Message {
    Message {
        id: id.to_string(),
        role,
        kind,
        text,
        ts_unix,
        usage: None,
        phase: String::new(),
        tool_name: String::new(),
        tool_call_id: String::new(),
    }
}

/// Percent-encodes a query value.
fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// A random version-4 UUID, as the records name themselves.
fn uuid_v4() -> String {
    let mut bytes = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = hex::encode(bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

fn rfc3339_now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    /// Answers requests from a queue of (url fragment, status, body) and
    /// records what was asked.
    struct FakeHttp {
        answers: Mutex<VecDeque<(&'static str, u16, String)>>,
        requests: Mutex<Vec<Request>>,
    }

    impl FakeHttp {
        fn new(answers: Vec<(&'static str, u16, Value)>) -> Arc<Self> {
            Arc::new(Self {
                answers: Mutex::new(
                    answers
                        .into_iter()
                        .map(|(m, s, b)| (m, s, b.to_string()))
                        .collect(),
                ),
                requests: Mutex::new(Vec::new()),
            })
        }
        fn requests(&self) -> Vec<Request> {
            self.requests.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl Http for FakeHttp {
        async fn send(&self, request: Request) -> Result<Response, String> {
            self.requests.lock().unwrap().push(request.clone());
            let mut answers = self.answers.lock().unwrap();
            match answers.iter().position(|(m, _, _)| request.url.contains(m)) {
                Some(at) => {
                    let (_, status, body) = answers.remove(at).unwrap();
                    Ok(Response { status, body })
                }
                None => Ok(Response {
                    status: 404,
                    body: "{}".into(),
                }),
            }
        }
    }

    fn claude_home(expires_in: i64) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let expires = (now_unix() as i64 + expires_in) * 1000;
        std::fs::write(
            dir.path().join(".credentials.json"),
            json!({"claudeAiOauth": {"accessToken": "tok-claude", "expiresAt": expires}})
                .to_string(),
        )
        .unwrap();
        dir
    }

    fn jwt(exp: u64) -> String {
        fn b64(bytes: &[u8]) -> String {
            const ALPHABET: &[u8] =
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
            let mut out = String::new();
            for chunk in bytes.chunks(3) {
                let mut buffer = 0u32;
                for (i, b) in chunk.iter().enumerate() {
                    buffer |= u32::from(*b) << (16 - 8 * i);
                }
                let chars = chunk.len() + 1;
                for i in 0..chars {
                    out.push(ALPHABET[((buffer >> (18 - 6 * i)) & 63) as usize] as char);
                }
            }
            out
        }
        format!(
            "{}.{}.sig",
            b64(br#"{"alg":"none"}"#),
            b64(json!({"exp": exp, "sub": "me"}).to_string().as_bytes())
        )
    }

    fn codex_home(exp: u64) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("auth.json"),
            json!({"tokens": {"access_token": jwt(exp), "account_id": "acct-1"}}).to_string(),
        )
        .unwrap();
        dir
    }

    fn claude_source(home: &Path, http: Arc<FakeHttp>) -> Source {
        Source::new(
            Provider::Claude,
            crate::profile::cloud("claude").unwrap(),
            home.into(),
            http,
        )
    }

    fn codex_source(home: &Path, http: Arc<FakeHttp>) -> Source {
        Source::new(
            Provider::Codex,
            crate::profile::cloud("codex").unwrap(),
            home.into(),
            http,
        )
    }

    #[test]
    fn ids_carry_the_provider_and_refuse_unsafe_remote_ids() {
        let id = session_id(Provider::Claude, "session_01AB");
        assert_eq!(id, "cloud:claude:session_01AB");
        assert_eq!(parse(&id), Some((Provider::Claude, "session_01AB")));
        assert!(is_cloud("cloud:codex:task_e_123"));
        assert!(!is_cloud("cloud:codex:"));
        assert!(!is_cloud("cloud:codex:a/b"));
        assert!(!is_cloud("cloud:nobody:x"));
        assert!(!is_cloud("chat:claude:new"));
        assert!(!is_cloud("01a0e659-2de9-7f63-b80b-3b53fc7754e0"));
    }

    #[test]
    fn credentials_follow_the_profile_paths_and_expiry() {
        let claude = crate::profile::cloud("claude").unwrap();
        let json =
            json!({"claudeAiOauth": {"accessToken": "t", "expiresAt": 2_000_000_000_000u64}});
        let c = Credentials::from_json(claude, &json).unwrap();
        assert_eq!(
            (c.token.as_str(), c.account.as_deref(), c.expires_unix),
            ("t", None, Some(2_000_000_000))
        );
        assert!(
            c.valid_at(1_999_999_000) && !c.valid_at(1_999_999_990) && !c.valid_at(2_000_000_001)
        );
        assert!(matches!(
            Credentials::from_json(claude, &json!({"claudeAiOauth": {"refreshToken": "r"}})),
            Err(CloudError::SignedOut(_))
        ));
        let codex = crate::profile::cloud("codex").unwrap();
        let json = json!({"tokens": {"access_token": jwt(1_700_000_000), "account_id": "acct"}});
        let c = Credentials::from_json(codex, &json).unwrap();
        assert_eq!(c.account.as_deref(), Some("acct"));
        assert_eq!(c.expires_unix, Some(1_700_000_000));
        assert!(matches!(
            Credentials::from_json(codex, &json!({"tokens": {"access_token": "x"}})),
            Err(CloudError::SignedOut(_))
        ));
        let opaque = Credentials::from_json(
            codex,
            &json!({"tokens": {"access_token": "opaque", "account_id": "a"}}),
        )
        .unwrap();
        assert!(opaque.expires_unix.is_none() && opaque.valid_at(u64::MAX - 100));
        let missing = tempfile::tempdir().unwrap();
        // Exercise a missing file without consulting the developer's Keychain.
        // Claude can legitimately remain signed in there on a test host.
        assert!(codex.credentials.keychain.is_none());
        assert!(matches!(
            read_credentials(codex, missing.path()),
            Err(CloudError::SignedOut(_))
        ));
    }

    #[tokio::test]
    async fn an_expired_token_never_reaches_the_api() {
        let home = claude_home(-10);
        let http = FakeHttp::new(vec![]);
        let source = claude_source(home.path(), http.clone());
        let error = source.list().await.unwrap_err();
        assert!(matches!(error, CloudError::SignedOut(ref m) if m.contains("expired")));
        assert!(http.requests().is_empty());
    }

    #[tokio::test]
    async fn claude_sessions_are_listed_without_bridged_archived_or_pooled_ones() {
        let home = claude_home(3600);
        let page1 = json!({"data": [
            {"id": "session_live", "title": "Fix the build", "status": "active", "worker_status": "running",
             "created_at": "2026-10-04T10:00:00Z", "last_event_at": "2026-10-04T12:00:00Z",
             "config": {"sources": [{"type": "git_repository", "url": "https://github.com/psyche08/Muster.git", "revision": "main"}]}},
            {"id": "session_bridge", "title": "On my Mac", "environment_kind": "bridge", "worker_status": "idle", "created_at": "2026-10-04T11:00:00Z"},
            {"id": "session_pooled", "title": "__CBU_POOLED__ x", "worker_status": "idle", "created_at": "2026-10-04T11:00:00Z"},
            {"id": "session_untitled", "title": "", "worker_status": "idle", "created_at": "2026-10-04T11:00:00Z"},
            {"id": "session_gone", "title": "Old", "status": "archived", "worker_status": "idle", "created_at": "2026-10-04T09:00:00Z"}
        ], "next_cursor": "c2"});
        let page2 = json!({"data": [
            {"id": "cse_waiting", "title": "Needs me", "worker_status": "requires_action", "created_at": "2026-10-04T13:00:00Z"}
        ]});
        let http = FakeHttp::new(vec![
            ("cursor=c2", 200, page2),
            ("/v1/code/sessions?limit=100", 200, page1),
        ]);
        let source = claude_source(home.path(), http.clone());
        let sessions = source.list().await.unwrap();
        let ids: Vec<&str> = sessions.iter().map(|s| s.remote_id.as_str()).collect();
        assert_eq!(ids, ["cse_waiting", "session_live", "session_gone"]);
        assert_eq!(sessions[1].state, SessionState::Running);
        assert_eq!(sessions[1].repo, "psyche08/Muster@main");
        assert_eq!(sessions[1].updated_unix, parse_ts("2026-10-04T12:00:00Z"));
        assert_eq!(sessions[0].state, SessionState::WaitingApproval);
        assert!(sessions[2].archived && sessions[2].state == SessionState::Ended);
        let requests = http.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].method, "GET");
        assert!(requests[0].url.ends_with("/v1/code/sessions?limit=100"));
        assert!(
            requests[1]
                .url
                .ends_with("/v1/code/sessions?limit=100&cursor=c2")
        );
        let headers = &requests[0].headers;
        assert!(headers.contains(&("Authorization".into(), "Bearer tok-claude".into())));
        assert!(headers.contains(&("anthropic-beta".into(), "oauth-2025-04-20".into())));
        assert!(headers.contains(&("anthropic-version".into(), "2023-06-01".into())));
    }

    #[tokio::test]
    async fn claude_events_become_messages_through_the_engine_and_continue_from_the_cursor() {
        let home = claude_home(3600);
        let events = json!({"data": [
            {"sequence_num": 1, "payload": {"type": "system", "subtype": "init", "session_id": "session_live"}},
            {"sequence_num": 2, "payload": {"type": "user", "uuid": "u1", "timestamp": "2026-10-04T12:00:00Z",
                "message": {"role": "user", "content": "Please fix the build"}}},
            {"sequence_num": 3, "payload": {"type": "assistant", "uuid": "a1", "timestamp": "2026-10-04T12:00:05Z",
                "message": {"role": "assistant", "stop_reason": "tool_use", "content": [
                    {"type": "text", "text": "Looking."},
                    {"type": "tool_use", "id": "call1", "name": "Bash", "input": {"command": "cargo build"}}
                ]}}},
            {"sequence_num": 4, "payload": {"type": "user", "uuid": "u2", "timestamp": "2026-10-04T12:00:09Z",
                "message": {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "call1", "content": "Finished"}
                ]}}}
        ], "next_cursor": "4"});
        let more = json!({"data": [
            {"sequence_num": 5, "payload": {"type": "assistant", "uuid": "a2", "timestamp": "2026-10-04T12:00:12Z",
                "message": {"role": "assistant", "stop_reason": "end_turn", "content": [{"type": "text", "text": "Fixed."}]}}}
        ]});
        let later = json!({"data": [
            {"sequence_num": 6, "payload": {"type": "user", "uuid": "u3", "timestamp": "2026-10-04T12:05:00Z",
                "message": {"role": "user", "content": "Thanks, now the tests"}}}
        ]});
        let http = FakeHttp::new(vec![
            ("cursor=5", 200, later),
            ("cursor=4", 200, more),
            ("/events?sort_order=asc", 200, events),
        ]);
        let source = claude_source(home.path(), http.clone());
        let remote = RemoteSession {
            remote_id: "session_live".into(),
            title: "Fix the build".into(),
            state: SessionState::Running,
            updated_unix: 1,
            archived: false,
            repo: String::new(),
        };
        let mut transcript = source.transcript(&remote);
        let fetched = source.read("session_live", &mut transcript).await.unwrap();
        assert!(!fetched.replace);
        let shape: Vec<(&str, &str, &str)> = fetched
            .messages
            .iter()
            .map(|m| (m.id.as_str(), m.kind.as_str(), m.role.as_str()))
            .collect();
        assert_eq!(
            shape,
            [
                ("2", "text", "user"),
                ("3", "text", "assistant"),
                ("3:1", "tool_call", "assistant"),
                ("4", "tool_result", "tool"),
                ("5", "text", "assistant"),
                ("5:usage", "turn_summary", "system"),
            ]
        );
        assert_eq!(fetched.messages[0].text, "Please fix the build");
        assert_eq!(
            fetched.messages[2].text,
            "Bash {\"command\":\"cargo build\"}"
        );
        assert_eq!(fetched.messages[3].text, "Bash: Finished");
        assert_eq!(fetched.messages[4].phase, "final_answer");
        assert_eq!(fetched.state, Some(SessionState::Idle));
        let Transcript::Claude { cursor, .. } = &transcript else {
            panic!("a claude transcript")
        };
        assert_eq!(cursor.as_deref(), Some("5"));

        let fetched = source.read("session_live", &mut transcript).await.unwrap();
        assert_eq!(fetched.messages.len(), 1);
        assert_eq!(fetched.messages[0].id, "6");
        assert_eq!(fetched.state, Some(SessionState::Running));
        let urls: Vec<String> = http.requests().into_iter().map(|r| r.url).collect();
        assert_eq!(urls.len(), 3);
        assert!(urls[2].ends_with("/v1/code/sessions/session_live/events?sort_order=asc&cursor=5"));
    }

    #[tokio::test]
    async fn a_claude_prompt_is_posted_as_a_user_record() {
        let home = claude_home(3600);
        let http = FakeHttp::new(vec![("/events", 200, json!({"ok": true}))]);
        let source = claude_source(home.path(), http.clone());
        source.send("session_live", "Run the tests").await.unwrap();
        let requests = http.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, "POST");
        assert!(
            requests[0]
                .url
                .ends_with("/v1/code/sessions/session_live/events")
        );
        let body: Value = serde_json::from_str(requests[0].body.as_deref().unwrap()).unwrap();
        let payload = &body["events"][0]["payload"];
        assert_eq!(payload["type"], "user");
        assert_eq!(payload["session_id"], "session_live");
        assert_eq!(payload["message"]["role"], "user");
        assert_eq!(payload["message"]["content"], "Run the tests");
        let uuid = payload["uuid"].as_str().unwrap();
        assert!(crate::viewer::is_uuid(uuid), "{uuid}");
        assert!(payload["timestamp"].as_str().unwrap().ends_with('Z'));
    }

    #[tokio::test]
    async fn a_refused_token_reads_as_signed_out() {
        let home = claude_home(3600);
        let http = FakeHttp::new(vec![(
            "/v1/code/sessions",
            401,
            json!({"error": {"message": "expired"}}),
        )]);
        let source = claude_source(home.path(), http);
        assert!(matches!(source.list().await, Err(CloudError::SignedOut(_))));
    }

    #[tokio::test]
    async fn codex_pages_preserve_listing_size_deduplicate_and_encode_cursors() {
        let home = codex_home(now_unix() + 3600);
        let items = |range: std::ops::Range<usize>| {
            range
                .map(|n| json!({"id":format!("task_{n}"), "updated_at":n}))
                .collect::<Vec<_>>()
        };
        let http = FakeHttp::new(vec![
            (
                "&cursor=next%2Ftwo%3F",
                200,
                json!({"items":items(19..39),"cursor":"third"}),
            ),
            (
                "&cursor=third",
                200,
                json!({"items":items(39..59),"cursor":"unused"}),
            ),
            (
                "/wham/tasks/list?limit=20&task_filter=current",
                200,
                json!({"items":items(0..20),"cursor":"next/two?"}),
            ),
        ]);
        let source = codex_source(home.path(), http.clone());
        let tasks = source.list().await.unwrap();
        assert_eq!(tasks.len(), 50);
        assert_eq!(
            tasks
                .iter()
                .map(|t| &t.remote_id)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            50
        );
        let requests = http.requests();
        assert_eq!(requests.len(), 3);
        assert!(
            requests
                .iter()
                .all(|r| r.url.contains("?limit=20&task_filter=current"))
        );
    }

    #[tokio::test]
    async fn codex_repeated_cursor_is_a_bounded_failure() {
        let home = codex_home(now_unix() + 3600);
        let http = FakeHttp::new(vec![
            ("&cursor=again", 200, json!({"items":[],"cursor":"again"})),
            (
                "/wham/tasks/list?limit=20",
                200,
                json!({"items":[],"cursor":"again"}),
            ),
        ]);
        let source = codex_source(home.path(), http.clone());
        assert!(
            matches!(source.list().await,Err(CloudError::Http(m)) if m.contains("pagination cursor"))
        );
        assert_eq!(http.requests().len(), 2);
    }

    #[tokio::test]
    async fn codex_tasks_are_listed_read_whole_and_take_no_prompt() {
        let home = codex_home(now_unix() + 3600);
        let list = json!({"items": [
            {"id": "task_e_1", "title": "Add tests", "updated_at": 1_759_580_000.5,
             "task_status_display": {"environment_label": "psyche08/Muster",
                 "latest_turn_status_display": {"turn_status": "completed"}}},
            {"id": "task_e_2", "title": "", "updated_at": 1_759_590_000,
             "task_status_display": {"latest_turn_status_display": {"turn_status": "in_progress"}}}
        ]});
        let details = json!({
            "current_user_turn": {"created_at": 1_759_570_000,
                "input_items": [{"type": "message", "role": "user", "content": [{"content_type": "text", "text": "Add tests for paging"}]}]},
            "current_assistant_turn": {"id": "turn1", "turn_status": "completed", "created_at": 1_759_580_000,
                "output_items": [
                    {"type": "message", "content": [{"content_type": "text", "text": "Added two tests."}]},
                    {"type": "output_diff", "diff": "diff --git a/x b/x\n+1\n"}
                ]}
        });
        let http = FakeHttp::new(vec![
            ("/wham/tasks/task_e_1", 200, details.clone()),
            ("/wham/tasks/task_e_1", 200, details),
            ("/wham/tasks/list?limit=20&task_filter=current", 200, list),
        ]);
        let source = codex_source(home.path(), http.clone());
        let tasks = source.list().await.unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].remote_id, "task_e_2");
        assert_eq!(tasks[0].title, "task task_e_2");
        assert_eq!(tasks[0].state, SessionState::Running);
        assert_eq!(tasks[1].repo, "psyche08/Muster");
        assert_eq!(tasks[1].updated_unix, 1_759_580_000);
        assert_eq!(tasks[1].state, SessionState::Idle);
        let headers = &http.requests()[0].headers;
        assert!(headers.contains(&("ChatGPT-Account-Id".into(), "acct-1".into())));
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "Authorization" && v.starts_with("Bearer "))
        );

        let mut transcript = source.transcript(&tasks[1]);
        let fetched = source.read("task_e_1", &mut transcript).await.unwrap();
        assert!(fetched.replace);
        let shape: Vec<(&str, &str)> = fetched
            .messages
            .iter()
            .map(|m| (m.id.as_str(), m.kind.as_str()))
            .collect();
        assert_eq!(
            shape,
            [
                ("prompt", "text"),
                ("reply:0", "text"),
                ("change:1", "tool_call"),
                ("diff:1", "tool_result")
            ]
        );
        assert_eq!(fetched.messages[0].text, "Add tests for paging");
        assert_eq!(fetched.messages[1].phase, "final_answer");
        assert_eq!(fetched.messages[3].tool_name, "diff");
        assert_eq!(fetched.messages[3].tool_call_id, "change:1");
        assert!(fetched.messages[3].text.starts_with("diff: diff --git"));
        assert_eq!(fetched.state, Some(SessionState::Idle));
        let again = source.read("task_e_1", &mut transcript).await.unwrap();
        assert!(again.messages.is_empty() && !again.replace);

        let error = source.send("task_e_1", "more").await.unwrap_err();
        assert!(matches!(error, CloudError::Unavailable(ref m) if m.contains("Codex Cloud")));
        assert_eq!(
            http.requests().len(),
            3,
            "a refused send never reaches the API"
        );
    }

    #[test]
    fn codex_errors_and_worklogs_are_read() {
        let details = json!({
            "current_user_turn": {"input_items": [{"type": "message", "content": ["Do it"]}]},
            "current_assistant_turn": {"turn_status": "failed",
                "worklog": {"messages": [
                    {"author": {"role": "user"}, "content": {"parts": ["Do it"]}},
                    {"author": {"role": "assistant"}, "content": {"parts": [{"content_type": "text", "text": "Tried."}]}}
                ]},
                "error": {"code": "env", "message": "the environment stopped"}}
        });
        let messages = codex_messages(&details);
        let texts: Vec<&str> = messages.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(
            texts,
            ["Do it", "Tried.", "Task failed: the environment stopped"]
        );
        assert_eq!(messages[1].role, Role::Assistant);
        assert!(
            messages[1].phase.is_empty(),
            "a failed turn has no final answer"
        );
    }

    #[test]
    fn helpers_encode_and_name_things() {
        assert_eq!(encode("a b/c=d"), "a%20b%2Fc%3Dd");
        assert_eq!(encode("plain-ok_1.2~"), "plain-ok_1.2~");
        assert!(crate::viewer::is_uuid(&uuid_v4()));
        assert_eq!(
            unix_time(&json!("2026-10-04T12:00:00Z")),
            parse_ts("2026-10-04T12:00:00Z")
        );
        assert_eq!(unix_time(&json!(12.9)), 12);
        assert_eq!(
            repo_label(
                &json!({"config": {"sources": [{"type": "git_repository", "url": "git@github.com:o/r.git"}]}})
            ),
            "o/r"
        );
        assert_eq!(repo_label(&json!({})), "");
        assert_eq!(base64url("aGVsbG8").unwrap(), b"hello");
        assert_eq!(base64url("aGVsbG8=").unwrap(), b"hello");
        assert!(base64url("not valid!").is_none());
    }
}
