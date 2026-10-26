//! The web window (ADR 0026): one loopback port on this machine that stands
//! for exactly one remote origin, so a member with no route to that origin —
//! and no account on it — can read it through this machine.
//!
//! The window listens on no TCP port. A member reaches it through a service
//! stream of its own, handed to [`Window::serve`] inside the daemon with a
//! [`Gate`] that says, request by request, what that member may do; the owner
//! of the machine reaches it on a Unix socket only their user can open. So no
//! other process on the machine, and nothing on the network, can use it
//! without passing one of those two doors.
//!
//! The window is what keeps a member's stream from becoming the open relay
//! forward streams exist to avoid — it is built for one origin and asks for
//! no other. An absolute request target is refused, `CONNECT` is refused, and
//! a reply that came from anywhere else is refused rather than served.
//!
//! The window fetches nothing itself. Every request is handed to an
//! [`Upstream`] — in Muster, the person's own browser through the Agent Halo
//! extension, which already holds the session for the site. So the reader
//! needs no account and is given no credential: cookies stay in the browser,
//! and `Set-Cookie` never reaches the reader.
//!
//! This is not a web client for Muster, which ARCHITECTURE keeps out of
//! scope. Muster serves none of these bytes; it carries somebody else's site
//! to a device that cannot reach it.

use std::fmt;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::task::{JoinHandle, JoinSet};

/// The whole request head, including the blank line.
const MAX_HEAD: usize = 32 * 1024;
/// A request body travels to the upstream in one message, so it is bounded
/// well below what that carrier allows.
const MAX_BODY: usize = 512 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Origin(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// One request for the upstream to make.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fetch {
    pub method: String,
    /// The absolute URL on the window's own origin.
    pub url: String,
    /// Lowercase header names, hop-by-hop and credential headers removed.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// What the upstream came back with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    pub status: u16,
    /// Where the request ended, after any redirect the upstream followed.
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Whoever actually reaches the site. The window knows nothing of how: in
/// Muster this is the browser the person is signed in with.
pub trait Upstream: Send + Sync + 'static {
    fn fetch(&self, request: Fetch) -> Pin<Box<dyn Future<Output = Result<Reply, String>> + Send + '_>>;
}

/// What one reader may do through the window. The window does not decide
/// this; it asks the [`Gate`] it was handed, before every request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grant {
    /// Read the site: GET, HEAD, OPTIONS.
    Read,
    /// Act on it as the signed-in person, any method.
    Act,
}

/// Asked before every request on a connection, so a revoked scope, a policy
/// change or the screen locking takes effect on the next request rather than
/// the next connection. `Err` carries the reason the reader is shown.
pub type Gate = Arc<dyn Fn() -> Result<Grant, String> + Send + Sync>;

/// The one origin a window serves: scheme, host and port, nothing else. A
/// path is not part of an origin, and neither is a credential.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
    secure: bool,
    host: String,
    port: Option<u16>,
}

impl Origin {
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Whether `text` is a URL on this same origin. Two things turn on it:
    /// a `Location` back to where the page already is, and whether a reply
    /// came from the origin this window serves at all.
    pub fn covers(&self, text: &str) -> bool {
        let Some((secure, rest)) = text
            .strip_prefix("https://")
            .map(|rest| (true, rest))
            .or_else(|| text.strip_prefix("http://").map(|rest| (false, rest)))
        else {
            return false;
        };
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        secure == self.secure && authority.eq_ignore_ascii_case(&self.authority())
    }

    fn authority(&self) -> String {
        match self.port {
            Some(port) => format!("{}:{port}", self.host),
            None => self.host.clone(),
        }
    }
}

impl FromStr for Origin {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let refuse = |why: &str| Error::Origin(format!("{text:?} is not an origin: {why}"));
        let (secure, rest) = text
            .strip_prefix("https://")
            .map(|rest| (true, rest))
            .or_else(|| text.strip_prefix("http://").map(|rest| (false, rest)))
            .ok_or_else(|| refuse("it must start with https:// or http://"))?;
        let rest = rest.strip_suffix('/').unwrap_or(rest);
        if rest.contains('/') || rest.contains('?') || rest.contains('#') {
            return Err(refuse("an origin is a scheme, a host and a port, no path"));
        }
        if rest.contains('@') {
            return Err(refuse("a credential does not belong in an origin"));
        }
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) => {
                let port: u16 = port
                    .parse()
                    .map_err(|_| refuse("the port is not a number"))?;
                (host, Some(port))
            }
            None => (rest, None),
        };
        if host.is_empty() || !host.bytes().all(|b| b.is_ascii_graphic() && b != b'[') {
            return Err(refuse("the host is empty or not a name"));
        }
        Ok(Self {
            secure,
            host: host.to_ascii_lowercase(),
            port,
        })
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let scheme = if self.secure { "https" } else { "http" };
        write!(f, "{scheme}://{}", self.authority())
    }
}

