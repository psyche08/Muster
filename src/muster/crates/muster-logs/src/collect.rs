//! Incremental collection: what each log file gained since the last upload
//! the collector acknowledged.
//!
//! A cursor names a file by stream and generation (its inode) and holds the
//! offset sent up to, so a rotated `app.log` (now `app.log.1`, same inode)
//! is finished where it was left and the new `app.log` starts at 0. Only
//! whole lines are sent; a line still being written waits for the next
//! upload. The first scan on a device starts each file at its tail instead
//! of its history, and a backlog beyond the per-upload limit sends its
//! newest part and reports the rest as skipped. Cursors move only when the
//! caller commits a scan, after the collector has answered.

use std::fs::{File, Metadata};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One log file and the stream it belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub stream: String,
    pub path: PathBuf,
}

impl Source {
    pub fn new(stream: &str, path: PathBuf) -> Self {
        Self {
            stream: stream.into(),
            path,
        }
    }
}

/// A desktop's log files (`muster_paths::log_dir`), older content of a
/// stream first: the app's rotated file before its current one.
pub fn default_sources(log_dir: &Path) -> Vec<Source> {
    vec![
        Source::new("musterd", log_dir.join("musterd.log")),
        Source::new("app", log_dir.join("app.log.1")),
        Source::new("app", log_dir.join("app.log")),
        Source::new("l4forwarder", log_dir.join("l4forwarder.log")),
    ]
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    pub stream: String,
    pub generation: u64,
    pub offset: u64,
}

/// The uploader's state file (`log-upload.json` in the state directory).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    /// `false` stops uploads on this device; the owner may set it here.
    #[serde(default = "enabled")]
    pub enabled: bool,
    /// Keys the redaction pseudonyms (hex, 32 bytes); never leaves the device.
    pub salt: String,
    /// Unix seconds of the last upload the collector answered.
    #[serde(default)]
    pub last_upload: Option<u64>,
    #[serde(default)]
    pub cursors: Vec<Cursor>,
    /// Whether a scan was ever committed: before that, files start at their tail.
    #[serde(default)]
    pub started: bool,
}

fn enabled() -> bool {
    true
}

impl State {
    pub fn fresh() -> Self {
        Self {
            enabled: true,
            salt: hex::encode(rand::random::<[u8; 32]>()),
            last_upload: None,
            cursors: Vec::new(),
            started: false,
        }
    }

    /// The saved state; a fresh one when there is none or it cannot be read
    /// (the next upload then starts again from the files' tails).
    pub fn load(path: &Path) -> Self {
        match std::fs::read(path) {
            Ok(bytes) => match serde_json::from_slice::<Self>(&bytes) {
                Ok(state) if state.salt_bytes().is_some() => state,
                Ok(_) | Err(_) => {
                    log::warn!("log upload: {} unreadable; starting over", path.display());
                    Self::fresh()
                }
            },
            Err(_) => Self::fresh(),
        }
    }

    /// Written whole and renamed into place, readable by the owner only.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let json = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        let mut temp = path.as_os_str().to_owned();
        temp.push(".tmp");
        let temp = PathBuf::from(temp);
        {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
            let mut file = options.open(&temp)?;
            io::Write::write_all(&mut file, &json)?;
            file.sync_all()?;
        }
        std::fs::rename(&temp, path)
    }

    pub fn salt_bytes(&self) -> Option<[u8; 32]> {
        hex::decode(&self.salt).ok()?.try_into().ok()
    }

    /// The collector has the scan's chunks (or refused them for good):
    /// files continue from where the scan ended.
    pub fn commit(&mut self, scan: &Scan, now: u64) {
        self.cursors = scan.cursors.clone();
        self.started = true;
        self.last_upload = Some(now);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// How much of a file's history the first scan sends.
    pub first_tail: u64,
    /// Bytes of one file in one upload at most.
    pub max_chunk: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            first_tail: 256 << 10,
            max_chunk: 2 << 20,
        }
    }
}

/// What one file gained: its bytes `start..end`, whole lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pending {
    pub stream: String,
    pub generation: u64,
    pub start: u64,
    pub end: u64,
    /// Bytes before `start` not sent: history, backlog, a partial first line.
    pub skipped: u64,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scan {
    pub pending: Vec<Pending>,
    /// Every present file's cursor once the pending bytes are sent; files
    /// that are gone drop out.
    pub cursors: Vec<Cursor>,
}

