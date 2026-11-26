use super::engine::*;
use crate::{Daemon, Error};
use muster_core::DeviceId;
use muster_net::remote_desktop::{
    Access, Handler, Session,
    gamestream::{self, EnginePorts},
};
use std::{
    future::Future,
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

#[cfg(windows)]
pub(super) const CAPTURE_HOST: &str = "Sunshine/muster-stream-host.exe";
#[cfg(target_os = "macos")]
pub(super) const CAPTURE_HOST: &str = "muster-desktop-host";

/// ScreenCaptureKit follows the Mac console through one local unlock.
#[cfg(target_os = "macos")]
fn console_lock_state(daemon: &Daemon) -> Option<bool> {
    if daemon.covered() {
        return None;
    }
    muster_platform::macos::lock::session_state()
        .filter(|s| s.on_console && s.login_done)
        .map(|s| s.screen_locked)
}

#[cfg(target_os = "macos")]
pub(crate) type IdleHost = super::idle::IdleSlot<HostProcess>;

pub(crate) struct HostProcess {
    #[cfg(windows)]
    _job: std::os::windows::io::OwnedHandle,
    pub(super) ports: EnginePorts,
    directory: tempfile::TempDir,
    pub(super) engine: Arc<Engine>,
    owner: tokio::process::ChildStdin,
    status: tokio::process::ChildStdout,
}

#[cfg(target_os = "macos")]
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
#[cfg(target_os = "macos")]
struct ActiveHost<'a> {
    host: &'a Host,
    process: Option<HostProcess>,
}

#[cfg(target_os = "macos")]
impl Drop for ActiveHost<'_> {
    fn drop(&mut self) {
        if let Some(process) = self.process.take()
            && let Some(daemon) = self.host.daemon.upgrade()
            && !self.host.surface.initially_locked
            && console_lock_state(&daemon) == Some(false)
            && self.host.lease.current(&daemon)
        {
            daemon.desktop_idle.park(process.reset());
        }
    }
}

struct ConsoleSurface {
    initially_locked: bool,
    unlocked: tokio_util::sync::CancellationToken,
}

impl ConsoleSurface {
    fn new(initially_locked: bool) -> Self {
        Self {
            initially_locked,
            unlocked: Default::default(),
        }
    }

    fn accepts(&self, locked: Option<bool>) -> bool {
        match locked {
            Some(false) => {
                if self.initially_locked {
                    self.unlocked.cancel();
                }
                true
            }
            Some(true) => self.initially_locked && !self.unlocked.is_cancelled(),
            None => false,
        }
    }
}

