//! The HTTPS transport behind the agent service's cloud sessions
//! (`muster_agent::cloud`): one client, WebPKI roots, bounded timeouts, no
//! redirects (a redirect could carry the bearer token elsewhere).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use muster_agent::cloud::{Http, Request, Response};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const TIMEOUT: Duration = Duration::from_secs(30);
/// Longest reply body kept; the lists and event pages are far smaller.
const MAX_BODY: usize = 8 * 1024 * 1024;

pub struct Client {
    client: reqwest::Client,
}

impl Client {
    /// A shared client for the agent service.
    pub fn connect() -> anyhow::Result<Arc<dyn Http>> {
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .build()?;
        Ok(Arc::new(Self { client }))
    }
}

#[async_trait]
impl Http for Client {
    async fn send(&self, request: Request) -> Result<Response, String> {
        let method = reqwest::Method::from_bytes(request.method.as_bytes())
            .map_err(|e| format!("method {}: {e}", request.method))?;
        let mut builder = self.client.request(method, &request.url);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        if let Some(body) = request.body {
            builder = builder.body(body);
        }
        let response = builder.send().await.map_err(|e| e.to_string())?;
        let status = response.status().as_u16();
        let bytes = response.bytes().await.map_err(|e| e.to_string())?;
        if bytes.len() > MAX_BODY {
            return Err(format!("reply of {} bytes is too large", bytes.len()));
        }
        Ok(Response {
            status,
            body: String::from_utf8_lossy(&bytes).into_owned(),
        })
    }
}
