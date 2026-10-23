//! muster-app-core (ARCHITECTURE §3.7): the one runtime every app shell
//! hosts. The shell sends Intents and answers Effects; the core keeps all
//! state, talks to daemons over muster-rpc, and hands back ViewModels. On
//! phones it embeds the daemon (its own link); on desktops it talks to
//! musterd over the socket.

mod access;
pub mod backend;
mod cache;
mod chat_hosts;
#[cfg(unix)]
pub mod desktop_startup;
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub mod desktop_update;
mod drafts;
mod render;
mod send_diagnostics;
mod state;
mod timeline;
mod trace;
mod update;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use muster_proto::Message as _;
pub use muster_proto::app;
use tokio::sync::mpsc;

use crate::backend::Backend;
use crate::state::State;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid: {0}")]
    Invalid(String),
    #[error("unavailable: {0}")]
    Unavailable(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("{0}")]
    Rpc(String),
    /// A desktop app is held by another operation on that Mac.
    #[error("{0}")]
    Busy(String),
}

impl From<muster_rpc::Error> for Error {
    fn from(error: muster_rpc::Error) -> Self {
        match error {
            muster_rpc::Error::Remote { code, message } => match code {
                muster_proto::v1::error::Code::Unauthorized => Error::Unauthorized(message),
                muster_proto::v1::error::Code::Invalid => Error::Invalid(message),
                muster_proto::v1::error::Code::Busy => Error::Busy(message),
                _ => Error::Rpc(message),
            },
            other => Error::Unavailable(other.to_string()),
        }
    }
}

/// What the platform provides: rendering, effects, and the hardware keys.
pub trait Shell: Send + Sync + 'static {
    /// A new ViewModel (encoded `app.ViewModel`).
    fn on_view(&self, view_model: &[u8]);
    /// Perform an Effect (encoded `app.Effect`) and answer with `effect_result`.
    fn on_effect(&self, id: u64, effect: &[u8]);
    /// The device key as a 65-byte X9.63 point; `None` uses a software key
    /// kept in the store (development only).
    fn hw_public_key(&self) -> Option<Vec<u8>>;
    /// ECDSA-P256-SHA256 with the device key, DER encoded.
    fn hw_sign(&self, message: &[u8]) -> Option<Vec<u8>>;
    /// The presence key's public point, if the platform has one.
    fn presence_public_key(&self) -> Option<Vec<u8>> {
        None
    }
}

#[derive(Clone, Debug)]
pub enum Mode {
    /// Own link inside the app (phones).
    Embedded {
        relay: Option<String>,
        name: String,
        kind: String,
        bind: SocketAddr,
        /// The app's release version, reported in settings and the
        /// heartbeat; empty = the crate's.
        version: String,
    },
    /// musterd on this machine (desktops).
    Socket { path: PathBuf },
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub dir: PathBuf,
    pub mode: Mode,
    pub allow_loopback: bool,
}

/// Everything that drives the reducer.
pub(crate) enum Loop {
    Intent(app::Intent),
    Msg(Box<update::Msg>),
    Effect(u64, app::EffectResult),
    Lifecycle(app::Lifecycle),
    Shutdown,
}

pub(crate) struct Ctx {
    pub shell: Arc<dyn Shell>,
    pub cache: Arc<cache::Cache>,
    pub backend: Arc<Backend>,
    pub tx: mpsc::UnboundedSender<Loop>,
}

pub struct AppCore {
    runtime: Option<tokio::runtime::Runtime>,
    tx: mpsc::UnboundedSender<Loop>,
    backend: Arc<Backend>,
}

