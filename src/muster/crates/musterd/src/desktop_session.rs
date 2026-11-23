//! A local streaming RPC is the lifetime of the local user's explicit choice.
pub(super) mod permissions;
use crate::{Daemon, Error};
use muster_core::DeviceId;
use muster_proto::v1::{self, desktop_session_request::Action};
use muster_rpc::EventSink;
use std::sync::Arc;
#[cfg(any(windows, target_os = "macos"))]
mod engine;
#[cfg(target_os = "macos")]
pub(super) mod host;
#[cfg(windows)]
mod viewer;

pub(super) async fn run(
    daemon: &Arc<Daemon>,
    request: v1::DesktopSessionRequest,
    events: EventSink,
) -> Result<(), Error> {
    let peer = DeviceId::from_hex(&request.device)
        .map_err(|_| Error::Invalid("invalid desktop device".into()))?;
    let action = request
        .action
        .ok_or_else(|| Error::Invalid("missing desktop action".into()))?;
    if let Action::RememberAccess(raw) = action {
        let access = v1::DesktopAccess::try_from(raw)
            .map_err(|_| Error::Invalid("unknown desktop permission".into()))?;
        if access != v1::DesktopAccess::None && !available().0 {
            return Err(Error::Unavailable("desktop host is unavailable".into()));
        }
        #[cfg(target_os = "macos")]
        if access != v1::DesktopAccess::None && !host::console_ready(daemon) {
            return Err(Error::Unauthorized(
                "unlock and uncover the local desktop to grant access".into(),
            ));
        }
        return daemon.desktop_permissions.set(daemon, peer, access);
    }
    let _slot = daemon
        .desktop_slot
        .try_acquire()
        .map_err(|_| Error::Busy("a local desktop session is already open".into()))?;
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
    return (engine::program(host::CAPTURE_HOST).is_ok(), false);
    #[cfg(windows)]
    return (false, engine::program(viewer::MOONLIGHT).is_ok());
    #[cfg(not(any(windows, target_os = "macos")))]
    (false, false)
}
