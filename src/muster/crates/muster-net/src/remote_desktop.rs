//! Reliable control and lossy media carried by one admitted PeerLink stream.

pub mod gamestream;

use std::collections::{HashMap, hash_map::Entry};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use bytes::{BufMut, Bytes, BytesMut};
use muster_core::DeviceId;
use quinn::{Connection, RecvStream, SendStream};
use tokio::sync::mpsc;

use crate::Error;
use crate::stream::{read_message, write_message};

pub(crate) const SERVICE: &str = "remote-desktop";
pub(crate) const VERSION: u32 = 1;
const MEDIA_TAG: u8 = 0x01;
const HEADER_LEN: usize = 1 + size_of::<u64>();
const MAX_CONTROL: usize = 16 * 1024;
const MAX_MEDIA: usize = 1150 - HEADER_LEN;
const QUEUE_DEPTH: usize = 32;
const AUTH_CHECK: Duration = Duration::from_millis(100);

pub(crate) type Guard = Arc<dyn Fn() -> bool + Send + Sync>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    View,
    ViewAndMouse,
}

/// A host offer stays alive only while its owner holds this handle.
/// Sessions retain weak references so a disconnect or a stalled reader
/// cannot prolong the owner's grant.
#[must_use]
pub struct Sharing {
    pub(crate) offer: Arc<Offer>,
}

pub(crate) struct Offer {
    pub viewer: DeviceId,
    pub handler: Arc<dyn Handler>,
    access: Mutex<Option<Access>>,
}

impl Sharing {
    pub(crate) fn new(viewer: DeviceId, access: Access, handler: Arc<dyn Handler>) -> Self {
        Self {
            offer: Arc::new(Offer {
                viewer,
                handler,
                access: Mutex::new(Some(access)),
            }),
        }
    }
}

impl Drop for Sharing {
    fn drop(&mut self) {
        self.offer.revoke();
    }
}

impl Offer {
    pub(crate) fn revoke(&self) {
        *self.access.lock().unwrap() = None;
    }

    pub(crate) fn allows(&self, viewer: DeviceId) -> bool {
        if viewer != self.viewer {
            return false;
        }
        let mut access = self.access.lock().unwrap();
        if access.is_some() && !self.handler.console_ready() {
            *access = None;
        }
        access.is_some()
    }

    fn mouse<T>(&self, action: impl FnOnce() -> T) -> Result<T, Error> {
        let mut access = self.access.lock().unwrap();
        if !self.handler.console_ready() {
            *access = None;
        }
        if *access != Some(Access::ViewAndMouse) {
            return Err(Error::Unauthorized("mouse control was not granted".into()));
        }
        Ok(action())
    }
}

pub trait Handler: Send + Sync {
    /// Report whether the host console is unlocked, active and available.
    fn console_ready(&self) -> bool;

    fn serve<'session>(
        &'session self,
        peer: DeviceId,
        session: &'session mut Session,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'session>>;
}

#[derive(Default)]
pub(crate) struct Routes(Mutex<HashMap<usize, Route>>);

struct Route {
    stream: u64,
    tx: mpsc::Sender<Bytes>,
}

impl Routes {
    pub(crate) fn register(
        self: &Arc<Self>,
        connection: &Connection,
        send: SendStream,
        recv: RecvStream,
        guard: Guard,
        host_offer: Option<Weak<Offer>>,
    ) -> Result<Session, Error> {
        let stream = u64::from(send.id());
        let (tx, media) = mpsc::channel(QUEUE_DEPTH);
        match self.0.lock().unwrap().entry(connection.stable_id()) {
            Entry::Vacant(entry) => entry.insert(Route { stream, tx }),
            Entry::Occupied(_) => return Err(Error::Protocol("remote desktop link busy".into())),
        };
        Ok(Session {
            connection: connection.clone(),
            routes: self.clone(),
            send,
            recv,
            media,
            guard,
            host_offer,
        })
    }

    pub(crate) fn deliver(&self, connection: usize, packet: &Bytes) -> bool {
        if packet.first() != Some(&MEDIA_TAG) {
            return false;
        }
        if packet.len() <= HEADER_LEN || packet.len() > HEADER_LEN + MAX_MEDIA {
            return true;
        }
        let stream = u64::from_be_bytes(packet[1..HEADER_LEN].try_into().unwrap());
        if let Some(route) = self.0.lock().unwrap().get(&connection)
            && route.stream == stream
        {
            let _ = route.tx.try_send(packet.slice(HEADER_LEN..));
        }
        true
    }
}

pub struct Session {
    connection: Connection,
    routes: Arc<Routes>,
    send: SendStream,
    recv: RecvStream,
    media: mpsc::Receiver<Bytes>,
    guard: Guard,
    host_offer: Option<Weak<Offer>>,
}

