//! Request one bounded image at a time. The reverse direction carries a bounded
//! batch of pointer events, in UI order, before the next image is captured.
use super::engine::unavailable;
use crate::{Daemon, Error};
use muster_core::DeviceId;
use muster_net::remote_desktop::Session;
use muster_proto::v1::{
    DesktopPointer, DesktopSessionProgress, desktop_pointer::Kind, desktop_session_progress::Phase,
    event::Ev,
};
use muster_rpc::EventSink;
use std::{sync::Mutex, time::Duration};
use tokio::{sync::mpsc, time::timeout};

pub(super) const HELLO: &[u8] = b"MDJ1";
const MAX_FRAME: usize = 768 * 1024;
const CHUNK: usize = 12 * 1024;
const MAX_INPUT: usize = 24;

#[derive(Default)]
pub(crate) struct PointerSlot(Mutex<Option<(DeviceId, u64, mpsc::Sender<[u8; 6]>)>>);
impl PointerSlot {
    pub(super) fn send(&self, peer: DeviceId, p: DesktopPointer) -> Result<(), Error> {
        let x = u16::try_from(p.x).map_err(|_| unavailable("invalid pointer x"))?;
        let y = u16::try_from(p.y).map_err(|_| unavailable("invalid pointer y"))?;
        let kind = Kind::try_from(p.kind).map_err(|_| unavailable("invalid pointer action"))?;
        let [xh, xl] = x.to_be_bytes();
        let [yh, yl] = y.to_be_bytes();
        let slot = self.0.lock().unwrap();
        let (_, _, sender) = slot
            .as_ref()
            .filter(|(d, id, _)| *d == peer && *id == p.session)
            .ok_or_else(|| unavailable("pointer session ended"))?;
        sender
            .try_send([
                kind as u8,
                xh,
                xl,
                yh,
                yl,
                p.scroll.clamp(-12, 12) as i8 as u8,
            ])
            .map_err(|_| unavailable("desktop pointer queue unavailable"))
    }
}
pub(super) async fn view(
    daemon: &Daemon,
    peer: DeviceId,
    id: u64,
    events: &EventSink,
) -> Result<(), Error> {
    let (mut session, ready) = super::handshake::open(daemon, peer, HELLO, events).await?;
    let mouse = match ready.as_slice() {
        b"JPG0" => false,
        b"JPG1" => true,
        _ => {
            return Err(unavailable(
                "Mac host could not start native capture. Update Muster and check Screen Recording permission.",
            ));
        }
    };
    let (tx, mut input) = mpsc::channel(MAX_INPUT);
    if mouse {
        *daemon.desktop_pointer.0.lock().unwrap() = Some((peer, id, tx));
    }
    let mut first = true;
    loop {
        let mut request = vec![b'N'];
        while request.len() < 1 + 6 * MAX_INPUT {
            let Ok(packet) = input.try_recv() else { break };
            request.extend(packet);
        }
        session.send_control(&request).await?;
        let frame = timeout(Duration::from_secs(12), read_frame(&mut session))
            .await
            .map_err(|_| unavailable("desktop frame timed out"))??;
        events
            .send(Ev::DesktopFrame(frame))
            .await
            .map_err(|_| unavailable("viewer closed"))?;
        if first {
            events
                .send(Ev::DesktopSession(DesktopSessionProgress {
                    phase: Phase::Visible as i32,
                    mouse,
                    takeover: None,
                }))
                .await
                .map_err(|_| unavailable("viewer closed"))?;
            first = false;
        }
    }
}

async fn read_frame(session: &mut Session) -> Result<Vec<u8>, Error> {
    let header = session
        .recv_control()
        .await?
        .ok_or_else(|| unavailable("desktop host closed"))?;
    let length = u32::from_be_bytes(
        header
            .try_into()
            .map_err(|_| unavailable("invalid desktop frame header"))?,
    ) as usize;
    if !(4..=MAX_FRAME).contains(&length) {
        return Err(unavailable("invalid desktop frame size"));
    }
    let mut frame = Vec::with_capacity(length);
    while frame.len() < length {
        let chunk = session
            .recv_control()
            .await?
            .ok_or_else(|| unavailable("incomplete desktop frame"))?;
        if chunk.len() != (length - frame.len()).min(CHUNK) {
            return Err(unavailable("invalid desktop frame chunk"));
        }
        frame.extend(chunk);
    }
    Ok(frame)
}

