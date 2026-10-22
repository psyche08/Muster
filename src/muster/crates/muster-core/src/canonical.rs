//! Deterministic byte encoding for everything that gets hashed or signed.
//! Fixed field order, big-endian integers, length-prefixed bytes and
//! strings, no optional whitespace, no map ordering to argue about.

use crate::Error;

#[derive(Default)]
pub struct Writer(Vec<u8>);

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }
    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }
    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }
    pub fn fixed(&mut self, v: &[u8]) -> &mut Self {
        self.0.extend_from_slice(v);
        self
    }
    pub fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.u32(v.len() as u32);
        self.0.extend_from_slice(v);
        self
    }
    pub fn str(&mut self, v: &str) -> &mut Self {
        self.bytes(v.as_bytes())
    }
    pub fn opt_bytes(&mut self, v: Option<&[u8]>) -> &mut Self {
        match v {
            Some(v) => {
                self.u8(1);
                self.bytes(v)
            }
            None => self.u8(0),
        }
    }
    pub fn finish(self) -> Vec<u8> {
        self.0
    }
}

pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self.pos.checked_add(n).ok_or(Error::Truncated)?;
        let slice = self.data.get(self.pos..end).ok_or(Error::Truncated)?;
        self.pos = end;
        Ok(slice)
    }
    pub fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16, Error> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    pub fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn u64(&mut self) -> Result<u64, Error> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub fn fixed<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        Ok(self.take(N)?.try_into().unwrap())
    }
    pub fn bytes(&mut self) -> Result<&'a [u8], Error> {
        let len = self.u32()? as usize;
        if len > 1 << 20 {
            return Err(Error::Malformed("length prefix over 1 MiB".into()));
        }
        self.take(len)
    }
    pub fn str(&mut self) -> Result<&'a str, Error> {
        std::str::from_utf8(self.bytes()?)
            .map_err(|_| Error::Malformed("string is not UTF-8".into()))
    }
    pub fn opt_bytes(&mut self) -> Result<Option<&'a [u8]>, Error> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.bytes()?)),
            other => Err(Error::Malformed(format!("option tag {other}"))),
        }
    }
    pub fn finish(self) -> Result<(), Error> {
        if self.pos == self.data.len() {
            Ok(())
        } else {
            Err(Error::Malformed(format!(
                "{} trailing bytes",
                self.data.len() - self.pos
            )))
        }
    }
}
