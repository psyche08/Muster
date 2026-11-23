use super::engine::*;
use crate::{Daemon, Error};
use muster_core::DeviceId;
use muster_net::remote_desktop::gamestream::{self, EnginePorts, ViewerPorts};
use muster_proto::v1::desktop_session_progress::Phase;
use muster_rpc::EventSink;
use std::{process::Stdio, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
    time::timeout,
};
pub(super) const MOONLIGHT: &str = "muster-moonlight.exe";

pub(super) async fn view(
    daemon: &Arc<Daemon>,
    peer: DeviceId,
    hwnd: u64,
    events: &EventSink,
) -> Result<(), Error> {
    let executable = program(MOONLIGHT)?;
    let ports = EnginePorts::MUSTER;
    let listeners = ViewerPorts::bind(ports).await?;
    progress(events, Phase::Starting).await?;
    let mut session = timeout(
        Duration::from_secs(25),
        daemon.node().open_remote_desktop(peer),
    )
    .await
    .map_err(|_| unavailable("timed out waiting for paired host authorization"))??;
    session.send_control(HELLO).await?;
    let mut reply = timeout(Duration::from_secs(50), session.recv_control())
        .await
        .map_err(|_| unavailable("paired host did not start its desktop"))??
        .ok_or_else(|| unavailable("host ended the desktop grant"))?;
    if let Some(raw) = reply.strip_prefix(BUSY) {
        let controller = DeviceId(
            raw.try_into()
                .map_err(|_| unavailable("invalid desktop occupant"))?,
        );
        let name = daemon
            .roster()
            .read()
            .unwrap()
            .state()
            .active(&controller)
            .map(|m| m.record.name.clone())
            .unwrap_or_default();
        let name: String = name
            .chars()
            .filter(|c| {
                !c.is_control() && !matches!(*c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
            })
            .take(64)
            .collect();
        let controller = format!("{} ({})", name.trim(), controller.short());
        let mut prompt = daemon.desktop_confirmation.ask(peer);
        events
            .send(muster_proto::v1::event::Ev::DesktopSession(
                muster_proto::v1::DesktopSessionProgress {
                    phase: Phase::Occupied as i32,
                    takeover: Some(muster_proto::v1::DesktopTakeoverPrompt {
                        confirmation: prompt.id,
                        controller,
                    }),
                },
            ))
            .await
            .map_err(|_| unavailable("viewer closed"))?;
        tokio::select! {
            choice = timeout(Duration::from_secs(85), &mut prompt.reply) => {
                choice.map_err(|_| unavailable("desktop confirmation expired"))?
                    .map_err(|_| unavailable("desktop confirmation cancelled"))?;
            }
            _ = session.recv_control() => return Err(unavailable("desktop host ended the pending request")),
        }
        session.send_control(REPLACE).await?;
        progress(events, Phase::Starting).await?;
        reply = timeout(Duration::from_secs(60), session.recv_control())
            .await
            .map_err(|_| unavailable("desktop takeover timed out"))??
            .ok_or_else(|| unavailable("desktop host ended the takeover"))?;
        if reply == CHANGED {
            let _ = session.send_control(b"received").await;
            return Err(unavailable(
                "The active viewer changed. Open the desktop again to see who is connected.",
            ));
        }
    }
    if reply == CAPTURE_UNAVAILABLE {
        let _ = session.send_control(b"received").await;
    }
    let launch = Launch::parse(&reply)?;
    let (child, mut input, output) =
        Engine::launch(Command::new(executable).stdout(Stdio::piped()))?;
    let engine = Arc::new(child);
    let output = output.ok_or_else(|| unavailable("missing viewer status pipe"))?;
    let mut init = Vec::from(*b"MVW1");
    init.extend([launch.mouse as u8, 1]);
    init.extend(ports.base().to_be_bytes());
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
        progress(events, Phase::Visible).await?;
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
