//! The web window's upstream (ADR 0049): the person's own browser, reached
//! through the Agent Halo extension over the same bridge socket `muster
//! browser` uses.
//!
//! The browser is the point. It already holds the session for the site, so a
//! member reading through the window needs no account there and is handed no
//! credential: the cookie never leaves Chrome, and the reply the window
//! passes on carries no `Set-Cookie`.
//!
//! Each request is one bridge call, and the native host serves one at a time,
//! so a page's sub-resources arrive one after another rather than at once.
//!
//! Members reach the window over the "web" service, never a port. The window
//! lends them the browser, so it is the browser's permission that decides:
//! `browser.view` reads through it, `browser.control` acts, both under
//! `offer_agent`, and while the screen is locked only with
//! `allow_locked_control` on. That is asked again before every request.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use data_encoding::BASE64;
use muster_core::DeviceId;
use muster_core::policy::{Decision, LocalPolicy, Permission, TargetState, authorize};
use muster_core::roster::State;
use muster_net::Node;
use muster_net::stream::{read_message, write_message};
use muster_web::{Fetch, Gate, Grant, Reply, Upstream};
use quinn::{RecvStream, SendStream};
use serde_json::Value;

use crate::Daemon;

pub const SERVICE: &str = "web";
pub const VERSION: u32 = 1;

/// Where the owner reaches the window on this machine: a socket in a
/// directory only their user may enter.
pub fn socket_in(state_dir: &Path) -> PathBuf {
    state_dir.join("web/window.sock")
}

/// What `peer` may do through the window right now, or why not.
pub fn grant(
    state: &State,
    peer: &DeviceId,
    policy: &LocalPolicy,
    locked: bool,
) -> Result<Grant, String> {
    let decide = |control: bool, locked: bool| {
        authorize(
            state,
            peer,
            &Permission::Browser { control },
            policy,
            &TargetState { locked },
        )
    };
    if decide(true, locked) == Decision::Allow {
        return Ok(Grant::Act);
    }
    match decide(false, locked) {
        Decision::Allow => Ok(Grant::Read),
        // Said plainly, because the same member may be let in again the
        // moment the screen is unlocked.
        _ if locked && decide(false, false) == Decision::Allow => Err(
            "the screen is locked, and this machine does not offer control while locked".into(),
        ),
        Decision::Deny(reason) => Err(format!("the browser is not lent to you: {reason}")),
        Decision::NeedsPresence => Err("the browser is not lent to you".into()),
    }
}

fn gate_for(daemon: &Arc<Daemon>, peer: DeviceId) -> Gate {
    let daemon = daemon.clone();
    Arc::new(move || {
        let locked = daemon.platform().locked();
        let roster = daemon.roster().read().unwrap();
        let policy = daemon.policy().read().unwrap();
        grant(roster.state(), &peer, &policy, locked)
    })
}

/// Status the window side answers with before any bytes of HTTP.
const OPEN: u8 = 0;
const REFUSED: u8 = 1;
const NO_WINDOW: u8 = 2;

/// Target side: one member's stream into this machine's window.
pub async fn serve(daemon: &Arc<Daemon>, peer: DeviceId, mut send: SendStream, mut recv: RecvStream) {
    let asked = match read_message(&mut recv, 512).await {
        Ok(Some(bytes)) => String::from_utf8(bytes).ok(),
        _ => None,
    };
    let Some(asked) = asked.and_then(|text| text.parse::<muster_web::Origin>().ok()) else {
        log::debug!("web stream from {} named no origin", peer.short());
        return;
    };
    let window = daemon.web.lock().await.clone();
    let Some(window) = window.filter(|window| window.origin() == &asked) else {
        let _ = write_message(&mut send, &[NO_WINDOW]).await;
        let _ = send.finish();
        return;
    };
    let gate = gate_for(daemon, peer);
    if let Err(reason) = gate() {
        log::info!("web window {asked} refused {}: {reason}", peer.short());
        let mut answer = vec![REFUSED];
        answer.extend_from_slice(reason.as_bytes());
        let _ = write_message(&mut send, &answer).await;
        let _ = send.finish();
        return;
    }
    if write_message(&mut send, &[OPEN]).await.is_err() {
        return;
    }
    window.serve(tokio::io::join(recv, send), gate).await;
}

/// Accessor side: a stream to `peer`'s window onto `origin`, or the member's
/// reason for refusing it.
pub async fn open(
    node: &Arc<Node>,
    peer: DeviceId,
    origin: &str,
) -> Result<(SendStream, RecvStream), String> {
    let exchange = async {
        let (mut send, mut recv) = node
            .open(peer, SERVICE, VERSION)
            .await
            .map_err(|error| format!("the member did not take the stream: {error}"))?;
        write_message(&mut send, origin.as_bytes())
            .await
            .map_err(|error| error.to_string())?;
        let answer = read_message(&mut recv, 1024)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "the member closed the stream".to_string())?;
        match answer.split_first() {
            Some((&OPEN, _)) => Ok((send, recv)),
            Some((&REFUSED, reason)) => Err(String::from_utf8_lossy(reason).into_owned()),
            Some((&NO_WINDOW, _)) => Err(format!("the member has no window onto {origin}")),
            _ => Err("the member answered something else".into()),
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(5), exchange)
        .await
        .map_err(|_| "the member did not answer within 5 seconds".to_string())?
}

pub struct Browser {
    socket: PathBuf,
}

impl Browser {
    pub fn new(socket: PathBuf) -> Arc<Self> {
        Arc::new(Self { socket })
    }

