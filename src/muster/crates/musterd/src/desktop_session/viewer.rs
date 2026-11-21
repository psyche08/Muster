use super::engine::*;
use crate::{Daemon, Error};
use muster_core::DeviceId;
use muster_net::remote_desktop::gamestream::{self, EnginePorts, ViewerPorts};
use muster_proto::v1::desktop_session_progress::Phase;
use muster_rpc::EventSink;
use std::{process::Stdio, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
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
    let reply = timeout(Duration::from_secs(50), session.recv_control())
        .await
        .map_err(|_| unavailable("paired host did not start its desktop"))??
        .ok_or_else(|| unavailable("host ended the desktop grant"))?;
    if reply == CAPTURE_UNAVAILABLE {
        let _ = session.send_control(b"received").await;
    }
    let launch = Launch::parse(&reply)?;
    let (child, mut input, output) =
        Engine::launch(Command::new(executable).stdout(Stdio::piped()))?;
    let engine = Arc::new(child);
    let mut output = output.ok_or_else(|| unavailable("missing viewer status pipe"))?;
    let mut init = Vec::from(*b"MVW1");
    init.extend([launch.mouse as u8, 1]);
    init.extend(ports.base().to_be_bytes());
    init.extend(hwnd.to_be_bytes());
    init.extend(launch.key);
    input
        .write_all(&init)
        .await
        .map_err(|_| unavailable("viewer refused initialization"))?;
    let first_frame = async {
        let mut marker = [0; 5];
        timeout(Duration::from_secs(35), output.read_exact(&mut marker))
            .await
            .map_err(|_| unavailable("timed out waiting for a desktop frame"))?
            .map_err(|_| unavailable("viewer stopped before displaying a frame"))?;
        if &marker != b"FRAME" {
            return Err(unavailable("invalid viewer progress"));
        }
        progress(events, Phase::Visible).await?;
        std::future::pending::<Result<(), Error>>().await
    };
    tokio::select! {
        biased;
        error=engine.exited()=>Err(error),
        result=first_frame=>result,
        result=gamestream::viewer(&mut session,listeners,engine.clone())=>Ok(result?),
    }
}
