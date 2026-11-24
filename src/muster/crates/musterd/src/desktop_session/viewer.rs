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
    let (mut session, reply) = super::handshake::open(daemon, peer, HELLO, events).await?;
    let launch = Launch::parse(&reply)?;
    let listeners = ViewerPorts::bind(ports).await?;
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
