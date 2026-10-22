//! Shared desktop updater. Velopack owns bundle replacement; Muster owns
//! network policy, signed-feed validation, downgrade policy and UI state.
use p256::{
    ecdsa::{Signature, VerifyingKey, signature::Verifier},
    pkcs8::DecodePublicKey,
};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path, sync::mpsc::Sender, time::Duration};
use velopack::{
    locator::{LocationContext, auto_locate_app_manifest},
    sources::UpdateSource,
    *,
};

const BASE: &str = "https://linsheng.dev:8443/muster/desktop/";
const PACKAGE_ID: &str = "Muster";
const MAX_PACKAGE: u64 = 1024 * 1024 * 1024;
const PUBLIC_KEY: &str = include_str!("release-public.pem");

pub fn startup() {
    // Installation hooks run before creating windows/runtime on Windows.
    // Never auto-apply a cached package without revalidating its signed feed.
    VelopackApp::build().set_auto_apply_on_startup(false).run();
}

/// Opaque local-daemon fence. Swift receives only the subsequent lifecycle
/// effect; it cannot infer permission to stop the daemon from UI state.
pub struct Lease {
    call: Option<muster_rpc::Call>,
}
impl Lease {
    pub async fn acquire(client: muster_rpc::Client) -> Result<Self, String> {
        use muster_proto::v1::{self, event::Ev, request::Req};
        let call = client
            .start(Req::DesktopUpdateLease(v1::DesktopUpdateLeaseRequest {}))
            .await
            .map_err(|e| e.to_string())?;
        let mut lease = Self { call: Some(call) };
        let event = tokio::time::timeout(
            Duration::from_secs(22),
            lease.call.as_mut().unwrap().events.recv(),
        )
        .await
        .map_err(|_| "等待 Mac 保护事务结束超时，更新未开始。".to_owned())?;
        match event {
            Some(Ev::DesktopUpdateReady(_)) => Ok(lease),
            Some(_) => Err("后台服务未返回有效的更新许可。".into()),
            None => match lease.call.take().unwrap().finish().await {
                Err(e) => Err(format!("无法取得安全更新许可，未停止后台服务：{e}")),
                Ok(_) => Err("后台服务尚不支持安全更新许可，请安装新版发行包。".into()),
            },
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(call) = self.call.take()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            runtime.spawn(async move {
                call.cancel().await;
            });
        }
    }
}

