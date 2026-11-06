//! Disposable, source-bound projections. Never used for control or authorization.
use crate::model::Message;
use muster_proto::{Message as _, v1};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Include identity and change time: a same-sized replacement or an in-place
/// rewrite must not inherit the previous projection. Only plain JSONL sources
/// use this store; multi-file databases and compressed readers retain their own
/// authoritative read path.
pub(crate) fn stamp(source: &Path) -> Option<String> {
    let metadata = std::fs::metadata(source).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(format!(
            "{}:{}:{}:{}:{}:{}:{}",
            metadata.dev(),
            metadata.ino(),
            metadata.len(),
            metadata.mtime(),
            metadata.mtime_nsec(),
            metadata.ctime(),
            metadata.ctime_nsec()
        ))
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        None
    }
}

fn path(root: &Path, session: &str, source: &Path) -> PathBuf {
    // Parser/profile changes in a newly installed binary cannot reuse an old
    // normalized projection even when the provider source has not changed.
    static BINARY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let binary = BINARY.get_or_init(|| {
        std::env::current_exe()
            .ok()
            .and_then(|p| stamp(&p))
            .unwrap_or_default()
    });
    let mut hash = Sha256::new();
    for part in [
        session.as_bytes(),
        source.as_os_str().as_encoded_bytes(),
        binary.as_bytes(),
    ] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part);
    }
    root.join(format!("{}.sqlite", hex::encode(hash.finalize())))
}

pub(crate) fn write(
    root: &Path,
    session: &str,
    source: &Path,
    expected: &str,
    messages: &[Message],
) -> Option<()> {
    if stamp(source).as_deref() != Some(expected) {
        return None;
    }
    let mut directory = std::fs::DirBuilder::new();
    directory.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        directory.mode(0o700);
    }
    directory.create(root).ok()?;
    let file = path(root, session, source);
    // Pre-create privately rather than relying on the process umask.
    let mut options = std::fs::OpenOptions::new();
    options.create(true).write(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(&file).ok()?;
    let mut db = Connection::open(&file).ok()?;
    db.busy_timeout(std::time::Duration::from_secs(1)).ok()?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS metadata (version INTEGER, stamp TEXT); CREATE TABLE IF NOT EXISTS messages (position INTEGER PRIMARY KEY, id TEXT, body BLOB); CREATE INDEX IF NOT EXISTS message_ids ON messages(id,position);").ok()?;
    let existing: Option<String> = db
        .query_row("SELECT stamp FROM metadata WHERE version=1", [], |row| {
            row.get(0)
        })
        .optional()
        .ok()?;
    if existing.as_deref() == Some(expected) {
        return Some(());
    }
    let tx = db.transaction().ok()?;
    tx.execute_batch("DELETE FROM metadata; DELETE FROM messages;")
        .ok()?;
    {
        let mut insert = tx.prepare("INSERT INTO messages VALUES (?1,?2,?3)").ok()?;
        for (position, message) in messages.iter().enumerate() {
            insert
                .execute(params![
                    position as i64,
                    message.id,
                    message.to_proto().encode_to_vec()
                ])
                .ok()?;
        }
    }
    if stamp(source).as_deref() != Some(expected) {
        return None;
    }
    tx.execute("INSERT INTO metadata VALUES (1,?1)", [expected])
        .ok()?;
    tx.commit().ok()?;
    drop(db);
    prune(root, 128 * 1024 * 1024);
    Some(())
}

fn prune(root: &Path, budget: u64) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut files: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "sqlite") {
                return None;
            }
            let meta = entry.metadata().ok()?;
            Some((meta.modified().ok()?, meta.len(), path))
        })
        .collect();
    files.sort_by_key(|(modified, _, _)| *modified);
    let mut total: u64 = files.iter().map(|(_, size, _)| size).sum();
    for (_, size, file) in files {
        if total <= budget {
            break;
        }
        if std::fs::remove_file(file).is_ok() {
            total -= size;
        }
    }
}

pub(crate) fn read(
    root: &Path,
    source: &Path,
    request: &v1::AgentMessagesRequest,
) -> Option<(v1::AgentMessagePage, String)> {
    // Catch-up proofs go to the live reader and its generation-scoped hash index.
    if !request.after_id.is_empty() {
        return None;
    }
    let expected = stamp(source)?;
    let mut db = Connection::open_with_flags(
        path(root, &request.session, source),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .ok()?;
    let tx = db.transaction().ok()?;
    let saved: String = tx
        .query_row("SELECT stamp FROM metadata WHERE version=1", [], |row| {
            row.get(0)
        })
        .ok()?;
    if saved != expected {
        return None;
    }
    let end: i64 = if request.before_id.is_empty() {
        tx.query_row(
            "SELECT COALESCE(MAX(position)+1,0) FROM messages",
            [],
            |r| r.get(0),
        )
        .ok()?
    } else {
        tx.query_row(
            "SELECT position FROM messages WHERE id=?1 ORDER BY position LIMIT 1",
            [&request.before_id],
            |r| r.get(0),
        )
        .optional()
        .ok()??
    };
    let count = if request.tail == 0 {
        50
    } else {
        request.tail.min(2000)
    } as i64;
    let start = (end - count).max(0);
    let messages = {
        let mut query = tx
            .prepare(
                "SELECT body FROM messages WHERE position>=?1 AND position<?2 ORDER BY position",
            )
            .ok()?;
        query
            .query_map(params![start, end], |row| row.get::<_, Vec<u8>>(0))
            .ok()?
            .map(|row| v1::AgentMessage::decode(row.ok()?.as_slice()).ok())
            .collect::<Option<Vec<_>>>()?
    };
    if messages.len() != (end - start) as usize || stamp(source).as_deref() != Some(&expected) {
        return None;
    }
    Some((
        v1::AgentMessagePage {
            session: request.session.clone(),
            messages,
            has_more: start > 0,
            ..Default::default()
        },
        expected,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pages_are_bounded_and_rewrites_invalidate() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("session.jsonl");
        std::fs::write(&source, "initial").unwrap();
        let messages: Vec<_> = (0..1000)
            .map(|i| Message {
                id: format!("m{i}"),
                role: crate::model::Role::User,
                kind: crate::model::MessageKind::Text,
                text: String::new(),
                ts_unix: 0,
                usage: None,
                phase: String::new(),
                tool_name: String::new(),
                tool_call_id: String::new(),
                tool_status: String::new(),
            })
            .collect();
        let saved = stamp(&source).unwrap();
        write(&dir.path().join("index"), "s", &source, &saved, &messages).unwrap();
        let mut request = v1::AgentMessagesRequest {
            session: "s".into(),
            tail: 10,
            ..Default::default()
        };
        let (page, _) = read(&dir.path().join("index"), &source, &request).unwrap();
        assert_eq!(page.messages.len(), 10);
        assert_eq!(page.messages[0].id, "m990");
        assert!(page.has_more);
        request.before_id = "m990".into();
        assert_eq!(
            read(&dir.path().join("index"), &source, &request)
                .unwrap()
                .0
                .messages[0]
                .id,
            "m980"
        );
        request.before_id = "missing".into();
        assert!(read(&dir.path().join("index"), &source, &request).is_none());
        request.before_id.clear();
        std::fs::write(&source, "changed").unwrap(); // same length, same inode
        assert!(read(&dir.path().join("index"), &source, &request).is_none());
        assert!(write(&dir.path().join("index"), "s", &source, &saved, &messages).is_none());
    }
}
