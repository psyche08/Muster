use crate::Error;
use muster_proto::v1::{self, desktop_session_progress::Phase, event::Ev};
use muster_rpc::EventSink;
pub(super) fn unavailable(message: impl Into<String>) -> Error {
    Error::Unavailable(message.into())
}

pub(super) async fn progress(events: &EventSink, phase: Phase) -> Result<(), Error> {
    events
        .send(Ev::DesktopSession(v1::DesktopSessionProgress {
            phase: phase as i32,
            takeover: None,
            keyboard: false,
            mouse: false,
        }))
        .await
        .map_err(|_| unavailable("desktop window closed"))
}
#[cfg(not(target_os = "ios"))]
mod process {
    use super::*;
    use muster_net::remote_desktop::gamestream::LocalEngine;
    use std::{
        net::SocketAddrV4,
        path::PathBuf,
        process::Stdio,
        sync::{Arc, Mutex},
        time::Duration,
    };
    use tokio::{
        process::{Child, Command},
        time::sleep,
    };

    pub(crate) fn program(name: &str) -> Result<PathBuf, Error> {
        let daemon =
            std::env::current_exe().map_err(|_| unavailable("cannot locate desktop package"))?;
        let parent = daemon
            .parent()
            .ok_or_else(|| unavailable("missing desktop package directory"))?;
        let executable = parent.join(name);
        if !executable.is_file() {
            return Err(unavailable(
                "this package does not contain the remote desktop engine",
            ));
        }
        Ok(executable)
    }

    // Retain the child until the broker drops all references. Check liveness on
    // both sides of the kernel socket lookup, including after an OS PID reuse.
    pub(crate) struct Engine(Mutex<Child>);
    impl Engine {
        pub fn launch(
            command: &mut Command,
        ) -> Result<
            (
                Self,
                tokio::process::ChildStdin,
                Option<tokio::process::ChildStdout>,
            ),
            Error,
        > {
            let mut child = command
                .kill_on_drop(true)
                .stdin(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|_| unavailable("cannot start the packaged desktop engine"))?;
            let input = child
                .stdin
                .take()
                .ok_or_else(|| unavailable("missing engine control pipe"))?;
            let output = child.stdout.take();
            Ok((Self(Mutex::new(child)), input, output))
        }
        #[cfg(windows)]
        pub(crate) fn capture_job(&self) -> std::io::Result<std::os::windows::io::OwnedHandle> {
            let child = self
                .0
                .lock()
                .map_err(|_| std::io::Error::other("engine lock poisoned"))?;
            let handle = child
                .raw_handle()
                .ok_or_else(|| std::io::Error::other("engine has exited"))?;
            // The child is waiting for bootstrap on stdin. Hold the original
            // process handle until assignment completes.
            super::super::console_worker::capture_job(unsafe {
                std::os::windows::io::BorrowedHandle::borrow_raw(handle)
            })
        }
        pub(crate) fn pid(&self) -> Option<u32> {
            let mut child = self.0.lock().ok()?;
            if child.try_wait().ok()?.is_some() {
                return None;
            }
            child.id()
        }
        fn owns(&self, addr: SocketAddrV4, tcp: bool) -> bool {
            self.pid().is_some_and(|pid| {
                muster_platform::engine_socket::owned(pid, addr, tcp) && self.pid() == Some(pid)
            })
        }
        pub async fn exited(self: &Arc<Self>) -> Error {
            while self.pid().is_some() {
                sleep(Duration::from_millis(100)).await;
            }
            unavailable("desktop engine exited")
        }
    }
    impl LocalEngine for Engine {
        fn owns_tcp(&self, addr: SocketAddrV4) -> bool {
            self.owns(addr, true)
        }
        fn owns_udp(&self, addr: SocketAddrV4) -> bool {
            self.owns(addr, false)
        }
    }
}
#[cfg(not(target_os = "ios"))]
pub(super) use process::*;
pub(super) const BUSY: &[u8; 4] = b"MDB1";
pub(super) const REPLACE: &[u8] = b"confirm-replace";
pub(super) const CHANGED: &[u8] = b"desktop-occupant-changed";
#[cfg(not(target_os = "ios"))]
pub(super) const HELLO: &[u8; 4] = b"MDV1";
#[cfg(any(windows, target_os = "macos"))]
pub(super) const FIXED_KEYBOARD_HELLO: &[u8; 4] = b"MDK1";
pub(super) const CONSOLE_UNAVAILABLE: &[u8] = b"ordinary-console-unavailable";
pub(super) const CONSOLE_MESSAGE: &str =
    "The paired Windows account needs an unlocked physical desktop and its stream worker.";
pub(super) const CAPTURE_UNAVAILABLE: &[u8] = b"capture-unavailable";
// The host fixes the cipher, profile and ports. Only the one-time input key,
// IV prefix, and the granted input mode need to cross PeerLink.
pub(super) struct Launch {
    pub mouse: bool,
    pub keyboard: bool,
    pub key: [u8; 20],
}
impl Launch {
    pub fn input_mode(&self) -> u8 {
        if self.keyboard { 2 } else { self.mouse as u8 }
    }
    #[cfg(any(target_os = "macos", windows, test))]
    pub fn encode(&self) -> [u8; 25] {
        let mut bytes = [0; 25];
        bytes[..4].copy_from_slice(b"MDS1");
        bytes[4] = self.input_mode();
        bytes[5..].copy_from_slice(&self.key);
        bytes
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes == CAPTURE_UNAVAILABLE {
            return Err(unavailable(
                "host could not start capture; check the host display, encoder and desktop sharing permissions for Muster",
            ));
        }
        let [b'M', b'D', b'S', b'1', mode @ 0..=2, key @ ..] = bytes else {
            return Err(unavailable("invalid desktop launch reply"));
        };
        Ok(Self {
            mouse: *mode != 0,
            keyboard: *mode == 2,
            key: key
                .try_into()
                .map_err(|_| unavailable("invalid desktop launch key"))?,
        })
    }
}

#[cfg(all(test, not(target_os = "ios")))]
mod tests {
    use super::*;
    #[test]
    fn launch_accepts_only_the_selected_mode_and_exact_key_length() {
        for mouse in [true, false] {
            let original = Launch {
                keyboard: false,
                mouse,
                key: [7; 20],
            };
            let mut wire = original.encode();
            let decoded = Launch::parse(&wire).unwrap();
            assert_eq!(decoded.key, original.key);
            assert_eq!(decoded.mouse, mouse);
            assert!(Launch::parse(&wire[..24]).is_err());
            wire[4] = 3;
            assert!(Launch::parse(&wire).is_err());
        }
    }
}
