//! The window against a stub upstream: what it carries, what it refuses,
//! what it rewrites, and what it never passes on in either direction.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use muster_web::{Fetch, Origin, Reply, Upstream, Window};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Stands in for the browser: records what it was asked for, answers with a
/// canned reply, so a test can show that nothing reached it at all.
struct Stub {
    asked: Mutex<Vec<Fetch>>,
    reply: Mutex<Reply>,
}

impl Stub {
    fn new(reply: Reply) -> Arc<Self> {
        Arc::new(Self {
            asked: Mutex::new(Vec::new()),
            reply: Mutex::new(reply),
        })
    }

    fn asked(&self) -> Vec<Fetch> {
        self.asked.lock().unwrap().clone()
    }
}

impl Upstream for Stub {
    fn fetch(
        &self,
        request: Fetch,
    ) -> Pin<Box<dyn Future<Output = Result<Reply, String>> + Send + '_>> {
        self.asked.lock().unwrap().push(request);
        let reply = self.reply.lock().unwrap().clone();
        Box::pin(async move { Ok(reply) })
    }
}

fn page() -> Reply {
    Reply {
        status: 200,
        url: "https://example.test/issues".into(),
        headers: vec![
            ("content-type".into(), "text/html".into()),
            (
                "strict-transport-security".into(),
                "max-age=63072000".into(),
            ),
            ("content-security-policy".into(), "default-src 'self'".into()),
            ("set-cookie".into(), "session=secret; Path=/".into()),
        ],
        body: b"Muster!".to_vec(),
    }
}

fn origin() -> Origin {
    "https://example.test".parse().expect("an origin")
}

/// One request down the window's loopback port, raw, so a test can write a
/// request line no client library would send.
async fn ask(window: &Window, request: &str) -> String {
    let mut stream = TcpStream::connect(window.local_addr())
        .await
        .expect("the window's port");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("a written request");
    stream.flush().await.expect("a flushed request");
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).await.expect("an answer");
    String::from_utf8_lossy(&answer).into_owned()
}

#[tokio::test]
async fn a_page_comes_through_under_the_origins_own_url() {
    let stub = Stub::new(page());
    let window = Window::open(origin(), 0, stub.clone())
        .await
        .expect("a window");

    let answer = ask(
        &window,
        "GET /issues HTTP/1.1\r\nhost: 127.0.0.1\r\naccept: text/html\r\ncookie: forged=1\r\nconnection: close\r\n\r\n",
    )
    .await;

    assert!(answer.starts_with("HTTP/1.1 200 OK\r\n"), "{answer}");
    assert!(answer.contains("content-length: 7\r\n"), "{answer}");
    assert!(answer.ends_with("Muster!"), "{answer}");

    let asked = stub.asked();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].url, "https://example.test/issues");
    assert_eq!(asked[0].method, "GET");
    // The reader's own host and cookie are not what the browser is asked with.
    assert!(
        asked[0]
            .headers
            .iter()
            .all(|(name, _)| name != "host" && name != "cookie"),
        "{:?}",
        asked[0].headers
    );
    assert!(
        asked[0]
            .headers
            .iter()
            .any(|(name, value)| name == "accept" && value == "text/html")
    );
}

/// The page arrives on a loopback port, fetched with the browser's session.
/// A pin would reach every other local port, a policy would forbid the page
/// its own scripts, and the cookie would hand over the account.
#[tokio::test]
async fn the_reply_is_fit_for_the_port_it_arrives_on_and_keeps_the_session() {
    let stub = Stub::new(page());
    let window = Window::open(origin(), 0, stub).await.expect("a window");

    let answer = ask(&window, "GET /issues HTTP/1.1\r\nconnection: close\r\n\r\n")
        .await
        .to_ascii_lowercase();

    assert!(!answer.contains("strict-transport-security"), "{answer}");
    assert!(!answer.contains("content-security-policy"), "{answer}");
    assert!(!answer.contains("set-cookie"), "{answer}");
    assert!(answer.contains("content-type: text/html"), "{answer}");
}

/// The two ways a request could ask the window for a second origin.
#[tokio::test]
async fn a_request_naming_another_origin_never_reaches_the_browser() {
    let stub = Stub::new(page());
    let window = Window::open(origin(), 0, stub.clone())
        .await
        .expect("a window");

    let absolute = ask(
        &window,
        "GET http://example.invalid/ HTTP/1.1\r\nhost: example.invalid\r\n\r\n",
    )
    .await;
    assert!(absolute.starts_with("HTTP/1.1 400 "), "{absolute}");

    let tunnel = ask(&window, "CONNECT example.invalid:443 HTTP/1.1\r\n\r\n").await;
    assert!(tunnel.starts_with("HTTP/1.1 405 "), "{tunnel}");

    assert!(
        stub.asked().is_empty(),
        "the browser was asked: {:?}",
        stub.asked()
    );
}

/// The browser follows redirects as a browser does, so the window checks
/// where the request ended rather than trusting the route it took.
#[tokio::test]
async fn a_reply_from_another_origin_is_refused_not_served() {
    let stub = Stub::new(Reply {
        url: "https://sso.example.invalid/login".into(),
        ..page()
    });
    let window = Window::open(origin(), 0, stub).await.expect("a window");

    let answer = ask(&window, "GET /issues HTTP/1.1\r\nconnection: close\r\n\r\n").await;

    assert!(answer.starts_with("HTTP/1.1 502 "), "{answer}");
    assert!(answer.contains("serves one origin"), "{answer}");
    assert!(!answer.contains("Muster!"), "no foreign body is served");
}

/// Ending somewhere else on the same origin is a redirect the reader must
/// see, so that its address bar and the page's relative links agree.
#[tokio::test]
async fn ending_on_another_path_sends_the_reader_there() {
    let stub = Stub::new(Reply {
        url: "https://example.test/login?next=%2Fissues".into(),
        ..page()
    });
    let window = Window::open(origin(), 0, stub).await.expect("a window");

    let get = ask(&window, "GET /issues HTTP/1.1\r\nconnection: close\r\n\r\n").await;
    assert!(get.starts_with("HTTP/1.1 302 "), "{get}");
    assert!(get.contains("location: /login?next=%2Fissues\r\n"), "{get}");

    // After a POST the next request is a GET, which is what 303 says.
    let post = ask(
        &window,
        "POST /issues HTTP/1.1\r\ncontent-length: 2\r\nconnection: close\r\n\r\nhi",
    )
    .await;
    assert!(post.starts_with("HTTP/1.1 303 "), "{post}");
}

/// Definite framing, so one connection carries more than one request.
#[tokio::test]
async fn one_connection_carries_two_requests() {
    let stub = Stub::new(page());
    let window = Window::open(origin(), 0, stub.clone())
        .await
        .expect("a window");

    let mut stream = TcpStream::connect(window.local_addr())
        .await
        .expect("the window's port");
    stream
        .write_all(b"GET /issues HTTP/1.1\r\nhost: x\r\n\r\nGET /issues HTTP/1.1\r\nhost: x\r\nconnection: close\r\n\r\n")
        .await
        .expect("two written requests");
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).await.expect("two answers");

    let answer = String::from_utf8_lossy(&answer);
    assert_eq!(answer.matches("HTTP/1.1 200 OK").count(), 2, "{answer}");
    assert_eq!(stub.asked().len(), 2);
}

/// Loopback only, by construction: the window binds no other address.
#[tokio::test]
async fn the_window_is_bound_to_loopback() {
    let stub = Stub::new(page());
    let window = Window::open(origin(), 0, stub).await.expect("a window");
    assert!(window.local_addr().ip().is_loopback());
}
