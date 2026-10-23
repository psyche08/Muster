//! The window against a stub upstream: what it carries, what it refuses,
//! what it rewrites, what it never passes on in either direction, and who
//! may do what through it.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use muster_web::{Fetch, Gate, Grant, Origin, Reply, Upstream, Window};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

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

/// A socket path of its own per test, short enough for `sun_path`.
fn socket() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mw-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("w.sock")
}

async fn open(stub: Arc<Stub>) -> Window {
    Window::open(origin(), &socket(), stub)
        .await
        .expect("a window")
}

/// One request through the owner's socket, raw, so a test can write a
/// request line no client library would send.
async fn ask(window: &Window, request: &str) -> String {
    let mut stream = UnixStream::connect(window.socket())
        .await
        .expect("the window's socket");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("a written request");
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).await.expect("an answer");
    String::from_utf8_lossy(&answer).into_owned()
}

/// Requests from a member's stream, as the daemon hands it over: in process,
/// under the gate that member's grant decides.
async fn ask_as(window: &Window, gate: Gate, requests: &str) -> String {
    let (mut reader, served) = tokio::io::duplex(64 * 1024);
    let client = async {
        reader
            .write_all(requests.as_bytes())
            .await
            .expect("written requests");
        let mut answer = Vec::new();
        reader.read_to_end(&mut answer).await.expect("an answer");
        String::from_utf8_lossy(&answer).into_owned()
    };
    let ((), answer) = tokio::join!(window.serve(served, gate), client);
    answer
}

fn always(grant: Grant) -> Gate {
    Arc::new(move || Ok(grant))
}

#[tokio::test]
async fn a_page_comes_through_under_the_origins_own_url() {
    let stub = Stub::new(page());
    let window = open(stub.clone()).await;

    let answer = ask(
        &window,
        "GET /issues HTTP/1.1\r\nhost: localhost\r\naccept: text/html\r\ncookie: forged=1\r\nconnection: close\r\n\r\n",
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

/// No TCP port: the window is a socket only this user can open, and it does
/// not take over one that is still being served.
#[tokio::test]
async fn the_window_is_a_socket_only_its_user_can_open() {
    use std::os::unix::fs::PermissionsExt;
    let stub = Stub::new(page());
    let window = open(stub.clone()).await;

    let mode = |path: &std::path::Path| {
        std::fs::metadata(path).expect("metadata").permissions().mode() & 0o777
    };
    assert_eq!(mode(window.socket()), 0o600);
    assert_eq!(mode(window.socket().parent().expect("a directory")), 0o700);

    let second = Window::open(origin(), window.socket(), stub).await;
    assert!(second.is_err(), "a live window is not replaced");

    let path = window.socket().to_path_buf();
    drop(window);
    assert!(!path.exists(), "closing the window removes its socket");
}

/// The page arrives somewhere other than its origin, fetched with the
/// browser's session. A pin would reach every other local service, a policy
/// would forbid the page its own scripts, and the cookie would hand over the
/// account.
#[tokio::test]
async fn the_reply_is_fit_for_where_it_arrives_and_keeps_the_session() {
    let stub = Stub::new(page());
    let window = open(stub).await;

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
    let window = open(stub.clone()).await;

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
    let window = open(stub).await;

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
    let window = open(stub).await;

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
    let window = open(stub.clone()).await;

    let answer = ask(
        &window,
        "GET /issues HTTP/1.1\r\nhost: x\r\n\r\nGET /issues HTTP/1.1\r\nhost: x\r\nconnection: close\r\n\r\n",
    )
    .await;

    assert_eq!(answer.matches("HTTP/1.1 200 OK").count(), 2, "{answer}");
    assert_eq!(stub.asked().len(), 2);
}

/// A member who may only read gets the page, and is refused anything that
/// would act on the site as the person — before the browser is asked.
#[tokio::test]
async fn a_reader_may_read_but_not_act() {
    let stub = Stub::new(page());
    let window = open(stub.clone()).await;

    let read = ask_as(
        &window,
        always(Grant::Read),
        "GET /issues HTTP/1.1\r\nconnection: close\r\n\r\n",
    )
    .await;
    assert!(read.starts_with("HTTP/1.1 200 "), "{read}");

    let act = ask_as(
        &window,
        always(Grant::Read),
        "POST /issues HTTP/1.1\r\ncontent-length: 2\r\nconnection: close\r\n\r\nhi",
    )
    .await;
    assert!(act.starts_with("HTTP/1.1 403 "), "{act}");
    assert!(act.contains("not act on it"), "{act}");
    assert_eq!(stub.asked().len(), 1, "only the read reached the browser");

    let acted = ask_as(
        &window,
        always(Grant::Act),
        "POST /issues HTTP/1.1\r\ncontent-length: 2\r\nconnection: close\r\n\r\nhi",
    )
    .await;
    assert!(acted.starts_with("HTTP/1.1 200 "), "{acted}");
    assert_eq!(stub.asked()[1].body, b"hi");
}

/// The gate is asked per request, so a grant withdrawn while a connection is
/// open — a scope revoked, the screen locked — stops the very next request.
#[tokio::test]
async fn a_grant_withdrawn_mid_connection_stops_the_next_request() {
    let stub = Stub::new(page());
    let window = open(stub.clone()).await;

    let asked = Arc::new(AtomicUsize::new(0));
    let counter = asked.clone();
    let gate: Gate = Arc::new(move || {
        if counter.fetch_add(1, Ordering::Relaxed) == 0 {
            Ok(Grant::Read)
        } else {
            Err("the screen is locked and this machine does not offer control while locked".into())
        }
    });

    let answer = ask_as(
        &window,
        gate,
        "GET /issues HTTP/1.1\r\nhost: x\r\n\r\nGET /issues HTTP/1.1\r\nhost: x\r\n\r\n",
    )
    .await;

    assert!(answer.starts_with("HTTP/1.1 200 "), "{answer}");
    assert!(answer.contains("HTTP/1.1 403 Forbidden"), "{answer}");
    assert!(answer.contains("screen is locked"), "{answer}");
    assert_eq!(stub.asked().len(), 1);
}
