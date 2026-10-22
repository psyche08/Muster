//! The web window (ADR 0026): one loopback port on this machine that stands
//! for exactly one remote origin, so a member with no route to that origin can
//! read it through this machine.
//!
//! A member reaches the window the way phones reach SSH — a forward stream to
//! one of this machine's own loopback ports (ARCHITECTURE §3.5), decided by
//! the same `net:` scope and open-port policy. The window itself is what keeps
//! that from becoming the open relay forward streams exist to avoid: it is
//! built for one origin and dials no other. The request's own text cannot
//! redirect it (an absolute request target is refused, `CONNECT` is refused),
//! and the client follows no redirects, so a reply cannot either.
//!
//! This is not a web client for Muster, which ARCHITECTURE keeps out of
//! scope. Muster serves none of these bytes; it carries somebody else's site
//! to a device that cannot reach it.

use std::fmt;
use std::net::{Ipv4Addr, SocketAddr};
use std::str::FromStr;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::{JoinHandle, JoinSet};

/// The whole request head, including the blank line.
const MAX_HEAD: usize = 32 * 1024;
/// A request body is held in memory before it is sent on, so it is bounded.
/// Replies are streamed and have no such limit.
const MAX_BODY: usize = 32 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Origin(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("http client: {0}")]
    Client(String),
}

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

    /// Whether `text` names this same origin, for the one rewrite that needs
    /// to know: a `Location` back to where the page already is.
    fn covers(&self, text: &str) -> bool {
        let Some(rest) = text
            .strip_prefix("https://")
            .map(|rest| (true, rest))
            .or_else(|| text.strip_prefix("http://").map(|rest| (false, rest)))
        else {
            return false;
        };
        let (secure, rest) = rest;
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

/// A running window. Dropping it closes the port and every connection on it.
pub struct Window {
    local: SocketAddr,
    origin: Origin,
    accept: JoinHandle<()>,
}

impl Window {
    /// Opens `port` on loopback for `origin`. Port 0 asks the OS for one.
    pub async fn open(origin: Origin, port: u16) -> Result<Self, Error> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).await?;
        let local = listener.local_addr()?;
        // No redirect is ever followed: that, and refusing absolute request
        // targets, is what holds the window to its one origin.
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| Error::Client(error.to_string()))?;
        log::info!("web window {local} serves {origin}");
        let accept = tokio::spawn(accept(listener, origin.clone(), client));
        Ok(Self {
            local,
            origin,
            accept,
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local
    }

    pub fn origin(&self) -> &Origin {
        &self.origin
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        self.accept.abort();
    }
}

