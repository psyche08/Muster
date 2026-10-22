//! The web window's upstream (ADR 0026): the person's own browser, reached
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

use std::path::PathBuf;
use std::sync::Arc;

use data_encoding::BASE64;
use muster_web::{Fetch, Reply, Upstream};
use serde_json::Value;

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
