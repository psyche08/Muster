//! Request one bounded image at a time. The reverse direction carries a bounded
//! batch of pointer events, in UI order, before the next image is captured.
use super::engine::unavailable;
use crate::{Daemon, Error};
use muster_core::DeviceId;
use muster_net::remote_desktop::Session;
use muster_proto::v1::{
    DesktopPointer, DesktopSessionProgress, DesktopStreamSettings, desktop_pointer::Kind,
    desktop_session_progress::Phase, event::Ev,
};
use muster_rpc::EventSink;
use std::{sync::Mutex, time::Duration};
use tokio::{sync::mpsc, time::timeout};

const MAX_FRAME: usize = 768 * 1024;
const CHUNK: usize = 12 * 1024;
const MAX_INPUT: usize = 24;

#[derive(Default)]
pub(crate) struct PointerSlot(Mutex<Option<InputTarget>>);
struct InputTarget {
    peer: DeviceId,
    id: u64,
    keyboard: bool,
    sender: mpsc::Sender<Vec<u8>>,
}
impl PointerSlot {
    pub(super) fn attach(
        &self,
        peer: DeviceId,
        id: u64,
        keyboard: bool,
    ) -> mpsc::Receiver<Vec<u8>> {
        let (sender, receiver) = mpsc::channel(MAX_INPUT);
        *self.0.lock().unwrap() = Some(InputTarget {
            peer,
            id,
            sender,
            keyboard,
        });
        receiver
    }
    pub(super) fn send(&self, peer: DeviceId, p: DesktopPointer) -> Result<(), Error> {
        let x = u16::try_from(p.x).map_err(|_| unavailable("invalid pointer x"))?;
        let y = u16::try_from(p.y).map_err(|_| unavailable("invalid pointer y"))?;
        let kind = Kind::try_from(p.kind).map_err(|_| unavailable("invalid pointer action"))?;
        let [xh, xl] = x.to_be_bytes();
        let [yh, yl] = y.to_be_bytes();
        self.enqueue(
            peer,
            p.session,
            false,
            vec![
                kind as u8,
                xh,
                xl,
                yh,
                yl,
                p.scroll.clamp(-12, 12) as i8 as u8,
            ],
        )
    }
    pub(super) fn keyboard(
        &self,
        peer: DeviceId,
        k: muster_proto::v1::DesktopKeyboard,
    ) -> Result<(), Error> {
        use muster_proto::v1::desktop_keyboard::{Input, Key};
        let bytes = match k.input {
            Some(Input::Text(text))
                if !text.is_empty() && text.len() <= 512 && !text.chars().any(char::is_control) =>
            {
                let [high, low] = (text.len() as u16).to_be_bytes();
                let mut bytes = vec![7, high, low, 0, 0, 0];
                bytes.extend_from_slice(text.as_bytes());
                bytes
            }
            Some(Input::Key(code)) if code == Key::Backspace as i32 => vec![8, 8, 0, 0, 0, 0],
            Some(Input::Key(code)) if code == Key::Return as i32 => vec![8, 13, 0, 0, 0, 0],
            Some(Input::Key(code)) if code == Key::Escape as i32 => vec![8, 27, 0, 0, 0, 0],
            _ => return Err(unavailable("invalid keyboard input")),
        };
        self.enqueue(peer, k.session, true, bytes)
    }
    fn enqueue(
        &self,
        peer: DeviceId,
        id: u64,
        keyboard: bool,
        bytes: Vec<u8>,
    ) -> Result<(), Error> {
        let slot = self.0.lock().unwrap();
        let target = slot
            .as_ref()
            .filter(|t| t.peer == peer && t.id == id && (!keyboard || t.keyboard))
            .ok_or_else(|| unavailable("desktop input unavailable"))?;
        target
            .sender
            .try_send(bytes)
            .map_err(|_| unavailable("desktop pointer queue unavailable"))
    }
}
pub(super) async fn view(
    daemon: &Daemon,
    peer: DeviceId,
    id: u64,
    settings: DesktopStreamSettings,
    events: &EventSink,
) -> Result<(), Error> {
    let (mut session, ready) =
        super::handshake::open(daemon, peer, &settings.hello(), events).await?;
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
        *daemon.desktop_pointer.0.lock().unwrap() = Some(InputTarget {
            peer,
            id,
            sender: tx,
            keyboard: false,
        });
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
                    keyboard: false,
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
        fn muster_remote_display_open(
            edge: u32,
            quality: u32,
            cursor: bool,
        ) -> *mut std::ffi::c_void;
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
        fn open(settings: DesktopStreamSettings) -> Option<Arc<Self>> {
            let ptr = unsafe {
                muster_remote_display_open(
                    settings.long_edge,
                    settings.jpeg_quality,
                    settings.show_cursor,
                )
            };
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
    pub(crate) async fn host(
        session: &mut Session,
        settings: DesktopStreamSettings,
    ) -> Result<(), Error> {
        let Some(display) = tokio::task::spawn_blocking(move || Display::open(settings))
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
        let mut next_frame = tokio::time::Instant::now();
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
            tokio::time::sleep_until(next_frame).await;
            let started = tokio::time::Instant::now();
            let capture = display.clone();
            let frame = tokio::task::spawn_blocking(move || capture.frame())
                .await
                .map_err(|_| unavailable("desktop capture stopped"))??;
            next_frame = started + settings.frame_interval(frame.len());
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

#[cfg(test)]
mod input_tests {
    use super::*;
    use muster_proto::v1::{
        DesktopKeyboard,
        desktop_keyboard::{Input, Key},
    };

    #[test]
    fn only_current_keyboard_target_receives_ordered_unicode_and_keys() {
        let slot = PointerSlot::default();
        let peer = DeviceId([1; 32]);
        let key = |session, input| DesktopKeyboard {
            session,
            input: Some(input),
        };
        let text = || Input::Text("文🌙".into());
        let mut mouse = slot.attach(peer, 1, false);
        assert!(slot.keyboard(peer, key(1, text())).is_err());
        assert!(
            slot.keyboard(peer, key(1, Input::Key(Key::Escape as i32)))
                .is_err()
        );
        assert!(mouse.try_recv().is_err());

        let mut target = slot.attach(peer, 2, true);
        assert!(slot.keyboard(peer, key(1, text())).is_err());
        assert!(slot.keyboard(DeviceId([2; 32]), key(2, text())).is_err());
        assert!(
            slot.keyboard(peer, key(2, Input::Text("x".repeat(513))))
                .is_err()
        );
        assert!(
            slot.keyboard(peer, key(2, Input::Text("\n".into())))
                .is_err()
        );
        slot.keyboard(peer, key(2, text())).unwrap();
        let data = target.try_recv().unwrap();
        assert_eq!(&data[..6], &[7, 0, 7, 0, 0, 0]);
        assert_eq!(&data[6..], "文🌙".as_bytes());
        for (action, expected) in [(Key::Backspace, 8), (Key::Return, 13), (Key::Escape, 27)] {
            slot.keyboard(peer, key(2, Input::Key(action as i32)))
                .unwrap();
            assert_eq!(target.try_recv().unwrap(), [8, expected, 0, 0, 0, 0]);
        }
    }
}
