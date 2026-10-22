//! muster-apns: the APNs provider behind the relay's push hook. For an
//! offline device that registered a push token, the relay runs
//! `muster-apns-push <device id hex> <kind> <token hex>`; that client hands
//! the request to this service over a local socket, and the service sends a
//! silent background push to the app's topic on a long-lived HTTP/2
//! connection per APNs environment. Only this service reads the provider
//! certificate's key; the relay never holds it.
//!
//! A token does not say which environment issued it: development builds get
//! sandbox tokens; TestFlight, App Store and Developer ID builds get
//! production ones. Production is tried first, a BadDeviceToken there is
//! retried on the sandbox, and the environment that accepted a token is
//! remembered.

use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

/// Push kind 1 in `SetPushToken` (app.proto): an APNs device token.
pub const KIND_APNS: u8 = 1;
/// Where the service listens unless told otherwise (systemd RuntimeDirectory).
pub const DEFAULT_SOCKET: &str = "/run/muster-apns/push.sock";
/// The longest token accepted, as musterd accepts (`set_push_token`).
const MAX_TOKEN: usize = 512;
/// Remembered token environments at most; the map starts over past this.
const MAX_REMEMBERED: usize = 100_000;
/// The wake-up: no alert, the app re-registers with the relay.
const WAKE_PAYLOAD: &str = r#"{"aps":{"content-available":1}}"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Environment {
    Production,
    Sandbox,
}

impl Environment {
    pub fn other(self) -> Self {
        match self {
            Self::Production => Self::Sandbox,
            Self::Sandbox => Self::Production,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Sandbox => "sandbox",
        }
    }
}

/// What APNs answered for one push.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Delivered,
    /// 400 BadDeviceToken: not a token of this environment, or not a token.
    BadDeviceToken,
    /// 410: the app left the device or its notifications were turned off.
    Unregistered,
    /// Anything else, with APNs's status and reason.
    Failed {
        status: u16,
        reason: String,
    },
}

/// Reads an APNs response: the status, and the JSON body's `reason`.
pub fn classify(status: u16, body: &[u8]) -> Outcome {
    let reason = serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|v| Some(v.get("reason")?.as_str()?.to_owned()))
        .unwrap_or_default();
    match (status, reason.as_str()) {
        (200, _) => Outcome::Delivered,
        (400, "BadDeviceToken") => Outcome::BadDeviceToken,
        (410, _) => Outcome::Unregistered,
        _ => Outcome::Failed { status, reason },
    }
}

/// One push to one environment; the routing above it is tested without APNs.
pub trait Transport: Send + Sync + 'static {
    fn push(&self, env: Environment, token: &[u8]) -> impl Future<Output = Result<Outcome>> + Send;
}

/// APNs's HTTP/2 provider API, authenticated by the provider certificate.
pub struct Http2 {
    client: reqwest::Client,
    topic: String,
    ttl: Duration,
    production: String,
    sandbox: String,
}

pub struct Http2Options {
    /// The provider key and certificate, PEM; None sends unauthenticated
    /// requests, which APNs refuses (a reachability probe).
    pub identity_pem: Option<Vec<u8>>,
    /// The app's bundle id.
    pub topic: String,
    /// A wake-up not delivered by then is dropped.
    pub ttl: Duration,
    /// Base URLs, e.g. `https://api.push.apple.com:2197`.
    pub production: String,
    pub sandbox: String,
}

impl Http2 {
    pub fn new(options: Http2Options) -> Result<Self> {
        // One connection per environment, kept open with PINGs: APNs treats
        // repeated connects as abuse.
        let mut builder = reqwest::Client::builder()
            .http2_prior_knowledge()
            .http2_keep_alive_interval(Duration::from_secs(60))
            .http2_keep_alive_while_idle(true)
            .pool_idle_timeout(None)
            .pool_max_idle_per_host(1)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20));
        if let Some(pem) = &options.identity_pem {
            builder = builder.identity(
                reqwest::Identity::from_pem(pem).context("provider key and certificate")?,
            );
        }
        Ok(Self {
            client: builder.build()?,
            topic: options.topic,
            ttl: options.ttl,
            production: options.production.trim_end_matches('/').to_string(),
            sandbox: options.sandbox.trim_end_matches('/').to_string(),
        })
    }
}

impl Transport for Http2 {
    async fn push(&self, env: Environment, token: &[u8]) -> Result<Outcome> {
        let base = match env {
            Environment::Production => &self.production,
            Environment::Sandbox => &self.sandbox,
        };
        let expiration = (SystemTime::now() + self.ttl)
            .duration_since(UNIX_EPOCH)?
            .as_secs();
        let response = self
            .client
            .post(format!("{base}/3/device/{}", hex::encode(token)))
            .header("apns-topic", &self.topic)
            .header("apns-push-type", "background")
            .header("apns-priority", "5")
            .header("apns-expiration", expiration.to_string())
            .header("apns-collapse-id", "muster-wake")
            .body(WAKE_PAYLOAD)
            .send()
            .await?;
        let status = response.status().as_u16();
        Ok(classify(status, &response.bytes().await?))
    }
}

