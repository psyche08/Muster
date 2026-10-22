//! Generated muster-rpc types plus the framing helpers every user of the
//! schema needs: one length-prefixed Envelope per message.

#![allow(clippy::all)]

pub mod v1 {
    include!(concat!(env!("OUT_DIR"), "/muster.v1.rs"));
}

/// The app shells' Intent / Effect / ViewModel contract.
pub mod app {
    include!(concat!(env!("OUT_DIR"), "/muster.app.rs"));
}

pub use prost::Message;
pub mod attachments;
pub mod locked_use;

pub const MAX_ENVELOPE: usize = 8 * 1024 * 1024;
/// The largest diagnostics upload; fits an envelope with room to spare.
pub const MAX_DIAGNOSTICS: usize = 6 * 1024 * 1024;
/// The kinds of diagnostics a daemon accepts.
pub const DIAGNOSTICS_KINDS: &[&str] = &["performance"];

/// Encodes an envelope with its 4-byte big-endian length prefix.
pub fn frame(envelope: &v1::Envelope) -> Vec<u8> {
    let body = envelope.encode_to_vec();
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(&body);
    out
}

#[derive(Debug)]
pub enum FrameError {
    TooLarge(usize),
    Decode(prost::DecodeError),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge(n) => write!(f, "envelope of {n} bytes exceeds {MAX_ENVELOPE}"),
            Self::Decode(e) => write!(f, "envelope: {e}"),
        }
    }
}

impl std::error::Error for FrameError {}

/// Splits one framed envelope off the front of `buf`, if a whole one is there.
pub fn unframe(buf: &mut Vec<u8>) -> Result<Option<v1::Envelope>, FrameError> {
    if buf.len() < 4 {
        return Ok(None);
    }
    let len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if len > MAX_ENVELOPE {
        return Err(FrameError::TooLarge(len));
    }
    if buf.len() < 4 + len {
        return Ok(None);
    }
    let envelope = v1::Envelope::decode(&buf[4..4 + len]).map_err(FrameError::Decode)?;
    buf.drain(..4 + len);
    Ok(Some(envelope))
}

/// The bytes a presence proof signs for call `id` carrying `req`.
pub fn presence_message(id: u64, req: &v1::request::Req) -> Vec<u8> {
    let mut out = b"MUSTER-PRESENCE-V1".to_vec();
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(
        &v1::Request {
            req: Some(req.clone()),
        }
        .encode_to_vec(),
    );
    out
}

impl v1::Envelope {
    pub fn request(id: u64, req: v1::request::Req) -> Self {
        Self {
            id,
            msg: Some(v1::envelope::Msg::Request(v1::Request { req: Some(req) })),
            presence: Vec::new(),
            target: String::new(),
            presence_enrollment: Vec::new(),
        }
    }
    pub fn response(id: u64, resp: v1::response::Resp) -> Self {
        Self {
            id,
            msg: Some(v1::envelope::Msg::Response(v1::Response {
                resp: Some(resp),
            })),
            presence: Vec::new(),
            target: String::new(),
            presence_enrollment: Vec::new(),
        }
    }
    pub fn event(id: u64, ev: v1::event::Ev) -> Self {
        Self {
            id,
            msg: Some(v1::envelope::Msg::Event(v1::Event { ev: Some(ev) })),
            presence: Vec::new(),
            target: String::new(),
            presence_enrollment: Vec::new(),
        }
    }
    pub fn error(id: u64, code: v1::error::Code, message: impl Into<String>) -> Self {
        Self::response(
            id,
            v1::response::Resp::Error(v1::Error {
                code: code as i32,
                message: message.into(),
            }),
        )
    }
    pub fn ack(id: u64) -> Self {
        Self::response(id, v1::response::Resp::Ack(v1::Ack {}))
    }
    pub fn cancel(id: u64) -> Self {
        Self {
            id,
            msg: Some(v1::envelope::Msg::Cancel(v1::Cancel {})),
            presence: Vec::new(),
            target: String::new(),
            presence_enrollment: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_round_trip() {
        let envelope =
            v1::Envelope::request(7, v1::request::Req::DeviceInfo(v1::DeviceInfoRequest {}));
        let mut buf = frame(&envelope);
        buf.extend_from_slice(&frame(&v1::Envelope::ack(7)));
        let first = unframe(&mut buf).unwrap().unwrap();
        assert_eq!(first.id, 7);
        assert!(matches!(first.msg, Some(v1::envelope::Msg::Request(_))));
        let second = unframe(&mut buf).unwrap().unwrap();
        assert!(matches!(second.msg, Some(v1::envelope::Msg::Response(_))));
        assert!(unframe(&mut buf).unwrap().is_none());
        assert!(buf.is_empty());
    }
}

pub mod history;