pub fn scan(state: &State, sources: &[Source], limits: &Limits) -> Scan {
    let mut scan = Scan::default();
    for source in sources {
        match read_source(source, state, limits) {
            Ok(Some((cursor, pending))) => {
                // The same file under two names (a rename mid-scan) is read once.
                if scan
                    .cursors
                    .iter()
                    .any(|c| c.stream == cursor.stream && c.generation == cursor.generation)
                {
                    continue;
                }
                scan.cursors.push(cursor);
                scan.pending.extend(pending);
            }
            Ok(None) => {}
            Err(error) => log::debug!("log upload: {}: {error}", source.path.display()),
        }
    }
    scan
}

#[cfg(unix)]
fn generation(meta: &Metadata) -> u64 {
    std::os::unix::fs::MetadataExt::ino(meta)
}

#[cfg(not(unix))]
fn generation(meta: &Metadata) -> u64 {
    meta.created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as u64)
}

fn read_source(
    source: &Source,
    state: &State,
    limits: &Limits,
) -> io::Result<Option<(Cursor, Option<Pending>)>> {
    let mut file = match File::open(&source.path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Ok(None);
    }
    let generation = generation(&meta);
    let size = meta.len();
    let known = state
        .cursors
        .iter()
        .find(|c| c.stream == source.stream && c.generation == generation);
    // `align`: `start` may be inside a line, whose rest is skipped.
    let (mut start, mut skipped, mut align) = match known {
        Some(cursor) if cursor.offset <= size => (cursor.offset, 0, false),
        // Shorter than what was sent: truncated in place, start over.
        Some(_) => (0, 0, false),
        None if !state.started => {
            let start = size.saturating_sub(limits.first_tail);
            (start, start, start > 0)
        }
        // Appeared since the last upload: all of it.
        None => (0, 0, false),
    };
    if size - start > limits.max_chunk {
        let newest = size - limits.max_chunk;
        skipped += newest - start;
        start = newest;
        align = true;
    }
    let cursor = |offset| Cursor {
        stream: source.stream.clone(),
        generation,
        offset,
    };
    if start == size {
        return Ok(Some((cursor(size), None)));
    }
    // Read one byte early when aligning: a newline there means `start`
    // already begins a line.
    let base = if align { start - 1 } else { start };
    file.seek(SeekFrom::Start(base))?;
    let mut buf = Vec::with_capacity((size - base) as usize);
    file.take(size - base).read_to_end(&mut buf)?;
    let begin = if align {
        match buf.iter().position(|&b| b == b'\n') {
            Some(at) => at + 1,
            None => buf.len(),
        }
    } else {
        0
    };
    let end = match buf[begin..].iter().rposition(|&b| b == b'\n') {
        Some(at) => begin + at + 1,
        // One line longer than the limit is sent as it is, or it never would be.
        None if (buf.len() - begin) as u64 >= limits.max_chunk => buf.len(),
        None => begin,
    };
    let from = base + begin as u64;
    skipped += from - start;
    if end == begin {
        return Ok(Some((cursor(from), None)));
    }
    let to = base + end as u64;
    buf.truncate(end);
    buf.drain(..begin);
    Ok(Some((
        cursor(to),
        Some(Pending {
            stream: source.stream.clone(),
            generation,
            start: from,
            end: to,
            skipped,
            bytes: buf,
        }),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn append(path: &Path, text: &str) {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
    }

    fn sent(scan: &Scan) -> Vec<(String, String)> {
        scan.pending
            .iter()
            .map(|p| {
                (
                    p.stream.clone(),
                    String::from_utf8(p.bytes.clone()).unwrap(),
                )
            })
            .collect()
    }

    fn limits() -> Limits {
        Limits {
            first_tail: 10,
            max_chunk: 1000,
        }
    }

    #[test]
    fn first_scan_sends_the_tail_from_a_line_start_then_only_growth() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("musterd.log");
        append(&log, "old line one\nold two\nnew\n");
        let sources = default_sources(dir.path());
        let mut state = State::fresh();
        let first = scan(&state, &sources, &limits());
        // The last 10 bytes start inside "old two"; the whole line after it goes.
        assert_eq!(sent(&first), [("musterd".into(), "new\n".into())]);
        assert_eq!(first.pending[0].skipped, 21);
        state.commit(&first, 1);
        assert!(scan(&state, &sources, &limits()).pending.is_empty());
        append(&log, "next\npartial");
        let second = scan(&state, &sources, &limits());
        assert_eq!(sent(&second), [("musterd".into(), "next\n".into())]);
        assert_eq!(second.pending[0].skipped, 0);
        state.commit(&second, 2);
        append(&log, " done\n");
        assert_eq!(
            sent(&scan(&state, &sources, &limits())),
            [("musterd".into(), "partial done\n".into())]
        );
    }

    #[test]
    fn a_tail_starting_on_a_line_boundary_keeps_that_line() {
        let dir = tempfile::tempdir().unwrap();
        append(&dir.path().join("app.log"), "0123456789a\n987654321\n");
        let first = scan(&State::fresh(), &default_sources(dir.path()), &limits());
        assert_eq!(sent(&first), [("app".into(), "987654321\n".into())]);
    }

    #[test]
    fn a_rotated_file_is_finished_before_the_new_one_starts_at_zero() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("app.log");
        append(&log, "a\n");
        let sources = default_sources(dir.path());
        let mut state = State::fresh();
        let first = scan(&state, &sources, &limits());
        state.commit(&first, 1);
        append(&log, "b\n");
        std::fs::rename(&log, dir.path().join("app.log.1")).unwrap();
        append(&log, "c\n");
        let after = scan(&state, &sources, &limits());
        assert_eq!(
            sent(&after),
            [("app".into(), "b\n".into()), ("app".into(), "c\n".into())]
        );
        assert_eq!(after.pending[0].start, 2);
        assert_eq!(after.pending[1].start, 0);
        state.commit(&after, 2);
        // The previous generation is replaced on the next rotation and drops out.
        std::fs::rename(&log, dir.path().join("app.log.1")).unwrap();
        let next = scan(&state, &sources, &limits());
        assert!(next.pending.is_empty());
        assert_eq!(next.cursors.len(), 1);
    }

    #[test]
    fn a_truncated_file_starts_over() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("musterd.log");
        append(&log, "a\nb\n");
        let sources = default_sources(dir.path());
        let mut state = State::fresh();
        let first = scan(&state, &sources, &limits());
        state.commit(&first, 1);
        std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&log)
            .unwrap();
        append(&log, "c\n");
        assert_eq!(
            sent(&scan(&state, &sources, &limits())),
            [("musterd".into(), "c\n".into())]
        );
    }

    #[test]
    fn a_backlog_beyond_the_limit_sends_its_newest_lines() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("musterd.log");
        let sources = default_sources(dir.path());
        let mut state = State::fresh();
        let first = scan(&state, &sources, &limits());
        state.commit(&first, 1);
        let lines: String = (0..100).map(|i| format!("line {i:04}\n")).collect();
        append(&log, &lines);
        let small = Limits {
            first_tail: 10,
            max_chunk: 25,
        };
        let backlog = scan(&state, &sources, &small);
        assert_eq!(
            sent(&backlog),
            [("musterd".into(), "line 0098\nline 0099\n".into())]
        );
        assert_eq!(backlog.pending[0].skipped, 980);
        assert_eq!(backlog.pending[0].end, 1000);
    }

    #[test]
    fn cursors_move_only_on_commit() {
        let dir = tempfile::tempdir().unwrap();
        append(&dir.path().join("musterd.log"), "a\n");
        let sources = default_sources(dir.path());
        let mut state = State::fresh();
        state.started = true;
        let first = scan(&state, &sources, &limits());
        assert_eq!(sent(&first), [("musterd".into(), "a\n".into())]);
        // Not committed (the upload failed): the same bytes again.
        assert_eq!(scan(&state, &sources, &limits()), first);
    }

    #[test]
    fn state_round_trips_and_a_damaged_file_starts_over() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log-upload.json");
        let mut state = State::fresh();
        state.cursors.push(Cursor {
            stream: "app".into(),
            generation: 3,
            offset: 9,
        });
        state.save(&path).unwrap();
        assert_eq!(State::load(&path), state);
        #[cfg(unix)]
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(
                &std::fs::metadata(&path).unwrap().permissions()
            ) & 0o777,
            0o600
        );
        std::fs::write(&path, b"{").unwrap();
        let fresh = State::load(&path);
        assert!(fresh.cursors.is_empty() && fresh.enabled && fresh.salt_bytes().is_some());
        std::fs::write(&path, br#"{"enabled": false, "salt": "00"}"#).unwrap();
        assert!(State::load(&path).enabled, "a bad salt is a damaged file");
        let off = format!(r#"{{"enabled": false, "salt": "{}"}}"#, "00".repeat(32));
        std::fs::write(&path, off).unwrap();
        assert!(!State::load(&path).enabled);
    }
}
