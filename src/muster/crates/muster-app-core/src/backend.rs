//! Where the core's requests go. Phones embed the daemon (their own link,
//! D8); desktops talk to musterd over its socket and reach other members
//! through it (Envelope.target).

use std::path::PathBuf;
use std::sync::Arc;

use muster_core::{DeviceId, HwSigner, PublicKey, SharedSigner};
use muster_rpc::{Client, Principal};
use musterd::{Daemon, DaemonConfig, MusterHandler};

use crate::{AppConfig, Error, Mode, Shell};

/// The device key held by the platform (secure element), used through the
/// shell's synchronous sign callback.
pub struct ShellSigner {
    shell: Arc<dyn Shell>,
    public: PublicKey,
}

impl std::fmt::Debug for ShellSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ShellSigner({})", self.public.device_id().short())
    }
}

impl HwSigner for ShellSigner {
    fn public_key(&self) -> PublicKey {
        self.public.clone()
    }
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, muster_core::Error> {
        self.shell
            .hw_sign(message)
            .ok_or_else(|| muster_core::Error::Signature("the platform refused to sign".into()))
    }
    fn backend(&self) -> &'static str {
        "platform"
    }
}

pub enum Backend {
    Embedded { daemon: Arc<Daemon>, local: Client },
    Socket { path: PathBuf, local: Client },
}

impl Backend {
    pub async fn connect(config: &AppConfig, shell: Arc<dyn Shell>) -> Result<Backend, Error> {
        match &config.mode {
            Mode::Socket { path } => {
                let stream = tokio::net::UnixStream::connect(path).await.map_err(|e| {
                    Error::Unavailable(format!("musterd at {}: {e}", path.display()))
                })?;
                let local = Client::new(stream);
                if let Some(public_key) = shell.presence_public_key() {
                    let result = tokio::time::timeout(
                        std::time::Duration::from_secs(3),
                        local.call(muster_proto::v1::request::Req::DevicePresence(
                            muster_proto::v1::DevicePresenceRequest { public_key },
                        )),
                    )
                    .await;
                    if !matches!(result, Ok(Ok(_))) {
                        log::warn!(
                            "desktop Presence candidate registration unavailable; update musterd before enrollment"
                        );
                    } else {
                        log::info!("desktop Presence candidate registered with local daemon");
                    }
                } else {
                    log::warn!(
                        "desktop Presence public key unavailable; check native keychain entitlements"
                    );
                }
                Ok(Backend::Socket {
                    path: path.clone(),
                    local,
                })
            }
            Mode::Embedded {
                relay,
                name,
                kind,
                bind,
                version,
            } => {
                let mut daemon_config = DaemonConfig::new(&config.dir);
                daemon_config.version = version.clone();
                daemon_config.bind = *bind;
                daemon_config.relay = relay.clone();
                daemon_config.name = name.clone();
                daemon_config.kind =
                    musterd::parse_kind(kind).unwrap_or(muster_core::roster::DeviceKind::Phone);
                daemon_config.agent = None;
                daemon_config.terminal = None;
                daemon_config.listen = false;
                daemon_config.allow_loopback = config.allow_loopback;
                daemon_config.presence_key = shell
                    .presence_public_key()
                    .and_then(|raw| PublicKey::from_raw(&raw).ok());
                let signer: Option<SharedSigner> = match shell.hw_public_key() {
                    Some(raw) => {
                        let public = PublicKey::from_raw(&raw)
                            .map_err(|e| Error::Invalid(format!("device key: {e}")))?;
                        Some(Arc::new(ShellSigner {
                            shell: shell.clone(),
                            public,
                        }))
                    }
                    None => None,
                };
                let daemon =
                    Daemon::start_with(daemon_config, Arc::new(musterd::DefaultPlatform), signer)
                        .await
                        .map_err(|e| Error::Unavailable(format!("start: {e}")))?;
                let diagnostic_shell = shell.clone();
                daemon
                    .node()
                    .set_close_observer(Arc::new(move |peer, reason| {
                        crate::session_diagnostics::emit(
                            diagnostic_shell.as_ref(),
                            serde_json::json!({
                        "record_type":"connection_diagnostic","event":"closed","reason":reason,
                        "target_ref":crate::cache::Cache::key(&[&peer.to_hex()])[..16]}),
                        );
                    }));
                let (app_side, daemon_side) = tokio::io::duplex(1 << 20);
                let handler: Arc<dyn muster_rpc::Handler> =
                    Arc::new(MusterHandler::new(Arc::downgrade(&daemon)));
                tokio::spawn(muster_rpc::serve(daemon_side, Principal::Local, handler));
                Ok(Backend::Embedded {
                    daemon,
                    local: Client::new(app_side),
                })
            }
        }
    }

    pub fn is_embedded(&self) -> bool {
        matches!(self, Backend::Embedded { .. })
    }