/// A running window. Dropping it closes its socket and every connection the
/// socket accepted; streams handed to [`Window::serve`] end with their caller.
pub struct Window {
    socket: PathBuf,
    origin: Origin,
    upstream: Arc<dyn Upstream>,
    accept: JoinHandle<()>,
}

impl Window {
    /// Opens the window for `origin` on a Unix socket at `socket`, in a
    /// directory only this user can enter. A socket another process still
    /// serves is refused rather than replaced.
    pub async fn open(
        origin: Origin,
        socket: &Path,
        upstream: Arc<dyn Upstream>,
    ) -> Result<Self, Error> {
        use std::os::unix::fs::{DirBuilderExt, FileTypeExt, PermissionsExt};
        let dir = socket
            .parent()
            .ok_or_else(|| Error::Origin("the window's socket has no directory".into()))?;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        if let Ok(meta) = std::fs::symlink_metadata(socket) {
            if !meta.file_type().is_socket() {
                return Err(Error::Origin(format!(
                    "{} exists and is not a socket",
                    socket.display()
                )));
            }
            if std::os::unix::net::UnixStream::connect(socket).is_ok() {
                return Err(Error::Origin(format!(
                    "{} is already served",
                    socket.display()
                )));
            }
            std::fs::remove_file(socket)?;
        }
        let listener = UnixListener::bind(socket)?;
        std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))?;
        log::info!("web window {} serves {origin}", socket.display());
        let accept = tokio::spawn(accept(listener, origin.clone(), upstream.clone()));
        Ok(Self {
            socket: socket.to_path_buf(),
            origin,
            upstream,
            accept,
        })
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }

    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// Carries one reader's stream — a member's service stream, in Muster —
    /// until it ends. `gate` is asked before every request on it.
    pub async fn serve<S>(&self, stream: S, gate: Gate)
    where
        S: AsyncRead + AsyncWrite + Unpin + Send,
    {
        connection(stream, &self.origin, self.upstream.as_ref(), gate).await;
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        self.accept.abort();
        let _ = std::fs::remove_file(&self.socket);
    }
}

async fn accept(listener: UnixListener, origin: Origin, upstream: Arc<dyn Upstream>) {
    // Whoever opened this socket is this user: the directory admits no one
    // else. That is the machine's owner, who may do anything the browser may.
    let owner: Gate = Arc::new(|| Ok(Grant::Act));
    let mut open = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    let origin = origin.clone();
                    let upstream = upstream.clone();
                    let owner = owner.clone();
                    open.spawn(async move {
                        connection(stream, &origin, upstream.as_ref(), owner).await;
                    });
                }
                Err(error) => {
                    log::warn!("web window {origin} stopped accepting: {error}");
                    return;
                }
            },
            // Reaping finished connections; the window itself never ends here.
            Some(_) = open.join_next(), if !open.is_empty() => {}
        }
    }
}

/// What the window answers by itself, when a request is not one it will carry.
#[derive(Debug)]
struct Refusal {
    status: u16,
    reason: &'static str,
    text: String,
}

impl Refusal {
    fn new(status: u16, reason: &'static str, text: impl Into<String>) -> Self {
        Self {
            status,
            reason,
            text: text.into(),
        }
    }
}

async fn connection<S>(stream: S, origin: &Origin, upstream: &dyn Upstream, gate: Gate)
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (read, mut write) = tokio::io::split(stream);
    let mut reader = BufReader::new(read);
    loop {
        let head = match read_head(&mut reader).await {
            Ok(Some(head)) => head,
            Ok(None) => return,
            Err(refusal) => {
                let _ = refuse(&mut write, refusal).await;
                return;
            }
        };
        let keep = head.keep_alive();
        match carry(&mut reader, &mut write, origin, upstream, &gate, head).await {
            Ok(()) if keep => continue,
            Ok(()) => return,
            Err(refusal) => {
                let _ = refuse(&mut write, refusal).await;
                return;
            }
        }
    }
}

