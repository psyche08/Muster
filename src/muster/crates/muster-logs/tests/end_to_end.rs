//! The uploader against a real collector on loopback (ADR 0228): grouping
//! by network, redaction on the device, incremental sends, duplicates
//! dropped by the collector, failures that leave the cursors alone, and the
//! TLS listener.

#![cfg(feature = "server")]

use std::io::Read;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use flate2::read::MultiGzDecoder;
use muster_core::{HwSigner, SoftSigner};
use muster_logs::collect::{Source, State};
use muster_logs::server::{self, Collector, Options, UNJOINED};
use muster_logs::upload::{self, Config, Identity, Outcome};
use muster_logs::wire;

const NETWORK: [u8; 32] = [7; 32];

struct Device {
    dir: tempfile::TempDir,
    signer: Arc<SoftSigner>,
    network: Option<[u8; 32]>,
}

impl Device {
    fn new(network: Option<[u8; 32]>) -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
            signer: Arc::new(SoftSigner::generate()),
            network,
        }
    }

    fn log(&self) -> PathBuf {
        self.dir.path().join("musterd.log")
    }

    fn append(&self, text: &str) {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.log())
            .unwrap();
        file.write_all(text.as_bytes()).unwrap();
    }

    fn state(&self) -> PathBuf {
        self.dir.path().join("log-upload.json")
    }

    fn config(&self, url: &str) -> Config {
        Config::new(
            url.into(),
            self.state(),
            vec![Source::new("musterd", self.log())],
        )
        .unwrap()
    }

    fn identity(&self) -> Identity {
        Identity {
            signer: self.signer.clone(),
            network: self.network,
            kind: "desktop".into(),
            version: "1.0.0".into(),
            commit: "0000000".into(),
            names: vec!["Sakura-MacBook".into()],
        }
    }

    fn hex(&self) -> String {
        self.signer.public_key().device_id().to_hex()
    }
}

