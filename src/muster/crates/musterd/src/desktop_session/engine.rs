use crate::Error;
use muster_net::remote_desktop::gamestream::LocalEngine;
use muster_proto::v1::{self, desktop_session_progress::Phase, event::Ev};
use muster_rpc::EventSink;
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

pub(super) fn unavailable(message: impl Into<String>) -> Error {
    Error::Unavailable(message.into())
}

pub(super) async fn progress(events: &EventSink, phase: Phase) -> Result<(), Error> {
    events
        .send(Ev::DesktopSession(v1::DesktopSessionProgress {
            phase: phase as i32,
        }))
        .await
        .map_err(|_| unavailable("desktop window closed"))
}
pub(super) fn program(name: &str) -> Result<PathBuf, Error> {
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
pub(super) struct Engine(Mutex<Child>);
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
    fn pid(&self) -> Option<u32> {
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

pub(super) const HELLO: &[u8; 4] = b"MDV1";
pub(super) const CAPTURE_UNAVAILABLE: &[u8] = b"capture-unavailable";
// The host fixes the cipher, profile and ports. Only the one-time input key,
// IV prefix, and the granted input mode need to cross PeerLink.
pub(super) struct Launch {
    pub mouse: bool,
    pub key: [u8; 20],
}
impl Launch {
    #[cfg(any(target_os = "macos", test))]
    pub fn encode(&self) -> [u8; 25] {
        let mut bytes = [0; 25];
        bytes[..4].copy_from_slice(b"MDS1");
        bytes[4] = self.mouse as u8;
        bytes[5..].copy_from_slice(&self.key);
        bytes
    }
    #[cfg(any(windows, test))]
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes == CAPTURE_UNAVAILABLE {
            return Err(unavailable(
                "host could not start capture; check the Mac display, encoder and desktop sharing permissions for Muster",
            ));
        }
        let [b'M', b'D', b'S', b'1', mode @ 0..=1, key @ ..] = bytes else {
            return Err(unavailable("invalid desktop launch reply"));
        };
        Ok(Self {
            mouse: *mode == 1,
            key: key
                .try_into()
                .map_err(|_| unavailable("invalid desktop launch key"))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launch_accepts_only_the_selected_mode_and_exact_key_length() {
        for mouse in [true, false] {
            let original = Launch {
                mouse,
                key: [7; 20],
            };
            let mut wire = original.encode();
            let decoded = Launch::parse(&wire).unwrap();
            assert_eq!(decoded.key, original.key);
            assert_eq!(decoded.mouse, mouse);
            assert!(Launch::parse(&wire[..24]).is_err());
            wire[4] = 2;
            assert!(Launch::parse(&wire).is_err());
        }
    }
}
