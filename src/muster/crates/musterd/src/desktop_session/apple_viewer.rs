//! Apple presentation receives ordered H.264 units from the authenticated GameStream broker.
use super::engine::{Launch, unavailable};
use crate::{Daemon, Error};
use muster_core::DeviceId;
use muster_net::remote_desktop::gamestream::{self, EnginePorts, LocalEngine, ViewerPorts};
use muster_proto::v1::{
    DesktopSessionProgress, DesktopStreamSettings, desktop_session_progress::Phase, event::Ev,
};
use muster_rpc::EventSink;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

type Reader = Box<dyn AsyncRead + Send + Unpin>;
type Writer = Box<dyn AsyncWrite + Send + Unpin>;

#[cfg(target_os = "macos")]
fn start() -> Result<(Arc<dyn LocalEngine>, Writer, Reader), Error> {
    use super::engine::{Engine, program};
    let mut cmd = tokio::process::Command::new(program("muster-moonlight")?);
    cmd.stdout(std::process::Stdio::piped());
    let (engine, input, output) = Engine::launch(&mut cmd)?;
    Ok((
        Arc::new(engine),
        Box::new(input),
        Box::new(output.ok_or_else(|| unavailable("missing Moonlight output"))?),
    ))
}

#[cfg(target_os = "ios")]
fn start() -> Result<(Arc<dyn LocalEngine>, Writer, Reader), Error> {
    use std::net::SocketAddrV4;
    use std::os::fd::AsRawFd;
    struct Embedded;
    impl LocalEngine for Embedded {
        fn owns_tcp(&self, addr: SocketAddrV4) -> bool {
            owns(addr, true)
        }
        fn owns_udp(&self, addr: SocketAddrV4) -> bool {
            owns(addr, false)
        }
    }
    fn owns(addr: SocketAddrV4, tcp: bool) -> bool {
        unsafe extern "C" {
            fn muster_moonlight_owns(port: u16, tcp: bool) -> bool;
        }
        addr.ip().is_loopback() && unsafe { muster_moonlight_owns(addr.port(), tcp) }
    }
    let (rust, engine) =
        std::os::unix::net::UnixStream::pair().map_err(|e| unavailable(e.to_string()))?;
    rust.set_nonblocking(true)
        .map_err(|e| unavailable(e.to_string()))?;
    let stream = tokio::net::UnixStream::from_std(rust).map_err(|e| unavailable(e.to_string()))?;
    tokio::task::spawn_blocking(move || {
        unsafe extern "C" {
            fn muster_moonlight_run(input: i32, output: i32) -> i32;
        }
        unsafe { muster_moonlight_run(engine.as_raw_fd(), engine.as_raw_fd()) };
    });
    let (reader, writer) = stream.into_split();
    Ok((Arc::new(Embedded), Box::new(writer), Box::new(reader)))
}

pub(super) async fn view(
    daemon: &Daemon,
    peer: DeviceId,
    id: u64,
    settings: DesktopStreamSettings,
    events: &EventSink,
) -> Result<(), Error> {
    let (mut session, reply) =
        super::handshake::open(daemon, peer, &settings.video_hello(), events).await?;
    if reply.len() != 29 || !reply.starts_with(b"MDS2") {
        return Err(unavailable(
            "The host could not start Sunshine. Check the host version and capture permissions.",
        ));
    }
    let mut credentials = reply[..25].to_vec();
    credentials[3] = b'1';
    let launch = Launch::parse(&credentials)?;
    let width = u16::from_be_bytes([reply[25], reply[26]]);
    let height = u16::from_be_bytes([reply[27], reply[28]]);
    if width < 2
        || height < 2
        || u32::from(width.max(height)) > settings.long_edge
        || width % 2 != 0
        || height % 2 != 0
    {
        return Err(unavailable("invalid negotiated desktop extent"));
    }
    let ports = ViewerPorts::bind(EnginePorts::MUSTER).await?;
    let (engine, mut sink, mut source) = start()?;
    let mut init = b"MAV1".to_vec();
    init.push(launch.mouse.into());
    init.extend(width.to_be_bytes());
    init.extend(height.to_be_bytes());
    init.extend((settings.fps as u16).to_be_bytes());
    init.extend(settings.video_kbps(width, height).to_be_bytes());
    init.extend(launch.key);
    sink.write_all(&init)
        .await
        .map_err(|e| unavailable(e.to_string()))?;
    let mut inputs = launch
        .mouse
        .then(|| daemon.desktop_pointer.attach(peer, id));
    let input = async {
        if let Some(rx) = &mut inputs {
            while let Some(packet) = rx.recv().await {
                sink.write_all(&packet)
                    .await
                    .map_err(|e| unavailable(e.to_string()))?;
            }
        }
        std::future::pending::<Result<(), Error>>().await
    };
    let frames = async {
        let mut first = true;
        loop {
            let header = if first {
                tokio::time::timeout(std::time::Duration::from_secs(35), source.read_u32())
                    .await
                    .map_err(|_| unavailable("timed out waiting for Moonlight video"))?
            } else {
                source.read_u32().await
            };
            let len =
                header.map_err(|_| unavailable("Moonlight closed the video stream"))? as usize;
            if !(4..=4 * 1024 * 1024).contains(&len) {
                return Err(unavailable(
                    "Moonlight stream ended or produced an invalid frame",
                ));
            }
            let mut unit = vec![0; len];
            source
                .read_exact(&mut unit)
                .await
                .map_err(|_| unavailable("incomplete Moonlight video frame"))?;
            events
                .send(Ev::DesktopVideo(unit))
                .await
                .map_err(|_| unavailable("viewer closed"))?;
            if first {
                events
                    .send(Ev::DesktopSession(DesktopSessionProgress {
                        phase: Phase::Visible as i32,
                        mouse: launch.mouse,
                        takeover: None,
                    }))
                    .await
                    .map_err(|_| unavailable("viewer closed"))?;
                first = false;
            }
        }
    };
    tokio::select! {r=input=>r,r=frames=>r,r=gamestream::viewer(&mut session,ports,engine)=>Ok(r?)}
}
