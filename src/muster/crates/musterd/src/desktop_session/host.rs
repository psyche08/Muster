use super::engine::*;
use crate::{Daemon, Error};
use muster_core::DeviceId;
use muster_net::remote_desktop::{
    Access, Handler, Session,
    gamestream::{self, EnginePorts},
};
use std::{
    future::Future,
    os::unix::fs::PermissionsExt,
    path::Path,
    pin::Pin,
    process::Stdio,
    sync::{Arc, Weak},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    time::{sleep, timeout},
};

pub(super) const CAPTURE_HOST: &str = "muster-desktop-host";

pub(super) fn console_ready(daemon: &Daemon) -> bool {
    !daemon.covered()
        && muster_platform::macos::lock::session_state()
            .is_some_and(|s| s.on_console && s.login_done && !s.screen_locked)
}

pub(crate) type IdleHost = super::idle::IdleSlot<HostProcess>;

pub(crate) struct HostProcess {
    directory: tempfile::TempDir,
    engine: Arc<Engine>,
    owner: tokio::process::ChildStdin,
    status: tokio::process::ChildStdout,
}

impl HostProcess {
    async fn reset(mut self) -> Option<Self> {
        self.owner.write_all(b"P").await.ok()?;
        if timeout(Duration::from_secs(10), self.status.read_u8())
            .await
            .ok()?
            .ok()?
            != b'I'
        {
            return None;
        }
        // The dormant image has no credentials. Remove the old TLS files too.
        for name in ["host.key", "host.pem"] {
            std::fs::remove_file(self.directory.path().join(name)).ok()?;
        }
        Some(self)
    }
}

// Dropping the broker future also runs this path on a transport disconnect.
struct ActiveHost<'a> {
    host: &'a Host,
    process: Option<HostProcess>,
}

impl Drop for ActiveHost<'_> {
    fn drop(&mut self) {
        if let Some(process) = self.process.take()
            && let Some(daemon) = self.host.daemon.upgrade()
            && console_ready(&daemon)
            && self.host.lease.current(&daemon)
        {
            daemon.desktop_idle.park(process.reset());
        }
    }
}

struct Host {
    daemon: Weak<Daemon>,
    program: std::path::PathBuf,
    lease: super::permissions::Lease,
    _slot: tokio::sync::OwnedSemaphorePermit,
}
impl Handler for Host {
    fn console_ready(&self) -> bool {
        self.daemon
            .upgrade()
            .is_some_and(|d| console_ready(&d) && self.lease.current(&d))
    }
    fn serve<'a>(
        &'a self,
        _: DeviceId,
        session: &'a mut Session,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            if let Err(error) = self.stream(session).await {
                log::info!("Desktop session ended: {error}");
            }
        })
    }
}
impl Host {
    async fn stream(&self, session: &mut Session) -> Result<(), Error> {
        let hello = timeout(Duration::from_secs(5), session.recv_control())
            .await
            .map_err(|_| unavailable("viewer handshake timed out"))??;
        if hello.as_deref() != Some(HELLO) {
            return Err(unavailable("unsupported desktop viewer"));
        }
        let ports = EnginePorts::MUSTER;
        let launch = Launch {
            mouse: session.with_mouse(|| ()).is_ok(),
            key: rand::random(),
        };
        let daemon = self
            .daemon
            .upgrade()
            .ok_or_else(|| unavailable("daemon stopped"))?;
        let parked = daemon.desktop_idle.take().await;
        if !self.console_ready() {
            return Err(unavailable("desktop permission ended"));
        }
        let process = match start(&self.program, parked, ports, &launch).await {
            Ok(started) => started,
            Err(error) => {
                session.send_control(CAPTURE_UNAVAILABLE).await?;
                let _ = timeout(Duration::from_secs(1), session.recv_control()).await;
                return Err(error);
            }
        };
        let engine = process.engine.clone();
        let _active = ActiveHost {
            host: self,
            process: Some(process),
        };
        session.send_control(&launch.encode()).await?;
        tokio::select! {
            biased;
            error=engine.exited()=>Err(error),
            result=gamestream::host(session,ports,engine.clone())=>Ok(result?),
        }
    }
}
pub(crate) fn admit(
    daemon: &Arc<Daemon>,
    peer: DeviceId,
) -> Option<muster_net::remote_desktop::Sharing> {
    let slot = daemon.desktop_slot.clone().try_acquire_owned().ok()?;
    let program = program(CAPTURE_HOST).ok()?;
    let (mode, lease) = daemon.desktop_permissions.admit(daemon, peer)?;
    let host = Arc::new(Host {
        daemon: Arc::downgrade(daemon),
        program,
        lease,
        _slot: slot,
    });
    let access = if mode == muster_proto::v1::DesktopAccess::Mouse {
        Access::ViewAndMouse
    } else {
        Access::View
    };
    Some(muster_net::remote_desktop::Sharing::new(peer, access, host))
}