fn today() -> String {
    server::utc_day(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
}

fn stored(root: &Path, network: &str, device: &Device, stream: &str) -> String {
    let path = root
        .join("networks")
        .join(network)
        .join(device.hex())
        .join(today())
        .join(format!("{stream}.log.gz"));
    let mut text = String::new();
    MultiGzDecoder::new(std::fs::File::open(&path).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    text
}

async fn collector(options: Options) -> (SocketAddr, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(server::serve(
        listener,
        None,
        Arc::new(Collector::new(options)),
    ));
    (addr, format!("http://{addr}{}", wire::PATH))
}

#[tokio::test]
async fn uploads_land_redacted_grouped_by_network_and_only_once() {
    let root = tempfile::tempdir().unwrap();
    let (_, url) = collector(Options::new(root.path().into())).await;
    let client = reqwest::Client::new();

    let joined = Device::new(Some(NETWORK));
    joined.append(
        "INFO started on Sakura-MacBook\n\
         INFO resolver 8.8.8.8 answered\n\
         INFO peer 192.168.1.20 up\n\
         DEBUG Authorization: Bearer eyJsecretsecretsecret123\n",
    );
    let config = joined.config(&url);
    let sent = upload::upload_once(&client, &config, &joined.identity())
        .await
        .unwrap();
    assert!(matches!(sent, Outcome::Sent { chunks: 1, .. }), "{sent:?}");

    let network = hex::encode(NETWORK);
    let text = stored(root.path(), &network, &joined, "musterd");
    assert!(text.contains("INFO peer 192.168.1.20 up\n"), "{text}");
    assert!(text.starts_with("INFO started on <name:"), "{text}");
    for secret in ["Sakura-MacBook", "8.8.8.8", "eyJsecretsecretsecret123"] {
        assert!(
            !text.contains(secret),
            "{secret} reached the collector: {text}"
        );
    }
    let device = root
        .path()
        .join("networks")
        .join(&network)
        .join(joined.hex());
    let info: serde_json::Value =
        serde_json::from_slice(&std::fs::read(device.join("device.json")).unwrap()).unwrap();
    assert_eq!(info["network"], network.as_str());
    assert_eq!(info["kind"], "desktop");

    // Nothing new: nothing is sent.
    let nothing = upload::upload_once(&client, &config, &joined.identity())
        .await
        .unwrap();
    assert_eq!(nothing, Outcome::Nothing);

    // Only what was appended goes next, and a half-written line waits.
    let before = std::fs::read(joined.state()).unwrap();
    joined.append("INFO second hour\nINFO half a li");
    let sent = upload::upload_once(&client, &config, &joined.identity())
        .await
        .unwrap();
    assert!(matches!(sent, Outcome::Sent { chunks: 1, .. }), "{sent:?}");
    let text = stored(root.path(), &network, &joined, "musterd");
    assert_eq!(text.matches("INFO started on").count(), 1, "{text}");
    assert!(text.ends_with("INFO second hour\n"), "{text}");

    // A device that lost its acknowledgement sends the range again; the
    // collector keeps one copy.
    let after = std::fs::read(joined.state()).unwrap();
    std::fs::write(joined.state(), &before).unwrap();
    let resent = upload::upload_once(&client, &config, &joined.identity())
        .await
        .unwrap();
    assert!(
        matches!(resent, Outcome::Sent { chunks: 1, .. }),
        "{resent:?}"
    );
    let again = stored(root.path(), &network, &joined, "musterd");
    assert_eq!(again, text, "the collector stored a duplicate range");
    let acknowledged: State = serde_json::from_slice(&after).unwrap();
    assert_eq!(State::load(&joined.state()).cursors, acknowledged.cursors);

    // The line finishes: it goes whole.
    joined.append("ne\n");
    upload::upload_once(&client, &config, &joined.identity())
        .await
        .unwrap();
    let text = stored(root.path(), &network, &joined, "musterd");
    assert!(
        text.ends_with("INFO second hour\nINFO half a line\n"),
        "{text}"
    );

    // A device in no network yet is kept apart.
    let loner = Device::new(None);
    loner.append("INFO not joined yet\n");
    let sent = upload::upload_once(&client, &loner.config(&url), &loner.identity())
        .await
        .unwrap();
    assert!(matches!(sent, Outcome::Sent { chunks: 1, .. }), "{sent:?}");
    assert_eq!(
        stored(root.path(), UNJOINED, &loner, "musterd"),
        "INFO not joined yet\n"
    );
}

#[tokio::test]
async fn a_refused_upload_keeps_the_lines_for_the_next_hour() {
    let root = tempfile::tempdir().unwrap();
    let mut options = Options::new(root.path().into());
    options.device_daily_bytes = 16;
    let (_, url) = collector(options).await;
    let client = reqwest::Client::new();

    let device = Device::new(Some(NETWORK));
    device.append("INFO more than sixteen compressed bytes\n");
    let error = upload::upload_once(&client, &device.config(&url), &device.identity())
        .await
        .unwrap_err();
    assert!(error.contains("429"), "{error}");
    assert!(
        State::load(&device.state()).cursors.is_empty(),
        "cursors moved past lines the collector never took"
    );

    // Turned off in the state file: nothing leaves.
    let mut state = State::load(&device.state());
    state.enabled = false;
    state.save(&device.state()).unwrap();
    let off = upload::upload_once(&client, &device.config(&url), &device.identity())
        .await
        .unwrap();
    assert_eq!(off, Outcome::Disabled);
}

#[tokio::test]
async fn the_collector_answers_garbage_and_health_checks() {
    let root = tempfile::tempdir().unwrap();
    let (addr, url) = collector(Options::new(root.path().into())).await;
    let client = reqwest::Client::new();
    let health = client
        .get(format!("http://{addr}/healthz"))
        .send()
        .await
        .unwrap();
    assert_eq!(health.status().as_u16(), 200);
    let garbage = client
        .post(&url)
        .body("MLOG nonsense")
        .send()
        .await
        .unwrap();
    assert_eq!(garbage.status().as_u16(), 400);
    let missing = client
        .get(format!("http://{addr}/v1/nothing"))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status().as_u16(), 404);
    assert!(!root.path().join("networks").exists());
}

#[tokio::test]
async fn uploads_over_tls() {
    let dir = tempfile::tempdir().unwrap();
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let (cert_pem, key_pem) = (
        dir.path().join("fullchain.pem"),
        dir.path().join("privkey.pem"),
    );
    std::fs::write(&cert_pem, cert.cert.pem()).unwrap();
    std::fs::write(&key_pem, cert.key_pair.serialize_pem()).unwrap();
    let tls = Arc::new(server::tls_config(&cert_pem, &key_pem).unwrap());

    let root = dir.path().join("root");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(server::serve(
        listener,
        Some(tls),
        Arc::new(Collector::new(Options::new(root.clone()))),
    ));
    let client = reqwest::Client::builder()
        .add_root_certificate(reqwest::Certificate::from_der(cert.cert.der()).unwrap())
        .resolve("localhost", addr)
        .build()
        .unwrap();

    let device = Device::new(Some(NETWORK));
    device.append("INFO over tls\n");
    let url = format!("https://localhost:{}{}", addr.port(), wire::PATH);
    let sent = upload::upload_once(&client, &device.config(&url), &device.identity())
        .await
        .unwrap();
    assert!(matches!(sent, Outcome::Sent { chunks: 1, .. }), "{sent:?}");
    assert_eq!(
        stored(&root, &hex::encode(NETWORK), &device, "musterd"),
        "INFO over tls\n"
    );
}