struct Head {
    method: String,
    target: String,
    version: String,
    headers: Vec<(String, String)>,
}

impl Head {
    fn get(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(held, _)| held == name)
            .map(|(_, value)| value.as_str())
    }

    fn keep_alive(&self) -> bool {
        let connection = self
            .get("connection")
            .unwrap_or_default()
            .to_ascii_lowercase();
        if connection.split(',').any(|token| token.trim() == "close") {
            return false;
        }
        self.version == "HTTP/1.1"
    }
}

/// Reads one request head. `Ok(None)` is a connection the client closed
/// between requests, which is ordinary.
async fn read_head<R>(reader: &mut BufReader<R>) -> Result<Option<Head>, Refusal>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut lines = Vec::new();
    let mut size = 0;
    loop {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .await
            .map_err(|error| Refusal::new(400, "Bad Request", format!("read: {error}")))?;
        if read == 0 {
            return if lines.is_empty() {
                Ok(None)
            } else {
                Err(Refusal::new(400, "Bad Request", "the head is unfinished"))
            };
        }
        size += read;
        if size > MAX_HEAD {
            return Err(Refusal::new(
                431,
                "Request Header Fields Too Large",
                "the head is larger than this window carries",
            ));
        }
        let line = line.trim_end_matches(['\r', '\n']).to_string();
        if line.is_empty() {
            break;
        }
        lines.push(line);
    }
    let mut lines = lines.into_iter();
    let request_line = lines
        .next()
        .ok_or_else(|| Refusal::new(400, "Bad Request", "no request line"))?;
    let mut parts = request_line.split(' ');
    let method = parts
        .next()
        .ok_or_else(|| Refusal::new(400, "Bad Request", "no method"))?
        .to_string();
    let target = parts
        .next()
        .ok_or_else(|| Refusal::new(400, "Bad Request", "no request target"))?
        .to_string();
    let version = parts.next().unwrap_or("HTTP/1.1").to_string();
    let mut headers = Vec::new();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return Err(Refusal::new(400, "Bad Request", "a header has no colon"));
        };
        if name.is_empty() || !name.bytes().all(is_token) {
            return Err(Refusal::new(
                400,
                "Bad Request",
                "a header name is not a name",
            ));
        }
        headers.push((name.to_ascii_lowercase(), value.trim().to_string()));
    }
    Ok(Some(Head {
        method,
        target,
        version,
        headers,
    }))
}

fn is_token(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

/// Checks the request against what a one-origin window will carry, and returns
/// the path to ask the origin for.
fn target_of(head: &Head) -> Result<&str, Refusal> {
    if head.method.eq_ignore_ascii_case("CONNECT") {
        return Err(Refusal::new(
            405,
            "Method Not Allowed",
            "this is a window onto one site, not a proxy: CONNECT is refused",
        ));
    }
    if head.method.is_empty() || !head.method.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(Refusal::new(
            400,
            "Bad Request",
            "the method is not a method",
        ));
    }
    if !head.target.starts_with('/') {
        return Err(Refusal::new(
            400,
            "Bad Request",
            "this window serves one origin, so a request names a path, not a URL",
        ));
    }
    if !head.target.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(Refusal::new(400, "Bad Request", "the path is not a path"));
    }
    if head.get("upgrade").is_some() {
        return Err(Refusal::new(
            501,
            "Not Implemented",
            "this window carries requests and replies, not protocol upgrades",
        ));
    }
    Ok(&head.target)
}

/// Headers a connection owns and a hop never passes on (RFC 9110 §7.6.1).
fn hop_by_hop(name: &str) -> bool {
    matches!(
        name,
        "connection"
            | "proxy-connection"
            | "keep-alive"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "proxy-authenticate"
            | "proxy-authorization"
    )
}

/// Methods that only read. Anything else may change the site, as the person.
fn reads(method: &str) -> bool {
    matches!(method, "GET" | "HEAD" | "OPTIONS")
}

/// A request header the window passes on; `None` drops it.
fn forward(name: &str) -> bool {
    // Host belongs to the origin, not to whoever reached the window, and the
    // session is the upstream browser's: a reader neither sends a cookie nor
    // needs one.
    !hop_by_hop(name)
        && !matches!(
            name,
            "host" | "content-length" | "cookie" | "authorization" | "accept-encoding"
        )
}

