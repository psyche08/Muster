//! A local streaming RPC is the lifetime of the local user's explicit choice.
use crate::{Daemon, Error};
use muster_core::DeviceId;
use muster_proto::v1::{self, desktop_session_request::Action};
use muster_rpc::EventSink;
use std::sync::Arc;
#[cfg(any(windows, target_os = "macos"))]
mod engine;
#[cfg(target_os = "macos")]
mod host;
#[cfg(windows)]
mod viewer;

pub(super) async fn run(
    daemon: &Arc<Daemon>,
    request: v1::DesktopSessionRequest,
    events: EventSink,
) -> Result<(), Error> {
    let _slot = daemon
        .desktop_slot
        .try_acquire()
        .map_err(|_| Error::Busy("a local desktop session is already open".into()))?;
    let peer = DeviceId::from_hex(&request.device)
        .map_err(|_| Error::Invalid("invalid desktop device".into()))?;
    let action = request
        .action
        .ok_or_else(|| Error::Invalid("missing desktop action".into()))?;
    tokio::select! {
        biased;
        _ = events.cancelled().cancelled() => Ok(()),
        result = async {
            match action {
                #[cfg(target_os="macos")]
                Action::ShareView(_) | Action::ShareMouse(_) => host::offer(daemon, peer, matches!(action, Action::ShareMouse(_)), &events).await,
                #[cfg(windows)]
                Action::ViewInWindow(hwnd) if hwnd != 0 => viewer::view(daemon, peer, hwnd, &events).await,
                _ => Err(Error::Unavailable("this desktop action is unavailable on this platform".into())),
            }
        } => result,
    }
}

pub(super) fn available() -> (bool, bool) {
    #[cfg(target_os = "macos")]
    return (engine::program(host::SUNSHINE).is_ok(), false);
    #[cfg(windows)]
    return (false, engine::program(viewer::MOONLIGHT).is_ok());
    #[cfg(not(any(windows, target_os = "macos")))]
    (false, false)
}
