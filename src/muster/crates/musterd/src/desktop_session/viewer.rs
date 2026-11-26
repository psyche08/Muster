use super::engine::*;
use crate::{Daemon, Error};
use muster_core::DeviceId;
use muster_net::remote_desktop::gamestream::{self, EnginePorts, ViewerPorts};
use muster_proto::v1::{DesktopSessionProgress, desktop_session_progress::Phase, event::Ev};
use muster_rpc::EventSink;
use std::{process::Stdio, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
    time::timeout,
};
pub(super) const MOONLIGHT: &str = "muster-moonlight.exe";

/// Capabilities select a wire format. Only the receiver's live launch reply grants input.
async fn fixed_hello(daemon: &Daemon, peer: DeviceId) -> &'static [u8; 4] {
    use muster_proto::v1::{request::Req, response::Resp};
    let discover = async {
        let (send, recv) = daemon
            .node()
            .open(peer, crate::SERVICE, crate::SERVICE_VERSION)
            .await
            .ok()?;
        let client = muster_rpc::Client::new(tokio::io::join(recv, send));
        let Resp::DeviceInfo(info) = client
            .call(Req::DeviceInfo(Default::default()))
            .await
            .ok()?
        else {
            return None;
        };
        info.services
            .iter()
            .any(|s| s == super::FIXED_KEYBOARD_SERVICE)
            .then_some(())
    };
    if matches!(
        timeout(Duration::from_secs(4), discover).await,
        Ok(Some(()))
    ) {
        FIXED_KEYBOARD_HELLO
    } else {
        HELLO
    }
}

fn negotiated_mode(hello: &[u8], launch: &Launch) -> u8 {
    if hello == FIXED_KEYBOARD_HELLO {
        launch.input_mode()
    } else {
        u8::from(launch.mouse)
    }
}

pub(super) async fn view(
    daemon: &Arc<Daemon>,
    peer: DeviceId,
    hwnd: u64,
    events: &EventSink,
) -> Result<(), Error> {
    let executable = program(MOONLIGHT)?;
    let ports = EnginePorts::MUSTER;
    let hello = fixed_hello(daemon, peer).await;
    let (listeners, rtsp_port) = ViewerPorts::bind_windows(ports).await?;
    let (mut session, reply) = super::handshake::open(daemon, peer, hello, events).await?;
    let launch = Launch::parse(&reply)?;
    let (child, mut input, output) =
        Engine::launch(Command::new(executable).stdout(Stdio::piped()))?;
    let engine = Arc::new(child);
    let output = output.ok_or_else(|| unavailable("missing viewer status pipe"))?;
    let mut init = Vec::from(*b"MVW2");
    let mode = negotiated_mode(hello, &launch);
    init.extend([mode, 1]);
    init.extend(rtsp_port.to_be_bytes());
    init.extend(hwnd.to_be_bytes());
    init.extend(launch.key);
    input
        .write_all(&init)
        .await
        .map_err(|_| unavailable("viewer refused initialization"))?;
    let status = async {
        let mut output = BufReader::new(output);
        timeout(Duration::from_secs(35), read_status(&mut output))
            .await
            .map_err(|_| unavailable("timed out waiting for a desktop frame"))??;
        events
            .send(Ev::DesktopSession(DesktopSessionProgress {
                phase: Phase::Visible as i32,
                keyboard: mode == 2,
                mouse: mode != 0,
                takeover: None,
            }))
            .await
            .map_err(|_| unavailable("desktop window closed"))?;
        read_status(&mut output).await?;
        Err(unavailable("duplicate viewer frame notification"))
    };
    tokio::select! {
        biased;
        result=status=>result,
        result=gamestream::viewer(&mut session,listeners,engine.clone())=>Ok(result?),
    }
}

/// A child emits one paint receipt and a bounded terminal status. EOF covers a crash.
async fn read_status(reader: &mut BufReader<impl AsyncRead + Unpin>) -> Result<(), Error> {
    let mut line = Vec::new();
    reader
        .take(96)
        .read_until(b'\n', &mut line)
        .await
        .map_err(|_| unavailable("cannot read desktop viewer status"))?;
    if line.is_empty() {
        return Err(unavailable("desktop viewer stopped"));
    }
    if !line.ends_with(b"\n") {
        return Err(unavailable("invalid desktop viewer status"));
    }
    let record = std::str::from_utf8(&line)
        .unwrap_or("")
        .trim_end_matches(['\r', '\n']);
    if record == "FRAME" {
        return Ok(());
    }
    if let Some(detail) = record.strip_prefix("ERROR ") {
        if !detail.is_empty()
            && detail
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b" ()-_".contains(&b))
        {
            return Err(unavailable(format!("desktop viewer: {detail}")));
        }
    }
    Err(unavailable("invalid desktop viewer status"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_keyboard_needs_capability_and_receiver_grant() {
        for mode in 0..=2 {
            let mut reply = [0; 25];
            reply[..4].copy_from_slice(b"MDS1");
            reply[4] = mode;
            let launch = Launch::parse(&reply).unwrap();
            assert_eq!(negotiated_mode(HELLO, &launch), mode.min(1));
            assert_eq!(negotiated_mode(FIXED_KEYBOARD_HELLO, &launch), mode);
        }
    }

    #[tokio::test]
    async fn viewer_status_distinguishes_paint_failure_and_truncation() {
        let mut pipe = BufReader::new(&b"FRAME\r\nERROR connection (-102)\n"[..]);
        read_status(&mut pipe).await.unwrap();
        assert!(
            read_status(&mut pipe)
                .await
                .unwrap_err()
                .to_string()
                .contains("connection (-102)")
        );
        for invalid in [
            vec![b'A'; 96],
            b"ERROR private/path\n".to_vec(),
            b"FRAME".to_vec(),
        ] {
            let mut pipe = BufReader::new(&invalid[..]);
            assert!(
                read_status(&mut pipe)
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("invalid desktop viewer status")
            );
        }
    }
}
