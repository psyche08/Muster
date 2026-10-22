//! CatDesk's local command socket (`~/.catdesk/catdesk.sock`, owner-only):
//! the app's own `catdesk` CLI speaks HTTP/1.1 over it. Muster asks it only
//! read-only questions — which process serves it, the sessions with their
//! live status and project paths, and the session the main window shows.
//! It is never an input channel: sessions change only through
//! Accessibility (ARCHITECTURE §3.6.2), and nothing here grants any.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

use crate::controller::ProcessIdentity;
use crate::model::SessionState;

const TIMEOUT: Duration = Duration::from_millis(800);
const MAX_RESPONSE: usize = 4 << 20;

/// The socket, when the app created it.
pub fn socket() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("HOME")?).join(".catdesk/catdesk.sock");
    path.exists().then_some(path)
}

/// A session as the running app reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppSession {
    pub id: String,
    pub title: String,
    /// Loading, streaming or waiting for an approval in the app right now.
    pub running: bool,
    pub project: Option<PathBuf>,
    pub current: bool,
}

/// What the running app reported, bound to the process that serves it
/// (musterd verifies that process before handing this to the service).
#[derive(Clone, Debug)]
pub struct Runtime {
    pub process: ProcessIdentity,
    pub sessions: Vec<AppSession>,
}

impl Runtime {
    pub fn activity(&self, session: &AppSession) -> SessionState {
        if session.running {
            SessionState::Running
        } else {
            SessionState::Idle
        }
    }
}

/// The pid of the GUI process serving the socket.
pub fn health(socket: &Path) -> Result<u32, String> {
    let (status, body) = call(socket, "GET", "/cli/health", None)?;
    let value: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    if status != 200 || value["status"] != "ok" {
        return Err(format!("CatDesk health returned {status}"));
    }
    value["pid"]
        .as_u64()
        .and_then(|p| u32::try_from(p).ok())
        .filter(|p| *p > 1)
        .ok_or_else(|| "CatDesk health has no pid".into())
}

pub fn sessions(socket: &Path) -> Result<Vec<AppSession>, String> {
    let result = session_action(socket, "list")?.ok_or("CatDesk listed no sessions")?;
    let list = result
        .as_array()
        .ok_or("CatDesk session list is not a list")?;
    Ok(list.iter().filter_map(parse_session).collect())
}

/// The session the main window shows; `None` when it shows none.
pub fn current(socket: &Path) -> Result<Option<String>, String> {
    Ok(session_action(socket, "current")?
        .as_ref()
        .and_then(parse_session)
        .filter(|s| s.current)
        .map(|s| s.id))
}

fn parse_session(v: &Value) -> Option<AppSession> {
    let id = v["sessionId"].as_str()?;
    if !crate::viewer::is_uuid(id) {
        return None;
    }
    Some(AppSession {
        id: id.to_ascii_lowercase(),
        title: v["title"].as_str().unwrap_or("").to_string(),
        running: v["status"] == "running",
        project: v["projectPath"]
            .as_str()
            .filter(|p| p.starts_with('/'))
            .map(PathBuf::from),
        current: v["isCurrent"] == true,
    })
}

/// `/cli/session`: its `result` is JSON text inside the JSON reply.
fn session_action(socket: &Path, action: &str) -> Result<Option<Value>, String> {
    let body = serde_json::json!({"action": "session", "sessionAction": action}).to_string();
    let (status, reply) = call(socket, "POST", "/cli/session", Some(&body))?;
    let reply: Value = serde_json::from_slice(&reply).map_err(|e| e.to_string())?;
    if reply["success"] != true {
        let error = reply["error"].as_str().unwrap_or("");
        // The documented answer when the main window shows no session.
        if action == "current" && error == "No active session" {
            return Ok(None);
        }
        return Err(format!(
            "CatDesk session {action} failed ({status}): {error}"
        ));
    }
    let result = reply["data"]["result"]
        .as_str()
        .ok_or("CatDesk reply has no result")?;
    serde_json::from_str(result)
        .map(Some)
        .map_err(|e| e.to_string())
}

#[cfg(unix)]
fn call(
    socket: &Path,
    method: &str,
    path: &str,
    body: Option<&str>,
) -> Result<(u16, Vec<u8>), String> {
    use std::io::{Read, Write};
    let mut stream = std::os::unix::net::UnixStream::connect(socket)
        .map_err(|e| format!("CatDesk socket: {e}"))?;
    stream
        .set_read_timeout(Some(TIMEOUT))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(TIMEOUT))
        .map_err(|e| e.to_string())?;
    let body = body.unwrap_or("");
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("CatDesk socket: {e}"))?;
    let mut raw = Vec::new();
    stream
        .take(MAX_RESPONSE as u64 + 1)
        .read_to_end(&mut raw)
        .map_err(|e| format!("CatDesk socket: {e}"))?;
    if raw.len() > MAX_RESPONSE {
        return Err("CatDesk reply too large".into());
    }
    parse_response(&raw)
}