/// One reply header as the window passes it on; `None` drops it.
///
/// The page is served from a loopback port now, and the credentials that
/// fetched it are the browser's, not the reader's.
fn rewrite(origin: &Origin, name: &str, value: &str) -> Option<String> {
    match name {
        _ if hop_by_hop(name) => None,
        // The window frames and encodes the reply itself.
        "content-length" | "content-encoding" => None,
        // The session stays in the browser that holds it. Handing a reader
        // this site's cookie would hand over the account.
        "set-cookie" => None,
        // A loopback port must never be pinned to HTTPS: the pin is kept per
        // host, so one site's header would break every other local port.
        "strict-transport-security" => None,
        // These name the origin the page no longer has, so they would forbid
        // the page its own scripts and styles.
        "content-security-policy" | "content-security-policy-report-only" => None,
        // Back to where the page already is: keep it inside the window. A
        // `Location` anywhere else is passed on untouched — the window will
        // not fetch it, and the browser leaving is visible, not a quiet relay.
        "location" if origin.covers(value) => Some(path_of(value)),
        _ => Some(value.to_string()),
    }
}

/// The path and query of an absolute URL, as a request target.
fn path_of(url: &str) -> String {
    let rest = url.split_once("//").map(|(_, rest)| rest).unwrap_or(url);
    match rest.find('/') {
        Some(at) => rest[at..].split('#').next().unwrap_or("/").to_string(),
        None => "/".to_string(),
    }
}

/// Carries one request to the origin and the reply back.
async fn carry<R, W>(
    reader: &mut BufReader<R>,
    write: &mut W,
    origin: &Origin,
    upstream: &dyn Upstream,
    gate: &Gate,
    head: Head,
) -> Result<(), Refusal>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let target = target_of(&head)?.to_string();
    // Asked now, not when the stream opened: a scope revoked or a screen
    // locked since then counts from this request on.
    let grant = gate().map_err(|why| Refusal::new(403, "Forbidden", why))?;
    if grant == Grant::Read && !reads(&head.method) {
        return Err(Refusal::new(
            403,
            "Forbidden",
            "this reader may read the site through the window, not act on it",
        ));
    }
    let body = read_body(reader, &head).await?.unwrap_or_default();
    let request = Fetch {
        method: head.method.clone(),
        url: format!("{origin}{target}"),
        headers: head
            .headers
            .iter()
            .filter(|(name, _)| forward(name))
            .cloned()
            .collect(),
        body,
    };
    let reply = upstream
        .fetch(request)
        .await
        .map_err(|error| Refusal::new(502, "Bad Gateway", error))?;

    // Where the request ended decides what the reader is told. The upstream
    // follows redirects as the browser would, so the window checks the
    // destination afterwards instead of trusting the route.
    if !origin.covers(&reply.url) {
        return Err(Refusal::new(
            502,
            "Bad Gateway",
            format!("the request ended off {origin}; this window serves one origin"),
        ));
    }
    let landed = path_of(&reply.url);
    if landed != target {
        // Send the reader to where it actually ended, so its address and its
        // relative links are the ones the page was written for. A request that
        // was not a GET sees 303: the next one is a GET, as a browser does.
        let status = if matches!(head.method.as_str(), "GET" | "HEAD") {
            "302 Found"
        } else {
            "303 See Other"
        };
        let out = format!(
            "HTTP/1.1 {status}\r\nlocation: {landed}\r\ncontent-length: 0\r\n{}\r\n",
            connection_header(&head)
        );
        write
            .write_all(out.as_bytes())
            .await
            .map_err(|error| Refusal::new(0, "", format!("write: {error}")))?;
        let _ = write.flush().await;
        return Ok(());
    }

    let bodyless = head.method == "HEAD" || matches!(reply.status, 204 | 304);
    let mut out = format!("HTTP/1.1 {} {}\r\n", reply.status, reason(reply.status));
    for (name, value) in &reply.headers {
        let name = name.to_ascii_lowercase();
        if let Some(value) = rewrite(origin, &name, value) {
            out.push_str(&format!("{name}: {value}\r\n"));
        }
    }
    if !bodyless {
        out.push_str(&format!("content-length: {}\r\n", reply.body.len()));
    }
    out.push_str(&connection_header(&head));
    out.push_str("\r\n");
    write
        .write_all(out.as_bytes())
        .await
        .map_err(|error| Refusal::new(0, "", format!("write: {error}")))?;
    if !bodyless {
        write
            .write_all(&reply.body)
            .await
            .map_err(|error| Refusal::new(0, "", format!("write: {error}")))?;
    }
    let _ = write.flush().await;
    Ok(())
}