impl AppCore {
    /// Starts the core: connects the backend and renders the first view.
    pub fn new(config: AppConfig, shell: Arc<dyn Shell>) -> Result<AppCore, Error> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("muster-core")
            .build()
            .map_err(|e| Error::Unavailable(format!("runtime: {e}")))?;
        let backend = runtime.block_on(Backend::connect(&config, shell.clone()))?;
        let backend = Arc::new(backend);
        let (tx, rx) = mpsc::unbounded_channel();
        let cache = {
            let _entered = runtime.enter();
            Arc::new(cache::Cache::new(config.dir.join("view-cache-v1")))
        };
        let ctx = Ctx {
            cache,
            shell,
            backend: backend.clone(),
            tx: tx.clone(),
        };
        runtime.spawn(run(rx, ctx));
        Ok(AppCore {
            runtime: Some(runtime),
            tx,
            backend,
        })
    }

    /// Runs the core on an existing runtime (tests, or a host that has one).
    pub async fn start(config: AppConfig, shell: Arc<dyn Shell>) -> Result<AppCore, Error> {
        let backend = Arc::new(Backend::connect(&config, shell.clone()).await?);
        let (tx, rx) = mpsc::unbounded_channel();
        let ctx = Ctx {
            cache: Arc::new(cache::Cache::new(config.dir.join("view-cache-v1"))),
            shell,
            backend: backend.clone(),
            tx: tx.clone(),
        };
        tokio::spawn(run(rx, ctx));
        Ok(AppCore {
            runtime: None,
            tx,
            backend,
        })
    }

    pub fn intent(&self, intent: app::Intent) {
        let _ = self.tx.send(Loop::Intent(intent));
    }

    pub fn intent_bytes(&self, bytes: &[u8]) -> Result<(), Error> {
        let intent =
            app::Intent::decode(bytes).map_err(|e| Error::Invalid(format!("intent: {e}")))?;
        self.intent(intent);
        Ok(())
    }

    pub fn effect_result(&self, id: u64, result: app::EffectResult) {
        let _ = self.tx.send(Loop::Effect(id, result));
    }

    pub fn effect_result_bytes(&self, id: u64, bytes: &[u8]) -> Result<(), Error> {
        let result = app::EffectResult::decode(bytes)
            .map_err(|e| Error::Invalid(format!("effect result: {e}")))?;
        self.effect_result(id, result);
        Ok(())
    }

    pub fn lifecycle(&self, event: app::Lifecycle) {
        let _ = self.tx.send(Loop::Lifecycle(event));
    }

    pub fn lifecycle_raw(&self, event: u32) {
        if let Ok(event) = app::Lifecycle::try_from(event as i32) {
            self.lifecycle(event);
        }
    }

    pub fn backend(&self) -> &Arc<Backend> {
        &self.backend
    }

    pub fn shutdown(mut self) {
        let _ = self.tx.send(Loop::Shutdown);
        if let Some(runtime) = self.runtime.take() {
            let backend = self.backend.clone();
            runtime.block_on(async move { backend.shutdown().await });
            runtime.shutdown_timeout(std::time::Duration::from_secs(2));
        }
    }
}

async fn run(mut rx: mpsc::UnboundedReceiver<Loop>, ctx: Ctx) {
    let mut state = State::new(ctx.backend.is_embedded(), ctx.backend.device_id());
    update::init(&mut state, &ctx);
    let tick_tx = ctx.tx.clone();
    let ticker = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            if tick_tx
                .send(Loop::Msg(Box::new(update::Msg::CodeTick)))
                .is_err()
            {
                break;
            }
        }
    });
    let mut renderer = render::Renderer::default();
    renderer.push(&state, &ctx);
    'running: while let Some(mut item) = rx.recv().await {
        // One short display frame coalesces bursts without delaying effects or
        // losing reducer events. The deadline never extends under continuous load.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(16);
        loop {
            match item {
                Loop::Intent(intent) => update::intent(&mut state, &ctx, intent),
                Loop::Msg(msg) => update::msg(&mut state, &ctx, *msg),
                Loop::Effect(id, result) => update::effect_result(&mut state, &ctx, id, result),
                Loop::Lifecycle(event) => update::lifecycle(&mut state, &ctx, event),
                Loop::Shutdown => break 'running,
            }
            if tokio::time::Instant::now() >= deadline {
                break;
            }
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Ok(Some(next)) => item = next,
                _ => break,
            }
        }
        update::flush_chat_cache(&mut state, &ctx);
        renderer.push(&state, &ctx);
    }
    update::flush_chat_cache(&mut state, &ctx);
    state.cancel_all();
    ticker.abort();
}
