//! The collector: `POST /v1/logs` over HTTPS on muster.linsheng.dev, and
//! `GET /healthz`. Each upload is verified (signature, digest, limits,
//! clock), ranges already held are dropped, and every new chunk's gzip
//! member is appended to its stream's file for the day:
//!
//! ```text
//! <root>/networks/<network id | unjoined>/<device id>/
//!     device.json              what the device last reported
//!     state.json               per stream and generation: bytes held up to
//!     <YYYY-MM-DD>/<stream>.log.gz
//! ```
//!
//! A day's file is a sequence of gzip members (`zcat` reads it whole).
//! Days older than the retention are removed. The collector takes no
//! authorization decision: a valid signature proves the uploader holds the
//! device key, not that it is a member of the network it names (ADR 0228).

use std::collections::{BTreeMap, HashMap};
use std::convert::Infallible;
use std::io::{self, Read, Write};
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, ensure};
use flate2::read::GzDecoder;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use muster_core::DeviceId;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

use crate::wire::{self, WireError};

/// The group of devices that have not joined a network.
pub const UNJOINED: &str = "unjoined";

#[derive(Clone, Debug)]
pub struct Options {
    pub root: PathBuf,
    /// Days of logs kept.
    pub retention_days: u64,
    /// Compressed bytes one device may add in one (UTC) day.
    pub device_daily_bytes: u64,
    /// Uploads one device may make per hour (it makes one, plus retries).
    pub device_uploads_per_hour: u32,
    /// Uploads one source address may make per hour; `None` behind a proxy.
    pub source_uploads_per_hour: Option<u32>,
    /// How far an upload's clock may be from the collector's.
    pub max_skew: Duration,
    /// Connections served at once.
    pub max_connections: usize,
}

impl Options {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            retention_days: 30,
            device_daily_bytes: 128 << 20,
            device_uploads_per_hour: 12,
            source_uploads_per_hour: Some(240),
            max_skew: Duration::from_secs(24 * 3600),
            max_connections: 256,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("{0}")]
    Malformed(String),
    #[error("{0}")]
    Signature(String),
    #[error("too many uploads")]
    RateLimited,
    #[error("daily quota reached")]
    Quota,
    #[error("storage: {0}")]
    Io(#[from] io::Error),
}

