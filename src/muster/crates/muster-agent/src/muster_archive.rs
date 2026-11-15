//! Muster's own archive for CLI-started sessions (ADR 0212). A session the
//! provider's desktop app does not list cannot be archived through that app
//! (ADR 0111), and its provider store is never written. Muster keeps the ids
//! it was asked to archive, with the time, in its own file and leaves those
//! sessions out of its catalog until the provider records newer activity.
use crate::Error;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Bounds the file; the oldest archives are forgotten first.
const MAX_ENTRIES: usize = 10_000;

#[derive(Default)]
pub(crate) struct Archive {
    path: Option<PathBuf>,
    /// Session id → the activity time (unix seconds) it was archived at.
    entries: Mutex<HashMap<String, u64>>,
}

impl Archive {
    /// `None` keeps the archive in memory (tests, hosts without a data dir).
    pub(crate) fn open(path: Option<PathBuf>) -> Self {
        let entries = path
            .as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|bytes| serde_json::from_slice::<HashMap<String, u64>>(&bytes).ok())
            .unwrap_or_default();
        Self {
            path,
            entries: Mutex::new(entries),
        }
    }

    /// Archived, and the provider has recorded nothing since.
    pub(crate) fn hides(&self, id: &str, updated_unix: u64) -> bool {
        self.entries
            .lock()
            .unwrap()
            .get(id)
            .is_some_and(|&at| updated_unix <= at)
    }

    pub(crate) fn archive(&self, id: &str, updated_unix: u64) -> Result<(), Error> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut entries = self.entries.lock().unwrap();
        let mut next = entries.clone();
        next.insert(id.to_owned(), now.max(updated_unix));
        if next.len() > MAX_ENTRIES {
            let mut by_age: Vec<_> = next.iter().map(|(k, v)| (*v, k.clone())).collect();
            by_age.sort();
            for (_, id) in by_age.into_iter().take(next.len() - MAX_ENTRIES) {
                next.remove(&id);
            }
        }
        if let Some(path) = &self.path {
            let bytes = serde_json::to_vec(&next).map_err(|e| Error::Io(e.to_string()))?;
            let tmp = path.with_extension("json.tmp");
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| Error::Io(e.to_string()))?;
            }
            std::fs::write(&tmp, bytes).map_err(|e| Error::Io(e.to_string()))?;
            std::fs::rename(&tmp, path).map_err(|e| Error::Io(e.to_string()))?;
        }
        *entries = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_persists_and_yields_to_newer_activity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.json");
        let archive = Archive::open(Some(path.clone()));
        assert!(!archive.hides("s", 10));
        archive.archive("s", 10).unwrap();
        assert!(archive.hides("s", 10));

        let reopened = Archive::open(Some(path));
        assert!(reopened.hides("s", 10), "the archive survives a restart");
        let at = *reopened.entries.lock().unwrap().get("s").unwrap();
        assert!(
            !reopened.hides("s", at + 1),
            "activity after the archive brings the session back"
        );
        assert!(!reopened.hides("other", 0));
    }
}
