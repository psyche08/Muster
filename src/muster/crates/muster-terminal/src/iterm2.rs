//! The iTerm2 API client (ARCHITECTURE §3.6.3): WebSocket over iTerm2's
//! private Unix socket, protobuf messages, screen-update notifications,
//! and per-line deltas computed here so the wire carries only what changed.
//! Proven by the P0 itermrs POC; this is the same code with a service face.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use prost::Message as _;
use tokio::net::UnixStream;
use tokio::sync::{Mutex, broadcast, mpsc, oneshot, watch};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;

use crate::proto::client_originated_message::Submessage as Req;
use crate::proto::server_originated_message::Submessage as Resp;
use crate::proto::{
    ClientOriginatedMessage, GetBufferRequest, LineRange, ListSessionsRequest,
    ListSessionsResponse, Notification, NotificationRequest, NotificationType, SendTextRequest,
    ServerOriginatedMessage, SessionSummary, SplitTreeNode,
    split_tree_node::split_tree_link::Child,
};
use crate::{Capture, CapturedLine, Error, TerminalFrame, TerminalLine, TerminalSession};

pub const SUBPROTOCOL: &str = "api.iterm2.com";
pub const LIBRARY_VERSION: &str = "rust 0.1";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// Screen updates within this window collapse into one fetch.
const COALESCE: Duration = Duration::from_millis(16);

#[derive(Clone, Debug)]
pub struct Cookie {
    pub cookie: String,
    pub key: String,
}

impl Cookie {
    fn parse(raw: &str) -> Result<Self, Error> {
        let mut parts = raw.split_whitespace();
        match (parts.next(), parts.next()) {
            (Some(cookie), Some(key)) => Ok(Self {
                cookie: cookie.to_string(),
                key: key.to_string(),
            }),
            _ => Err(Error::Unavailable(format!(
                "expected \"COOKIE KEY\", got {raw:?}"
            ))),
        }
    }

    /// From the environment iTerm2 sets in its own shells (useful for tests).
    pub fn from_env() -> Option<Self> {
        Some(Self {
            cookie: std::env::var("ITERM2_COOKIE").ok()?,
            key: std::env::var("ITERM2_KEY").ok()?,
        })
    }

