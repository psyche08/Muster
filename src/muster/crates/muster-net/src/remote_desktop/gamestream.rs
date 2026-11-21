//! Fixed GameStream channels. The peer supplies neither addresses nor ports.
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::Arc;

use bytes::{BufMut, Bytes, BytesMut};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use super::{Guard, HEADER_LEN, MAX_CONTROL, MAX_MEDIA, MEDIA_TAG, Session};
use crate::Error;
use crate::stream::{read_message, write_message};

const TCP_SLOTS: usize = 8;
const TCP_CHUNK: usize = 8192;
const QUEUE: usize = 16;
const MEDIA_CHUNK: usize = MAX_MEDIA - 1;
const OPEN: u8 = 1;
const DATA: u8 = 2;
const FIN: u8 = 3;
const CLOSED: u8 = 4;

const RTSP_OFFSET: i16 = 21;
const UDP_OFFSETS: [i16; 3] = [9, 10, 11]; // video, ENet control, audio

/// Port base chosen locally for this pair of media engines. Both ends use
/// matching offsets so RTSP-negotiated media ports lead back into the broker.
#[derive(Clone, Copy)]
pub struct EnginePorts(u16);
impl EnginePorts {
    pub const MUSTER: Self = Self(51389);
    pub fn from_local_base(base: u16) -> Result<Self, Error> {
        if !(49157..=65514).contains(&base) {
            return Err(invalid());
        }
        Ok(Self(base))
    }
    fn addr(self, offset: i16) -> SocketAddrV4 {
        SocketAddrV4::new(
            Ipv4Addr::LOCALHOST,
            self.0.checked_add_signed(offset).unwrap(),
        )
    }

    pub fn base(self) -> u16 {
        self.0
    }
}

/// The native process adapter verifies loopback socket ownership against the
/// launched engine before accepting any local client. Never learn an engine
/// address from an unverified first UDP packet.
pub trait LocalEngine: Send + Sync {
    fn owns_tcp(&self, local_source: SocketAddrV4) -> bool;
    fn owns_udp(&self, local_source: SocketAddrV4) -> bool;
}

enum Frame<'a> {
    Open(u8),
    Data(u8, &'a [u8]),
    Fin(u8),
    Closed(u8),
}
fn invalid() -> Error {
    Error::Protocol("invalid GameStream engine frame".into())
}
fn io(err: impl std::fmt::Display) -> Error {
    Error::Io(err.to_string())
}
fn check(guard: &Guard) -> Result<(), Error> {
    if guard() {
        Ok(())
    } else {
        Err(Error::Unauthorized("desktop session ended".into()))
    }
}
fn parse(bytes: &[u8]) -> Result<Frame<'_>, Error> {
    let [kind, id, rest @ ..] = bytes else {
        return Err(invalid());
    };
    if usize::from(*id) >= TCP_SLOTS {
        return Err(invalid());
    }
    match (*kind, rest) {
        (OPEN, []) => Ok(Frame::Open(*id)),
        (DATA, data) if !data.is_empty() && data.len() <= TCP_CHUNK => Ok(Frame::Data(*id, data)),
        (FIN, []) => Ok(Frame::Fin(*id)),
        (CLOSED, []) => Ok(Frame::Closed(*id)),
        _ => Err(invalid()),
    }
}

pub struct ViewerPorts {
    rtsp: TcpListener,
    udp: Vec<UdpSocket>,
}
impl ViewerPorts {
    pub async fn bind(ports: EnginePorts) -> Result<Self, Error> {
        let rtsp = TcpListener::bind(ports.addr(RTSP_OFFSET))
            .await
            .map_err(io)?;
        let mut udp = Vec::new();
        for offset in UDP_OFFSETS {
            udp.push(UdpSocket::bind(ports.addr(offset)).await.map_err(io)?);
        }
        Ok(Self { rtsp, udp })
    }
}

enum SocketEvent {
    Accept(TcpStream),
    Done(u8),
    Peer(Vec<u8>),
}

enum Incoming {
    Data(Vec<u8>),
    Fin,
}
struct Slot {
    tx: mpsc::Sender<Incoming>,
    local_done: bool,
    remote_done: bool,
    remote_fin: bool,
}