fn connection_header(head: &Head) -> String {
    if head.keep_alive() {
        "connection: keep-alive\r\n".into()
    } else {
        "connection: close\r\n".into()
    }
}

/// Enough of the registry to name the statuses a site actually answers with;
/// the number is what matters to the reader's browser.
fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        410 => "Gone",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "",
    }
}

async fn read_body<R>(reader: &mut BufReader<R>, head: &Head) -> Result<Option<Vec<u8>>, Refusal>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let length = head.get("content-length");
    let chunked = head
        .get("transfer-encoding")
        .is_some_and(|value| value.to_ascii_lowercase().contains("chunked"));
    if length.is_some() && chunked {
        return Err(Refusal::new(
            400,
            "Bad Request",
            "a body is framed one way, not two",
        ));
    }
    if let Some(length) = length {
        let length: usize = length
            .trim()
            .parse()
            .map_err(|_| Refusal::new(400, "Bad Request", "the body length is not a number"))?;
        if length > MAX_BODY {
            return Err(Refusal::new(
                413,
                "Content Too Large",
                "the body is larger than this window carries",
            ));
        }
        let mut body = vec![0u8; length];
        reader
            .read_exact(&mut body)
            .await
            .map_err(|error| Refusal::new(400, "Bad Request", format!("body: {error}")))?;
        return Ok(Some(body));
    }
    if !chunked {
        return Ok(None);
    }
    let mut body = Vec::new();
    loop {
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .await
            .map_err(|error| Refusal::new(400, "Bad Request", format!("chunk: {error}")))?;
        let size = line.trim().split(';').next().unwrap_or("");
        let size = usize::from_str_radix(size, 16)
            .map_err(|_| Refusal::new(400, "Bad Request", "a chunk size is not a number"))?;
        if size == 0 {
            break;
        }
        if body.len() + size > MAX_BODY {
            return Err(Refusal::new(
                413,
                "Content Too Large",
                "the body is larger than this window carries",
            ));
        }
        let at = body.len();
        body.resize(at + size, 0);
        let mut end = [0u8; 2];
        reader
            .read_exact(&mut body[at..])
            .await
            .and(reader.read_exact(&mut end).await)
            .map_err(|error| Refusal::new(400, "Bad Request", format!("chunk: {error}")))?;
    }
    // Trailers, up to the blank line that ends them.
    loop {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .await
            .map_err(|error| Refusal::new(400, "Bad Request", format!("trailer: {error}")))?;
        if read == 0 || line.trim_end_matches(['\r', '\n']).is_empty() {
            break;
        }
    }
    Ok(Some(body))
}