    /// Through Apple Events, which needs the Automation grant (decision D6).
    /// The in-process NSAppleScript shim replaces this once muster-platform
    /// carries it; `osascript` attributes to the same responsible process.
    pub async fn request(app: &str, name: &str) -> Result<Self, Error> {
        let script = format!(
            "tell application \"{}\" to request cookie and key for app named \"{}\"",
            app.replace('"', ""),
            name.replace('"', "")
        );
        let output = tokio::time::timeout(
            REQUEST_TIMEOUT,
            tokio::process::Command::new("/usr/bin/osascript")
                .args(["-e", &script])
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| {
            Error::Unavailable(
                "iTerm2 cookie request timed out; check local Automation setup".into(),
            )
        })?
        .map_err(|e| Error::Unavailable(format!("osascript: {e}")))?;
        if !output.status.success() {
            return Err(Error::Unavailable(format!(
                "iTerm2 cookie request failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Self::parse(String::from_utf8_lossy(&output.stdout).trim())
    }
}

pub fn default_socket() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join("Library/Application Support/iTerm2/private/socket")
}

type Pending = Arc<Mutex<HashMap<i64, oneshot::Sender<ServerOriginatedMessage>>>>;

/// One connection to iTerm2.
pub struct Iterm2 {
    outbound: mpsc::Sender<Vec<u8>>,
    closed: watch::Receiver<bool>,
    pending: Pending,
    next_id: AtomicI64,
    notifications: broadcast::Sender<Arc<Notification>>,
}

impl Iterm2 {
    pub async fn connect(
        socket: &Path,
        cookie: &Cookie,
        advisory_name: &str,
    ) -> Result<Arc<Self>, Error> {
        let stream = UnixStream::connect(socket)
            .await
            .map_err(|e| Error::Unavailable(format!("iTerm2 socket {}: {e}", socket.display())))?;
        // The handshake headers (Host, Upgrade, Sec-WebSocket-Key, ...)
        // come from tungstenite; iTerm2's are added on top.
        let mut request = "ws://localhost/"
            .into_client_request()
            .map_err(|e| Error::Protocol(e.to_string()))?;
        let headers = request.headers_mut();
        for (name, value) in [
            ("Origin", "http://localhost"),
            ("Sec-WebSocket-Protocol", SUBPROTOCOL),
            ("x-iterm2-library-version", LIBRARY_VERSION),
            ("x-iterm2-advisory-name", advisory_name),
            ("x-iterm2-disable-auth-ui", "true"),
            ("x-iterm2-cookie", cookie.cookie.as_str()),
            ("x-iterm2-key", cookie.key.as_str()),
        ] {
            headers.insert(
                name,
                HeaderValue::from_str(value).map_err(|e| Error::Protocol(e.to_string()))?,
            );
        }
        let (ws, _) = tokio_tungstenite::client_async(request, stream)
            .await
            .map_err(|e| Error::Unavailable(format!("iTerm2 refused the connection: {e}")))?;
        let (mut sink, mut source) = ws.split();
        let (outbound, mut outbound_rx) = mpsc::channel::<Vec<u8>>(64);
        let (notifications, _) = broadcast::channel(1024);
        let pending: Pending = Arc::default();
        let (close_tx, closed) = watch::channel(false);
        let writer_closed = close_tx.clone();
        let writer_pending = pending.clone();
        tokio::spawn(async move {
            while let Some(bytes) = outbound_rx.recv().await {
                if sink.send(Message::Binary(bytes.into())).await.is_err() {
                    break;
                }
            }
            let _ = writer_closed.send(true);
            writer_pending.lock().await.clear();
            let _ = sink.close().await;
        });
        let reader_pending = pending.clone();
        let reader_closed = close_tx;
        let notify = notifications.clone();
        tokio::spawn(async move {
            while let Some(frame) = source.next().await {
                let bytes = match frame {
                    Ok(Message::Binary(bytes)) => bytes,
                    Ok(Message::Close(_)) | Err(_) => break,
                    Ok(_) => continue,
                };
                let Ok(message) = ServerOriginatedMessage::decode(&bytes[..]) else {
                    continue;
                };
                match (message.id, message.submessage) {
                    (_, Some(Resp::Notification(notification))) => {
                        let _ = notify.send(Arc::new(notification));
                    }
                    (Some(id), submessage) => {
                        if let Some(waiter) = reader_pending.lock().await.remove(&id) {
                            let _ = waiter.send(ServerOriginatedMessage {
                                id: Some(id),
                                submessage,
                            });
                        }
                    }
                    _ => {}
                }
            }
            let _ = reader_closed.send(true);
            reader_pending.lock().await.clear();
        });
        Ok(Arc::new(Self {
            outbound,
            closed,
            pending,
            next_id: AtomicI64::new(1),
            notifications,
        }))
    }

    pub fn is_closed(&self) -> bool {
        *self.closed.borrow()
    }

    pub async fn closed(&self) {
        let mut closed = self.closed.clone();
        let _ = closed.wait_for(|closed| *closed).await;
    }

    async fn request(&self, submessage: Req) -> Result<Resp, Error> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            if self.is_closed() {
                return Err(Error::Unavailable("iTerm2 connection closed".into()));
            }
            pending.insert(id, tx);
        }
        let message = ClientOriginatedMessage {
            id: Some(id),
            submessage: Some(submessage),
        };
        if self.outbound.send(message.encode_to_vec()).await.is_err() {
            self.pending.lock().await.remove(&id);
            return Err(Error::Unavailable("iTerm2 connection closed".into()));
        }
        let reply = match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(reply)) => reply,
            Ok(Err(_)) => return Err(Error::Unavailable("iTerm2 connection closed".into())),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                return Err(Error::Protocol(format!("iTerm2 request {id} timed out")));
            }
        };
        match reply.submessage {
            Some(Resp::Error(text)) => Err(Error::Protocol(format!("iTerm2: {text}"))),
            Some(other) => Ok(other),
            None => Err(Error::Protocol("empty iTerm2 response".into())),
        }
    }

    pub fn subscribe_notifications(&self) -> broadcast::Receiver<Arc<Notification>> {
        self.notifications.subscribe()
    }

    pub async fn sessions(&self) -> Result<Vec<TerminalSession>, Error> {
        match self
            .request(Req::ListSessionsRequest(ListSessionsRequest {}))
            .await?
        {
            Resp::ListSessionsResponse(reply) => Ok(flatten(&reply)),
            other => Err(Error::Protocol(format!("unexpected reply {other:?}"))),
        }
    }

    pub async fn subscribe(
        &self,
        kind: NotificationType,
        session: Option<&str>,
    ) -> Result<(), Error> {
        let reply = self
            .request(Req::NotificationRequest(NotificationRequest {
                session: session.map(str::to_string),
                subscribe: Some(true),
                notification_type: Some(kind as i32),
                arguments: None,
            }))
            .await?;
        match reply {
            Resp::NotificationResponse(reply) => {
                use crate::proto::notification_response::Status;
                match reply.status() {
                    Status::Ok | Status::AlreadySubscribed => Ok(()),
                    other => Err(Error::Protocol(format!("subscribe {kind:?}: {other:?}"))),
                }
            }
            other => Err(Error::Protocol(format!("unexpected reply {other:?}"))),
        }
    }

    /// The visible screen (or the last `trailing` lines) with cursor.
    pub async fn buffer(
        &self,
        session: &str,
        trailing: Option<i32>,
    ) -> Result<(Vec<String>, (u32, u32)), Error> {
        let line_range = match trailing {
            Some(n) => LineRange {
                trailing_lines: Some(n),
                ..Default::default()
            },
            None => LineRange {
                screen_contents_only: Some(true),
                ..Default::default()
            },
        };
        let reply = self
            .request(Req::GetBufferRequest(GetBufferRequest {
                session: Some(session.to_string()),
                line_range: Some(line_range),
                include_styles: Some(false),
            }))
            .await?;
        let Resp::GetBufferResponse(reply) = reply else {
            return Err(Error::Protocol("unexpected GetBuffer reply".into()));
        };
        if reply.status() != crate::proto::get_buffer_response::Status::Ok {
            return Err(Error::NotFound(format!(
                "session {session}: {:?}",
                reply.status()
            )));
        }
        let cursor = screen_cursor(&reply);
        let lines = reply
            .contents
            .into_iter()
            .map(|l| l.text.unwrap_or_default())
            .collect();
        Ok((lines, cursor))
    }

    /// Session variables (e.g. "session.jobPid", "session.tty") as JSON values.
    pub async fn variables(
        &self,
        session: &str,
        names: &[&str],
    ) -> Result<HashMap<String, serde_json::Value>, Error> {
        let reply = self
            .request(Req::VariableRequest(crate::proto::VariableRequest {
                scope: Some(crate::proto::variable_request::Scope::SessionId(
                    session.to_string(),
                )),
                set: Vec::new(),
                get: names.iter().map(|n| n.to_string()).collect(),
            }))
            .await?;
        let Resp::VariableResponse(reply) = reply else {
            return Err(Error::Protocol("unexpected Variable reply".into()));
        };
        if reply.status() != crate::proto::variable_response::Status::Ok {
            return Err(Error::NotFound(format!(
                "session {session}: {:?}",
                reply.status()
            )));
        }
        Ok(names
            .iter()
            .zip(reply.values)
            .map(|(name, value)| {
                (
                    name.to_string(),
                    serde_json::from_str(&value).unwrap_or(serde_json::Value::Null),
                )
            })
            .collect())
    }

    /// The screen with line boundaries, the cursor and the job behind the
    /// session, for the controller that types into a CLI.
    pub async fn capture(&self, session: &str) -> Result<Capture, Error> {
        let vars = self
            .variables(session, &["session.jobPid", "session.tty"])
            .await?;
        let reply = self
            .request(Req::GetBufferRequest(GetBufferRequest {
                session: Some(session.to_string()),
                line_range: Some(LineRange {
                    screen_contents_only: Some(true),
                    ..Default::default()
                }),
                include_styles: Some(false),
            }))
            .await?;
        let Resp::GetBufferResponse(reply) = reply else {
            return Err(Error::Protocol("unexpected GetBuffer reply".into()));
        };
        if reply.status() != crate::proto::get_buffer_response::Status::Ok {
            return Err(Error::NotFound(format!(
                "session {session}: {:?}",
                reply.status()
            )));
        }
        let cursor = screen_cursor(&reply);
        let lines = reply
            .contents
            .into_iter()
            .map(|l| CapturedLine {
                hard_eol: l.continuation() == crate::proto::line_contents::Continuation::HardEol,
                text: l.text.unwrap_or_default(),
            })
            .collect();
        Ok(Capture {
            lines,
            cursor: (cursor.0 as usize, cursor.1 as usize),
            job_pid: vars
                .get("session.jobPid")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            tty: vars
                .get("session.tty")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        })
    }

    /// Sends `text` only if the screen still equals `expected` (captured
    /// again right before, broadcast suppressed). A failure after the send
    /// went out is reported as `Unavailable`; a changed screen as `Conflict`.
    pub async fn send_if_unchanged(
        &self,
        session: &str,
        expected: &Capture,
        text: &str,
    ) -> Result<(), Error> {
        let now = self.capture(session).await?;
        if now != *expected {
            return Err(Error::Conflict("the screen changed".into()));
        }
        self.send_text(session, text).await
    }

    pub async fn send_text(&self, session: &str, text: &str) -> Result<(), Error> {
        let reply = self
            .request(Req::SendTextRequest(SendTextRequest {
                session: Some(session.to_string()),
                text: Some(text.to_string()),
                suppress_broadcast: Some(true),
            }))
            .await?;
        match reply {
            Resp::SendTextResponse(reply)
                if reply.status() == crate::proto::send_text_response::Status::Ok =>
            {
                Ok(())
            }
            Resp::SendTextResponse(reply) => Err(Error::NotFound(format!(
                "session {session}: {:?}",
                reply.status()
            ))),
            other => Err(Error::Protocol(format!("unexpected reply {other:?}"))),
        }
    }

    pub async fn create_tab(
        &self,
        profile: Option<&str>,
        background: bool,
    ) -> Result<TerminalSession, Error> {
        self.create_window(profile, background, None).await
    }

    /// The command is a new profile's initial process, never input to a shell.
    pub async fn create_window(
        &self,
        profile: Option<&str>,
        background: bool,
        command: Option<&str>,
    ) -> Result<TerminalSession, Error> {
        let reply = self
            .request(Req::CreateTabRequest(create_window_request(
                profile, background, command,
            )))
            .await?;
        let Resp::CreateTabResponse(reply) = reply else {
            return Err(Error::Protocol("unexpected CreateTab reply".into()));
        };
        if reply.status() != crate::proto::create_tab_response::Status::Ok {
            return Err(Error::Protocol(format!("create tab: {:?}", reply.status())));
        }
        Ok(TerminalSession {
            id: reply.session_id.unwrap_or_default(),
            title: String::new(),
            cols: 0,
            rows: 0,
            window_id: reply.window_id.unwrap_or_default(),
            tab_id: reply.tab_id.map(|id| id.to_string()).unwrap_or_default(),
            source: "iterm2".into(),
            device_id: String::new(),
            device_name: String::new(),
        })
    }

    pub async fn resize(&self, session: &str, cols: u32, rows: u32) -> Result<(), Error> {
        let reply = self
            .request(Req::SetPropertyRequest(crate::proto::SetPropertyRequest {
                identifier: Some(crate::proto::set_property_request::Identifier::SessionId(
                    session.to_string(),
                )),
                name: Some("grid_size".into()),
                json_value: Some(format!("{{\"width\":{cols},\"height\":{rows}}}")),
            }))
            .await?;
        match reply {
            Resp::SetPropertyResponse(reply)
                if reply.status() == crate::proto::set_property_response::Status::Ok =>
            {
                Ok(())
            }
            Resp::SetPropertyResponse(reply) => {
                Err(Error::Protocol(format!("resize: {:?}", reply.status())))
            }
            other => Err(Error::Protocol(format!("unexpected reply {other:?}"))),
        }
    }

    pub async fn close(&self, session: &str) -> Result<(), Error> {
        let reply = self
            .request(Req::CloseRequest(crate::proto::CloseRequest {
                target: Some(crate::proto::close_request::Target::Sessions(
                    crate::proto::close_request::CloseSessions {
                        session_ids: vec![session.to_string()],
                    },
                )),
                force: Some(false),
            }))
            .await?;
        match reply {
            Resp::CloseResponse(_) => Ok(()),
            other => Err(Error::Protocol(format!("unexpected reply {other:?}"))),
        }
    }

    /// Frames for one session: a full snapshot first, then only changed
    /// lines, driven by screen-update notifications coalesced over 16 ms.
    /// Ends when the session terminates or the receiver is dropped.
    pub async fn attach(
        self: &Arc<Self>,
        session: &str,
        since_rev: u64,
        frames: mpsc::Sender<TerminalFrame>,
    ) -> Result<(), Error> {
        self.subscribe(NotificationType::NotifyOnScreenUpdate, Some(session))
            .await?;
        self.subscribe(NotificationType::NotifyOnTerminateSession, None)
            .await?;
        let mut notifications = self.subscribe_notifications();
        let mut rev = since_rev;
        let (lines, cursor) = self.buffer(session, None).await?;
        rev += 1;
        let full = TerminalFrame {
            session: session.to_string(),
            rev,
            full: true,
            cols: lines
                .iter()
                .map(|l| l.chars().count() as u32)
                .max()
                .unwrap_or(0),
            rows: lines.len() as u32,
            lines: lines
                .iter()
                .enumerate()
                .map(|(i, text)| TerminalLine {
                    index: i as u32,
                    text: text.clone(),
                    styles: Vec::new(),
                })
                .collect(),
            cursor_x: cursor.0,
            cursor_y: cursor.1,
            ended: false,
            ..Default::default()
        };
        let mut previous: Vec<String> = lines;
        if frames.send(full).await.is_err() {
            return Ok(());
        }
        loop {
            let next = tokio::select! {
                _ = frames.closed() => return Ok(()),
                _ = self.closed() => return Err(Error::Unavailable("iTerm2 connection closed".into())),
                next = notifications.recv() => next,
            };
            let notification = match next {
                Ok(n) => n,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            };
            if notification
                .terminate_session_notification
                .as_ref()
                .is_some_and(|t| t.session_id() == session)
            {
                let _ = frames
                    .send(TerminalFrame {
                        session: session.to_string(),
                        rev: rev + 1,
                        full: false,
                        cols: 0,
                        rows: 0,
                        lines: Vec::new(),
                        cursor_x: 0,
                        cursor_y: 0,
                        ended: true,
                        ..Default::default()
                    })
                    .await;
                break;
            }
            let Some(update) = &notification.screen_update_notification else {
                continue;
            };
            if update.session() != session {
                continue;
            }
            // Collapse the burst.
            let deadline = tokio::time::sleep(COALESCE);
            tokio::pin!(deadline);
            loop {
                tokio::select! {
                    _ = &mut deadline => break,
                    more = notifications.recv() => match more {
                        Ok(n) if n.screen_update_notification.as_ref().is_some_and(|u| u.session() == session) => {}
                        Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                        Err(_) => break,
                    }
                }
            }
            let (lines, cursor) = match self.buffer(session, None).await {
                Ok(v) => v,
                Err(error) => {
                    log::debug!("GetBuffer {session}: {error}");
                    continue;
                }
            };
            let Some(delta) = crate::line_delta(&previous, &lines) else {
                previous = lines;
                continue;
            };
            previous = lines;
            rev += 1;
            let frame = TerminalFrame {
                session: session.to_string(),
                rev,
                full: delta.full,
                cols: previous
                    .iter()
                    .map(|l| l.chars().count() as u32)
                    .max()
                    .unwrap_or(0),
                rows: previous.len() as u32,
                lines: delta.lines,
                cursor_x: cursor.0,
                cursor_y: cursor.1,
                ended: false,
                ..Default::default()
            };
            if frames.send(frame).await.is_err() {
                break;
            }
        }
        Ok(())
    }
}