#[cfg(target_os = "macos")]
mod capture {
    use super::*;
    use std::sync::Arc;
    unsafe extern "C" {
        fn muster_remote_display_open() -> *mut std::ffi::c_void;
        fn muster_remote_frame(handle: *mut std::ffi::c_void, out: *mut u8, cap: usize) -> usize;
        fn muster_remote_close(handle: *mut std::ffi::c_void);
        fn muster_remote_pointer(
            handle: *mut std::ffi::c_void,
            kind: i32,
            x: u16,
            y: u16,
            scroll: i32,
        ) -> bool;
    }
    struct Display(*mut std::ffi::c_void);
    // Immutable retained capture configuration, used on one blocking worker at a time.
    unsafe impl Send for Display {}
    unsafe impl Sync for Display {}
    impl Drop for Display {
        fn drop(&mut self) {
            unsafe { muster_remote_close(self.0) }
        }
    }
    impl Display {
        fn open() -> Option<Arc<Self>> {
            let ptr = unsafe { muster_remote_display_open() };
            (!ptr.is_null()).then(|| Arc::new(Self(ptr)))
        }
        fn frame(&self) -> Result<Vec<u8>, Error> {
            let mut frame = vec![0; MAX_FRAME];
            let n = unsafe { muster_remote_frame(self.0, frame.as_mut_ptr(), frame.len()) };
            if n == 0 || n > frame.len() {
                return Err(unavailable("Mac desktop capture failed"));
            }
            frame.truncate(n);
            Ok(frame)
        }
        fn pointer(&self, kind: Kind, x: u16, y: u16, scroll: i32) -> bool {
            unsafe { muster_remote_pointer(self.0, kind as i32, x, y, scroll) }
        }
    }
    struct Pointer {
        display: Arc<Display>,
        held: Option<(u16, u16)>,
    }
    impl Drop for Pointer {
        fn drop(&mut self) {
            if let Some((x, y)) = self.held.take() {
                self.display.pointer(Kind::DragEnd, x, y, 0);
            }
        }
    }
    impl Pointer {
        fn apply(&mut self, packet: &[u8]) -> Result<(), Error> {
            let kind = Kind::try_from(i32::from(packet[0]))
                .map_err(|_| unavailable("invalid pointer action"))?;
            let x = u16::from_be_bytes([packet[1], packet[2]]);
            let y = u16::from_be_bytes([packet[3], packet[4]]);
            if self.held.is_some() != matches!(kind, Kind::DragMove | Kind::DragEnd) {
                return Err(unavailable("invalid drag sequence"));
            }
            if !self
                .display
                .pointer(kind, x, y, i32::from(packet[5] as i8).clamp(-12, 12))
            {
                return Err(unavailable("Mac mouse permission unavailable"));
            }
            self.held = matches!(kind, Kind::DragStart | Kind::DragMove).then_some((x, y));
            Ok(())
        }
    }
    pub(crate) async fn host(session: &mut Session) -> Result<(), Error> {
        let Some(display) = tokio::task::spawn_blocking(Display::open)
            .await
            .ok()
            .flatten()
        else {
            session
                .send_control(super::super::engine::CAPTURE_UNAVAILABLE)
                .await?;
            let _ = timeout(Duration::from_secs(1), session.recv_control()).await;
            return Err(unavailable("Mac screen capture unavailable"));
        };
        let mouse = session.with_mouse(|| ()).is_ok();
        session
            .send_control(if mouse { b"JPG1" } else { b"JPG0" })
            .await?;
        let mut pointer = Pointer {
            display: display.clone(),
            held: None,
        };
        let mut ticks = tokio::time::interval(Duration::from_millis(100));
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        while let Some(request) = session.recv_control().await? {
            let Some((&b'N', items)) = request.split_first() else {
                return Err(unavailable("invalid frame request"));
            };
            if items.len() > 6 * MAX_INPUT || items.len() % 6 != 0 {
                return Err(unavailable("invalid pointer batch"));
            }
            for packet in items.chunks_exact(6) {
                session.with_mouse(|| pointer.apply(packet))??;
            }
            ticks.tick().await;
            let capture = display.clone();
            let frame = tokio::task::spawn_blocking(move || capture.frame())
                .await
                .map_err(|_| unavailable("desktop capture stopped"))??;
            session
                .send_control(&(frame.len() as u32).to_be_bytes())
                .await?;
            for chunk in frame.chunks(CHUNK) {
                session.send_control(chunk).await?;
            }
        }
        Ok(())
    }
}
#[cfg(target_os = "macos")]
pub(super) use capture::host;
