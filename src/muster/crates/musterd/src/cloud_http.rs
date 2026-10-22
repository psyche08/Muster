//! The HTTPS transport behind the agent service's cloud sessions
//! (`muster_agent::cloud`): one client, platform trust on macOS, bounded timeouts, no
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
        let builder = client_builder();
        // Scope OS trust to provider Cloud HTTPS. Relay, device identity,
        // DoH and update clients retain their independently configured trust.
        #[cfg(target_os = "macos")]
        let builder = {
            use rustls_platform_verifier::BuilderVerifierExt;
            let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()?
            .with_platform_verifier()?
            .with_no_client_auth();
            builder.use_preconfigured_tls(tls)
        };
        let client = builder.build()?;
        Ok(Arc::new(Self { client }))
    }
}

fn client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .https_only(true)
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
        let response = builder
            .send()
            .await
            .map_err(|e| transport_error("request", &e))?;
        let status = response.status().as_u16();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| transport_error("body", &e))?;
        if bytes.len() > MAX_BODY {
            return Err(format!("reply of {} bytes is too large", bytes.len()));
        }
        Ok(Response {
            status,
            body: String::from_utf8_lossy(&bytes).into_owned(),
        })
    }
}

/// Do not format reqwest/source errors: their URLs can carry cursors or secrets.
/// Preserve typed causes instead of losing them behind "error sending request".
fn transport_error(phase: &str, error: &reqwest::Error) -> String {
    use std::error::Error;
    let mut source: Option<&(dyn Error + 'static)> = Some(error);
    let mut cause = None;
    for _ in 0..16 {
        let Some(current) = source else { break };
        if let Some(tls) = current.downcast_ref::<rustls::Error>() {
            cause = Some(match tls {
                rustls::Error::InvalidCertificate(rustls::CertificateError::UnknownIssuer) => {
                    "tls_unknown_issuer"
                }
                rustls::Error::InvalidCertificate(_) => "tls_certificate",
                _ => "tls_handshake",
            });
            break;
        }
        // io::Error::source may skip its payload's own type. Inspect that
        // payload first so a nested rustls certificate error remains visible.
        source = current
            .downcast_ref::<std::io::Error>()
            .and_then(std::io::Error::get_ref)
            .map(|inner| inner as &(dyn Error + 'static))
            .or_else(|| current.source());
    }
    let cause = cause.unwrap_or_else(|| {
        if error.is_timeout() {
            "timeout"
        } else if error.is_connect() {
            "connect"
        } else if error.is_body() {
            "body"
        } else if error.is_decode() {
            "decode"
        } else if error.is_builder() {
            "configuration"
        } else {
            "transport"
        }
    });
    format!("Cloud HTTPS {phase} failed: {cause}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn request(url: String) -> Request {
        Request {
            method: "GET",
            url,
            headers: vec![],
            body: None,
        }
    }

    #[tokio::test]
    async fn plaintext_and_secret_urls_fail_without_disclosure() {
        let client = Client::connect().unwrap();
        let error = client
            .send(request(
                "http://example.invalid/private?token=SECRET".into(),
            ))
            .await
            .unwrap_err();
        assert_eq!(error, "Cloud HTTPS request failed: configuration");
        assert!(!error.contains("SECRET"));
        assert!(!error.contains("example.invalid"));
    }

    #[tokio::test]
    async fn platform_client_rejects_an_untrusted_certificate() {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let server = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.cert.der().clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()).into(),
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            tokio_rustls::TlsAcceptor::from(Arc::new(server))
                .accept(socket)
                .await
                .is_err()
        });
        let client = Client::connect().unwrap();
        let error = client
            .send(request(format!("https://localhost:{port}/?token=SECRET")))
            .await
            .unwrap_err();
        assert!(
            error.contains("tls_certificate") || error.contains("tls_unknown_issuer"),
            "{error}"
        );
        assert!(!error.contains("SECRET"));
        assert!(task.await.unwrap());
    }

    #[tokio::test]
    async fn https_redirect_is_returned_without_a_second_request() {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let root = reqwest::Certificate::from_der(cert.cert.der()).unwrap();
        let server = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.cert.der().clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()).into(),
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut tls = tokio_rustls::TlsAcceptor::from(Arc::new(server))
                .accept(socket)
                .await
                .unwrap();
            let mut buf = [0u8; 4096];
            let mut used = 0;
            while !buf[..used].windows(4).any(|w| w == b"\r\n\r\n") {
                assert!(used < buf.len(), "bounded test request headers");
                let read = tls.read(&mut buf[used..]).await.unwrap();
                assert_ne!(read, 0, "request ended before headers");
                used += read;
            }
            tls.write_all(format!("HTTP/1.1 302 Found\r\nLocation: https://localhost:{port}/other\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
            tls.shutdown().await.unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(200), listener.accept())
                    .await
                    .is_err()
            );
        });
        // Test-only root; no system store or production verifier is modified.
        let client = Client {
            client: client_builder()
                .use_rustls_tls()
                .tls_built_in_root_certs(false)
                .add_root_certificate(root)
                .build()
                .unwrap(),
        };
        assert_eq!(
            client
                .send(request(format!("https://localhost:{port}/")))
                .await
                .unwrap()
                .status,
            302
        );
        task.await.unwrap();
    }

    #[tokio::test]
    async fn transport_timeout_is_distinct_and_does_not_include_the_url() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            tokio::time::sleep(Duration::from_millis(300)).await;
            drop(socket);
        });
        let client = Client {
            client: client_builder()
                .timeout(Duration::from_millis(100))
                .build()
                .unwrap(),
        };
        let error = client
            .send(request(format!("https://{addr}/private?token=SECRET")))
            .await
            .unwrap_err();
        assert_eq!(error, "Cloud HTTPS request failed: timeout");
        task.await.unwrap();
    }
}