/// The cursor as (column, row on screen), from the absolute buffer coordinates.
fn screen_cursor(reply: &crate::proto::GetBufferResponse) -> (u32, u32) {
    let first_line = reply
        .windowed_coord_range
        .as_ref()
        .and_then(|w| w.coord_range.as_ref())
        .and_then(|r| r.start.as_ref())
        .map(|c| c.y())
        .unwrap_or(0);
    reply
        .cursor
        .as_ref()
        .map(|c| (c.x().max(0) as u32, (c.y() - first_line).max(0) as u32))
        .unwrap_or((0, 0))
}

fn walk<'a>(node: &'a SplitTreeNode, out: &mut Vec<&'a SessionSummary>) {
    for link in &node.links {
        match &link.child {
            Some(Child::Session(session)) => out.push(session),
            Some(Child::Node(inner)) => walk(inner, out),
            None => {}
        }
    }
}

fn flatten(list: &ListSessionsResponse) -> Vec<TerminalSession> {
    let mut out = Vec::new();
    for window in &list.windows {
        for tab in &window.tabs {
            let mut sessions = Vec::new();
            if let Some(root) = &tab.root {
                walk(root, &mut sessions);
            }
            for session in sessions {
                out.push(TerminalSession {
                    id: session.unique_identifier().to_string(),
                    title: session.title().to_string(),
                    cols: session
                        .grid_size
                        .as_ref()
                        .map(|s| s.width() as u32)
                        .unwrap_or(0),
                    rows: session
                        .grid_size
                        .as_ref()
                        .map(|s| s.height() as u32)
                        .unwrap_or(0),
                    window_id: window.window_id().to_string(),
                    tab_id: tab.tab_id().to_string(),
                    source: "iterm2".into(),
                    device_id: String::new(),
                    device_name: String::new(),
                });
            }
        }
    }
    out
}

