//! Service streams (ARCHITECTURE §3.5): every QUIC bidirectional stream opens
//! with a hello naming the service, then carries length-prefixed messages
//! whose encoding the service decides (canonical bytes here, protobuf in
//! muster-rpc).

use std::time::Duration;

use muster_core::canonical::{Reader, Writer};
use quinn::{RecvStream, SendStream};

use crate::Error;

const HELLO_VERSION: u8 = 1;
const MAX_HELLO: usize = 256;
pub const MAX_MESSAGE: usize = 4 * 1024 * 1024;
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hello {
    pub service: String,
    pub version: u32,
}

impl Hello {
    pub fn new(service: &str, version: u32) -> Self {
        Self {
            service: service.to_string(),
            version,
        }
    }

    fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(HELLO_VERSION).str(&self.service).u32(self.version);
        w.finish()
    }

    fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(bytes);
        if r.u8()? != HELLO_VERSION {
            return Err(Error::Protocol("hello version".into()));
        }
        let service = r.str()?.to_string();
        let version = r.u32()?;
        r.finish()?;
        Ok(Self { service, version })
    }
}

pub async fn write_message(send: &mut SendStream, bytes: &[u8]) -> Result<(), Error> {
    if bytes.len() > MAX_MESSAGE {
        return Err(Error::Protocol("message over 4 MiB".into()));
    }
    let len = (bytes.len() as u32).to_be_bytes();
    send.write_all(&len)
        .await
        .map_err(|e| Error::Io(e.to_string()))?;
    send.write_all(bytes)
        .await
        .map_err(|e| Error::Io(e.to_string()))?;
    Ok(())
}

/// Reads one message; `Ok(None)` at a clean end of stream.
pub async fn read_message(recv: &mut RecvStream, max: usize) -> Result<Option<Vec<u8>>, Error> {
    let mut len = [0u8; 4];
    match recv.read_exact(&mut len).await {
        Ok(()) => {}
        Err(quinn::ReadExactError::FinishedEarly(0)) => return Ok(None),
        Err(error) => return Err(Error::Io(error.to_string())),
    }
    let len = u32::from_be_bytes(len) as usize;
    if len > max {
        return Err(Error::Protocol(format!(
            "message of {len} bytes exceeds {max}"
        )));
    }
    let mut body = vec![0u8; len];
    recv.read_exact(&mut body)
        .await
        .map_err(|e| Error::Io(e.to_string()))?;
    Ok(Some(body))
}

pub async fn write_hello(send: &mut SendStream, hello: &Hello) -> Result<(), Error> {
    write_message(send, &hello.encode()).await
}

pub async fn read_hello(recv: &mut RecvStream) -> Result<Hello, Error> {
    let bytes = tokio::time::timeout(HELLO_TIMEOUT, read_message(recv, MAX_HELLO))
        .await
        .map_err(|_| Error::Timeout("service hello"))??
        .ok_or_else(|| Error::Protocol("stream closed before hello".into()))?;
    Hello::decode(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_round_trip() {
        let hello = Hello::new("roster", 1);
        assert_eq!(Hello::decode(&hello.encode()).unwrap(), hello);
    }
}
