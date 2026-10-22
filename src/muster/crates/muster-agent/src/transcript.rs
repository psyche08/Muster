//! Incremental JSONL reading shared by the viewers: remember the byte
//! offset, read what was appended, keep a partial trailing line for the next
//! round, and hand each complete record to the provider's parser with the
//! offset it started at (which becomes the message id).

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Records larger than this are skipped rather than buffered forever.
const MAX_RECORD: usize = 16 * 1024 * 1024;

pub struct Tail {
    path: PathBuf,
    offset: u64,
    partial: Vec<u8>,
    dropping: bool,
}

impl Tail {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            offset: 0,
            partial: Vec::new(),
            dropping: false,
        }
    }

    pub fn from_end(path: &Path) -> std::io::Result<Self> {
        let mut tail = Self::new(path);
        tail.offset = std::fs::metadata(path)?.len();
        Ok(tail)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Reads whatever was appended since the last call. If the file shrank
    /// (rewritten), starts over from the beginning and reports `truncated`.
    pub fn read(&mut self) -> std::io::Result<Read1> {
        let mut file = std::fs::File::open(&self.path)?;
        let len = file.metadata()?.len();
        let mut truncated = false;
        if len < self.offset {
            self.offset = 0;
            self.partial.clear();
            self.dropping = false;
            truncated = true;
        }
        let mut records = Vec::new();
        if len == self.offset {
            return Ok(Read1 { records, truncated });
        }
        file.seek(SeekFrom::Start(self.offset))?;
        let mut buf = Vec::with_capacity((len - self.offset).min(4 << 20) as usize);
        file.take(len - self.offset).read_to_end(&mut buf)?;
        let mut line_start_offset = self.offset - self.partial.len() as u64;
        let mut consumed = 0usize;
        for (i, byte) in buf.iter().enumerate() {
            if *byte != b'\n' {
                continue;
            }
            let chunk = &buf[consumed..i];
            if self.dropping {
                self.dropping = false;
            } else {
                let mut line = std::mem::take(&mut self.partial);
                line.extend_from_slice(chunk);
                let record_offset = line_start_offset;
                if let Ok(text) = std::str::from_utf8(&line) {
                    let text = text.trim();
                    if !text.is_empty()
                        && let Ok(value) = serde_json::from_str::<Value>(text)
                    {
                        records.push((record_offset, value));
                    }
                }
            }
            consumed = i + 1;
            line_start_offset = self.offset + consumed as u64;
        }
        if consumed < buf.len() {
            if self.dropping {
                // keep dropping until the newline
            } else if self.partial.len() + (buf.len() - consumed) > MAX_RECORD {
                self.partial.clear();
                self.dropping = true;
            } else {
                self.partial.extend_from_slice(&buf[consumed..]);
            }
        }
        self.offset = len;
        Ok(Read1 { records, truncated })
    }
}

pub struct Read1 {
    /// (byte offset of the record's first byte, the parsed record)
    pub records: Vec<(u64, Value)>,
    pub truncated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn reads_incrementally_and_keeps_partial_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"{\"a\":1}\n{\"a\":2}\n{\"a\":").unwrap();
        let mut tail = Tail::new(&path);
        let r = tail.read().unwrap();
        assert_eq!(r.records.len(), 2);
        assert_eq!(r.records[0].0, 0);
        assert_eq!(r.records[1].0, 8);
        f.write_all(b"3}\n").unwrap();
        let r = tail.read().unwrap();
        assert_eq!(r.records.len(), 1);
        assert_eq!(r.records[0].0, 16);
        assert_eq!(r.records[0].1["a"], 3);
        assert!(tail.read().unwrap().records.is_empty());
        // Rewritten shorter: start over.
        std::fs::write(&path, b"{\"b\":1}\n").unwrap();
        let r = tail.read().unwrap();
        assert!(r.truncated);
        assert_eq!(r.records.len(), 1);
        assert_eq!(r.records[0].1["b"], 1);
    }
}
