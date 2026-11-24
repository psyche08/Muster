use super::*;
use crate::state::DesktopSession;
use muster_proto::v1::{
    desktop_session_progress::Phase, desktop_session_request::Action, event::Ev,
};

pub(super) fn start(state: &mut State, ctx: &Ctx, mut request: v1::DesktopSessionRequest) {
    if let Some(Action::Pointer(ref p)) = request.action {
        if let Some(s) = state.desktop_session.as_ref().filter(|s| {
            s.mouse
                && s.device == request.device
                && s.id == p.session
                && (state.screen == Screen::Remote
                    || p.kind == v1::desktop_pointer::Kind::DragEnd as i32)
        }) && let Some(queue) = &s.pointer
            && queue.try_send(p.clone()).is_err()
        {
            state.desktop_session = None;
            state.toast("Desktop input queue full. Reconnect to continue.", true);
        }
        return;
    }
    let remote = (state.screen == Screen::Remote).then(|| crate::render::remote_workspace(state));
    if !(remote
        .as_ref()
        .is_some_and(|v| !v.selected_device.is_empty() && v.selected_device == request.device)
        || state.screen == Screen::Device && request.device == state.target())
    {
        return;
    }
    if let Some(Action::ConfirmTakeover(token)) = request.action {
        if let Some(s) = state.desktop_session.as_mut()
            && s.device == request.device
            && s.takeover.as_ref().is_some_and(|p| p.confirmation == token)
        {
            s.takeover = None;
            super::network::admin_request(
                state,
                ctx,
                "desktop takeover",
                state.self_id.clone(),
                Req::DesktopSession(request),
            );
        }
        return;
    }
    if state.desktop_session.is_some() {
        return;
    }
    let actions = crate::render::device_actions(state);
    if matches!(request.action, Some(Action::RememberAccess(_))) {
        if actions.can_share_desktop {
            super::network::admin_request(
                state,
                ctx,
                "desktop permission",
                state.self_id.clone(),
                Req::DesktopSession(request),
            );
        }
        return;
    }
    let allowed = match request.action {
        Some(Action::ShareView(_) | Action::ShareMouse(_)) => actions.can_share_desktop,
        Some(Action::ViewNative(_)) => {
            cfg!(target_vendor = "apple") && remote.as_ref().is_some_and(|v| v.can_connect)
        }
        Some(Action::ViewInWindow(hwnd)) => {
            remote
                .as_ref()
                .map_or(actions.can_view_desktop, |v| v.can_connect)
                && hwnd != 0
        }
        _ => false,
    };
    if !allowed {
        return;
    }
    let id = crate::state::next_content_revision();
    let (watch, token) = state.new_watch();
    let native = matches!(request.action, Some(Action::ViewNative(_)));
    if native {
        let settings = state.desktop_preferences.settings;
        request.action = Some(if settings == v1::DesktopStreamSettings::ORIGINAL {
            Action::ViewNative(id)
        } else {
            Action::ViewConfigured(v1::ConfiguredDesktopViewer {
                session: id,
                settings: Some(settings),
            })
        });
    }
    let (pointer, mut input) = tokio::sync::mpsc::channel(32);
    let device = request.device.clone();
    state.desktop_session = Some(DesktopSession {
        pointer: native.then_some(pointer),
        mouse: false,
        jpeg: Vec::new(),
        opened_from: state.screen,
        device: request.device.clone(),
        id,
        status: "Opening desktop session…".into(),
        takeover: None,
        _watch: watch,
    });
    let client = ctx.backend.local();
    let tx = ctx.tx.clone();
    spawn(ctx, async move {
        // A delayed reducer needs only the latest frame. Keep at most one
        // pending notification for this lease, including while another tab is open.
        let latest = std::sync::Arc::new(std::sync::Mutex::new(None));
        let session = async {
            let mut call = client.start(Req::DesktopSession(request)).await?;
            while let Some(event) = call.events.recv().await {
                if let Ev::DesktopFrame(frame) = event {
                    if !native || frame.len() > 768 * 1024 {
                        return Err(unexpected());
                    }
                    let notify = latest.lock().unwrap().replace(frame).is_none();
                    if notify
                        && tx
                            .send(loop_message(Msg::DesktopFrame(id, latest.clone())))
                            .is_err()
                    {
                        return Ok(());
                    }
                    continue;
                }
                let Ev::DesktopSession(event) = event else {
                    return Err(unexpected());
                };
                Phase::try_from(event.phase).map_err(|_| unexpected())?;
                if tx
                    .send(loop_message(Msg::DesktopProgress(id, event)))
                    .is_err()
                {
                    return Ok(());
                }
            }
            match call.finish().await? {
                Resp::Ack(_) => Ok(()),
                _ => Err(unexpected()),
            }
        };
        let pointer_calls = async {
            while let Some(p) = input.recv().await {
                client
                    .call(Req::DesktopSession(v1::DesktopSessionRequest {
                        device: device.clone(),
                        action: Some(Action::Pointer(p)),
                    }))
                    .await?;
            }
            std::future::pending::<Result<(), Error>>().await
        };
        let result = tokio::select! {biased; _=token.cancelled()=>Ok(()), r=session=>r,r=pointer_calls,if native=>r};
        Msg::DesktopFinished(id, result)
    });
}
pub(super) fn progress(state: &mut State, id: u64, event: v1::DesktopSessionProgress) {
    let Ok(phase) = Phase::try_from(event.phase) else {
        return;
    };
    if let Some(s) = state.desktop_session.as_mut().filter(|s| s.id == id) {
        s.mouse = event.mouse && phase == Phase::Visible;
        s.takeover = if phase == Phase::Occupied {
            event.takeover
        } else {
            None
        };
        s.status = match phase {
            Phase::Occupied => "Desktop in use",
            Phase::Offered => "Shared with this device · waiting for viewer",
            Phase::Starting => "Connecting desktop…",
            Phase::Visible => "Viewing desktop",
        }
        .into();
    }
}
pub(super) fn finish(state: &mut State, id: u64, result: Result<(), Error>) {
    if !state.desktop_session.as_ref().is_some_and(|s| s.id == id) {
        return;
    }
    state.desktop_session = None;
    match result {
        Ok(()) => state.toast("Desktop session ended", false),
        Err(e) => state.toast(e.to_string(), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_workspace_lists_paired_desktops_without_connecting_on_selection() {
        let mut state = State::new(false, None);
        let member = |id: &str, kind: &str, local| v1::MemberView {
            device_id: id.into(),
            kind: kind.into(),
            is_self: local,
            connected: true,
            ..Default::default()
        };
        state.network = Some(v1::NetworkStatus {
            members: vec![
                member("here", "desktop", true),
                member("peer", "desktop", false),
                member("phone", "phone", false),
            ],
            ..Default::default()
        });
        state.remote_selected = "peer".into();
        state.go(Screen::Remote);
        let view = crate::render::remote_workspace(&state);
        assert_eq!(view.devices.len(), 1);
        assert_eq!(view.selected_device, "peer");
        assert!(!view.session_open);
        assert_eq!(state.workspace(), Screen::Remote);
        assert!(!state.can_go_back());
    }

    #[test]
    fn desktop_session_prompt_tracks_the_live_request_and_clears_when_connecting() {
        let mut state = State::new(false, None);
        let (watch, _) = state.new_watch();
        state.desktop_session = Some(DesktopSession {
            pointer: None,
            mouse: false,
            jpeg: Vec::new(),
            opened_from: Screen::Remote,
            device: "host".into(),
            id: 3,
            status: String::new(),
            takeover: None,
            _watch: watch,
        });
        let occupied = v1::DesktopSessionProgress {
            mouse: false,
            phase: Phase::Occupied as i32,
            takeover: Some(v1::DesktopTakeoverPrompt {
                confirmation: 9,
                controller: "Test viewer".into(),
            }),
        };
        progress(&mut state, 2, occupied.clone());
        assert!(state.desktop_session.as_ref().unwrap().takeover.is_none());
        progress(&mut state, 3, occupied.clone());
        assert_eq!(
            state.desktop_session.as_ref().unwrap().takeover,
            occupied.takeover
        );
        progress(
            &mut state,
            3,
            v1::DesktopSessionProgress {
                mouse: false,
                phase: Phase::Starting as i32,
                takeover: None,
            },
        );
        assert!(state.desktop_session.as_ref().unwrap().takeover.is_none());
    }
}
