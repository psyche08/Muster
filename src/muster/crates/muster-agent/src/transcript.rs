//! Incremental JSONL reading shared by the viewers: remember the byte
//! offset, read what was appended, keep a partial trailing line for the next
//! round, and hand each complete record to the provider's parser with the
//! offset it started at (which becomes the message id).
//!
//! A log written as a stream of Zstandard frames (DeepSeek Harness) is read
//! the same way, one complete frame at a time: offsets are then positions in
//! the decoded stream, and a frame still being written waits for the next
//! round.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Records larger than this are skipped rather than buffered forever.
const MAX_RECORD: usize = 16 * 1024 * 1024;

pub struct Tail {
    path: PathBuf,
    /// How far the file was read: always at a frame boundary for frames.
    offset: u64,
    partial: Vec<u8>,
    dropping: bool,
    /// `Some` when the file is a stream of Zstandard frames (boxed: the
    /// decoder is large and most tails are plain).
    frames: Option<Box<Frames>>,
}

/// The decoded side of a stream of Zstandard frames.
struct Frames {
    decoder: ruzstd::decoding::FrameDecoder,
    /// How many decoded bytes the frames read so far hold.
    decoded: u64,
}

impl Frames {
    /// Decodes the complete frames at the start of `input`: their content,
    /// and how many bytes of `input` they took. A frame that does not decode
    /// (one still being written) and everything after it are left for later.
    fn decode(&mut self, mut input: &[u8]) -> (Vec<u8>, usize) {
        use ruzstd::decoding::BlockDecodingStrategy;
        use ruzstd::decoding::errors::{FrameDecoderError, ReadFrameHeaderError};
        let mut out = Vec::new();
        let mut used = 0;
        while !input.is_empty() {
            let mut frame = input;
            match self.decoder.reset(&mut frame) {
                Ok(()) => {}
                // Skippable frames: magic, length, then that many bytes.
                Err(FrameDecoderError::ReadFrameHeaderError(ReadFrameHeaderError::SkipFrame {
                    length,
                    ..
                })) => {
                    let skip = 8 + length as usize;
                    if input.len() < skip {
                        break;
                    }
                    input = &input[skip..];
                    used += skip;
                    continue;
                }
                Err(_) => break,
            }
            if !matches!(
                self.decoder
                    .decode_blocks(&mut frame, BlockDecodingStrategy::All),
                Ok(true)
            ) {
                break;
            }
            let Some(content) = self.decoder.collect() else {
                break;
            };
            out.extend_from_slice(&content);
            used += input.len() - frame.len();
            input = frame;
        }
        self.decoded += out.len() as u64;
        (out, used)
    }
}

impl Tail {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            offset: 0,
            partial: Vec::new(),
            dropping: false,
            frames: None,
        }
    }

    /// A log written as a stream of Zstandard frames.
    pub fn zstd(path: impl Into<PathBuf>) -> Self {
        Self {
            frames: Some(Box::new(Frames {
                decoder: ruzstd::decoding::FrameDecoder::new(),
                decoded: 0,
            })),
            ..Self::new(path)
        }
    }

    /// A tail that starts at the current end of a plain JSONL file.
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
            if let Some(frames) = &mut self.frames {
                frames.decoded = 0;
            }
            truncated = true;
        }
        let mut records = Vec::new();
        if len == self.offset {
            return Ok(Read1 { records, truncated });
        }
        file.seek(SeekFrom::Start(self.offset))?;
        let mut buf = Vec::with_capacity((len - self.offset).min(4 << 20) as usize);
        file.take(len - self.offset).read_to_end(&mut buf)?;
        // Where `buf` starts in the stream the offsets count: the file, or
        // the decoded frames.
        let (start, read) = match &mut self.frames {
            None => (self.offset, buf.len()),
            Some(frames) => {
                let start = frames.decoded;
                let (decoded, used) = frames.decode(&buf);
                buf = decoded;
                (start, used)
            }
        };
        let mut line_start_offset = start - self.partial.len() as u64;
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
            line_start_offset = start + consumed as u64;
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
        self.offset += read as u64;
        Ok(Read1 { records, truncated })
    }
}

/// The first record of a log, read from at most its first `limit` bytes: a
/// plain JSONL file's first line, or the first line of a stream of
/// Zstandard frames (`zstd`). `None` when it is not there yet.
pub fn first_record(path: &Path, zstd: bool, limit: u64) -> Option<Value> {
    let mut head = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(limit)
        .read_to_end(&mut head)
        .ok()?;
    if zstd {
        let mut frames = Frames {
            decoder: ruzstd::decoding::FrameDecoder::new(),
            decoded: 0,
        };
        head = frames.decode(&head).0;
    }
    let line = &head[..head.iter().position(|b| *b == b'\n')?];
    serde_json::from_slice(line).ok()
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

    fn frame(text: &str) -> Vec<u8> {
        use ruzstd::encoding::{CompressionLevel, compress_to_vec};
        compress_to_vec(text.as_bytes(), CompressionLevel::Fastest)
    }

    /// Each flush of a DeepSeek Harness log is one frame; offsets count the
    /// decoded stream, and a frame still being written waits.
    #[test]
    fn reads_zstd_frames_as_they_complete() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.v4.jsonl.zstd");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(&frame("{\"a\":1}\n{\"a\":2}\n")).unwrap();
        // A skippable frame between them is passed over.
        f.write_all(&[0x50, 0x2a, 0x4d, 0x18, 3, 0, 0, 0, 9, 9, 9])
            .unwrap();
        let third = frame("{\"a\":3}\n");
        f.write_all(&third[..third.len() - 2]).unwrap();
        let mut tail = Tail::zstd(&path);
        let r = tail.read().unwrap();
        assert_eq!(r.records.len(), 2);
        assert_eq!((r.records[0].0, r.records[1].0), (0, 8));
        f.write_all(&third[third.len() - 2..]).unwrap();
        let r = tail.read().unwrap();
        assert_eq!(r.records.len(), 1);
        assert_eq!(r.records[0].0, 16);
        assert_eq!(r.records[0].1["a"], 3);
        assert!(tail.read().unwrap().records.is_empty());
        // A record split across two frames is one record.
        f.write_all(&frame("{\"a\":")).unwrap();
        assert!(tail.read().unwrap().records.is_empty());
        f.write_all(&frame("4}\n")).unwrap();
        let r = tail.read().unwrap();
        assert_eq!((r.records[0].0, &r.records[0].1["a"]), (24, &4.into()));
        // Rewritten shorter: start over in the decoded stream too.
        std::fs::write(&path, frame("{\"b\":1}\n")).unwrap();
        let r = tail.read().unwrap();
        assert!(r.truncated);
        assert_eq!((r.records[0].0, &r.records[0].1["b"]), (0, &1.into()));
    }
}
