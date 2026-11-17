//! The device side: once an hour, scan the log files, redact and compress
//! what they gained, sign the batch and post it to the collector. Cursors
//! advance only when the collector has answered, so an upload that fails is
//! sent again, and the collector drops what it already holds.

use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use flate2::Compression;
use flate2::write::GzEncoder;
use muster_core::SharedSigner;
use rand::Rng;

use crate::collect::{self, Limits, Scan, Source, State};
use crate::redact::Redactor;
use crate::wire::{self, Chunk, Manifest};

/// Uploads are an hour apart.
pub const INTERVAL: Duration = Duration::from_secs(3600);
/// The first retry after a failure; doubled per failure up to `INTERVAL`.
const RETRY: Duration = Duration::from_secs(300);
/// The first upload after start waits this long at least (a random point
/// in the range), so a restart does not upload at once and devices that
/// start together do not upload together.
const FIRST_DELAY: (u64, u64) = (120, 600);

#[derive(Clone, Debug)]
pub struct Config {
    /// The collector's upload URL: https, or http to a loopback address.
    pub url: String,
    /// The uploader's state file.
    pub state: PathBuf,
    pub sources: Vec<Source>,
    pub limits: Limits,
    pub interval: Duration,
}

impl Config {
    pub fn new(url: String, state: PathBuf, sources: Vec<Source>) -> Result<Self, String> {
        check_url(&url)?;
        Ok(Self {
            url,
            state,
            sources,
            limits: Limits::default(),
            interval: INTERVAL,
        })
    }
}

/// Logs leave the device encrypted: plain HTTP only to this machine.
pub fn check_url(url: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("log upload URL {url:?}: {e}"))?;
    let loopback = match parsed.host_str() {
        Some("localhost") => true,
        Some(host) => host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback()),
        None => false,
    };
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if loopback => Ok(()),
        _ => Err(format!(
            "log upload URL {url:?}: https only (http to loopback for tests)"
        )),
    }
}

/// Who is uploading, read afresh for every upload: membership and names
/// change while the daemon runs.
pub struct Identity {
    pub signer: SharedSigner,
    /// The network's id (genesis hash); `None` before joining one.
    pub network: Option<[u8; 32]>,
    pub kind: String,
    pub version: String,
    pub commit: String,
    /// Names to pseudonymize: the network's device names, the host name.
    pub names: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The collector took (or already had) `chunks` chunks.
    Sent { chunks: usize, bytes: usize },
    /// The files gained no whole line.
    Nothing,
    /// The owner turned uploads off in the state file.
    Disabled,
    /// The collector refused this batch for good (status); it is not sent again.
    Refused(u16),
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Runs until `identity` returns `None` (the daemon is gone).
pub async fn run<F>(config: Config, identity: F)
where
    F: Fn() -> Option<Identity> + Send + Sync + 'static,
{
    let client = match reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(120))
        .user_agent(concat!("muster-logs/", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            log::warn!("log upload off: {error}");
            return;
        }
    };
    let state_path = config.state.clone();
    let last = tokio::task::spawn_blocking(move || State::load(&state_path).last_upload)
        .await
        .ok()
        .flatten();
    let mut wait = first_wait(last, now_secs(), config.interval);
    let mut retry = RETRY;
    loop {
        tokio::time::sleep(wait).await;
        let Some(identity) = identity() else { return };
        match upload_once(&client, &config, &identity).await {
            Ok(outcome) => {
                match outcome {
                    Outcome::Sent { chunks, bytes } => {
                        log::info!("log upload: {chunks} chunks, {bytes} bytes")
                    }
                    Outcome::Refused(status) => {
                        log::warn!("log upload: refused ({status}); those lines are skipped")
                    }
                    Outcome::Nothing | Outcome::Disabled => {}
                }
                wait = config.interval;
                retry = RETRY;
            }
            Err(error) => {
                log::warn!(
                    "log upload failed, retrying in {} s: {error}",
                    retry.as_secs()
                );
                wait = retry;
                retry = (retry * 2).min(config.interval);
            }
        }
    }
}

/// An hour after the last upload, but not sooner than a few minutes after
/// start.
fn first_wait(last: Option<u64>, now: u64, interval: Duration) -> Duration {
    let floor = Duration::from_secs(rand::thread_rng().gen_range(FIRST_DELAY.0..FIRST_DELAY.1));
    let due = last
        .map(|last| last.saturating_add(interval.as_secs()).saturating_sub(now))
        .map_or(Duration::ZERO, |secs| {
            Duration::from_secs(secs).min(interval)
        });
    due.max(floor)
}

/// The batch for one upload: chunks, payload, and the scan to commit.
struct Batch {
    chunks: Vec<Chunk>,
    payload: Vec<u8>,
    scan: Scan,
    raw: usize,
}