async fn start(
    executable: &Path,
    parked: Option<HostProcess>,
    ports: EnginePorts,
    launch: &Launch,
) -> Result<HostProcess, Error> {
    let (directory, warm) = if let Some(host) = parked.filter(|host| host.engine.pid().is_some()) {
        (host.directory, Some((host.engine, host.owner, host.status)))
    } else {
        (
            tempfile::Builder::new()
                .prefix("muster-desktop-")
                .permissions(std::fs::Permissions::from_mode(0o700))
                .tempdir()
                .map_err(|_| unavailable("cannot create a private engine directory"))?,
            None,
        )
    };
    let dir = directory.path();
    let tls_error = || unavailable("cannot set up private desktop engine TLS");
    let server =
        rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).map_err(|_| tls_error())?;
    let client = rcgen::generate_simple_self_signed(vec!["muster-viewer.invalid".into()])
        .map_err(|_| tls_error())?;
    let cert = server.cert.pem();
    let client_cert = client.cert.pem().replace("\r\n", "\n");
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(server.cert.der().clone())
        .map_err(|_| tls_error())?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|_| tls_error())?
        .with_root_certificates(roots)
        .with_client_auth_cert(
            vec![client.cert.der().clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(client.key_pair.serialize_der()).into(),
        )
        .map_err(|_| tls_error())?;
    // The host verifies the selected client on each HTTPS connection.
    config.resumption = rustls::client::Resumption::disabled();
    let client = reqwest::Client::builder()
        .use_preconfigured_tls(config)
        .http1_only()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(1))
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|_| tls_error())?;
    let base = ports.base();
    let config_directory = dir.display();
    let config = format!(
        "port = {base}\npkey = {config_directory}/host.key\ncert = {config_directory}/host.pem\nfile_apps = {config_directory}/apps.json\n"
    );
    for (name, bytes) in [
        ("sunshine.conf", config.as_bytes()),
        ("host.pem", cert.as_bytes()),
        ("host.key", server.key_pair.serialize_pem().as_bytes()),
        ("apps.json", br#"{"env":{},"apps":[]}"#),
    ] {
        std::fs::write(dir.join(name), bytes)
            .map_err(|_| unavailable("cannot prepare private host configuration"))?;
    }
    let (engine, mut owner, status) = if let Some((engine, mut owner, status)) = warm {
        owner
            .write_all(b"R")
            .await
            .map_err(|_| unavailable("cannot resume idle host"))?;
        (engine, owner, status)
    } else {
        let mut cmd = Command::new(executable);
        cmd.current_dir(dir)
            .arg("--muster-managed")
            .arg(dir.join("sunshine.conf"))
            .env("MUSTER_DESKTOP_SESSION_DIR", dir)
            .stdout(Stdio::piped());
        let (child, owner, status) = Engine::launch(&mut cmd)?;
        (
            Arc::new(child),
            owner,
            status.ok_or_else(|| unavailable("missing host status pipe"))?,
        )
    };
    let mut bootstrap = Vec::from(*b"MST1");
    bootstrap.push(launch.mouse as u8);
    bootstrap.extend(
        u16::try_from(client_cert.len())
            .map_err(|_| tls_error())?
            .to_be_bytes(),
    );
    bootstrap.extend(client_cert.as_bytes());
    owner
        .write_all(&bootstrap)
        .await
        .map_err(|_| unavailable("host rejected bootstrap"))?;
    let origin = format!("https://127.0.0.1:{}", base - 5);
    let ready = async {
        loop {
            if let Ok(response) = client.get(format!("{origin}/applist")).send().await {
                let xml = bounded_xml(response).await?;
                if !xml.contains("status_code=\"200\"")
                    || !xml.contains("<AppTitle>Desktop</AppTitle>")
                    || !xml.contains("<ID>0</ID>")
                {
                    return Err(unavailable(
                        "packaged host rejected private desktop pairing",
                    ));
                }
                return Ok(());
            }
            sleep(Duration::from_millis(150)).await;
        }
    };
    tokio::select! {
        error=engine.exited()=>return Err(error),
        result=timeout(Duration::from_secs(30),ready)=>result.map_err(|_|unavailable("desktop host did not start in time"))??,
    }
    let key = hex::encode(&launch.key[..16]);
    let iv = i32::from_be_bytes(launch.key[16..].try_into().unwrap());
    let response = client
        .get(format!("{origin}/launch"))
        .query(&[
            ("appid", "0"),
            ("mode", "1280x720x30"),
            ("sops", "0"),
            ("rikey", &key),
            ("rikeyid", &iv.to_string()),
            ("localAudioPlayMode", "0"),
            ("surroundAudioInfo", "196610"),
            ("corever", "1"),
            ("additionalStates", "1"),
        ])
        .send()
        .await
        .map_err(|_| unavailable("private desktop launch connection failed"))?;
    let xml = bounded_xml(response).await?;
    if xml.contains("status_code=\"503\"") {
        let message = if launch.mouse {
            "host capture/encoder unavailable (503); check the Mac display and Muster's Screen Recording and Accessibility permissions"
        } else {
            "host capture/encoder unavailable (503); check the Mac display and Muster's Screen Recording permission"
        };
        return Err(unavailable(message));
    }
    if !xml.contains("status_code=\"200\"")
        || !xml.contains("<gamesession>1</gamesession>")
        || !xml.contains(&format!(
            "<sessionUrl0>rtspenc://127.0.0.1:{}</sessionUrl0>",
            base + 21
        ))
    {
        return Err(unavailable(
            "host did not return an encrypted desktop session",
        ));
    }
    Ok(HostProcess {
        directory,
        engine,
        owner,
        status,
    })
}

async fn bounded_xml(mut response: reqwest::Response) -> Result<String, Error> {
    let invalid = || unavailable("invalid private host response");
    if !response.status().is_success() {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| invalid())? {
        if chunk.len() > 65536 - bytes.len() {
            return Err(invalid());
        }
        bytes.extend(chunk);
    }
    String::from_utf8(bytes).map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Run on an opted-in test host with the packaged Muster capture tool.
    #[tokio::test]
    #[ignore = "requires an interactive Mac and a locally selected Sunshine test binary"]
    async fn private_host_launch() {
        let executable = std::env::var_os("MUSTER_TEST_SUNSHINE").expect("test host executable");
        let launch = Launch {
            mouse: false,
            key: rand::random(),
        };
        let result = start(Path::new(&executable), None, EnginePorts::MUSTER, &launch).await;
        match result {
            Ok(_process) => eprintln!("Private host accepted encrypted view-only launch"),
            Err(error) => panic!("{error}"),
        }
    }
}