async fn socket_task(
    id: u8,
    socket: TcpStream,
    mut input: mpsc::Receiver<Incoming>,
    output: mpsc::Sender<Vec<u8>>,
    guard: Guard,
) -> Result<(), Error> {
    let (mut read, mut write) = socket.into_split();
    let from_socket = async {
        let mut buf = vec![0; TCP_CHUNK + 2];
        buf[..2].copy_from_slice(&[DATA, id]);
        loop {
            let n = read.read(&mut buf[2..]).await.map_err(io)?;
            check(&guard)?;
            if n == 0 {
                output.send(vec![FIN, id]).await.map_err(io)?;
                return Ok(());
            }
            output.send(buf[..n + 2].to_vec()).await.map_err(io)?;
        }
    };
    let to_socket = async {
        while let Some(frame) = input.recv().await {
            check(&guard)?;
            match frame {
                Incoming::Data(bytes) => write.write_all(&bytes).await.map_err(io)?,
                Incoming::Fin => {
                    write.shutdown().await.map_err(io)?;
                    return Ok(());
                }
            }
        }
        Err::<(), Error>(Error::Io("engine channel closed".into()))
    };
    tokio::try_join!(from_socket, to_socket)?;
    Ok(())
}

enum Side {
    Viewer(ViewerPorts, Arc<dyn LocalEngine>),
    Host(EnginePorts, Arc<dyn LocalEngine>),
}

/// Run the viewer broker until the session or a mandatory engine channel ends.
pub async fn viewer(
    session: &mut Session,
    ports: ViewerPorts,
    owner: Arc<dyn LocalEngine>,
) -> Result<(), Error> {
    run(session, Side::Viewer(ports, owner)).await
}

/// Connect only to the three UDP endpoints and the RTSP listener of the session child.
pub async fn host(
    session: &mut Session,
    ports: EnginePorts,
    owner: Arc<dyn LocalEngine>,
) -> Result<(), Error> {
    if session.host_offer.is_none() {
        return Err(invalid());
    }
    run(session, Side::Host(ports, owner)).await
}

