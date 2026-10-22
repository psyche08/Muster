//! The window against a stub origin: what it carries, what it refuses, and
//! what it rewrites on the way back.

use std::sync::{Arc, Mutex};

use muster_web::{Origin, Window};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

/// A stub origin. It answers every request with `reply` and records the heads
/// it was asked for, so a test can show that nothing reached it.
struct Stub {
    port: u16,
    seen: Arc<Mutex<Vec<String>>>,
}

async fn stub(reply: &str) -> Stub {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a stub port");
    let port = listener.local_addr().expect("an address").port();
    // So a reply can name the stub's own origin, which it only now knows.
    let reply = Arc::new(reply.replace("{origin}", &format!("http://127.0.0.1:{port}")));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let record = record.clone();
            let reply = reply.clone();
            tokio::spawn(async move {
                let (read, mut write) = stream.into_split();
                let mut reader = BufReader::new(read);
                let mut head = String::new();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                        return;
                    }
                    if line.trim_end_matches(['\r', '\n']).is_empty() {
                        break;
                    }
                    head.push_str(&line);
                }
                record.lock().unwrap().push(head);
                let _ = write.write_all(reply.as_bytes()).await;
                let _ = write.flush().await;
            });
        }
    });
    Stub { port, seen }
}

impl Stub {
    fn origin(&self) -> Origin {
        format!("http://127.0.0.1:{}", self.port)
            .parse()
            .expect("an origin")
    }

    fn seen(&self) -> Vec<String> {
        self.seen.lock().unwrap().clone()
    }
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
    stream
        .read_to_end(&mut answer)
        .await
        .expect("an answer");
    String::from_utf8_lossy(&answer).into_owned()
}

const PAGE: &str = "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: 7\r\nstrict-transport-security: max-age=63072000\r\nset-cookie: s=1; Domain=example.test; Path=/; Secure\r\nconnection: close\r\n\r\nMuster!";

#[tokio::test]
async fn a_page_comes_through_the_window_with_the_origins_own_host() {
    let stub = stub(PAGE).await;
    let window = Window::open(stub.origin(), 0).await.expect("a window");

    let answer = ask(
        &window,
        "GET /issues?page=2 HTTP/1.1\r\nhost: 127.0.0.1\r\nconnection: close\r\n\r\n",
    )
    .await;

    assert!(answer.starts_with("HTTP/1.1 200 OK\r\n"), "{answer}");
    assert!(answer.ends_with("Muster!"), "{answer}");
    let seen = stub.seen();
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert!(seen[0].starts_with("GET /issues?page=2 "), "{:?}", seen[0]);
    assert!(
        seen[0].contains(&format!("host: 127.0.0.1:{}", stub.port)),
        "the origin is asked under its own host: {:?}",
        seen[0]
    );
}

/// The page now lives on a loopback port. A pin would break every other port
/// on this machine, and the cookie's own attributes would keep it from being
/// sent back at all.
#[tokio::test]
async fn the_reply_is_fit_for_the_port_it_arrives_on() {
    let stub = stub(PAGE).await;
    let window = Window::open(stub.origin(), 0).await.expect("a window");

    let answer = ask(&window, "GET / HTTP/1.1\r\nconnection: close\r\n\r\n").await;

    assert!(
        !answer.to_ascii_lowercase().contains("strict-transport-security"),
        "{answer}"
    );
    assert!(answer.contains("set-cookie: s=1; Path=/\r\n"), "{answer}");
}

/// The two ways a request could ask the window for a second origin.
#[tokio::test]
async fn a_request_naming_another_origin_never_reaches_anything() {
    let stub = stub(PAGE).await;
    let window = Window::open(stub.origin(), 0).await.expect("a window");

    let absolute = ask(
        &window,
        "GET http://example.test/ HTTP/1.1\r\nhost: example.test\r\n\r\n",
    )
    .await;
    assert!(absolute.starts_with("HTTP/1.1 400 "), "{absolute}");

    let tunnel = ask(&window, "CONNECT example.test:443 HTTP/1.1\r\n\r\n").await;
    assert!(tunnel.starts_with("HTTP/1.1 405 "), "{tunnel}");

    assert!(stub.seen().is_empty(), "the origin was asked: {:?}", stub.seen());
}

/// A reply that points somewhere else is passed on, not followed: the window
/// fetches one origin and nothing it is told to.
#[tokio::test]
async fn a_redirect_off_the_origin_is_passed_on_not_followed() {
    let stub = stub(
        "HTTP/1.1 302 Found\r\nlocation: http://example.test/login\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
    )
    .await;
    let window = Window::open(stub.origin(), 0).await.expect("a window");

    let answer = ask(&window, "GET / HTTP/1.1\r\nconnection: close\r\n\r\n").await;

    assert!(answer.starts_with("HTTP/1.1 302 "), "{answer}");
    assert!(answer.contains("location: http://example.test/login"), "{answer}");
    assert_eq!(stub.seen().len(), 1, "only the window's own origin was asked");
}

/// A redirect back to where the page already is stays inside the window,
/// since the browser knows the page by its loopback port.
#[tokio::test]
async fn a_redirect_back_to_the_origin_stays_in_the_window() {
    let stub = stub(
        "HTTP/1.1 302 Found\r\nlocation: {origin}/issues\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
    )
    .await;
    let window = Window::open(stub.origin(), 0).await.expect("a window");

    let answer = ask(&window, "GET / HTTP/1.1\r\nconnection: close\r\n\r\n").await;

    assert!(answer.starts_with("HTTP/1.1 302 "), "{answer}");
    assert!(answer.contains("location: /issues\r\n"), "{answer}");
}

/// Loopback only, by construction: the window binds no other address.
#[tokio::test]
async fn the_window_is_bound_to_loopback() {
    let stub = stub(PAGE).await;
    let window = Window::open(stub.origin(), 0).await.expect("a window");
    assert!(window.local_addr().ip().is_loopback());
}