async fn refuse<W>(write: &mut W, refusal: Refusal) -> std::io::Result<()>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    if refusal.status == 0 {
        return Ok(());
    }
    log::info!("web window refused a request: {}", refusal.text);
    let body = format!("{}\n", refusal.text);
    let head = format!(
        "HTTP/1.1 {} {}\r\ncontent-type: text/plain; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        refusal.status,
        refusal.reason,
        body.len()
    );
    write.write_all(head.as_bytes()).await?;
    write.write_all(body.as_bytes()).await?;
    write.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin(text: &str) -> Origin {
        text.parse().expect("an origin")
    }

    #[test]
    fn an_origin_is_a_scheme_a_host_and_a_port() {
        assert_eq!(
            origin("https://example.test").to_string(),
            "https://example.test"
        );
        assert_eq!(
            origin("https://example.test/").to_string(),
            "https://example.test"
        );
        assert_eq!(
            origin("http://Example.Test:8080").to_string(),
            "http://example.test:8080"
        );
        for refused in [
            "example.test",
            "ftp://example.test",
            "https://example.test/path",
            "https://user:secret@example.invalid",
            "https://example.test:http",
            "https://",
        ] {
            assert!(
                refused.parse::<Origin>().is_err(),
                "{refused} should not parse as an origin"
            );
        }
    }

    #[test]
    fn only_the_windows_own_origin_counts_as_its_own() {
        let origin = origin("https://example.test");
        assert!(origin.covers("https://example.test/a/b?c"));
        assert!(origin.covers("https://EXAMPLE.test"));
        assert!(!origin.covers("http://example.test/a"));
        assert!(!origin.covers("https://example.test.evil/a"));
        assert!(!origin.covers("https://example.test:8443/a"));
        assert!(!origin.covers("/a/b"));
    }

    #[test]
    fn a_url_becomes_the_request_target_it_names() {
        assert_eq!(path_of("https://example.test/issues?page=2"), "/issues?page=2");
        assert_eq!(path_of("https://example.test"), "/");
        assert_eq!(path_of("https://example.test/a#b"), "/a");
    }

    /// The page arrives on a loopback port, fetched with somebody else's
    /// session. Four headers would work against it, and one would hand the
    /// reader the account.
    #[test]
    fn a_reply_is_rewritten_for_where_it_now_arrives() {
        let origin = origin("https://example.test");
        let rewrite = |name: &str, value: &str| rewrite(&origin, name, value);

        assert_eq!(
            rewrite("strict-transport-security", "max-age=63072000"),
            None
        );
        assert_eq!(rewrite("content-security-policy", "default-src 'self'"), None);
        assert_eq!(rewrite("connection", "keep-alive"), None);
        assert_eq!(rewrite("transfer-encoding", "chunked"), None);
        // The browser holds the session; the reader is given none of it.
        assert_eq!(rewrite("set-cookie", "s=1; Path=/; HttpOnly"), None);

        assert_eq!(
            rewrite("location", "https://example.test/issues?page=2"),
            Some("/issues?page=2".into())
        );
        assert_eq!(rewrite("location", "/issues"), Some("/issues".into()));
        // Somewhere else: passed on as it is. The window fetches nothing.
        assert_eq!(
            rewrite("location", "https://sso.example.test/login"),
            Some("https://sso.example.test/login".into())
        );
        assert_eq!(
            rewrite("content-type", "text/html"),
            Some("text/html".into())
        );
    }

    /// What a reader sends upstream, and what it cannot.
    #[test]
    fn a_reader_cannot_name_the_host_or_hand_over_a_cookie() {
        assert!(forward("accept"));
        assert!(forward("content-type"));
        for refused in ["host", "cookie", "authorization", "connection", "content-length"] {
            assert!(!forward(refused), "{refused} must not be passed on");
        }
    }

    fn head(text: &str) -> Head {
        let mut lines = text.lines();
        let mut parts = lines.next().expect("a request line").split(' ');
        Head {
            method: parts.next().unwrap_or("").to_string(),
            target: parts.next().unwrap_or("").to_string(),
            version: parts.next().unwrap_or("HTTP/1.1").to_string(),
            headers: lines
                .filter_map(|line| line.split_once(':'))
                .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_string()))
                .collect(),
        }
    }

    /// The window has one origin by construction. These are the ways a
    /// request could ask it for another, and each is refused rather than
    /// quietly rewritten.
    #[test]
    fn a_request_cannot_name_another_origin() {
        let absolute = target_of(&head("GET http://evil.test/ HTTP/1.1")).err();
        assert_eq!(absolute.map(|r| r.status), Some(400));
        let tunnel = target_of(&head("CONNECT evil.test:443 HTTP/1.1")).err();
        assert_eq!(tunnel.map(|r| r.status), Some(405));
        let upgrade = target_of(&head("GET /live HTTP/1.1\nUpgrade: websocket")).err();
        assert_eq!(upgrade.map(|r| r.status), Some(501));
        assert_eq!(target_of(&head("GET /issues HTTP/1.1")).ok(), Some("/issues"));
    }

    #[tokio::test]
    async fn a_chunked_request_body_reaches_the_origin_whole() {
        let head = head("POST /x HTTP/1.1\nTransfer-Encoding: chunked");
        let wire = b"4\r\nMust\r\n3\r\ner!\r\n0\r\n\r\n";
        let mut reader = BufReader::new(&wire[..]);
        let body = read_body(&mut reader, &head).await.expect("a body");
        assert_eq!(body.as_deref(), Some(&b"Muster!"[..]));
    }

    #[tokio::test]
    async fn a_body_framed_twice_is_refused() {
        let head = head("POST /x HTTP/1.1\nTransfer-Encoding: chunked\nContent-Length: 7");
        let mut reader = BufReader::new(&b""[..]);
        let refusal = read_body(&mut reader, &head).await.expect_err("refused");
        assert_eq!(refusal.status, 400);
    }
}