struct Host {
    surface: ConsoleSurface,
    #[cfg(windows)]
    console_session: std::sync::OnceLock<u32>,
    daemon: Weak<Daemon>,
    lease: super::permissions::Lease,
    peer: DeviceId,
    stop: tokio_util::sync::CancellationToken,
}
impl Handler for Host {
    fn console_ready(&self) -> bool {
        !self.stop.is_cancelled()
            && self.daemon.upgrade().is_some_and(|d| {
                #[cfg(target_os = "macos")]
                let ready = self.surface.accepts(console_lock_state(&d));
                // Before binding, this grant permits only the viewer handshake.
                // Media starts after the worker has attested the ordinary desktop.
                #[cfg(windows)]
                let ready = !d.covered()
                    && self.console_session.get().is_none_or(|session| {
                        super::console_worker::physical_id() == Some(*session)
                    });
                self.lease.current(&d) && ready
            })
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
        let native = hello
            .as_deref()
            .and_then(muster_proto::v1::DesktopStreamSettings::from_hello);
        let video = hello
            .as_deref()
            .and_then(muster_proto::v1::DesktopStreamSettings::from_video_hello);
        #[cfg(windows)]
        if native.is_some() {
            return Err(unavailable("this host requires Moonlight"));
        }
        let fixed_keyboard = hello.as_deref() == Some(FIXED_KEYBOARD_HELLO);
        if video.is_none() && native.is_none() && !fixed_keyboard && hello.as_deref() != Some(HELLO)
        {
            return Err(unavailable("unsupported desktop viewer"));
        }
        let daemon = self
            .daemon
            .upgrade()
            .ok_or_else(|| unavailable("daemon stopped"))?;
        let reserve = |expected: Option<&super::occupancy::Occupant>| {
            daemon
                .desktop_occupancy
                .reserve(self.peer, &self.stop, expected)
        };
        let _reservation = match reserve(None) {
            Ok(lease) => lease,
            Err(Some(current)) => {
                let mut busy = Vec::from(*BUSY);
                busy.extend_from_slice(&current.peer.0);
                session.send_control(&busy).await?;
                let choice = timeout(Duration::from_secs(90), session.recv_control())
                    .await
                    .map_err(|_| unavailable("desktop confirmation expired"))??;
                if choice.as_deref() != Some(REPLACE) || !self.console_ready() {
                    return Err(unavailable("desktop takeover cancelled"));
                }
                match reserve(Some(&current)) {
                    Ok(lease) => lease,
                    Err(_) => {
                        session.send_control(CHANGED).await?;
                        let _ = timeout(Duration::from_secs(1), session.recv_control()).await;
                        return Err(unavailable("desktop viewer changed"));
                    }
                }
            }
            Err(None) => return Err(unavailable("desktop viewer changed")),
        };
        // The previous broker must drop its sockets before the next engine binds.
        let _slot = timeout(Duration::from_secs(10), daemon.desktop_slot.acquire())
            .await
            .map_err(|_| unavailable("previous desktop viewer is still closing"))?
            .map_err(|_| unavailable("desktop service stopped"))?;
        #[cfg(target_os = "macos")]
        if let Some(settings) = native {
            if self.surface.initially_locked {
                session.send_control(CAPTURE_UNAVAILABLE).await?;
                return Err(unavailable("locked capture requires Moonlight"));
            }
            return super::native::host(session, settings).await;
        }
        #[cfg(target_os = "macos")]
        let ports = EnginePorts::MUSTER;
        let launch = Launch {
            mouse: session.with_mouse(|| ()).is_ok(),
            keyboard: !self.surface.initially_locked
                && self.lease.keyboard
                && (fixed_keyboard
                    || (video.is_some()
                        && hello.as_deref().is_some_and(
                            muster_proto::v1::DesktopStreamSettings::viewer_has_keyboard,
                        )))
                && session.with_mouse(|| ()).is_ok(),
            key: rand::random(),
        };
        if !self.console_ready() {
            return Err(unavailable("desktop permission ended"));
        }
        #[cfg(target_os = "macos")]
        let start_locked = self.surface.initially_locked && !self.surface.unlocked.is_cancelled();
        #[cfg(target_os = "macos")]
        let started = async {
            let size = video_mode(video.as_ref(), muster_platform::desktop_pixels())?;
            let mode = format!("{}x{}x{}", size.0, size.1, video.map_or(30, |p| p.fps));
            let parked = daemon
                .desktop_idle
                .take()
                .await
                .filter(|_| !self.surface.initially_locked);
            let process = start(
                &program(CAPTURE_HOST)?,
                parked,
                ports,
                &launch,
                &mode,
                start_locked,
                video.is_none_or(|s| s.show_cursor),
            )
            .await?;
            Ok::<_, Error>((process, size))
        }
        .await;
        #[cfg(windows)]
        let started = super::console_worker::request(&launch, video.as_ref()).await;
        let (process, (width, height)) = match started {
            Ok(result) => result,
            Err(error) => {
                #[cfg(windows)]
                let reason = if matches!(&error, Error::Unavailable(s) if s == CONSOLE_MESSAGE) {
                    CONSOLE_UNAVAILABLE
                } else {
                    CAPTURE_UNAVAILABLE
                };
                #[cfg(target_os = "macos")]
                let reason = CAPTURE_UNAVAILABLE;
                session.send_control(reason).await?;
                let _ = timeout(Duration::from_secs(1), session.recv_control()).await;
                return Err(error);
            }
        };
        let ports = process.ports;
        let engine = process.engine.clone();
        #[cfg(windows)]
        let mut connection = {
            if *self.console_session.get_or_init(|| process.session) != process.session
                || !self.console_ready()
            {
                return Err(unavailable(
                    "physical console changed before stream startup",
                ));
            }
            process
        };
        #[cfg(target_os = "macos")]
        let mut active = ActiveHost {
            host: self,
            process: Some(process),
        };
        let mut reply = launch.encode().to_vec();
        if video.is_some() {
            reply[3] = b'2';
            reply.extend(width.to_be_bytes());
            reply.extend(height.to_be_bytes());
        }
        session.send_control(&reply).await?;
        let switch_surface = async {
            #[cfg(target_os = "macos")]
            if start_locked {
                self.surface.unlocked.cancelled().await;
                if !self.console_ready() {
                    return Err(unavailable("desktop permission ended"));
                }
                active
                    .process
                    .as_mut()
                    .ok_or_else(|| unavailable("desktop host ended"))?
                    .owner
                    .write_all(b"U")
                    .await
                    .map_err(|_| unavailable("capture surface switch failed"))?;
            }
            #[cfg(windows)]
            {
                connection.closed().await
            }
            #[cfg(target_os = "macos")]
            std::future::pending::<Result<(), Error>>().await
        };
        tokio::select! {
            biased;
            result=switch_surface=>result,
            error=engine.exited()=>Err(error),
            result=gamestream::host(session,ports,engine.clone())=>Ok(result?),
        }
    }
}
pub(crate) fn admit(
    daemon: &Arc<Daemon>,
    peer: DeviceId,
) -> Option<muster_net::remote_desktop::Sharing> {
    let (mode, lease) = daemon.desktop_permissions.admit(daemon, peer)?;
    #[cfg(target_os = "macos")]
    let locked = console_lock_state(daemon)?;
    #[cfg(windows)]
    let locked = {
        if daemon.covered() {
            return None;
        }
        false
    };
    let host = Arc::new(Host {
        surface: ConsoleSurface::new(locked),
        #[cfg(windows)]
        console_session: Default::default(),
        daemon: Arc::downgrade(daemon),
        lease,
        peer,
        stop: Default::default(),
    });
    let access = if mode == muster_proto::v1::DesktopAccess::Mouse {
        Access::ViewAndMouse
    } else {
        Access::View
    };
    Some(muster_net::remote_desktop::Sharing::new(peer, access, host))
}