impl Session {
    pub(crate) async fn send_ack(&mut self, ack: u8) -> Result<(), Error> {
        self.authorize()?;
        self.send
            .write_all(&[ack])
            .await
            .map_err(|error| Error::Io(error.to_string()))
    }

    pub async fn send_control(&mut self, payload: &[u8]) -> Result<(), Error> {
        self.authorize()?;
        if payload.len() > MAX_CONTROL {
            return Err(Error::Protocol(
                "remote desktop control frame too large".into(),
            ));
        }
        write_message(&mut self.send, payload).await
    }

    pub async fn recv_control(&mut self) -> Result<Option<Vec<u8>>, Error> {
        self.authorize()?;
        let message = read_message(&mut self.recv, MAX_CONTROL).await?;
        self.authorize()?;
        Ok(message)
    }

    pub fn send_media(&self, payload: &[u8]) -> Result<(), Error> {
        self.authorize()?;
        if payload.is_empty() || payload.len() > MAX_MEDIA {
            return Err(Error::Protocol(
                "invalid remote desktop media length".into(),
            ));
        }
        let mut packet = BytesMut::with_capacity(HEADER_LEN + payload.len());
        packet.put_u8(MEDIA_TAG);
        packet.put_u64(self.send.id().into());
        packet.extend_from_slice(payload);
        self.connection
            .send_datagram(packet.freeze())
            .map_err(|error| Error::Io(error.to_string()))
    }

    pub async fn recv_media(&mut self) -> Result<Bytes, Error> {
        self.authorize()?;
        let packet = tokio::select! {
            biased;
            reason = self.connection.closed() => return Err(reason.into()),
            _ = self.send.stopped() => return Err(Error::Io("remote desktop stream stopped".into())),
            packet = self.media.recv() => packet.ok_or_else(|| Error::Io("remote desktop media ended".into()))?,
        };
        self.authorize()?;
        Ok(packet)
    }

    /// Execute one synchronous OS mouse operation under the live host grant.
    /// A view-only session or the viewer end of any session always refuses it.
    pub fn with_mouse<T>(&self, action: impl FnOnce() -> T) -> Result<T, Error> {
        self.authorize()?;
        self.host_offer
            .as_ref()
            .and_then(Weak::upgrade)
            .ok_or_else(|| Error::Unauthorized("no host input grant".into()))?
            .mouse(action)
    }

    fn authorize(&self) -> Result<(), Error> {
        if (self.guard)() {
            Ok(())
        } else {
            Err(Error::Unauthorized("remote desktop grant ended".into()))
        }
    }

    pub(crate) async fn serve(&mut self, peer: DeviceId, handler: Arc<dyn Handler>) {
        let connection = self.connection.clone();
        let stopped = self.send.stopped();
        let guard = self.guard.clone();
        tokio::select! {
            biased;
            _ = connection.closed() => {}
            _ = stopped => {}
            _ = async {
                while guard() {
                    tokio::time::sleep(AUTH_CHECK).await;
                }
            } => {}
            _ = handler.serve(peer, self) => {}
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(offer) = self.host_offer.as_ref().and_then(Weak::upgrade) {
            offer.revoke();
        }
        self.routes
            .0
            .lock()
            .unwrap()
            .remove(&self.connection.stable_id());
        let _ = self.send.reset(0u32.into());
        let _ = self.recv.stop(0u32.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(stream: u64, payload: &[u8]) -> Bytes {
        let mut packet = BytesMut::new();
        packet.put_u8(MEDIA_TAG);
        packet.put_u64(stream);
        packet.extend_from_slice(payload);
        packet.freeze()
    }

    #[test]
    fn media_is_bound_to_its_connection_and_stream_and_never_falls_back_to_vnet() {
        let routes = Routes::default();
        let (tx, mut rx) = mpsc::channel(1);
        routes.0.lock().unwrap().insert(1, Route { stream: 8, tx });
        assert!(!routes.deliver(1, &Bytes::from_static(&[0x45])));
        assert!(routes.deliver(1, &Bytes::from_static(&[MEDIA_TAG])));
        assert!(routes.deliver(2, &packet(8, b"wrong link")));
        assert!(routes.deliver(1, &packet(4, b"wrong stream")));
        assert!(routes.deliver(1, &packet(8, &[0; MAX_MEDIA + 1])));
        assert!(rx.try_recv().is_err());
        assert!(routes.deliver(1, &packet(8, b"accepted")));
        assert!(routes.deliver(1, &packet(8, b"queue full")));
        assert_eq!(rx.try_recv().unwrap(), &b"accepted"[..]);
        assert!(rx.try_recv().is_err());
        routes.0.lock().unwrap().remove(&1);
        assert!(routes.deliver(1, &packet(8, b"expired")));
        assert!(rx.try_recv().is_err());
    }
}