async fn run(session: &mut Session, side: Side) -> Result<(), Error> {
    session.authorize()?;
    let guard = session.guard.clone();
    let (send_frame, mut outbound) = mpsc::channel::<Vec<u8>>(QUEUE);
    let (events, mut event_rx) = mpsc::channel::<SocketEvent>(QUEUE);
    let (inbound, mut peer_frames) = mpsc::channel::<Vec<u8>>(QUEUE);
    let mut tasks: JoinSet<Result<(), Error>> = JoinSet::new();
    let host_ports = match &side {
        Side::Host(ports, owner) => Some((*ports, owner.clone())),
        _ => None,
    };
    let mut udp_sockets = Vec::new();
    match side {
        Side::Viewer(ports, owner) => {
            {
                let listener = ports.rtsp;
                let (events, owner) = (events.clone(), owner.clone());
                tasks.spawn(async move {
                    loop {
                        let (socket, addr) = listener.accept().await.map_err(io)?;
                        if let std::net::SocketAddr::V4(addr) = addr
                            && addr.ip().is_loopback()
                            && owner.owns_tcp(addr)
                        {
                            events.send(SocketEvent::Accept(socket)).await.map_err(io)?;
                        }
                    }
                });
            }
            for socket in ports.udp {
                udp_sockets.push((socket, None, owner.clone()));
            }
        }
        Side::Host(ports, owner) => {
            for offset in UDP_OFFSETS {
                let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
                    .await
                    .map_err(io)?;
                udp_sockets.push((socket, Some(ports.addr(offset)), owner.clone()));
            }
        }
    }

    let mut udp_inputs = Vec::new();
    for (channel, (socket, host_addr, owner)) in udp_sockets.into_iter().enumerate() {
        let (to_engine, mut udp_rx) = mpsc::channel::<Bytes>(QUEUE);
        udp_inputs.push(to_engine);
        let connection = session.connection.clone();
        let stream = u64::from(session.send.id());
        let guard = guard.clone();
        tasks.spawn(async move {
            let mut target = host_addr;
            let mut buf = vec![0u8; MEDIA_CHUNK + 1];
            loop {
                tokio::select! {
                    datagram = socket.recv_from(&mut buf) => {
                        let (n, addr) = datagram.map_err(io)?;
                        let std::net::SocketAddr::V4(addr) = addr else { continue; };
                        if n == 0 || n > MEDIA_CHUNK || !addr.ip().is_loopback()
                            || target.is_some_and(|peer| peer != addr) || !owner.owns_udp(addr) {
                            continue;
                        }
                        check(&guard)?;
                        target = Some(addr);
                        let mut packet = BytesMut::with_capacity(HEADER_LEN + 1 + n);
                        packet.put_u8(MEDIA_TAG);
                        packet.put_u64(stream);
                        packet.put_u8(channel as u8);
                        packet.extend_from_slice(&buf[..n]);
                        connection.send_datagram(packet.freeze()).map_err(io)?;
                    }
                    Some(bytes) = udp_rx.recv() => {
                        if let Some(addr) = target && owner.owns_udp(addr) {
                            check(&guard)?;
                            socket.send_to(&bytes, addr).await.map_err(io)?;
                        }
                    }
                }
            }
        });
    }

    let mut slots: [Option<Slot>; TCP_SLOTS] = std::array::from_fn(|_| None);
    let control = async {
        loop {
            let event = tokio::select! {
                event = event_rx.recv() => event.ok_or_else(invalid)?,
                frame = peer_frames.recv() => SocketEvent::Peer(frame.ok_or_else(invalid)?),
                result = tasks.join_next() => {
                    result.ok_or_else(invalid)?.map_err(io)??;
                    continue;
                }
            };
            let (id, socket) = match event {
                SocketEvent::Accept(socket) => {
                    let Some(id) = slots.iter().position(Option::is_none) else {
                        continue;
                    };
                    send_frame.send(vec![OPEN, id as u8]).await.map_err(io)?;
                    (id as u8, socket)
                }
                SocketEvent::Done(id) => {
                    let slot = slots[id as usize].as_mut().ok_or_else(invalid)?;
                    slot.local_done = true;
                    send_frame.send(vec![CLOSED, id]).await.map_err(io)?;
                    if slot.remote_done {
                        slots[id as usize] = None;
                    }
                    continue;
                }
                SocketEvent::Peer(frame) => {
                    check(&guard)?;
                    match parse(&frame)? {
                        Frame::Open(id) => {
                            let (ports, owner) = host_ports.as_ref().ok_or_else(invalid)?;
                            if slots[id as usize].is_some() {
                                return Err::<(), Error>(invalid());
                            }
                            let addr = ports.addr(RTSP_OFFSET);
                            if !owner.owns_tcp(addr) {
                                return Err(Error::Unauthorized(
                                    "engine socket owner changed".into(),
                                ));
                            }
                            let socket = TcpStream::connect(addr).await.map_err(io)?;
                            if !owner.owns_tcp(addr) {
                                return Err(Error::Unauthorized(
                                    "engine socket owner changed".into(),
                                ));
                            }
                            (id, socket)
                        }
                        Frame::Data(id, data) => {
                            let slot = slots[id as usize].as_ref().ok_or_else(invalid)?;
                            if slot.remote_fin {
                                return Err(invalid());
                            }
                            slot.tx
                                .send(Incoming::Data(data.to_vec()))
                                .await
                                .map_err(io)?;
                            continue;
                        }
                        Frame::Fin(id) => {
                            let slot = slots[id as usize].as_mut().ok_or_else(invalid)?;
                            if slot.remote_fin {
                                return Err(invalid());
                            }
                            slot.remote_fin = true;
                            slot.tx.send(Incoming::Fin).await.map_err(io)?;
                            continue;
                        }
                        Frame::Closed(id) => {
                            let slot = slots[id as usize].as_mut().ok_or_else(invalid)?;
                            if !slot.remote_fin || slot.remote_done {
                                return Err(invalid());
                            }
                            slot.remote_done = true;
                            if slot.local_done {
                                slots[id as usize] = None;
                            }
                            continue;
                        }
                    }
                }
            };
            let (tx, rx) = mpsc::channel(QUEUE);
            slots[id as usize] = Some(Slot {
                tx,
                local_done: false,
                remote_done: false,
                remote_fin: false,
            });
            let (output, events, guard) = (send_frame.clone(), events.clone(), guard.clone());
            tasks.spawn(async move {
                socket_task(id, socket, rx, output, guard).await?;
                events.send(SocketEvent::Done(id)).await.map_err(io)?;
                // A finished slot is local bookkeeping, independent of the other streams.
                Ok(())
            });
        }
    };
    let receiver = async {
        while let Some(data) = read_message(&mut session.recv, MAX_CONTROL).await? {
            check(&guard)?;
            inbound.send(data).await.map_err(io)?;
        }
        Err::<(), Error>(Error::Io("desktop control stream ended".into()))
    };
    let writer = async {
        while let Some(data) = outbound.recv().await {
            check(&guard)?;
            write_message(&mut session.send, &data).await?;
        }
        Err::<(), Error>(Error::Io("desktop control writer stopped".into()))
    };
    let media = async {
        while let Some(packet) = session.media.recv().await {
            check(&guard)?;
            if let Some((&tag, body)) = packet.split_first()
                && let Some(input) = udp_inputs.get(tag as usize)
                && !body.is_empty()
            {
                let _ = input.try_send(packet.slice(1..));
            }
        }
        Err::<(), Error>(Error::Io("desktop media ended".into()))
    };
    let watch = async {
        while guard() {
            tokio::time::sleep(super::AUTH_CHECK).await;
        }
        Err::<(), Error>(Error::Unauthorized("desktop session ended".into()))
    };
    tokio::try_join!(control, receiver, writer, media, watch)?;
    Ok(())
}