fn create_window_request(
    profile: Option<&str>,
    background: bool,
    command: Option<&str>,
) -> crate::proto::CreateTabRequest {
    crate::proto::CreateTabRequest {
        profile_name: profile.map(str::to_owned),
        select_tab: Some(!background),
        custom_profile_properties: command
            .map(|command| {
                vec![
                    crate::proto::ProfileProperty {
                        key: Some("Custom Command".into()),
                        json_value: Some("\"Yes\"".into()),
                    },
                    crate::proto::ProfileProperty {
                        key: Some("Command".into()),
                        json_value: Some(
                            serde_json::to_string(command).expect("string serialization"),
                        ),
                    },
                ]
            })
            .unwrap_or_default(),
        ..Default::default()
    }
}

#[cfg(test)]
mod creation_tests {
    use super::*;
    #[test]
    fn command_is_initial_profile_data_and_window_target_is_unset() {
        let command = "/bin/sh -c 'printf \"hello\\n\"'";
        let request = create_window_request(Some("Fixture profile"), false, Some(command));
        assert!(request.window_id.is_none());
        assert_eq!(request.select_tab, Some(true));
        assert_eq!(
            request.custom_profile_properties[0].json_value.as_deref(),
            Some("\"Yes\"")
        );
        let encoded = request.custom_profile_properties[1]
            .json_value
            .as_ref()
            .unwrap();
        assert_eq!(serde_json::from_str::<String>(encoded).unwrap(), command);
        assert!(
            create_window_request(None, true, None)
                .custom_profile_properties
                .is_empty()
        );
    }
}