    pub fn local(&self) -> Client {
        match self {
            Backend::Embedded { local, .. } | Backend::Socket { local, .. } => local.clone(),
        }
    }

    pub fn daemon(&self) -> Option<&Arc<Daemon>> {
        match self {
            Backend::Embedded { daemon, .. } => Some(daemon),
            Backend::Socket { .. } => None,
        }
    }

    pub fn socket_path(&self) -> Option<&PathBuf> {
        match self {
            Backend::Socket { path, .. } => Some(path),
            Backend::Embedded { .. } => None,
        }
    }

    pub fn device_id(&self) -> Option<String> {
        self.daemon().map(|d| d.device_id().to_hex())
    }

    /// The device's own key, for the in-app SSH client (embedded only).
    pub fn signer(&self) -> Option<SharedSigner> {
        self.daemon().map(|d| d.signer().clone())
    }

    /// An operation-owned remote service stream over Node's pooled peer QUIC
    /// connection. Local operations retain their in-process/socket transport.
    pub async fn client_for(&self, device: &str, self_id: &str) -> Result<Client, Error> {
        self.client_for_observed(device, self_id, None).await
    }
    pub async fn client_for_observed(
        &self,
        device: &str,
        self_id: &str,
        observer: Option<muster_rpc::TimingObserver>,
    ) -> Result<Client, Error> {
        let result = self.open_client(device, self_id, observer.clone()).await?;
        Ok(result.with_timing_observer(observer))
    }
    async fn open_client(
        &self,
        device: &str,
        self_id: &str,
        observer: Option<muster_rpc::TimingObserver>,
    ) -> Result<Client, Error> {
        if device.is_empty() || device == self_id {
            return Ok(self.local());
        }
        match self {
            Backend::Socket { local, .. } => Ok(local.with_target(device)),
            Backend::Embedded { daemon, .. } => {
                let peer = DeviceId::from_hex(device)
                    .map_err(|e| Error::Invalid(format!("device: {e}")))?;
                let (send, recv) = tokio::time::timeout(
                    muster_net::reconnect::CONNECT_TIMEOUT,
                    daemon.node().open_observed(
                        peer,
                        musterd::SERVICE,
                        musterd::SERVICE_VERSION,
                        observer.map(|observer| {
                            Arc::new(move |timing| {
                                observer(crate::session_diagnostics::open_timing(timing))
                            }) as muster_net::node::OpenObserver
                        }),
                    ),
                )
                .await
                .map_err(|_| Error::Unavailable("the device did not answer".into()))?
                .map_err(|e| Error::Unavailable(format!("link: {e}")))?;
                let client = Client::new(tokio::io::join(recv, send));
                Ok(client)
            }
        }
    }

    /// Retire only the unresponsive operation stream, never the local shared
    /// transport or the peer's underlying authenticated QUIC connection.
    pub fn discard(&self, _device: &str, client: &Client) {
        if let Backend::Embedded { local, .. } = self
            && !local.same_connection(client)
        {
            client.close();
        }
    }

    /// A forward stream to `port` on `device` (embedded only: desktops use vnet).
    pub async fn forward(
        &self,
        device: &str,
        port: u16,
    ) -> Result<Box<dyn muster_rpc::Stream>, Error> {
        match self {
            Backend::Embedded { daemon, .. } => {
                let peer = DeviceId::from_hex(device)
                    .map_err(|e| Error::Invalid(format!("device: {e}")))?;
                let (send, recv) = tokio::time::timeout(
                    muster_net::reconnect::CONNECT_TIMEOUT,
                    daemon.node().forward(peer, port),
                )
                .await
                .map_err(|_| Error::Unavailable("the device did not answer".into()))?
                .map_err(|e| Error::Unavailable(format!("forward: {e}")))?;
                Ok(Box::new(tokio::io::join(recv, send)))
            }
            Backend::Socket { .. } => {
                Err(Error::Unavailable("on a desktop, use ssh over vnet".into()))
            }
        }
    }

    /// Re-register/re-probe the underlying peer transport. Operation streams
    /// follow their QUIC connection's lifecycle and are not cached here.
    pub fn network_changed(&self) {
        if let Backend::Embedded { daemon, .. } = self {
            daemon.network_changed();
        }
    }

    /// The user refreshed: offline members are tried again. Embedded only;
    /// a desktop app asks its daemon with `NetworkRefresh`.
    pub fn refresh_presence(&self) {
        if let Backend::Embedded { daemon, .. } = self {
            daemon.refresh_presence();
        }
    }

    /// The app went to the background (`true`) or came back.
    pub fn suspend(&self, suspended: bool) {
        if let Backend::Embedded { daemon, .. } = self {
            if suspended {
                daemon.suspend();
            } else {
                daemon.resume();
            }
        }
    }

    pub async fn shutdown(&self) {
        if let Backend::Embedded { daemon, .. } = self {
            daemon.shutdown().await;
        }
    }
}