async fn accept(listener: TcpListener, origin: Origin, client: reqwest::Client) {
    let mut open = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    open.spawn(connection(stream, origin.clone(), client.clone()));
                }
                Err(error) => {
                    log::warn!("web window {} stopped accepting: {error}", origin);
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

async fn connection(stream: TcpStream, origin: Origin, client: reqwest::Client) {
    let (read, mut write) = stream.into_split();
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
        match carry(&mut reader, &mut write, &origin, &client, head).await {
            Ok(true) if keep => continue,
            Ok(_) => return,
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
        let connection = self.get("connection").unwrap_or_default().to_ascii_lowercase();
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
            return Err(Refusal::new(400, "Bad Request", "a header name is not a name"));
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
    if head.method.is_empty() || !head.method.bytes().all(is_token) {
        return Err(Refusal::new(400, "Bad Request", "the method is not a method"));
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

/// One reply header as the window passes it on; `None` drops it.
///
/// Three kinds are rewritten, because the page now lives on a loopback port
/// and no longer on the origin it came from.
fn rewrite(origin: &Origin, name: &str, value: &str) -> Option<String> {
    match name {
        _ if hop_by_hop(name) => None,
        // The window frames the reply itself.
        "content-length" => Some(value.to_string()),
        // A loopback port must never be pinned to HTTPS: the pin is kept per
        // host, so one site's header would break every other local port.
        "strict-transport-security" => None,
        // These name the origin the page no longer has, so they would forbid
        // the page its own scripts and styles.
        "content-security-policy" | "content-security-policy-report-only" => None,
        // `Domain` names the origin's host, which the browser will not accept
        // from a loopback page, and `Secure` would withhold the cookie from a
        // loopback port that has no TLS.
        "set-cookie" => Some(
            value
                .split(';')
                .map(str::trim)
                .filter(|part| {
                    let part = part.to_ascii_lowercase();
                    !part.starts_with("domain=") && part != "secure"
                })
                .collect::<Vec<_>>()
                .join("; "),
        ),
        // Back to where the page already is: keep it inside the window. A
        // `Location` anywhere else is passed on untouched — the window will
        // not fetch it, and the browser leaving is visible, not a silent relay.
        "location" if origin.covers(value) => {
            let rest = value
                .split_once("//")
                .map(|(_, rest)| rest)
                .unwrap_or(value);
            let path = rest.find('/').map(|at| &rest[at..]).unwrap_or("/");
            Some(path.to_string())
        }
        _ => Some(value.to_string()),
    }
}

/// Carries one request to the origin and the reply back. `Ok(true)` leaves the
/// connection usable for another request.
async fn carry<R, W>(
    reader: &mut BufReader<R>,
    write: &mut W,
    origin: &Origin,
    client: &reqwest::Client,
    head: Head,
) -> Result<bool, Refusal>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let target = target_of(&head)?.to_string();
    let body = read_body(reader, &head).await?;
    let method = reqwest::Method::from_bytes(head.method.as_bytes())
        .map_err(|_| Refusal::new(400, "Bad Request", "the method is not a method"))?;
    let bodyless_request = method == reqwest::Method::HEAD;

    let mut request = client.request(method, format!("{origin}{target}"));
    for (name, value) in &head.headers {
        // Host comes from the origin, not from whoever reached the window.
        if hop_by_hop(name) || name == "host" || name == "content-length" {
            continue;
        }
        request = request.header(name, value);
    }
    if let Some(body) = body {
        request = request.body(body);
    }
    let mut response = request.send().await.map_err(|error| {
        Refusal::new(
            502,
            "Bad Gateway",
            format!("{origin} did not answer: {error}"),
        )
    })?;

    let status = response.status();
    let bodyless = bodyless_request || status.as_u16() == 204 || status.as_u16() == 304;
    let length = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());

    let mut out = format!(
        "HTTP/1.1 {} {}\r\n",
        status.as_u16(),
        status.canonical_reason().unwrap_or("")
    );
    for (name, value) in response.headers() {
        let Ok(value) = value.to_str() else { continue };
        let name = name.as_str();
        if !bodyless && name == "content-length" && length.is_none() {
            continue;
        }
        if let Some(value) = rewrite(origin, name, value) {
            out.push_str(&format!("{name}: {value}\r\n"));
        }
    }
    let chunked = !bodyless && length.is_none();
    if chunked {
        out.push_str("transfer-encoding: chunked\r\n");
    }
    out.push_str(if head.keep_alive() {
        "connection: keep-alive\r\n\r\n"
    } else {
        "connection: close\r\n\r\n"
    });
    write
        .write_all(out.as_bytes())
        .await
        .map_err(|error| Refusal::new(0, "", format!("write: {error}")))?;

    if bodyless {
        return Ok(true);
    }
    let mut written = 0u64;
    loop {
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            // The head is already out, so the only honest end is to close:
            // a short body under a Content-Length would otherwise be read as
            // the start of the next reply.
            Err(error) => {
                log::info!("web window: {origin}{target} broke off: {error}");
                return Ok(false);
            }
        };
        if chunk.is_empty() {
            continue;
        }
        written += chunk.len() as u64;
        let wrote = if chunked {
            write
                .write_all(format!("{:x}\r\n", chunk.len()).as_bytes())
                .await
                .and(write.write_all(&chunk).await)
                .and(write.write_all(b"\r\n").await)
        } else {
            write.write_all(&chunk).await
        };
        if let Err(error) = wrote {
            log::debug!("web window: the reader of {origin}{target} left: {error}");
            return Ok(false);
        }
    }
    if chunked {
        let _ = write.write_all(b"0\r\n\r\n").await;
    } else if length != Some(written) {
        // Promised one length and sent another: this connection can no longer
        // be told apart from the next reply on it.
        log::info!("web window: {origin}{target} sent {written} of {length:?} bytes");
        return Ok(false);
    }
    let _ = write.flush().await;
    Ok(true)
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
        assert_eq!(origin("https://example.test").to_string(), "https://example.test");
        assert_eq!(origin("https://example.test/").to_string(), "https://example.test");
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

    /// A page served from a loopback port is no longer on the origin it came
    /// from, so three headers would work against it and one would reach
    /// beyond this window.
    #[test]
    fn a_reply_is_rewritten_for_the_port_it_now_arrives_on() {
        let origin = origin("https://example.test");
        let rewrite = |name: &str, value: &str| rewrite(&origin, name, value);

        assert_eq!(rewrite("strict-transport-security", "max-age=63072000"), None);
        assert_eq!(rewrite("content-security-policy", "default-src 'self'"), None);
        assert_eq!(rewrite("connection", "keep-alive"), None);
        assert_eq!(rewrite("transfer-encoding", "chunked"), None);

        assert_eq!(
            rewrite("set-cookie", "s=1; Domain=example.test; Path=/; Secure; HttpOnly"),
            Some("s=1; Path=/; HttpOnly".into())
        );
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
        assert_eq!(rewrite("content-type", "text/html"), Some("text/html".into()));
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

    /// The window has one origin by construction. These are the two ways a
    /// request could ask it for another, and both are refused rather than
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
