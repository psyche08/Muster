//! A viewer follows one transcript file and turns appended records into
//! messages. Discovery (which files exist) is the provider's `scan`.

use std::path::PathBuf;
use std::time::SystemTime;

use crate::model::{Message, Provider, SessionMeta, SessionState};

/// A transcript found on disk, before anyone opened it.
#[derive(Clone, Debug)]
pub struct Discovered {
    pub id: String,
    pub provider: Provider,
    pub path: PathBuf,
    pub modified: SystemTime,
    pub size: u64,
    pub archived: bool,
    /// Subagent / sidechain transcripts: kept out of lists.
    pub hidden: bool,
    pub parent_id: Option<String>,
    /// Provider-declared relationship, independent of visibility/control.
    pub relation: &'static str,
    /// Cheap metadata from the head of the file (title, cwd).
    pub meta: SessionMeta,
    /// `meta.title` is a name the user gave the session (Claude's
    /// `custom-title`), which the desktop app shows over generated titles.
    pub custom_title: bool,
    /// The client that wrote the transcript, as the provider records it
    /// (Claude: `claude-desktop`, `cli`, `sdk-cli`); `None` when unrecorded.
    pub entrypoint: Option<String>,
}

impl Discovered {
    pub fn modified_unix(&self) -> u64 {
        self.modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// What one poll produced.
#[derive(Default)]
pub struct Polled {
    pub messages: Vec<Message>,
    /// The file was rewritten: earlier messages are void.
    pub reset: bool,
    pub state: Option<SessionState>,
    pub meta_changed: bool,
}

pub trait Viewer: Send {
    fn provider(&self) -> Provider;
    fn meta(&self) -> &SessionMeta;
    /// `meta().title` is a name the user gave the session.
    fn custom_title(&self) -> bool {
        false
    }
    fn state(&self) -> SessionState;
    fn transcript_notice(&self) -> &str {
        ""
    }
    fn path(&self) -> &std::path::Path;
    /// Changes whenever the source may have new content: the transcript's
    /// (mtime, length) by default.
    fn stamp(&self) -> std::io::Result<(SystemTime, u64)> {
        let meta = std::fs::metadata(self.path())?;
        Ok((
            meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            meta.len(),
        ))
    }
    /// Reads what was appended since the last poll.
    fn poll(&mut self) -> std::io::Result<Polled>;
}

pub fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

/// Reads at most `limit` lines from the head of a file as JSON records.
/// What a scan read from each transcript's head, reused while the file
/// keeps its size and modification time. Heads are the costly part of a
/// scan (up to 128 records parsed per file), and a rescan follows every
/// write, so only files that changed are read again. A pass keeps just
/// the files it saw.
pub struct HeadCache<T> {
    last: std::collections::HashMap<PathBuf, (SystemTime, u64, T)>,
    this: std::collections::HashMap<PathBuf, (SystemTime, u64, T)>,
}

impl<T> Default for HeadCache<T> {
    fn default() -> Self {
        Self {
            last: Default::default(),
            this: Default::default(),
        }
    }
}

impl<T: Clone> HeadCache<T> {
    pub fn get(
        &mut self,
        path: &std::path::Path,
        modified: SystemTime,
        size: u64,
        read: impl FnOnce(&std::path::Path) -> T,
    ) -> T {
        let known = self
            .this
            .remove(path)
            .or_else(|| self.last.remove(path))
            .filter(|(m, s, _)| *m == modified && *s == size);
        let head = match known {
            Some((_, _, head)) => head,
            None => read(path),
        };
        self.this
            .insert(path.to_owned(), (modified, size, head.clone()));
        head
    }

    /// Ends a pass: files it did not see are forgotten.
    pub fn finish(&mut self) {
        self.last = std::mem::take(&mut self.this);
    }
}

pub fn head_records(path: &std::path::Path, limit: usize) -> Vec<serde_json::Value> {
    use std::io::BufRead;
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let reader = std::io::BufReader::new(file);
    let mut out = Vec::new();
    for line in reader.split(b'\n').take(limit) {
        let Ok(line) = line else { break };
        if line.len() > 4 * 1024 * 1024 {
            continue;
        }
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&line) {
            out.push(value);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::HeadCache;
    use std::time::{Duration, SystemTime};

    #[test]
    fn a_head_is_read_again_only_when_its_file_changed() {
        let mut heads = HeadCache::default();
        let (a, b) = (std::path::Path::new("/a"), std::path::Path::new("/b"));
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
        let reads = std::cell::Cell::new(0);
        let read = |n: u32| {
            let reads = &reads;
            move |_: &std::path::Path| {
                reads.set(reads.get() + 1);
                n
            }
        };
        assert_eq!(heads.get(a, t, 10, read(1)), 1);
        assert_eq!(heads.get(b, t, 10, read(2)), 2);
        heads.finish();
        assert_eq!(reads.get(), 2);
        // Unchanged: the head from the last pass, nothing read.
        assert_eq!(heads.get(a, t, 10, read(9)), 1);
        // Grown or touched: read again.
        assert_eq!(heads.get(b, t, 11, read(3)), 3);
        heads.finish();
        assert_eq!(reads.get(), 3);
        assert_eq!(heads.get(b, t + Duration::from_secs(1), 11, read(4)), 4);
        heads.finish();
        // `a` went unseen in the last pass, so it is forgotten.
        assert_eq!(heads.get(a, t, 10, read(5)), 5);
        assert_eq!(reads.get(), 5);
    }
}
