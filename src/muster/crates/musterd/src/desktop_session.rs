//! A local streaming RPC is the lifetime of the local user's explicit choice.
#[cfg(any(windows, target_vendor = "apple", test))]
pub(super) mod confirmation;
#[cfg(any(windows, target_os = "macos", test))]
pub(super) mod occupancy;
pub(super) mod permissions;
use crate::{Daemon, Error};
#[cfg(any(windows, target_os = "macos"))]
pub(crate) const FIXED_KEYBOARD_SERVICE: &str = "desktop.fixed-keyboard.v1";
use muster_core::DeviceId;
use muster_proto::v1::{self, desktop_session_request::Action};
use muster_rpc::EventSink;
use std::sync::Arc;
#[cfg(any(windows, target_vendor = "apple"))]
mod engine;
#[cfg(any(windows, target_os = "macos"))]
pub(super) mod host;
#[cfg(any(target_os = "macos", test))]
mod idle;
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
    if matches!(
        action,
        Action::RememberAccess(_) | Action::ShareView(_) | Action::ShareMouse(_)
    ) {
        return Err(Error::Invalid(
            "Manage incoming desktop access in Settings".into(),
        ));
    }
    #[cfg(any(windows, target_vendor = "apple"))]
    if let Action::ConfirmTakeover(id) = action {
        return daemon.desktop_confirmation.confirm(peer, id);
    }
    #[cfg(target_vendor = "apple")]
    if let Action::Pointer(p) = action {
        return daemon.desktop_pointer.send(peer, p);
    }
    #[cfg(target_vendor = "apple")]
    if let Action::Keyboard(k) = action {
        return daemon.desktop_pointer.keyboard(peer, k);
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
                #[cfg(target_vendor="apple")]
                Action::ViewVideo(view) if view.session != 0 => {
                    let settings = view.settings.filter(|s| s.valid())
                        .ok_or_else(|| Error::Invalid("invalid video settings".into()))?;
                    apple_viewer::view(daemon, peer, view.session, settings, &events).await
                }
                #[cfg(target_vendor="apple")]
                Action::ViewNative(id) if id != 0 => native::view(daemon, peer, id, v1::DesktopStreamSettings::ORIGINAL, &events).await,
                #[cfg(target_vendor="apple")]
                Action::ViewConfigured(view) if view.session != 0 => {
                    let settings = view.settings.filter(|s| s.valid())
                        .ok_or_else(|| Error::Invalid("unsupported desktop stream settings".into()))?;
                    native::view(daemon, peer, view.session, settings, &events).await
                }
                #[cfg(windows)]
                Action::ViewInWindow(hwnd) if hwnd != 0 => viewer::view(daemon, peer, hwnd, &events).await,
                _ => Err(Error::Unavailable("this desktop action is unavailable on this platform".into())),
            }
        } => result,
    }
}

pub(super) fn available() -> (bool, bool) {
    #[cfg(target_os = "macos")]
    return (
        engine::program(host::CAPTURE_HOST).is_ok(),
        engine::program("muster-moonlight").is_ok(),
    );
    #[cfg(windows)]
    return (
        engine::program(host::CAPTURE_HOST).is_ok(),
        engine::program(viewer::MOONLIGHT).is_ok(),
    );
    #[cfg(target_os = "ios")]
    return (false, true);
    #[cfg(not(any(windows, target_vendor = "apple")))]
    (false, false)
}

#[cfg(target_vendor = "apple")]
mod apple_viewer;
#[cfg(any(windows, target_vendor = "apple"))]
mod handshake;
#[cfg(target_vendor = "apple")]
pub(crate) mod native;