/// Local diagnostics exercise the installed capture helper without opening a peer session.
#[cfg(target_os = "macos")]
pub(crate) async fn display_sample(daemon: &Daemon) -> Result<String, Error> {
    if daemon.covered() {
        return Err(unavailable("display is covered"));
    }
    let _slot = daemon
        .desktop_slot
        .try_acquire()
        .map_err(|_| unavailable("desktop in use"))?;
    let mut command = Command::new(program(CAPTURE_HOST)?);
    command
        .arg("--muster-display-sample")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let result = timeout(Duration::from_secs(12), command.output())
        .await
        .map_err(|_| unavailable("display sampler timed out"))?
        .map_err(|_| unavailable("display sampler could not start"))?;
    if !result.status.success() {
        return Err(unavailable("display sampler failed"));
    }
    Ok(String::from_utf8_lossy(&result.stdout).trim().to_owned())
}

pub(super) fn video_mode(
    video: Option<&muster_proto::v1::DesktopStreamSettings>,
    display: Option<(u32, u32)>,
) -> Result<(u16, u16), Error> {
    match video {
        None => Ok((1280, 720)),
        Some(settings) => display
            .and_then(|(w, h)| settings.video_size(w, h))
            .ok_or_else(|| unavailable("cannot read a valid primary display size")),
    }
}

