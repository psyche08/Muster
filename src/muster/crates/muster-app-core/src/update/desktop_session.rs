use super::*;
use crate::state::DesktopSession;
use muster_proto::v1::{
    desktop_session_progress::Phase, desktop_session_request::Action, event::Ev,
};

pub(super) fn start(state: &mut State, ctx: &Ctx, request: v1::DesktopSessionRequest) {
    if state.screen != Screen::Device
        || request.device != state.target()
        || state.desktop_session.is_some()
    {
        return;
    }
    let actions = crate::render::device_actions(state);
    let allowed = match request.action {
        Some(Action::ShareView(_) | Action::ShareMouse(_)) => actions.can_share_desktop,
        Some(Action::ViewInWindow(hwnd)) => actions.can_view_desktop && hwnd != 0,
        None => false,
    };
    if !allowed {
        return;
    }
    let id = crate::state::next_content_revision();
    let (watch, token) = state.new_watch();
    state.desktop_session = Some(DesktopSession {
        device: request.device.clone(),
        id,
        status: "Opening desktop session…".into(),
        _watch: watch,
    });
    let client = ctx.backend.local();
    let tx = ctx.tx.clone();
    spawn(ctx, async move {
        let session = async {
            let mut call = client.start(Req::DesktopSession(request)).await?;
            while let Some(event) = call.events.recv().await {
                let Ev::DesktopSession(event) = event else {
                    return Err(unexpected());
                };
                let phase = Phase::try_from(event.phase).map_err(|_| unexpected())?;
                if tx
                    .send(loop_message(Msg::DesktopProgress(id, phase)))
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
        let result = tokio::select! {biased; _=token.cancelled()=>Ok(()), r=session=>r};
        Msg::DesktopFinished(id, result)
    });
}
pub(super) fn progress(state: &mut State, id: u64, phase: Phase) {
    if let Some(s) = state.desktop_session.as_mut().filter(|s| s.id == id) {
        s.status = match phase {
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
