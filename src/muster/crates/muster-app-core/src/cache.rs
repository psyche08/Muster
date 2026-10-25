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
            let path = self.root.join(key);
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
        if let Err(error) = write_entry(&self.root, &key, &bytes, true) {
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