#[cfg(test)]
mod native_protocol_tests {
    use super::*;
    use crate::proto::*;

    #[tokio::test]
    #[allow(clippy::result_large_err)] // tungstenite fixes the handshake callback error type.
    async fn existing_sessions_and_new_windows_share_connection_without_input_rescans() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("api.sock");
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let command = "/bin/sh -c 'printf fixture'";
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_hdr_async(stream, |_: &tokio_tungstenite::tungstenite::handshake::server::Request, mut response: tokio_tungstenite::tungstenite::handshake::server::Response| {
                response.headers_mut().insert("Sec-WebSocket-Protocol", HeaderValue::from_static(SUBPROTOCOL));
                Ok(response)
            }).await.unwrap();
            let mut lists = 0;
            let mut inputs = 0;
            for _ in 0..4 {
                let Message::Binary(bytes) = ws.next().await.unwrap().unwrap() else {
                    panic!("binary request expected")
                };
                let request = ClientOriginatedMessage::decode(&bytes[..]).unwrap();
                let reply = match request.submessage.unwrap() {
                    Req::ListSessionsRequest(_) => {
                        lists += 1;
                        Resp::ListSessionsResponse(ListSessionsResponse {
                            windows: vec![list_sessions_response::Window {
                                window_id: Some("window-a".into()),
                                tabs: vec![list_sessions_response::Tab {
                                    tab_id: Some("tab-a".into()),
                                    root: Some(SplitTreeNode {
                                        links: vec![split_tree_node::SplitTreeLink {
                                            child: Some(Child::Session(SessionSummary {
                                                unique_identifier: Some("existing:opaque".into()),
                                                title: Some("fixture shell".into()),
                                                ..Default::default()
                                            })),
                                        }],
                                        ..Default::default()
                                    }),
                                    ..Default::default()
                                }],
                                ..Default::default()
                            }],
                            ..Default::default()
                        })
                    }
                    Req::SendTextRequest(r) => {
                        assert_eq!(r.session.as_deref(), Some("existing:opaque"));
                        inputs += 1;
                        Resp::SendTextResponse(SendTextResponse::default())
                    }
                    Req::CreateTabRequest(r) => {
                        assert!(r.window_id.is_none());
                        assert_eq!(
                            serde_json::from_str::<String>(
                                r.custom_profile_properties[1].json_value.as_ref().unwrap()
                            )
                            .unwrap(),
                            command
                        );
                        Resp::CreateTabResponse(CreateTabResponse {
                            session_id: Some("new-native-id".into()),
                            window_id: Some("new-window".into()),
                            ..Default::default()
                        })
                    }
                    other => panic!("unexpected request {other:?}"),
                };
                ws.send(Message::Binary(
                    ServerOriginatedMessage {
                        id: request.id,
                        submessage: Some(reply),
                    }
                    .encode_to_vec()
                    .into(),
                ))
                .await
                .unwrap();
            }
            assert_eq!((lists, inputs), (1, 2));
            ws.close(None).await.unwrap();
        });
        let client = Iterm2::connect(
            &path,
            &Cookie {
                cookie: "fixture".into(),
                key: "fixture".into(),
            },
            "fixture",
        )
        .await
        .unwrap();
        let service = crate::TerminalService {
            config: crate::TerminalConfig::default(),
            connection: Mutex::new(Some(client.clone())),
        };
        assert_eq!(service.sessions().await.unwrap()[0].id, "existing:opaque");
        service.input("existing:opaque", b"a").await.unwrap();
        service.input("existing:opaque", b"b").await.unwrap();
        assert_eq!(
            service
                .create_command(None, false, command)
                .await
                .unwrap()
                .id,
            "new-native-id"
        );
        server.await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            while !client.is_closed() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(
            client
                .send_text("existing:opaque", "must not replay")
                .await
                .is_err()
        );
    }
}