pub(super) async fn start(
    executable: &Path,
    parked: Option<HostProcess>,
    ports: EnginePorts,
    launch: &Launch,
    mode: &str,
    _locked: bool,
    cursor: bool,
) -> Result<HostProcess, Error> {
    let (directory, warm) = if let Some(host) = parked.filter(|host| host.engine.pid().is_some()) {
        (host.directory, Some((host.engine, host.owner, host.status)))
    } else {
        (
            {
                let mut builder = tempfile::Builder::new();
                builder.prefix("muster-desktop-");
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    builder.permissions(std::fs::Permissions::from_mode(0o700));
                }
                builder.tempdir()
            }
            .map_err(|_| unavailable("cannot create a private engine directory"))?,
            None,
        )
    };
    let dir = directory.path();
    #[cfg(windows)]
    let private = dir.join("session");
    #[cfg(windows)]
    let dir = {
        muster_local::create_private_directory(&private)
            .map_err(|_| unavailable("cannot protect host credentials"))?;
        private.as_path()
    };
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
    #[cfg(windows)]
    let (ports, reservations) = ports.reserve_host_tcp()?;
    let base = ports.base();
    let media_base = ports.media_base();
    let config_directory = dir.display();
    let config = format!(
        "port = {base}\nmuster_media_port = {media_base}\npkey = {config_directory}/host.key\ncert = {config_directory}/host.pem\nfile_apps = {config_directory}/apps.json\n"
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
        #[cfg(target_os = "macos")]
        {
            cmd.current_dir(dir)
                .env_remove("MUSTER_DESKTOP_LOCK_SURFACE");
            if _locked {
                cmd.env("MUSTER_DESKTOP_LOCK_SURFACE", "1");
            }
        }
        #[cfg(windows)]
        cmd.current_dir(
            executable
                .parent()
                .ok_or_else(|| unavailable("missing host directory"))?,
        );
        cmd.arg("--muster-managed")
            .arg(dir.join("sunshine.conf"))
            .env("MUSTER_DESKTOP_SESSION_DIR", dir)
            .stdout(Stdio::piped());
        #[cfg(windows)]
        drop(reservations);
        let (child, owner, status) = Engine::launch(&mut cmd)?;
        (
            Arc::new(child),
            owner,
            status.ok_or_else(|| unavailable("missing host status pipe"))?,
        )
    };
    #[cfg(windows)]
    let job = engine
        .capture_job()
        .map_err(|_| unavailable("cannot contain the capture process tree"))?;
    let mut bootstrap = Vec::from(*b"MST1");
    if !cursor {
        bootstrap[3] = b'2';
    }
    bootstrap.push(launch.input_mode());
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
            ("mode", mode),
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
        return Err(unavailable(
            "host capture/encoder unavailable (503); check the interactive display and screen capture permissions",
        ));
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
        #[cfg(windows)]
        _job: job,
        ports,
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
    #[test]
    fn remote_surface_allows_one_unlock_of_the_admitted_console() {
        let surface = ConsoleSurface::new(true);
        assert!(surface.accepts(Some(true)));
        assert!(!surface.unlocked.is_cancelled());
        assert!(surface.accepts(Some(false)));
        assert!(surface.unlocked.is_cancelled());
        assert!(surface.accepts(Some(false)));
        assert!(!surface.accepts(Some(true)));
        assert!(!surface.accepts(None));
        let desktop = ConsoleSurface::new(false);
        assert!(desktop.accepts(Some(false)));
        assert!(!desktop.accepts(Some(true)));
        assert!(!desktop.accepts(None));
    }

    #[test]
    fn configured_viewers_require_the_worker_display_extent() {
        let settings = muster_proto::v1::DesktopStreamSettings {
            long_edge: 1920,
            ..muster_proto::v1::DesktopStreamSettings::ORIGINAL
        };
        assert!(video_mode(Some(&settings), None).is_err());
        assert!(video_mode(Some(&settings), Some((0, 1080))).is_err());
        assert_eq!(
            video_mode(Some(&settings), Some((1440, 2560))).unwrap(),
            (1080, 1920)
        );
        assert_eq!(video_mode(None, None).unwrap(), (1280, 720));
    }

    /// Run on an opted-in test host with the packaged Muster capture tool.
    #[tokio::test]
    #[ignore = "requires an interactive Mac and a locally selected Sunshine test binary"]
    async fn private_host_launch() {
        let executable = std::env::var_os("MUSTER_TEST_SUNSHINE").expect("test host executable");
        let launch = Launch {
            mouse: false,
            keyboard: false,
            key: rand::random(),
        };
        let result = start(
            Path::new(&executable),
            None,
            EnginePorts::MUSTER,
            &launch,
            "1280x720x30",
            false,
            true,
        )
        .await;
        match result {
            Ok(_process) => eprintln!("Private host accepted encrypted view-only launch"),
            Err(error) => panic!("{error}"),
        }
    }
}
