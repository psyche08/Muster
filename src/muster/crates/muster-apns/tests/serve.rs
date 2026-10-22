//! The hook's request through the socket to a transport standing in for
//! APNs: environment fallback, memory, and the reply lines the hook reads.
use std::sync::{Arc, Mutex};

use anyhow::Result;
use muster_apns::{Environment, Outcome, Pusher, Request, Transport, classify, request, serve};
use tokio::net::UnixListener;

const SANDBOX_TOKEN: [u8; 32] = [2; 32];
const GONE_TOKEN: [u8; 32] = [9; 32];

/// Takes SANDBOX_TOKEN only on the sandbox, GONE_TOKEN nowhere (410) and
/// any other token only in production; records every attempt.
struct Fake(Arc<Mutex<Vec<Environment>>>);

impl Transport for Fake {
    async fn push(&self, env: Environment, token: &[u8]) -> Result<Outcome> {
        self.0.lock().unwrap().push(env);
        Ok(if token == GONE_TOKEN {
            Outcome::Unregistered
        } else if (token == SANDBOX_TOKEN) == (env == Environment::Sandbox) {
            Outcome::Delivered
        } else {
            Outcome::BadDeviceToken
        })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn falls_back_to_the_sandbox_and_remembers() {
    use Environment::{Production, Sandbox};
    let socket = std::env::temp_dir().join(format!("muster-apns-test-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket);
    let attempts = Arc::new(Mutex::new(Vec::new()));
    let pusher = Arc::new(Pusher::new(Fake(attempts.clone())));
    tokio::spawn(serve(UnixListener::bind(&socket).unwrap(), pusher));

    let hook = |token: [u8; 32]| {
        let socket = socket.clone();
        let r = Request {
            device: "ab".repeat(32),
            kind: 1,
            token: token.to_vec(),
        };
        tokio::task::spawn_blocking(move || request(&socket, &r).unwrap())
    };
    assert_eq!(hook([1; 32]).await.unwrap(), "ok production");
    assert_eq!(hook(SANDBOX_TOKEN).await.unwrap(), "ok sandbox");
    assert_eq!(hook(SANDBOX_TOKEN).await.unwrap(), "ok sandbox");
    assert_eq!(hook(GONE_TOKEN).await.unwrap(), "error Unregistered");
    assert_eq!(
        *attempts.lock().unwrap(),
        // The second sandbox push goes straight there; a 410 is not retried.
        [Production, Production, Sandbox, Sandbox, Production]
    );

    let r = Request {
        device: "ab".repeat(32),
        kind: 2,
        token: vec![1; 32],
    };
    let s = socket.clone();
    let answer = tokio::task::spawn_blocking(move || request(&s, &r).unwrap())
        .await
        .unwrap();
    assert_eq!(answer, "error kind 2 is not APNs");
    let _ = std::fs::remove_file(&socket);
}

#[test]
fn requests_are_checked() {
    let token = "0a".repeat(32);
    let ok = Request::parse(&format!("{} 1 {token}\n", "cd".repeat(32))).unwrap();
    assert_eq!((ok.kind, ok.token.len()), (1, 32));
    assert_eq!(Request::parse(&ok.line()).unwrap(), ok);
    for bad in [
        "",
        "ab 1",
        "ab 1 zz",
        "xyz 1 0a",
        "ab 1 0a extra",
        "ab 300 0a",
    ] {
        assert!(Request::parse(bad).is_err(), "{bad:?} accepted");
    }
    assert!(Request::parse(&format!("ab 1 {}", "00".repeat(513))).is_err());
}

#[test]
fn apns_answers_are_classified() {
    assert_eq!(classify(200, b""), Outcome::Delivered);
    assert_eq!(
        classify(400, br#"{"reason":"BadDeviceToken"}"#),
        Outcome::BadDeviceToken
    );
    assert_eq!(
        classify(410, br#"{"reason":"Unregistered","timestamp":1}"#),
        Outcome::Unregistered
    );
    assert_eq!(
        classify(403, br#"{"reason":"MissingProviderToken"}"#),
        Outcome::Failed {
            status: 403,
            reason: "MissingProviderToken".into()
        }
    );
    assert_eq!(
        classify(500, b"not json"),
        Outcome::Failed {
            status: 500,
            reason: String::new()
        }
    );
}