/// Picks the environment for each token and remembers the one that took it.
pub struct Pusher<T> {
    transport: T,
    remembered: Mutex<HashMap<Vec<u8>, Environment>>,
}

impl<T: Transport> Pusher<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            remembered: Mutex::new(HashMap::new()),
        }
    }

    pub async fn push(&self, token: &[u8]) -> Result<(Environment, Outcome)> {
        let known = self.remembered.lock().unwrap().get(token).copied();
        let first = known.unwrap_or(Environment::Production);
        let mut env = first;
        let mut outcome = self.transport.push(first, token).await?;
        if outcome == Outcome::BadDeviceToken && known.is_none() {
            env = first.other();
            outcome = self.transport.push(env, token).await?;
        }
        let mut remembered = self.remembered.lock().unwrap();
        match outcome {
            Outcome::Delivered => {
                if remembered.len() >= MAX_REMEMBERED {
                    remembered.clear();
                }
                remembered.insert(token.to_vec(), env);
            }
            Outcome::BadDeviceToken | Outcome::Unregistered => {
                remembered.remove(token);
            }
            Outcome::Failed { .. } => {}
        }
        Ok((env, outcome))
    }
}

/// One line from the hook: `<device id hex> <kind> <token hex>`.
#[derive(Debug, PartialEq, Eq)]
pub struct Request {
    pub device: String,
    pub kind: u8,
    pub token: Vec<u8>,
}

impl Request {
    pub fn parse(line: &str) -> Result<Self> {
        let mut words = line.split_ascii_whitespace();
        let (Some(device), Some(kind), Some(token), None) =
            (words.next(), words.next(), words.next(), words.next())
        else {
            bail!("expected <device id hex> <kind> <token hex>");
        };
        if device.len() > 128 || !device.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("device id must be hex");
        }
        let kind = kind.parse().context("kind")?;
        let token = hex::decode(token).context("token must be hex")?;
        if token.is_empty() || token.len() > MAX_TOKEN {
            bail!("token must be 1 to {MAX_TOKEN} bytes");
        }
        Ok(Self {
            device: device.to_string(),
            kind,
            token,
        })
    }

    pub fn line(&self) -> String {
        format!(
            "{} {} {}\n",
            self.device,
            self.kind,
            hex::encode(&self.token)
        )
    }
}

/// The reply line for an outcome; the hook exits 0 only on `ok`.
pub fn reply(env: Environment, outcome: &Outcome) -> String {
    match outcome {
        Outcome::Delivered => format!("ok {}\n", env.name()),
        Outcome::BadDeviceToken => "error BadDeviceToken\n".into(),
        Outcome::Unregistered => "error Unregistered\n".into(),
        Outcome::Failed { status, reason } => format!("error {status} {reason}\n"),
    }
}

/// Answers hook requests on `listener` until it fails.
pub async fn serve<T: Transport>(listener: UnixListener, pusher: Arc<Pusher<T>>) -> Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        let pusher = pusher.clone();
        tokio::spawn(async move {
            if let Err(error) = handle(stream, &pusher).await {
                log::warn!("push request: {error:#}");
            }
        });
    }
}

async fn handle<T: Transport>(stream: UnixStream, pusher: &Pusher<T>) -> Result<()> {
    let (read, mut write) = stream.into_split();
    let mut line = String::new();
    tokio::time::timeout(
        Duration::from_secs(5),
        BufReader::new(read.take(2048)).read_line(&mut line),
    )
    .await
    .context("request timed out")??;
    let answer = match Request::parse(&line) {
        Err(error) => format!("error {error}\n"),
        Ok(request) if request.kind != KIND_APNS => {
            format!("error kind {} is not APNs\n", request.kind)
        }
        Ok(request) => {
            let short = &request.device[..request.device.len().min(8)];
            match pusher.push(&request.token).await {
                Ok((env, outcome)) => {
                    match &outcome {
                        Outcome::Delivered => {
                            log::info!("push {short}: delivered ({})", env.name())
                        }
                        other => log::warn!("push {short}: {other:?} ({})", env.name()),
                    }
                    reply(env, &outcome)
                }
                Err(error) => {
                    log::warn!("push {short}: {error:#}");
                    format!("error {error}\n")
                }
            }
        }
    };
    write.write_all(answer.as_bytes()).await?;
    Ok(())
}

/// The hook's side: sends one request and returns the reply line.
pub fn request(socket: &Path, request: &Request) -> Result<String> {
    use std::io::{BufRead, Write};
    let mut stream = std::os::unix::net::UnixStream::connect(socket)
        .with_context(|| format!("connect {}", socket.display()))?;
    // Production then sandbox, each bounded by the client's 20 s.
    stream.set_read_timeout(Some(Duration::from_secs(45)))?;
    stream.write_all(request.line().as_bytes())?;
    let mut answer = String::new();
    std::io::BufReader::new(stream).read_line(&mut answer)?;
    Ok(answer.trim_end().to_string())
}