    async fn ask(&self, request: Fetch) -> Result<Reply, String> {
        let operation = muster_browser::Request::Fetch {
            url: request.url,
            method: request.method,
            headers: request.headers,
            body: if request.body.is_empty() {
                String::new()
            } else {
                BASE64.encode(&request.body)
            },
        };
        let value = muster_browser::call(&self.socket, &operation)
            .await
            .map_err(|error| format!("the browser did not answer: {error}"))?;
        reply(&value).ok_or_else(|| "the browser's answer was not a reply".to_string())
    }
}

/// The extension's JSON as a reply, or nothing if it is not one. The shape is
/// the extension's `fetch` operation: status, where it ended, headers, body.
fn reply(value: &Value) -> Option<Reply> {
    let status = u16::try_from(value.get("status")?.as_u64()?).ok()?;
    let url = value.get("url")?.as_str()?.to_string();
    let headers = value
        .get("headers")?
        .as_array()?
        .iter()
        .filter_map(|pair| {
            let pair = pair.as_array()?;
            Some((
                pair.first()?.as_str()?.to_ascii_lowercase(),
                pair.get(1)?.as_str()?.to_string(),
            ))
        })
        .collect();
    let body = BASE64.decode(value.get("body")?.as_str()?.as_bytes()).ok()?;
    Some(Reply {
        status,
        url,
        headers,
        body,
    })
}

impl Upstream for Browser {
    fn fetch(
        &self,
        request: Fetch,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Reply, String>> + Send + '_>>
    {
        Box::pin(self.ask(request))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muster_core::roster::{Author, DeviceKind, DeviceRecord, Roster};
    use muster_core::{HwSigner, Scope, SoftSigner};

    /// A network whose admin added one phone holding `scopes`.
    fn member(scopes: &[&str]) -> (Roster, DeviceId) {
        let admin = SoftSigner::generate();
        let phone = SoftSigner::generate();
        let mut roster = Roster::new();
        let genesis = Author {
            roster: &roster,
            signer: &admin,
        }
        .genesis(
            "home",
            (std::net::Ipv4Addr::new(192, 168, 14, 0), 24),
            DeviceRecord::new(admin.public_key(), "a", DeviceKind::Desktop),
        )
        .unwrap();
        roster.insert(genesis).unwrap();
        let mut record = DeviceRecord::new(phone.public_key(), "p", DeviceKind::Phone);
        record
            .scopes
            .extend(scopes.iter().map(|s| s.parse::<Scope>().unwrap()));
        let add = Author {
            roster: &roster,
            signer: &admin,
        }
        .add(record)
        .unwrap();
        roster.insert(add).unwrap();
        (roster, phone.public_key().device_id())
    }

    /// The window lends the browser, so the browser's two scopes are its
    /// two grants, and a port scope — the old way in — is no way in at all.
    #[test]
    fn view_reads_control_acts_and_nothing_else_gets_in() {
        let policy = LocalPolicy::default();
        let (roster, viewer) = member(&["browser.view"]);
        assert_eq!(grant(roster.state(), &viewer, &policy, false), Ok(Grant::Read));
        let (roster, controller) = member(&["browser.control"]);
        assert_eq!(
            grant(roster.state(), &controller, &policy, false),
            Ok(Grant::Act)
        );
        let (roster, other) = member(&["net:tcp/8810", "agents", "agents:codex"]);
        assert!(grant(roster.state(), &other, &policy, false).is_err());
    }

    #[test]
    fn a_locked_screen_closes_the_window_unless_control_while_locked_is_offered() {
        let mut policy = LocalPolicy::default();
        let (roster, controller) = member(&["browser.view", "browser.control"]);
        let refused = grant(roster.state(), &controller, &policy, true).unwrap_err();
        assert!(refused.contains("screen is locked"), "{refused}");

        policy.allow_locked_control = true;
        assert_eq!(
            grant(roster.state(), &controller, &policy, true),
            Ok(Grant::Act)
        );
    }

    #[test]
    fn a_machine_that_stops_offering_its_agents_stops_lending_its_browser() {
        let policy = LocalPolicy {
            offer_agent: false,
            ..Default::default()
        };
        let (roster, controller) = member(&["browser.control"]);
        assert!(grant(roster.state(), &controller, &policy, false).is_err());
    }

    #[test]
    fn the_extensions_answer_becomes_a_reply() {
        let value = serde_json::json!({
            "status": 200,
            "url": "https://example.test/issues",
            "headers": [["Content-Type", "text/html"], ["content-length", "7"]],
            "body": BASE64.encode(b"Muster!"),
        });
        let reply = reply(&value).expect("a reply");
        assert_eq!(reply.status, 200);
        assert_eq!(reply.url, "https://example.test/issues");
        assert_eq!(reply.headers[0].0, "content-type");
        assert_eq!(reply.body, b"Muster!");
    }

    /// A missing field is not a half-built reply: the window answers 502
    /// rather than serving something it did not get.
    #[test]
    fn an_answer_that_is_not_a_reply_is_refused() {
        for value in [
            serde_json::json!({"status": 200, "headers": [], "body": ""}),
            serde_json::json!({"status": 200, "url": "x", "headers": []}),
            serde_json::json!({"status": 70000, "url": "x", "headers": [], "body": ""}),
            serde_json::json!({"status": 200, "url": "x", "headers": [], "body": "not base64!"}),
        ] {
            assert!(reply(&value).is_none(), "{value} should not be a reply");
        }
    }
}