#[cfg(test)]
mod disconnect_tests {
    use super::*;
    #[tokio::test]
    async fn idle_view_detaches_and_transport_loss_ends_the_stream() {
        for disconnect in [false, true] {
            let (outbound, mut requests) = mpsc::channel::<Vec<u8>>(8);
            let (close, closed) = watch::channel(false);
            let pending: Pending = Arc::default();
            let replies = pending.clone();
            let responder = tokio::spawn(async move {
                while let Some(bytes) = requests.recv().await {
                    let request = ClientOriginatedMessage::decode(&bytes[..]).unwrap();
                    let reply = match request.submessage.unwrap() {
                        Req::NotificationRequest(_) => {
                            Resp::NotificationResponse(crate::proto::NotificationResponse::default())
                        }
                        Req::GetBufferRequest(_) => {
                            Resp::GetBufferResponse(crate::proto::GetBufferResponse::default())
                        }
                        other => panic!("unexpected request {other:?}"),
                    };
                    if let Some(tx) = replies.lock().await.remove(&request.id.unwrap()) {
                        let _ = tx.send(ServerOriginatedMessage {
                            id: request.id,
                            submessage: Some(reply),
                        });
                    }
                }
            });
            let client = Arc::new(Iterm2 {
                outbound,
                closed,
                pending,
                next_id: AtomicI64::new(1),
                notifications: broadcast::channel(8).0,
            });
            let (frames, mut rx) = mpsc::channel(1);
            let attach = tokio::spawn(async move { client.attach("fixture", 0, frames).await });
            assert!(rx.recv().await.unwrap().full);
            if disconnect {
                close.send(true).unwrap();
            } else {
                drop(rx);
            }
            let result = tokio::time::timeout(Duration::from_secs(1), attach)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(result.is_err(), disconnect);
            tokio::time::timeout(Duration::from_secs(1), responder)
                .await
                .unwrap()
                .unwrap();
        }
    }
}