fn build(state: &State, config: &Config, names: &[String]) -> std::io::Result<Batch> {
    let scan = collect::scan(state, &config.sources, &config.limits);
    let redactor = Redactor::new(
        state.salt_bytes().unwrap_or_default(),
        std::env::var("HOME").ok(),
        &names_with_account(names),
    );
    let mut chunks = Vec::new();
    let mut payload = Vec::new();
    let mut raw = 0;
    for pending in &scan.pending {
        let mut text = redactor.redact(&String::from_utf8_lossy(&pending.bytes));
        if text.len() as u64 > wire::MAX_CHUNK_RAW {
            // Redaction lengthened it past the limit: keep whole lines.
            let cut = text[..wire::MAX_CHUNK_RAW as usize]
                .rfind('\n')
                .map_or(0, |at| at + 1);
            text.truncate(cut);
        }
        if text.is_empty() {
            continue;
        }
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        gz.write_all(text.as_bytes())?;
        let member = gz.finish()?;
        chunks.push(Chunk {
            stream: pending.stream.clone(),
            generation: pending.generation,
            start: pending.start,
            end: pending.end,
            skipped: pending.skipped,
            len: member.len() as u64,
            raw_len: text.len() as u64,
        });
        raw += text.len();
        payload.extend_from_slice(&member);
    }
    Ok(Batch {
        chunks,
        payload,
        scan,
        raw,
    })
}

/// The account's own name joins the names to pseudonymize.
fn names_with_account(names: &[String]) -> Vec<String> {
    let mut all = names.to_vec();
    for var in ["USER", "USERNAME", "LOGNAME"] {
        if let Ok(name) = std::env::var(var) {
            all.push(name);
        }
    }
    if let Some(home) = std::env::var_os("HOME")
        && let Some(base) = std::path::Path::new(&home).file_name()
    {
        all.push(base.to_string_lossy().into_owned());
    }
    all
}

pub async fn upload_once(
    client: &reqwest::Client,
    config: &Config,
    identity: &Identity,
) -> Result<Outcome, String> {
    let blocking = |e: tokio::task::JoinError| e.to_string();
    let (path, cfg, names) = (config.state.clone(), config.clone(), identity.names.clone());
    let (state, batch) = tokio::task::spawn_blocking(move || {
        let state = State::load(&path);
        if !state.enabled {
            return Ok::<_, std::io::Error>((state, None));
        }
        let batch = build(&state, &cfg, &names)?;
        Ok((state, Some(batch)))
    })
    .await
    .map_err(blocking)?
    .map_err(|e| e.to_string())?;
    let Some(batch) = batch else {
        return Ok(Outcome::Disabled);
    };
    if batch.chunks.is_empty() {
        save(config, state, &batch.scan).await?;
        return Ok(Outcome::Nothing);
    }
    let manifest = Manifest {
        v: 1,
        key: hex::encode(identity.signer.public_key().raw()),
        network: identity.network.map(hex::encode),
        kind: identity.kind.clone(),
        version: identity.version.clone(),
        commit: identity.commit.clone(),
        os: std::env::consts::OS.into(),
        created_at: now_secs(),
        nonce: hex::encode(rand::random::<[u8; 16]>()),
        chunks: batch.chunks.clone(),
        payload_sha256: wire::payload_digest(&batch.payload),
    };
    // The hardware key may take a moment; not on the async threads.
    let signer = identity.signer.clone();
    let payload = batch.payload;
    let body =
        tokio::task::spawn_blocking(move || wire::seal(&manifest, signer.as_ref(), &payload))
            .await
            .map_err(blocking)?
            .map_err(|e| format!("signing: {e}"))?;
    let response = client
        .post(&config.url)
        .header(reqwest::header::CONTENT_TYPE, wire::CONTENT_TYPE)
        .body(body)
        .send()
        .await
        .map_err(|e| format!("{e}"))?;
    let status = response.status().as_u16();
    let outcome = match status {
        200..=299 => Outcome::Sent {
            chunks: batch.chunks.len(),
            bytes: batch.raw,
        },
        // The batch itself is wrong; sending it again cannot help.
        400 | 413 | 415 | 422 => Outcome::Refused(status),
        _ => return Err(format!("collector answered {status}")),
    };
    save(config, state, &batch.scan).await?;
    Ok(outcome)
}

async fn save(config: &Config, mut state: State, scan: &Scan) -> Result<(), String> {
    state.commit(scan, now_secs());
    let path = config.state.clone();
    tokio::task::spawn_blocking(move || {
        // The owner may have turned uploads off while this one was out.
        if path.exists() {
            state.enabled = State::load(&path).enabled;
        }
        state.save(&path)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("saving {}: {e}", config.state.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_leaves_the_machine() {
        assert!(check_url(wire::DEFAULT_URL).is_ok());
        assert!(check_url("http://127.0.0.1:9180/v1/logs").is_ok());
        assert!(check_url("http://[::1]:9180/v1/logs").is_ok());
        assert!(check_url("http://localhost:9180/v1/logs").is_ok());
        assert!(check_url("http://muster.linsheng.dev/v1/logs").is_err());
        assert!(check_url("http://10.0.0.1/v1/logs").is_err());
        assert!(check_url("ftp://localhost/").is_err());
        assert!(check_url("not a url").is_err());
    }

    #[test]
    fn the_first_upload_waits_for_the_hour_but_at_least_a_few_minutes() {
        let hour = Duration::from_secs(3600);
        let floor = Duration::from_secs(FIRST_DELAY.0);
        let ceiling = Duration::from_secs(FIRST_DELAY.1);
        let never = first_wait(None, 10_000, hour);
        assert!(never >= floor && never < ceiling);
        let just = first_wait(Some(10_000 - 60), 10_000, hour);
        assert_eq!(just, Duration::from_secs(3540));
        let long_ago = first_wait(Some(1), 10_000, hour);
        assert!(long_ago >= floor && long_ago < ceiling);
        // A clock that went back never waits more than an hour.
        assert_eq!(first_wait(Some(50_000), 10_000, hour), hour);
    }
}
