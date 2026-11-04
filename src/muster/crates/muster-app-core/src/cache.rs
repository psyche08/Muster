//! Display cache, drafts and pending investigation requests. No permissions,
//! approvals or trusted send outcomes persist; recovery always queries the daemon.
use muster_proto::Message;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

type Entries = HashMap<String, Arc<Vec<u8>>>;
const MAX_BYTES: usize = 64 * 1024 * 1024;
/// What the evictable part of the cache may keep on disk. Every session a
/// device ever opened left a transcript page here and nothing removed it, so
/// the directory grew with use and never shrank.
const DISK_BYTES: u64 = 64 * 1024 * 1024;
/// Durable entries are the owner's own unsent text. They are named apart so
/// pruning can never take one, and pruning also leaves anything small enough
/// to be a draft written before that naming existed.
const KEEP: &str = ".keep";
const DRAFT_BYTES: u64 = 32 * 1024;

pub(crate) struct Cache {
    root: PathBuf,
    memory: Mutex<Entries>,
    pending: Arc<Mutex<Entries>>,
    wake: Arc<tokio::sync::Notify>,
    disk: Arc<Mutex<()>>,
    worker: tokio::task::JoinHandle<()>,
}
impl Drop for Cache {
    fn drop(&mut self) {
        self.worker.abort();
    }
}
impl Cache {
    pub fn new(root: PathBuf) -> Self {
        let pending = Arc::new(Mutex::new(Entries::new()));
        let wake = Arc::new(tokio::sync::Notify::new());
        let disk = Arc::new(Mutex::new(()));
        let disk_worker = disk.clone();
        let (path, writes, notify) = (root.clone(), pending.clone(), wake.clone());
        let worker = tokio::spawn(async move {
            {
                let (path, disk) = (path.clone(), disk_worker.clone());
                let _ = tokio::task::spawn_blocking(move || {
                    let _disk = disk.lock().unwrap();
                    prune(&path, DISK_BYTES);
                })
                .await;
            }
            loop {
                notify.notified().await;
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                let path = path.clone();
                let writes = writes.clone();
                let disk = disk_worker.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    // Take the batch only after locking disk, so a delayed
                    // cache worker cannot overwrite a newer durable draft.
                    let _disk = disk.lock().unwrap();
                    let batch = std::mem::take(&mut *writes.lock().unwrap());
                    for (key, bytes) in batch {
                        if let Err(error) = write_entry(&path, &key, &bytes, false) {
                            log::warn!("Local cache write failed: {error}");
                            writes.lock().unwrap().entry(key).or_insert(bytes);
                        }
                    }
                })
                .await;
            }
        });
        Self {
            root,
            memory: Mutex::new(HashMap::new()),
            pending,
            wake,
            disk,
            worker,
        }
    }
    pub fn key(parts: &[&str]) -> String {
        // Length-delimited components prevent collisions between host/session scopes.
        let messages: Vec<_> = parts
            .iter()
            .map(|p| muster_proto::v1::AgentMessage {
                text: (*p).into(),
                ..Default::default()
            })
            .collect();
        hex::encode(muster_proto::history::prefix_hash(&messages))
    }
    fn insert(memory: &mut Entries, key: String, bytes: Arc<Vec<u8>>) {
        if memory.values().map(|v| v.len()).sum::<usize>() + bytes.len() > MAX_BYTES {
            memory.clear(); // Disk remains the backing store for evicted entries.
        }
        memory.insert(key, bytes);
    }
    pub fn get<T: Message + Default>(&self, key: &str) -> Option<T> {
        let mut memory = self.memory.lock().unwrap();
        if !memory.contains_key(key) {
            // A durable entry written before it had its own name is still read
            // from the plain one.
            let path = [self.root.join(format!("{key}{KEEP}")), self.root.join(key)]
                .into_iter()
                .find(|p| p.is_file())?;
            if std::fs::metadata(&path).ok()?.len() > MAX_BYTES as u64 {
                return None;
            }
            Self::insert(&mut memory, key.into(), Arc::new(std::fs::read(path).ok()?));
        }
        T::decode(memory.get(key)?.as_slice()).ok()
    }
    /// Small user-authored drafts must survive navigation and immediate exit.
    /// Failed writes remain queued and in memory, with the error surfaced by UI.
    pub fn put_durable<T: Message>(&self, key: String, value: &T) -> std::io::Result<()> {
        if value.encoded_len() > MAX_BYTES {
            return Err(std::io::Error::other("local draft exceeds cache limit"));
        }
        let bytes = Arc::new(value.encode_to_vec());
        let _disk = self.disk.lock().unwrap();
        Self::insert(&mut self.memory.lock().unwrap(), key.clone(), bytes.clone());
        self.pending.lock().unwrap().remove(&key);
        if let Err(error) = write_entry(&self.root, &format!("{key}{KEEP}"), &bytes, true) {
            self.pending.lock().unwrap().insert(key, bytes);
            self.wake.notify_one();
            return Err(error);
        }
        Ok(())
    }
    /// Capture a submitted input buffer for receipt matching without writing it.
    /// Navigation, not typing or submitting, is the draft persistence boundary.
    pub fn remember<T: Message>(&self, key: String, value: &T) {
        let _disk = self.disk.lock().unwrap();
        self.pending.lock().unwrap().remove(&key);
        Self::insert(
            &mut self.memory.lock().unwrap(),
            key,
            Arc::new(value.encode_to_vec()),
        );
    }
    pub fn put<T: Message>(&self, key: String, value: &T) {
        if value.encoded_len() > MAX_BYTES {
            return;
        }
        let bytes = Arc::new(value.encode_to_vec());
        let _disk = self.disk.lock().unwrap();
        let mut memory = self.memory.lock().unwrap();
        if memory.get(&key).is_some_and(|old| **old == *bytes) {
            return;
        }
        Self::insert(&mut memory, key.clone(), bytes.clone());
        self.pending.lock().unwrap().insert(key, bytes);
        self.wake.notify_one();
    }
}

