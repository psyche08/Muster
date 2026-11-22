use super::engine::*;
use crate::{Daemon, Error};
use muster_core::DeviceId;
use muster_net::remote_desktop::{
    Access, Handler, Session,
    gamestream::{self, EnginePorts},
};
use muster_proto::v1::desktop_session_progress::Phase;
use muster_rpc::EventSink;
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
    io::AsyncWriteExt,
    process::Command,
    sync::watch,
    time::{sleep, timeout},
};

pub(super) const CAPTURE_HOST: &str = "muster-desktop-host";

struct Host {
    daemon: Weak<Daemon>,
    program: std::path::PathBuf,
    events: EventSink,
    finished: watch::Sender<Option<Result<(), String>>>,
}
impl Handler for Host {
    fn console_ready(&self) -> bool {
        self.daemon.upgrade().is_some_and(|d| !d.covered())
            && muster_platform::macos::lock::session_state()
                .is_some_and(|s| s.on_console && s.login_done && !s.screen_locked)
    }
    fn serve<'a>(
        &'a self,
        _: DeviceId,
        session: &'a mut Session,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            struct Completion<'a>(&'a watch::Sender<Option<Result<(), String>>>);
            impl Drop for Completion<'_> {
                fn drop(&mut self) {
                    self.0.send_if_modified(|r| {
                        if r.is_some() {
                            false
                        } else {
                            *r = Some(Ok(()));
                            true
                        }
                    });
                }
            }
            let _completion = Completion(&self.finished);
            let result = self.stream(session).await.map_err(|e| e.to_string());
            self.finished.send_replace(Some(result));
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
        progress(&self.events, Phase::Starting).await?;
        let ports = EnginePorts::MUSTER;
        let launch = Launch {
            mouse: session.with_mouse(|| ()).is_ok(),
            key: rand::random(),
        };
        let directory = tempfile::Builder::new()
            .prefix("muster-desktop-")
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .map_err(|_| unavailable("cannot create a private engine directory"))?;
        let (engine, _owner) = match start(&self.program, directory.path(), ports, &launch).await {
            Ok(started) => started,
            Err(error) => {
                session.send_control(CAPTURE_UNAVAILABLE).await?;
                let _ = timeout(Duration::from_secs(1), session.recv_control()).await;
                return Err(error);
            }
        };
        session.send_control(&launch.encode()).await?;
        tokio::select! {
            biased;
            error=engine.exited()=>Err(error),
            result=gamestream::host(session,ports,engine.clone())=>Ok(result?),
        }
    }
}
pub(super) async fn offer(
    daemon: &Arc<Daemon>,
    peer: DeviceId,
    mouse: bool,
    events: &EventSink,
) -> Result<(), Error> {
    let (finished, mut result) = watch::channel(None);
    let host = Arc::new(Host {
        daemon: Arc::downgrade(daemon),
        program: program(CAPTURE_HOST)?,
        events: events.clone(),
        finished,
    });
    if !host.console_ready() {
        return Err(unavailable("Unlock the Mac and uncover its active desktop before sharing"));
    }
    let _sharing = daemon.node().share_remote_desktop(
        peer,
        if mouse {
            Access::ViewAndMouse
        } else {
            Access::View
        },
        host.clone(),
    )?;
    progress(events, Phase::Offered).await?;
    loop {
        tokio::select! {
            change=result.changed()=>{
                change.map_err(|_|unavailable("desktop sharing ended"))?;
                if let Some(result)=result.borrow_and_update().clone(){return result.map_err(unavailable);}
            },
            _=sleep(Duration::from_millis(250))=>if !host.console_ready(){return Err(unavailable("desktop sharing ended because the console is unavailable"));},
        }
    }
}

async fn start(
    executable: &Path,
    dir: &Path,
    ports: EnginePorts,
    launch: &Launch,
) -> Result<(Arc<Engine>, tokio::process::ChildStdin), Error> {
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
    let directory = dir.display();
    let config = format!(
        "port = {base}\npkey = {directory}/host.key\ncert = {directory}/host.pem\nfile_apps = {directory}/apps.json\n"
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
    let mut cmd = Command::new(executable);
    cmd.current_dir(dir)
        .arg(dir.join("sunshine.conf"))
        .env("MUSTER_DESKTOP_SESSION_DIR", dir)
        .stdout(Stdio::null());
    let (child, mut owner, _) = Engine::launch(&mut cmd)?;
    let engine = Arc::new(child);
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
    Ok((engine, owner))
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
        let dir = tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let launch = Launch {
            mouse: false,
            key: rand::random(),
        };
        let result = start(
            Path::new(&executable),
            dir.path(),
            EnginePorts::MUSTER,
            &launch,
        )
        .await;
        match result {
            Ok((_engine, _owner)) => eprintln!("Private host accepted encrypted view-only launch"),
            Err(error) => panic!("{error}"),
        }
    }
}
