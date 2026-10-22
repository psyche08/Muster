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