/// Keeps the evictable cache within `budget`, oldest first. Durable entries and
/// anything small enough to be a draft from before durable naming are left
/// alone; so is a write still in flight, which carries the temporary suffix. A
/// dropped page costs one ordinary history fetch the next time it is opened.
fn prune(root: &std::path::Path, budget: u64) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut evictable = Vec::new();
    let mut kept = 0u64;
    for entry in entries.flatten() {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(KEEP) || name.ends_with(".tmp") || metadata.len() < DRAFT_BYTES {
            kept += metadata.len();
            continue;
        }
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        evictable.push((modified, metadata.len(), entry.path()));
    }
    let mut total: u64 = kept + evictable.iter().map(|(_, len, _)| len).sum::<u64>();
    if total <= budget {
        return;
    }
    evictable.sort_by_key(|(modified, _, _)| *modified);
    let mut removed = 0usize;
    for (_, len, path) in evictable {
        if total <= budget {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= len;
            removed += 1;
        }
    }
    if removed > 0 {
        log::info!("Local cache pruned {removed} entries to {total} bytes");
    }
}

fn write_entry(
    root: &std::path::Path,
    key: &str,
    bytes: &[u8],
    durable: bool,
) -> std::io::Result<()> {
    use std::io::Write;
    let mut directory = std::fs::DirBuilder::new();
    directory.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        directory.mode(0o700);
    }
    directory.create(root)?;
    let temporary = root.join(format!("{key}.tmp"));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut writer = options.open(&temporary)?;
    writer.write_all(bytes)?;
    if durable {
        writer.sync_all()?;
    }
    std::fs::rename(temporary, root.join(key))?;
    #[cfg(unix)]
    if durable {
        std::fs::File::open(root)?.sync_all()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use muster_proto::v1;
    #[tokio::test]
    async fn durable_edit_survives_immediate_exit_and_pending_old_writes() {
        let dir = tempfile::tempdir().unwrap();
        let key = Cache::key(&["draft", "host", "session"]);
        let cache = Cache::new(dir.path().into());
        let mut draft = muster_proto::app::ChatDraft {
            key: key.clone(),
            text: "old".into(),
            revision: "1".into(),
        };
        cache.put(key.clone(), &draft);
        draft.text = "每次输入都保存".into();
        draft.revision = "2".into();
        cache.put_durable(key.clone(), &draft).unwrap();
        assert_eq!(
            Cache::new(dir.path().into()).get::<muster_proto::app::ChatDraft>(&key),
            Some(draft.clone())
        );
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        drop(cache);
        assert_eq!(
            Cache::new(dir.path().into()).get::<muster_proto::app::ChatDraft>(&key),
            Some(draft.clone())
        );
        let cache = Cache::new(dir.path().into());
        draft.text = "last keystroke".into();
        cache.put_durable(key.clone(), &draft).unwrap();
        drop(cache); // No wait for the background cache worker.
        assert_eq!(
            Cache::new(dir.path().into()).get::<muster_proto::app::ChatDraft>(&key),
            Some(draft)
        );
    }

    #[test]
    fn pruning_bounds_pages_oldest_first_and_never_takes_the_owner_s_own_text() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let page = vec![b'p'; 64 * 1024];
        let write = |name: &str, bytes: &[u8], age_secs: u64| {
            let path = root.join(name);
            std::fs::write(&path, bytes).unwrap();
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(
                    std::time::UNIX_EPOCH
                        + std::time::Duration::from_secs(1_700_000_000 - age_secs),
                ))
                .unwrap();
        };
        for i in 0..6 {
            write(&format!("page{i}"), &page, 100 - i);
        }
        write(&format!("draft{KEEP}"), &page, 1000); // Durable and oldest.
        write("legacy-draft", b"unsent text", 1000); // Small and oldest.
        write("inflight.tmp", &page, 1000);
        prune(root, 4 * 64 * 1024);
        let left: std::collections::BTreeSet<_> = std::fs::read_dir(root)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        // The oldest pages go first and the newest is the last to go. What is
        // kept counts against the budget, so how many pages go with them
        // follows from their size, not from a fixed count.
        assert!(!left.contains("page0") && !left.contains("page1"));
        assert!(left.contains("page5"));
        assert!(left.contains(&format!("draft{KEEP}")));
        assert!(left.contains("legacy-draft"));
        assert!(left.contains("inflight.tmp"));
        let total: u64 = std::fs::read_dir(root)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy() != "inflight.tmp")
            .map(|e| e.metadata().unwrap().len())
            .sum();
        assert!(total <= 4 * 64 * 1024, "{total}");

        // A second pass has nothing left to do.
        let before = std::fs::read_dir(root).unwrap().count();
        prune(root, 4 * 64 * 1024);
        assert_eq!(std::fs::read_dir(root).unwrap().count(), before);
    }

    #[tokio::test]
    async fn a_pruned_root_still_serves_the_owner_s_durable_draft() {
        let dir = tempfile::tempdir().unwrap();
        let key = Cache::key(&["draft", "host", "session"]);
        let draft = muster_proto::app::ChatDraft {
            key: key.clone(),
            text: "unsent".into(),
            revision: "1".into(),
        };
        let cache = Cache::new(dir.path().into());
        cache.put_durable(key.clone(), &draft).unwrap();
        drop(cache);
        // Fill the root past its budget with pages, then reopen: starting the
        // cache prunes, and the draft survives it.
        for i in 0..((DISK_BYTES / (64 * 1024)) + 4) {
            std::fs::write(dir.path().join(format!("page{i}")), vec![b'p'; 64 * 1024]).unwrap();
        }
        let cache = Cache::new(dir.path().into());
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        assert_eq!(
            cache.get::<muster_proto::app::ChatDraft>(&key),
            Some(draft.clone())
        );
        assert!(dir.path().join(format!("{key}{KEEP}")).is_file());
        let pages = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("page"))
            .count();
        assert!(
            pages < usize::try_from(DISK_BYTES / (64 * 1024)).unwrap() + 4,
            "{pages}"
        );
    }

    #[tokio::test]
    async fn durable_write_failure_is_reported_and_preserves_in_memory_edit() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("not-a-directory");
        std::fs::write(&root, b"file").unwrap();
        let cache = Cache::new(root);
        let draft = muster_proto::app::ChatDraft {
            key: "draft".into(),
            text: "keep me".into(),
            revision: "1".into(),
        };
        assert!(cache.put_durable(draft.key.clone(), &draft).is_err());
        assert_eq!(
            cache.get::<muster_proto::app::ChatDraft>(&draft.key),
            Some(draft)
        );
    }

    #[tokio::test]
    async fn persists_newest_display_data_and_separates_network_host_session() {
        let dir = tempfile::tempdir().unwrap();
        let key = Cache::key(&["network", "host", "session"]);
        assert_ne!(key, Cache::key(&["network", "other-host", "session"]));
        assert_ne!(key, Cache::key(&["other-network", "host", "session"]));
        assert_ne!(Cache::key(&["ab", "c"]), Cache::key(&["a", "bc"]));
        let cache = Cache::new(dir.path().into());
        let mut page = v1::AgentMessagePage::default();
        for i in 0..20 {
            page.messages = vec![v1::AgentMessage {
                id: "reply".into(),
                text: i.to_string(),
                ..Default::default()
            }];
            cache.put(key.clone(), &page);
        }
        assert_eq!(cache.get::<v1::AgentMessagePage>(&key), Some(page.clone()));
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                if std::fs::read(dir.path().join(&key)).ok().as_deref()
                    == Some(page.encode_to_vec().as_slice())
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        let reopened = Cache::new(dir.path().into());
        assert_eq!(reopened.get::<v1::AgentMessagePage>(&key), Some(page));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(dir.path().join(&key))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        std::fs::write(dir.path().join("corrupt"), [0xff, 0xff]).unwrap();
        assert!(reopened.get::<v1::AgentMessagePage>("corrupt").is_none());
    }
}