pub struct Candidate {
    manager: UpdateManager,
    update: UpdateInfo,
}
pub struct Prepared {
    candidate: Candidate,
}
impl Candidate {
    pub fn version(&self) -> &str {
        &self.update.TargetFullRelease.Version
    }
    pub fn download(self) -> Result<Prepared, String> {
        self.manager
            .download_updates(&self.update, None)
            .map_err(|e| e.to_string())?;
        // SDK skips verification when a file already exists. Always verify it.
        verify_cached(&self.update.TargetFullRelease)?;
        Ok(Prepared { candidate: self })
    }
}
impl Prepared {
    /// Starts the component's relauncher. The shell then shuts down cleanly.
    pub fn apply(&self) -> Result<(), String> {
        verify_cached(&self.candidate.update.TargetFullRelease)?;
        self.candidate
            .manager
            .wait_exit_then_apply_updates(&self.candidate.update, false, true, ["--muster-updated"])
            .map_err(|e| e.to_string())
    }
}
pub fn check() -> Result<Option<Candidate>, String> {
    let manager = UpdateManager::new(
        SignedDohSource,
        Some(UpdateOptions {
            AllowVersionDowngrade: false,
            MaximumDeltasBeforeFallback: -1,
            ..Default::default()
        }),
        None,
    )
    .map_err(|e| format!("请先安装支持自升级的 Muster 发行版：{e}"))?;
    match manager.check_for_updates().map_err(|e| e.to_string())? {
        UpdateCheck::UpdateAvailable(update) => Ok(Some(Candidate {
            manager,
            update: *update,
        })),
        UpdateCheck::NoUpdateAvailable => Ok(None),
        UpdateCheck::RemoteIsEmpty => Err("尚未发布此平台的更新。".into()),
    }
}
fn verify_cached(asset: &VelopackAsset) -> Result<(), String> {
    let locator =
        auto_locate_app_manifest(LocationContext::FromCurrentExe).map_err(|e| e.to_string())?;
    verify_file(&locator.get_packages_dir().join(&asset.FileName), asset).map_err(|e| e.to_string())
}
fn verify_file(path: &Path, asset: &VelopackAsset) -> Result<(), Error> {
    let mut file = File::open(path)?;
    if file.metadata()?.len() != asset.Size {
        return Err(Error::Other("更新包大小不符".into()));
    }
    let mut hash = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    if hex::encode(hash.finalize()).eq_ignore_ascii_case(&asset.SHA256) {
        Ok(())
    } else {
        Err(Error::Other("更新包 SHA256 校验失败".into()))
    }
}
fn expected_channel() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "osx-arm64",
        ("macos", "x86_64") => "osx-x64",
        ("windows", "aarch64") => "win-arm64",
        ("windows", "x86_64") => "win-x64",
        _ => "unsupported",
    }
}
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 180
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn validate_asset(asset: &VelopackAsset) -> Result<(), Error> {
    if asset.PackageId != PACKAGE_ID
        || !safe_name(&asset.FileName)
        || !asset.FileName.ends_with("-full.nupkg")
        || asset.Type != "Full"
        || asset.Size == 0
        || asset.Size > MAX_PACKAGE
        || asset.SHA256.len() != 64
        || !asset.SHA256.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(Error::Other("无效的 Muster 更新包信息".into()));
    }
    Ok(())
}
fn verify_feed(
    name: &str,
    bytes: &[u8],
    signature: &[u8],
    key: &str,
) -> Result<VelopackAssetFeed, Error> {
    let key = VerifyingKey::from_public_key_pem(key).map_err(|e| Error::Other(e.to_string()))?;
    let signature = Signature::from_der(signature).map_err(|e| Error::Other(e.to_string()))?;
    key.verify(&[name.as_bytes(), b"\n", bytes].concat(), &signature)
        .map_err(|_| Error::Other("更新清单签名无效".into()))?;
    let feed: VelopackAssetFeed = serde_json::from_slice(bytes)?;
    for asset in &feed.Assets {
        validate_asset(asset)?;
    }
    Ok(feed)
}
struct NoSystemDns;
impl reqwest::dns::Resolve for NoSystemDns {
    fn resolve(&self, _: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async { Err(std::io::Error::other("system DNS disabled").into()) })
    }
}
struct SignedDohSource;
impl UpdateSource for SignedDohSource {
    fn get_release_feed(
        &self,
        channel: &str,
        _: &bundle::Manifest,
        _: &str,
    ) -> Result<VelopackAssetFeed, Error> {
        if channel != expected_channel() {
            return Err(Error::Other("无效的更新通道".into()));
        }
        let name = format!("releases.{channel}.json");
        let rt = runtime()?;
        rt.block_on(async {
            let feed = fetch(&name, 1024 * 1024).await?;
            let sig = fetch(&format!("{name}.sig"), 256).await?;
            verify_feed(&name, &feed, &sig, PUBLIC_KEY)
        })
    }
    fn download_release_entry(
        &self,
        asset: &VelopackAsset,
        path: &Path,
        _: Option<Sender<i16>>,
    ) -> Result<(), Error> {
        validate_asset(asset)?;
        runtime()?.block_on(async {
            use std::io::Write;
            let mut response = request(&asset.FileName).await?;
            let mut file = File::create(path)?;
            let mut size = 0u64;
            while let Some(chunk) = response.chunk().await.map_err(net_error)? {
                size += chunk.len() as u64;
                if size > asset.Size {
                    return Err(Error::Other("更新包超过声明大小".into()));
                }
                file.write_all(&chunk)?;
            }
            file.sync_all()?;
            verify_file(path, asset)
        })
    }
}
fn runtime() -> Result<tokio::runtime::Runtime, Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(Error::Io)
}
fn net_error(e: impl std::fmt::Display) -> Error {
    Error::Other(e.to_string())
}
async fn request(name: &str) -> Result<reqwest::Response, Error> {
    if !safe_name(name) {
        return Err(Error::Other("无效的更新文件名".into()));
    }
    let addresses = muster_net::doh::resolve("linsheng.dev", 8443)
        .await
        .map_err(net_error)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .https_only(true)
        .dns_resolver(std::sync::Arc::new(NoSystemDns))
        .resolve_to_addrs("linsheng.dev", &addresses)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(net_error)?;
    let response = client
        .get(format!("{BASE}{name}"))
        .send()
        .await
        .map_err(net_error)?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(Error::Other(format!("更新服务返回 {}", response.status())));
    }
    Ok(response)
}
async fn fetch(name: &str, limit: usize) -> Result<Vec<u8>, Error> {
    let mut response = request(name).await?;
    let mut data = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(net_error)? {
        if data.len() + chunk.len() > limit {
            return Err(Error::Other("更新清单过大".into()));
        }
        data.extend_from_slice(&chunk);
    }
    Ok(data)
}
#[cfg(test)]
mod tests {
    use super::*;
    use p256::{
        ecdsa::{SigningKey, signature::Signer},
        pkcs8::EncodePublicKey,
    };
    struct LeaseServer {
        ready: bool,
        released: std::sync::Arc<tokio::sync::Notify>,
    }
    impl muster_rpc::Handler for LeaseServer {
        fn call(
            &self,
            _: muster_rpc::Context,
            request: muster_proto::v1::request::Req,
            events: muster_rpc::EventSink,
        ) -> muster_rpc::HandlerFuture<'_> {
            Box::pin(async move {
                use muster_proto::v1::{self, request::Req, response::Resp};
                assert!(matches!(request, Req::DesktopUpdateLease(_)));
                if !self.ready {
                    return Resp::Error(v1::Error {
                        code: v1::error::Code::Conflict as i32,
                        message: "protection still pending".into(),
                    });
                }
                events
                    .send(v1::event::Ev::DesktopUpdateReady(v1::Ack {}))
                    .await
                    .unwrap();
                events.cancelled().cancelled().await;
                self.released.notify_one();
                Resp::Ack(v1::Ack {})
            })
        }
    }
    #[tokio::test]
    async fn lease_requires_daemon_ack_and_stays_held_until_dropped() {
        for ready in [false, true] {
            let released = std::sync::Arc::new(tokio::sync::Notify::new());
            let (a, b) = tokio::io::duplex(4096);
            let server = tokio::spawn(muster_rpc::serve(
                b,
                muster_rpc::Principal::Local,
                std::sync::Arc::new(LeaseServer {
                    ready,
                    released: released.clone(),
                }),
            ));
            let client = muster_rpc::Client::new(a);
            let result = Lease::acquire(client.clone()).await;
            assert_eq!(result.is_ok(), ready);
            if ready {
                assert!(
                    tokio::time::timeout(Duration::from_millis(30), released.notified())
                        .await
                        .is_err()
                );
                drop(result);
                tokio::time::timeout(Duration::from_secs(1), released.notified())
                    .await
                    .unwrap();
            }
            client.close();
            server.await.unwrap();
        }
    }

    #[test]
    fn rejects_tampered_feed_and_invalid_paths() {
        let key = SigningKey::from_slice(&[7; 32]).unwrap();
        let pem = key
            .verifying_key()
            .to_public_key_pem(Default::default())
            .unwrap();
        let feed = br#"{"Assets":[]}"#;
        let signature: Signature =
            key.sign(&[b"releases.osx-arm64.json\n".as_slice(), feed].concat());
        assert!(
            verify_feed(
                "releases.osx-arm64.json",
                feed,
                signature.to_der().as_bytes(),
                &pem
            )
            .is_ok()
        );
        assert!(
            verify_feed(
                "releases.osx-arm64.json",
                b"{}",
                signature.to_der().as_bytes(),
                &pem
            )
            .is_err()
        );
        for name in [
            "../evil",
            "/tmp/evil",
            "a%2fb",
            "https://evil",
            "..",
            "a\\b",
        ] {
            assert!(!safe_name(name));
        }
    }
    #[test]
    #[ignore = "downloads the signed public release through DoH"]
    fn published_release_download_verifies() {
        let feed_name = format!("releases.{}.json", expected_channel());
        let feed = runtime().unwrap().block_on(async {
            let bytes = fetch(&feed_name, 1024 * 1024).await.unwrap();
            let signature = fetch(&format!("{feed_name}.sig"), 256).await.unwrap();
            verify_feed(&feed_name, &bytes, &signature, PUBLIC_KEY).unwrap()
        });
        let asset = feed.Assets.first().expect("published package");
        let package = tempfile::NamedTempFile::new().unwrap();
        SignedDohSource
            .download_release_entry(asset, package.path(), None)
            .unwrap();
        verify_file(package.path(), asset).unwrap();
        println!(
            "verified public signed package {} ({} bytes)",
            asset.FileName, asset.Size
        );
    }
    #[test]
    fn cached_package_is_rechecked() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"abc").unwrap();
        let asset = VelopackAsset {
            PackageId: "Muster".into(),
            Type: "Full".into(),
            FileName: "Muster-1.0.1-full.nupkg".into(),
            Size: 3,
            SHA256: hex::encode(Sha256::digest(b"abc")),
            ..Default::default()
        };
        assert!(validate_asset(&asset).is_ok());
        assert!(verify_file(file.path(), &asset).is_ok());
        std::fs::write(file.path(), b"abd").unwrap();
        assert!(verify_file(file.path(), &asset).is_err());
    }
}
