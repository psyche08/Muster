//! The app's log file. A desktop app opened from Finder or the Dock has
//! nowhere for stderr to go, so its log is kept as `app.log` beside the
//! daemon's `musterd.log` (ADR 0137).

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Past this size `app.log` becomes `app.log.1`, replacing the one before.
pub(crate) const LIMIT: u64 = 8 * 1024 * 1024;

/// `app.log`, appended to and kept to two files of about `limit` bytes.
/// Every record also goes to stderr, which a debugger's console shows.
pub(crate) struct LogFile {
    path: PathBuf,
    file: File,
    written: u64,
    limit: u64,
}

impl LogFile {
    pub(crate) fn open(dir: &Path, limit: u64) -> io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join("app.log");
        let file = append(&path)?;
        let written = file.metadata()?.len();
        Ok(Self {
            path,
            file,
            written,
            limit,
        })
    }

    fn rotate(&mut self) -> io::Result<()> {
        let mut previous = self.path.clone().into_os_string();
        previous.push(".1");
        match std::fs::rename(&self.path, previous) {
            // Removed from outside: start a new one all the same.
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
        self.file = append(&self.path)?;
        self.written = 0;
        Ok(())
    }
}

fn append(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

impl Write for LogFile {
    /// env_logger hands over one whole record per call, so a record is
    /// never split across the two files.
    fn write(&mut self, record: &[u8]) -> io::Result<usize> {
        let _ = io::stderr().write_all(record);
        if self.written > 0 && self.written + record.len() as u64 > self.limit {
            self.rotate()?;
        }
        self.file.write_all(record)?;
        self.written += record.len() as u64;
        Ok(record.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_keeps_two_files_and_whole_records() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("app.log"), "from the last launch\n").unwrap();
        let mut log = LogFile::open(dir.path(), 64).unwrap();
        for n in 0..10 {
            log.write_all(format!("record {n}: twenty bytes\n").as_bytes())
                .unwrap();
        }
        // Two 23-byte records fit in 64; the third starts a new file.
        let read = |name: &str| std::fs::read_to_string(dir.path().join(name)).unwrap();
        assert_eq!(read("app.log"), "record 9: twenty bytes\n");
        assert_eq!(
            read("app.log.1"),
            "record 7: twenty bytes\nrecord 8: twenty bytes\n"
        );
        assert!(!dir.path().join("app.log.2").exists());

        // Removed while the app runs: what is written goes nowhere until
        // the next rotation starts a new file.
        std::fs::remove_file(dir.path().join("app.log")).unwrap();
        for n in 10..13 {
            log.write_all(format!("record {n}: twenty bytes\n").as_bytes())
                .unwrap();
        }
        assert_eq!(
            read("app.log"),
            "record 11: twenty bytes\nrecord 12: twenty bytes\n"
        );
        assert_eq!(
            read("app.log.1"),
            "record 7: twenty bytes\nrecord 8: twenty bytes\n"
        );
    }
}