impl IngestError {
    pub fn status(&self) -> StatusCode {
        match self {
            Self::Malformed(_) => StatusCode::BAD_REQUEST,
            Self::Signature(_) => StatusCode::FORBIDDEN,
            Self::RateLimited | Self::Quota => StatusCode::TOO_MANY_REQUESTS,
            Self::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<WireError> for IngestError {
    fn from(error: WireError) -> Self {
        match error {
            WireError::Malformed(text) => Self::Malformed(text),
            WireError::Signature(text) => Self::Signature(text),
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct Ingested {
    pub stored: usize,
    pub duplicate: usize,
}

/// A token bucket refilled at `per_hour`, holding at most that many.
struct Bucket {
    tokens: f64,
    last: Instant,
}

/// Per stream and generation: the end offset held and when last seen.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Mark {
    end: u64,
    seen: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct DeviceState {
    #[serde(default)]
    streams: BTreeMap<String, Mark>,
    /// Compressed bytes stored per UTC day (recent days only).
    #[serde(default)]
    days: BTreeMap<String, u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeviceInfo {
    device: String,
    network: Option<String>,
    kind: String,
    version: String,
    commit: String,
    os: String,
    first_seen: u64,
    last_upload: u64,
}

pub struct Collector {
    options: Options,
    store: Mutex<()>,
    devices: Mutex<HashMap<DeviceId, Bucket>>,
    sources: Mutex<HashMap<IpAddr, Bucket>>,
}

fn secs(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Collector {
    pub fn new(options: Options) -> Self {
        Self {
            options,
            store: Mutex::new(()),
            devices: Mutex::new(HashMap::new()),
            sources: Mutex::new(HashMap::new()),
        }
    }

    pub fn options(&self) -> &Options {
        &self.options
    }

    /// One upload, from `source` when the collector sees the client
    /// directly. Blocking: verifies, decompresses and writes files.
    pub fn ingest(
        &self,
        body: &[u8],
        source: Option<IpAddr>,
        now: SystemTime,
    ) -> Result<Ingested, IngestError> {
        if let (Some(ip), Some(per_hour)) = (source, self.options.source_uploads_per_hour)
            && !admit(&mut self.sources.lock().unwrap(), ip, per_hour)
        {
            return Err(IngestError::RateLimited);
        }
        let opened = wire::open(body)?;
        let now_secs = secs(now);
        if now_secs.abs_diff(opened.manifest.created_at) > self.options.max_skew.as_secs() {
            return Err(IngestError::Malformed(
                "clock too far from the collector's".into(),
            ));
        }
        if !admit(
            &mut self.devices.lock().unwrap(),
            opened.device,
            self.options.device_uploads_per_hour,
        ) {
            return Err(IngestError::RateLimited);
        }
        // Every chunk must be exactly what the manifest says before any is kept.
        for (chunk, member) in &opened.chunks {
            let mut text = Vec::with_capacity(chunk.raw_len as usize);
            GzDecoder::new(*member)
                .take(chunk.raw_len + 1)
                .read_to_end(&mut text)
                .map_err(|e| IngestError::Malformed(format!("chunk of {}: {e}", chunk.stream)))?;
            if text.len() as u64 != chunk.raw_len {
                return Err(IngestError::Malformed(format!(
                    "chunk of {}: length",
                    chunk.stream
                )));
            }
        }
        let network = opened.manifest.network.as_deref().unwrap_or(UNJOINED);
        let dir = self
            .options
            .root
            .join("networks")
            .join(network)
            .join(opened.device.to_hex());
        let day = utc_day(now_secs);
        let _store = self.store.lock().unwrap();
        std::fs::create_dir_all(dir.join(&day))?;
        let mut state: DeviceState = read_json(&dir.join("state.json"))?.unwrap_or_default();
        let incoming: u64 = opened.chunks.iter().map(|(_, m)| m.len() as u64).sum();
        let today = state.days.get(&day).copied().unwrap_or(0);
        if today + incoming > self.options.device_daily_bytes {
            return Err(IngestError::Quota);
        }
        let mut result = Ingested::default();
        let mut written = 0u64;
        for (chunk, member) in &opened.chunks {
            let key = format!("{}/{}", chunk.stream, chunk.generation);
            let mark = state.streams.entry(key).or_default();
            if mark.seen > 0 && chunk.end <= mark.end {
                mark.seen = now_secs;
                result.duplicate += 1;
                continue;
            }
            let path = dir.join(&day).join(format!("{}.log.gz", chunk.stream));
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)?;
            if chunk.skipped > 0 {
                let note = format!(
                    "[muster-logs] {} bytes of {} (generation {}) before offset {} were not uploaded\n",
                    chunk.skipped, chunk.stream, chunk.generation, chunk.start
                );
                let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
                gz.write_all(note.as_bytes())?;
                let note = gz.finish()?;
                file.write_all(&note)?;
                written += note.len() as u64;
            }
            file.write_all(member)?;
            file.sync_data()?;
            written += member.len() as u64;
            mark.end = mark.end.max(chunk.end);
            mark.seen = now_secs;
            result.stored += 1;
        }
        *state.days.entry(day.clone()).or_default() += written;
        // Only today's total is ever consulted; marks outlive their files by
        // the retention, then a returning generation is new again.
        state.days.retain(|d, _| *d == day);
        let horizon = now_secs.saturating_sub(self.options.retention_days * 86_400);
        state.streams.retain(|_, mark| mark.seen >= horizon);
        write_json(&dir.join("state.json"), &state)?;
        let previous: Option<DeviceInfo> = read_json(&dir.join("device.json"))?;
        let m = &opened.manifest;
        write_json(
            &dir.join("device.json"),
            &DeviceInfo {
                device: opened.device.to_hex(),
                network: m.network.clone(),
                kind: clip(&m.kind),
                version: clip(&m.version),
                commit: clip(&m.commit),
                os: clip(&m.os),
                first_seen: previous.map_or(now_secs, |p| p.first_seen),
                last_upload: now_secs,
            },
        )?;
        Ok(result)
    }

    /// Removes days past the retention; returns how many directories went.
    pub fn sweep(&self, now: SystemTime) -> io::Result<usize> {
        let oldest = utc_day(secs(now).saturating_sub(self.options.retention_days * 86_400));
        let _store = self.store.lock().unwrap();
        let mut removed = 0;
        let networks = self.options.root.join("networks");
        for network in read_dirs(&networks)? {
            for device in read_dirs(&network)? {
                for day in read_dirs(&device)? {
                    let name = day.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                    if is_day(name) && name < oldest.as_str() {
                        std::fs::remove_dir_all(&day)?;
                        removed += 1;
                    }
                }
            }
        }
        Ok(removed)
    }
}

/// Manifest strings stored for the operator: bounded and printable.
fn clip(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(64).collect()
}

fn admit<K: std::hash::Hash + Eq>(buckets: &mut HashMap<K, Bucket>, key: K, per_hour: u32) -> bool {
    let now = Instant::now();
    let rate = f64::from(per_hour) / 3600.0;
    let burst = f64::from(per_hour.max(1));
    if buckets.len() > 100_000 {
        // Idle entries are full buckets; forgetting them changes nothing.
        buckets.retain(|_, b| now.duration_since(b.last) < Duration::from_secs(3600));
    }
    let bucket = buckets.entry(key).or_insert(Bucket {
        tokens: burst,
        last: now,
    });
    bucket.tokens =
        (bucket.tokens + now.duration_since(bucket.last).as_secs_f64() * rate).min(burst);
    bucket.last = now;
    if bucket.tokens >= 1.0 {
        bucket.tokens -= 1.0;
        true
    } else {
        false
    }
}

fn read_dirs(dir: &Path) -> io::Result<Vec<PathBuf>> {
    match std::fs::read_dir(dir) {
        Ok(entries) => Ok(entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.path())
            .collect()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> io::Result<Option<T>> {
    match std::fs::read(path) {
        // A damaged file is replaced by the next write rather than blocking uploads.
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    let mut file = std::fs::File::create(&temp)?;
    file.write_all(&serde_json::to_vec_pretty(value).map_err(io::Error::other)?)?;
    file.sync_all()?;
    std::fs::rename(&temp, path)
}

fn is_day(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// `YYYY-MM-DD` of a Unix time, UTC (Hinnant's civil-from-days).
pub fn utc_day(secs: u64) -> String {
    let days = (secs / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// TLS 1.2/1.3 with the public certificate (the relay's Let's Encrypt
/// pair), HTTP/1.1 only.
pub fn tls_config(cert: &Path, key: &Path) -> Result<rustls::ServerConfig> {
    let certs = rustls_pemfile::certs(&mut io::BufReader::new(
        std::fs::File::open(cert).with_context(|| format!("{}", cert.display()))?,
    ))
    .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(!certs.is_empty(), "certificate chain is empty");
    let key = rustls_pemfile::private_key(&mut io::BufReader::new(
        std::fs::File::open(key).with_context(|| format!("{}", key.display()))?,
    ))?
    .context("private key is missing")?;
    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_no_client_auth()
    .with_single_cert(certs, key)?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(config)
}

/// Serves until the listener fails. `tls` is `None` only behind a local
/// proxy (the caller checks that the listener is loopback).
pub async fn serve(
    listener: TcpListener,
    tls: Option<Arc<rustls::ServerConfig>>,
    collector: Arc<Collector>,
) -> Result<()> {
    let acceptor = tls.map(tokio_rustls::TlsAcceptor::from);
    let slots = Arc::new(Semaphore::new(collector.options.max_connections));
    loop {
        let permit = slots.clone().acquire_owned().await?;
        let (stream, peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(error) => {
                log::warn!("accept: {error}");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        let collector = collector.clone();
        let acceptor = acceptor.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let source = acceptor.is_some().then_some(peer.ip());
            match acceptor {
                Some(acceptor) => {
                    match tokio::time::timeout(Duration::from_secs(10), acceptor.accept(stream))
                        .await
                    {
                        Ok(Ok(stream)) => connection(stream, collector, source, peer).await,
                        Ok(Err(error)) => log::debug!("{peer}: tls: {error}"),
                        Err(_) => log::debug!("{peer}: tls handshake timed out"),
                    }
                }
                None => connection(stream, collector, source, peer).await,
            }
        });
    }
}

async fn connection<I>(io: I, collector: Arc<Collector>, source: Option<IpAddr>, peer: SocketAddr)
where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let service = hyper::service::service_fn(move |request| {
        let collector = collector.clone();
        async move { Ok::<_, Infallible>(handle(request, collector, source).await) }
    });
    let connection = hyper::server::conn::http1::Builder::new()
        .timer(TokioTimer::new())
        .header_read_timeout(Duration::from_secs(20))
        .serve_connection(TokioIo::new(io), service);
    match tokio::time::timeout(Duration::from_secs(300), connection).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => log::debug!("{peer}: {error}"),
        Err(_) => log::debug!("{peer}: connection timed out"),
    }
}

fn reply(status: StatusCode, body: impl Into<Bytes>) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(body.into()));
    *response.status_mut() = status;
    response
}

async fn handle(
    request: Request<Incoming>,
    collector: Arc<Collector>,
    source: Option<IpAddr>,
) -> Response<Full<Bytes>> {
    match (request.method(), request.uri().path()) {
        (&Method::GET, "/healthz") => reply(StatusCode::OK, "ok\n"),
        (&Method::POST, wire::PATH) => {
            let declared = request
                .headers()
                .get(hyper::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok()?.parse::<usize>().ok());
            if declared.is_some_and(|len| len > wire::MAX_BODY) {
                return reply(StatusCode::PAYLOAD_TOO_LARGE, "too large\n");
            }
            let body = match Limited::new(request.into_body(), wire::MAX_BODY)
                .collect()
                .await
            {
                Ok(collected) => collected.to_bytes(),
                Err(_) => return reply(StatusCode::PAYLOAD_TOO_LARGE, "too large\n"),
            };
            let outcome = tokio::task::spawn_blocking(move || {
                collector.ingest(&body, source, SystemTime::now())
            })
            .await;
            match outcome {
                Ok(Ok(ingested)) => reply(
                    StatusCode::OK,
                    serde_json::to_vec(&ingested).unwrap_or_default(),
                ),
                Ok(Err(error)) => {
                    if matches!(error, IngestError::Io(_)) {
                        log::warn!("ingest: {error}");
                    } else {
                        log::info!("refused upload: {error}");
                    }
                    reply(error.status(), format!("{error}\n"))
                }
                Err(error) => {
                    log::warn!("ingest task: {error}");
                    reply(StatusCode::INTERNAL_SERVER_ERROR, "internal error\n")
                }
            }
        }
        _ => reply(StatusCode::NOT_FOUND, "not found\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_days() {
        assert_eq!(utc_day(0), "1970-01-01");
        assert_eq!(utc_day(951_782_400), "2000-02-29");
        assert_eq!(utc_day(1_794_528_000), "2026-11-13");
        assert!(is_day("2026-11-12") && !is_day("2026-1-12") && !is_day("x026-11-12"));
    }

    #[test]
    fn buckets_admit_a_burst_then_refuse() {
        let mut buckets = HashMap::new();
        for _ in 0..3 {
            assert!(admit(&mut buckets, 1, 3));
        }
        assert!(!admit(&mut buckets, 1, 3));
        assert!(admit(&mut buckets, 2, 3));
    }
}