#[cfg(not(unix))]
fn call(
    _socket: &Path,
    _method: &str,
    _path: &str,
    _body: Option<&str>,
) -> Result<(u16, Vec<u8>), String> {
    Err("CatDesk runs on macOS only".into())
}

/// Status and body of an HTTP/1.1 reply read to EOF, chunked or not.
fn parse_response(raw: &[u8]) -> Result<(u16, Vec<u8>), String> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("CatDesk reply has no header end")?;
    let head =
        std::str::from_utf8(&raw[..split]).map_err(|_| "CatDesk reply header is not text")?;
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or("CatDesk reply has no status")?;
    let chunked = lines.any(|l| {
        l.split_once(':').is_some_and(|(k, v)| {
            k.trim().eq_ignore_ascii_case("transfer-encoding")
                && v.trim().eq_ignore_ascii_case("chunked")
        })
    });
    let body = &raw[split + 4..];
    if !chunked {
        return Ok((status, body.to_vec()));
    }
    let mut out = Vec::new();
    let mut rest = body;
    loop {
        let end = rest
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or("CatDesk reply has a broken chunk")?;
        let size = std::str::from_utf8(&rest[..end])
            .ok()
            .and_then(|s| usize::from_str_radix(s.split(';').next()?.trim(), 16).ok())
            .ok_or("CatDesk reply has a bad chunk size")?;
        rest = &rest[end + 2..];
        if size == 0 {
            return Ok((status, out));
        }
        if rest.len() < size + 2 {
            return Err("CatDesk reply was cut short".into());
        }
        out.extend_from_slice(&rest[..size]);
        rest = &rest[size + 2..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_chunked_and_plain_replies() {
        let mut raw = b"HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
        for chunk in [r#"{"success":false,"error":"#, r#""No active session"}"#] {
            raw.extend_from_slice(format!("{:x}\r\n{chunk}\r\n", chunk.len()).as_bytes());
        }
        raw.extend_from_slice(b"0\r\n\r\n");
        let (status, body) = parse_response(&raw).unwrap();
        assert_eq!(status, 400);
        assert_eq!(body, br#"{"success":false,"error":"No active session"}"#);
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}";
        assert_eq!(parse_response(raw).unwrap(), (200, b"{}".to_vec()));
        assert!(
            parse_response(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nff\r\nab")
                .is_err()
        );
    }

    #[test]
    fn parses_reported_sessions() {
        let v = serde_json::json!({"sessionId":"E6841C79-D515-4C39-AA56-3B70C404B093","title":"t",
            "status":"running","projectPath":"/Users/me/p","isCurrent":true});
        let s = parse_session(&v).unwrap();
        assert_eq!(s.id, "e6841c79-d515-4c39-aa56-3b70c404b093");
        assert!(s.running && s.current);
        assert_eq!(s.project.as_deref(), Some(Path::new("/Users/me/p")));
        assert!(parse_session(&serde_json::json!({"sessionId":"../x"})).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn asks_the_socket_read_only_questions() {
        use std::io::{Read, Write};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catdesk.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for reply in [
                r#"{"status":"ok","pid":3489}"#.to_string(),
                serde_json::json!({"success":true,"data":{"result":
                    serde_json::json!([{"sessionId":"e6841c79-d515-4c39-aa56-3b70c404b093",
                        "title":"t","status":"success","projectPath":null,"isCurrent":false}]).to_string()}})
                .to_string(),
                r#"{"success":false,"error":"No active session"}"#.to_string(),
            ] {
                let (mut conn, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let n = conn.read(&mut buf).unwrap();
                requests.push(String::from_utf8_lossy(&buf[..n]).into_owned());
                write!(conn, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{reply}", reply.len()).unwrap();
            }
            requests
        });
        assert_eq!(health(&path).unwrap(), 3489);
        let list = sessions(&path).unwrap();
        assert_eq!(list.len(), 1);
        assert!(!list[0].running && list[0].project.is_none());
        assert_eq!(current(&path).unwrap(), None);
        let requests = server.join().unwrap();
        assert!(requests[0].starts_with("GET /cli/health "));
        assert!(requests[1].contains(r#"{"action":"session","sessionAction":"list"}"#));
        assert!(requests[2].contains(r#""sessionAction":"current""#));
    }
}
