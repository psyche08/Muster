//! musterd's durable state (ARCHITECTURE §3.8): one SQLite file holding the
//! device identity, every roster entry, the local policy, small settings,
//! and the operation ledger that makes mutations idempotent.
//!
//! Encryption at rest with a hardware-wrapped key is not in yet: under the
//! decided threat model (same-UID malware out of scope) a plain file is
//! acceptable for now, and the API does not expose the choice.

use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use muster_core::policy::LocalPolicy;
use muster_core::roster::{Entry, Roster};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Core(#[from] muster_core::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS roster_entries (
    seq   INTEGER PRIMARY KEY AUTOINCREMENT,
    hash  BLOB NOT NULL UNIQUE,
    bytes BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS presence_bindings (
    device TEXT PRIMARY KEY,
    binding BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS ops (
    op_id     TEXT PRIMARY KEY,
    kind      TEXT NOT NULL,
    body_hash BLOB NOT NULL,
    status    TEXT NOT NULL,          -- pending | applied | rejected | unknown
    result    BLOB,
    created   INTEGER NOT NULL,
    updated   INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS kv (
    key   TEXT PRIMARY KEY,
    value BLOB NOT NULL
);
"#;

pub struct Store {
    conn: Mutex<Connection>,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Store")
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Other(format!("create {}: {e}", parent.display())))?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;",
        )?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn open_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(SCHEMA)?;
        // A crash mid-operation leaves pending rows; their outcome is now
        // undecidable and must never be retried automatically.
        conn.execute(
            "UPDATE ops SET status = 'unknown', updated = ?1 WHERE status = 'pending'",
            params![now()],
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    // ------------------------------------------------------------ meta / kv

    pub fn meta(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn set_meta(&self, key: &str, value: &[u8]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value", params![key, value])?;
        Ok(())
    }

    pub fn meta_string(&self, key: &str) -> Result<Option<String>> {
        Ok(self.meta(key)?.and_then(|v| String::from_utf8(v).ok()))
    }

    pub fn kv_get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row("SELECT value FROM kv WHERE key = ?1", params![key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    pub fn kv_set(&self, key: &str, value: &[u8]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("INSERT INTO kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value", params![key, value])?;
        Ok(())
    }

    // ---------------------------------------------------------------- roster

    /// Appends entries not yet stored; the roster is append-only so nothing
    /// is ever updated or deleted here.
    pub fn append_entries(&self, entries: &[Entry]) -> Result<usize> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let mut added = 0;
        for entry in entries {
            let hash = entry.hash();
            let changed = tx.execute(
                "INSERT OR IGNORE INTO roster_entries (hash, bytes) VALUES (?1, ?2)",
                params![hash.0.as_slice(), entry.encode()],
            )?;
            added += changed;
        }
        tx.commit()?;
        Ok(added)
    }

    pub fn load_entries(&self) -> Result<Vec<Entry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT bytes FROM roster_entries ORDER BY seq")?;
        let rows = stmt.query_map([], |row| row.get::<_, Vec<u8>>(0))?;
        let mut entries = Vec::new();
        for bytes in rows {
            entries.push(Entry::decode(&bytes?)?);
        }
        Ok(entries)
    }

    /// The roster as stored, or an empty one before any network exists.
    pub fn load_roster(&self) -> Result<Roster> {
        let entries = self.load_entries()?;
        if entries.is_empty() {
            return Ok(Roster::new());
        }
        let mut roster = Roster::from_entries(entries)?;
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT binding FROM presence_bindings")?;
        for bytes in stmt.query_map([], |row| row.get::<_, Vec<u8>>(0))? {
            if let Ok(binding) = muster_core::presence::Binding::decode(&bytes?) {
                // Revoked/old-network bindings are inert and never resurrected.
                let _ = roster.install_presence_binding(binding);
            }
        }
        Ok(roster)
    }

    pub fn save_presence_binding(&self, binding: &muster_core::presence::Binding) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO presence_bindings(device,binding) VALUES (?1,?2) ON CONFLICT(device) DO UPDATE SET binding=excluded.binding",
            params![binding.device.to_hex(), binding.encode()],
        )?;
        Ok(())
    }

    #[cfg(test)]
    pub fn has_entry(&self, hash: &muster_core::roster::EntryHash) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM roster_entries WHERE hash = ?1",
            params![hash.0.as_slice()],
            |row| row.get(0),
        )?;
        Ok(n > 0)
    }

    /// Forgets the network entirely (a clean switch, never a partial edit).
    pub fn clear_roster(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM roster_entries", [])?;
        Ok(())
    }

    // ---------------------------------------------------------------- policy

    pub fn policy(&self) -> Result<LocalPolicy> {
        match self.meta("policy")? {
            Some(bytes) => Ok(serde_json::from_slice(&bytes)?),
            None => Ok(LocalPolicy::default()),
        }
    }

    pub fn set_policy(&self, policy: &LocalPolicy) -> Result<()> {
        self.set_meta("policy", &serde_json::to_vec(policy)?)
    }

    // ---------------------------------------------------------------- ledger

    pub fn ledger(&self) -> Ledger<'_> {
        Ledger { store: self }
    }
}

/// What the ledger says about an operation the client wants to run.
#[derive(Debug, PartialEq, Eq)]
pub enum Begin {
    /// Run it; call `complete` (or `reject`) afterwards.
    Fresh,
    /// Same op_id, same body: here is the earlier result.
    Duplicate(Option<Vec<u8>>),
    /// Same op_id, different body.
    Conflict,
    /// Still pending, or a crash made the outcome undecidable.
    Unknown,
}

/// Read-only outcome of an exactly bound request. Absence never authorizes replay.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Unknown,
    Conflict,
    Applied(Vec<u8>),
    Rejected(Vec<u8>),
}

/// Exactly-once mutations (ARCHITECTURE §3.6.1): a pending row is written
/// before any side effect; the same op_id returns the recorded result; a
/// changed body is a conflict; a crash leaves "unknown", never a retry.
pub struct Ledger<'a> {
    store: &'a Store,
}

impl Ledger<'_> {
    pub fn body_hash(body: &[u8]) -> [u8; 32] {
        Sha256::digest(body).into()
    }

    pub fn begin(&self, op_id: &str, kind: &str, body: &[u8]) -> Result<Begin> {
        let hash = Self::body_hash(body);
        let mut conn = self.store.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let existing: Option<(Vec<u8>, String, Option<Vec<u8>>)> = tx
            .query_row(
                "SELECT body_hash, status, result FROM ops WHERE op_id = ?1",
                params![op_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let outcome = match existing {
            None => {
                tx.execute(
                    "INSERT INTO ops (op_id, kind, body_hash, status, created, updated) VALUES (?1, ?2, ?3, 'pending', ?4, ?4)",
                    params![op_id, kind, hash.as_slice(), now()],
                )?;
                Begin::Fresh
            }
            Some((stored_hash, _, _)) if stored_hash != hash => Begin::Conflict,
            Some((_, status, result)) => match status.as_str() {
                "applied" | "rejected" => Begin::Duplicate(result),
                _ => Begin::Unknown,
            },
        };
        tx.commit()?;
        Ok(outcome)
    }

    /// Provider evidence resolves only the exact original operation. Repeated
    /// confirmations are harmless; a rejected or differently bound row is never changed.
    pub fn confirm(&self, op_id: &str, kind: &str, hash: &[u8; 32], result: &[u8]) -> Result<()> {
        let conn = self.store.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE ops SET status = 'applied', result = ?4, updated = ?5 WHERE op_id = ?1 AND kind = ?2 AND body_hash = ?3 AND (status IN ('pending', 'unknown') OR (status = 'applied' AND result = ?4))",
            params![op_id, kind, hash.as_slice(), result, now()],
        )?;
        if changed == 0 {
            return Err(Error::Other(format!(
                "operation {op_id} cannot be confirmed with this evidence"
            )));
        }
        Ok(())
    }

    pub fn complete(&self, op_id: &str, result: &[u8]) -> Result<()> {
        self.finish(op_id, "applied", result)
    }

    pub fn reject(&self, op_id: &str, result: &[u8]) -> Result<()> {
        self.finish(op_id, "rejected", result)
    }

    fn finish(&self, op_id: &str, status: &str, result: &[u8]) -> Result<()> {
        let conn = self.store.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE ops SET status = ?2, result = ?3, updated = ?4 WHERE op_id = ?1 AND status = 'pending'",
            params![op_id, status, result, now()],
        )?;
        if changed == 0 {
            return Err(Error::Other(format!("operation {op_id} is not pending")));
        }
        Ok(())
    }

    /// Read-only receipt lookup. Never inserts, retries, or confirms a row;
    /// callers must supply the exact original kind and request hash.
    pub fn confirmed_result(
        &self,
        op_id: &str,
        kind: &str,
        hash: &[u8; 32],
    ) -> Result<Option<Vec<u8>>> {
        let conn = self.store.conn.lock().unwrap();
        Ok(conn.query_row(
            "SELECT result FROM ops WHERE op_id = ?1 AND kind = ?2 AND body_hash = ?3 AND status = 'applied'",
            params![op_id, kind, hash.as_slice()],
            |row| row.get(0),
        ).optional()?.flatten())
    }

    pub fn outcome(&self, op_id: &str, kind: &str, body: &[u8]) -> Result<Outcome> {
        let conn = self.store.conn.lock().unwrap();
        type StoredOutcome = (String, Vec<u8>, String, Option<Vec<u8>>);
        let row: Option<StoredOutcome> = conn
            .query_row(
                "SELECT kind, body_hash, status, result FROM ops WHERE op_id = ?1",
                params![op_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        Ok(match row {
            None => Outcome::Unknown,
            Some((stored_kind, hash, _, _))
                if stored_kind != kind || hash != Self::body_hash(body) =>
            {
                Outcome::Conflict
            }
            Some((_, _, status, result)) if status == "applied" => {
                Outcome::Applied(result.unwrap_or_default())
            }
            Some((_, _, status, result)) if status == "rejected" => {
                Outcome::Rejected(result.unwrap_or_default())
            }
            _ => Outcome::Unknown,
        })
    }

    pub fn status(&self, op_id: &str) -> Result<Option<String>> {
        let conn = self.store.conn.lock().unwrap();
        Ok(conn
            .query_row(
                "SELECT status FROM ops WHERE op_id = ?1",
                params![op_id],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Whether any operation is undecided.
    #[cfg(test)]
    pub fn has_unknown(&self, kind: &str) -> Result<bool> {
        let conn = self.store.conn.lock().unwrap();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM ops WHERE kind = ?1 AND status = 'unknown'",
            params![kind],
            |row| row.get(0),
        )?;
        Ok(n > 0)
    }

    pub fn prune(&self, older_than_secs: i64) -> Result<usize> {
        let conn = self.store.conn.lock().unwrap();
        Ok(conn.execute(
            "DELETE FROM ops WHERE status IN ('applied', 'rejected') AND updated < ?1",
            params![now() - older_than_secs],
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muster_core::roster::{Author, DeviceKind, DeviceRecord};
    use muster_core::{HwSigner, SoftSigner};
    use std::net::Ipv4Addr;

    #[test]
    fn outcome_lookup_never_creates_or_changes_rows_and_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store.db");
        let store = Store::open(&path).unwrap();
        assert_eq!(
            store.ledger().outcome("op", "agent.send", b"body").unwrap(),
            Outcome::Unknown
        );
        assert_eq!(store.ledger().status("op").unwrap(), None);
        store.ledger().begin("op", "agent.send", b"body").unwrap();
        assert_eq!(
            store
                .ledger()
                .outcome("op", "agent.respond", b"body")
                .unwrap(),
            Outcome::Conflict
        );
        assert_eq!(
            store
                .ledger()
                .outcome("op", "agent.send", b"changed")
                .unwrap(),
            Outcome::Conflict
        );
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(
            store.ledger().outcome("op", "agent.send", b"body").unwrap(),
            Outcome::Unknown
        );
        store
            .ledger()
            .confirm("op", "agent.send", &Ledger::body_hash(b"body"), b"session")
            .unwrap();
        assert_eq!(
            store.ledger().outcome("op", "agent.send", b"body").unwrap(),
            Outcome::Applied(b"session".to_vec())
        );
        store
            .ledger()
            .begin("rejected", "agent.send", b"body")
            .unwrap();
        store.ledger().reject("rejected", b"not sent").unwrap();
        assert_eq!(
            store
                .ledger()
                .outcome("rejected", "agent.send", b"body")
                .unwrap(),
            Outcome::Rejected(b"not sent".to_vec())
        );
    }

    #[test]
    fn provider_confirmation_is_bound_idempotent_and_can_resolve_unknown() {
        let store = Store::open_memory().unwrap();
        let ledger = store.ledger();
        let hash = Ledger::body_hash(b"body");
        ledger.begin("send", "agent.send", b"body").unwrap();
        assert!(
            ledger
                .confirm("send", "agent.respond", &hash, b"session")
                .is_err()
        );
        assert!(
            ledger
                .confirm(
                    "send",
                    "agent.send",
                    &Ledger::body_hash(b"other"),
                    b"session"
                )
                .is_err()
        );
        assert_eq!(ledger.status("send").unwrap().as_deref(), Some("pending"));
        store
            .conn
            .lock()
            .unwrap()
            .execute("UPDATE ops SET status = 'unknown' WHERE op_id = 'send'", [])
            .unwrap();
        ledger
            .confirm("send", "agent.send", &hash, b"session")
            .unwrap();
        ledger
            .confirm("send", "agent.send", &hash, b"session")
            .unwrap();
        assert!(
            ledger
                .confirm("send", "agent.send", &hash, b"other session")
                .is_err()
        );
        assert_eq!(
            ledger.begin("send", "agent.send", b"body").unwrap(),
            Begin::Duplicate(Some(b"session".to_vec()))
        );
        ledger.begin("rejected", "agent.send", b"body").unwrap();
        ledger.reject("rejected", b"not sent").unwrap();
        assert!(
            ledger
                .confirm("rejected", "agent.send", &hash, b"session")
                .is_err()
        );
        assert_eq!(
            ledger.status("rejected").unwrap().as_deref(),
            Some("rejected")
        );
    }

    #[test]
    fn receipt_lookup_never_creates_or_accepts_other_bindings() {
        let store = Store::open_memory().unwrap();
        let ledger = store.ledger();
        let hash = Ledger::body_hash(b"request");
        assert!(
            ledger
                .confirmed_result("op", "agent.send", &hash)
                .unwrap()
                .is_none()
        );
        assert!(ledger.status("op").unwrap().is_none());
        ledger.begin("op", "agent.send", b"request").unwrap();
        assert!(
            ledger
                .confirmed_result("op", "agent.send", &hash)
                .unwrap()
                .is_none()
        );
        ledger
            .confirm("op", "agent.send", &hash, b"session")
            .unwrap();
        assert_eq!(
            ledger.confirmed_result("op", "agent.send", &hash).unwrap(),
            Some(b"session".to_vec())
        );
        assert!(
            ledger
                .confirmed_result("op", "agent.respond", &hash)
                .unwrap()
                .is_none()
        );
        assert!(
            ledger
                .confirmed_result("op", "agent.send", &[0; 32])
                .unwrap()
                .is_none()
        );
        ledger.begin("rejected", "agent.send", b"request").unwrap();
        ledger.reject("rejected", b"no").unwrap();
        assert!(
            ledger
                .confirmed_result("rejected", "agent.send", &hash)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn roster_round_trip_and_policy() {
        let store = Store::open_memory().unwrap();
        assert!(store.load_roster().unwrap().is_empty());
        let signer = SoftSigner::generate();
        let mut roster = Roster::new();
        let genesis = Author {
            roster: &roster,
            signer: &signer,
        }
        .genesis(
            "home",
            (Ipv4Addr::new(192, 168, 14, 0), 24),
            DeviceRecord::new(signer.public_key(), "a", DeviceKind::Desktop),
        )
        .unwrap();
        roster.insert(genesis.clone()).unwrap();
        let add = Author {
            roster: &roster,
            signer: &signer,
        }
        .add(DeviceRecord::new(
            SoftSigner::generate().public_key(),
            "p",
            DeviceKind::Phone,
        ))
        .unwrap();
        roster.insert(add.clone()).unwrap();
        assert_eq!(
            store
                .append_entries(&[genesis.clone(), add.clone()])
                .unwrap(),
            2
        );
        assert_eq!(store.append_entries(std::slice::from_ref(&add)).unwrap(), 0);
        let loaded = store.load_roster().unwrap();
        assert_eq!(loaded.state(), roster.state());
        assert!(store.has_entry(&genesis.hash()).unwrap());

        let mut policy = store.policy().unwrap();
        policy.allow_locked_control = true;
        store.set_policy(&policy).unwrap();
        assert!(store.policy().unwrap().allow_locked_control);
    }

    #[test]
    fn ledger_semantics() {
        let store = Store::open_memory().unwrap();
        let ledger = store.ledger();
        assert_eq!(
            ledger.begin("op-1", "send", b"hello").unwrap(),
            Begin::Fresh
        );
        // Re-issued before completion: undecided, not re-run.
        assert_eq!(
            ledger.begin("op-1", "send", b"hello").unwrap(),
            Begin::Unknown
        );
        ledger.complete("op-1", b"msg-9").unwrap();
        assert_eq!(
            ledger.begin("op-1", "send", b"hello").unwrap(),
            Begin::Duplicate(Some(b"msg-9".to_vec()))
        );
        assert_eq!(
            ledger.begin("op-1", "send", b"other").unwrap(),
            Begin::Conflict
        );
        assert!(ledger.complete("op-1", b"again").is_err());
        assert_eq!(ledger.begin("op-2", "send", b"x").unwrap(), Begin::Fresh);
        ledger.reject("op-2", b"nope").unwrap();
        assert_eq!(ledger.status("op-2").unwrap().as_deref(), Some("rejected"));
        assert!(!ledger.has_unknown("send").unwrap());
    }

    #[test]
    fn crash_leaves_unknown() {
        let dir = std::env::temp_dir().join(format!("muster-store-{}", std::process::id()));
        let path = dir.join("state.sqlite");
        {
            let store = Store::open(&path).unwrap();
            assert_eq!(
                store.ledger().begin("op-c", "send", b"body").unwrap(),
                Begin::Fresh
            );
            // process dies here
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(
            store.ledger().begin("op-c", "send", b"body").unwrap(),
            Begin::Unknown
        );
        assert!(store.ledger().has_unknown("send").unwrap());
        assert_eq!(
            store.ledger().begin("op-d", "send", b"body").unwrap(),
            Begin::Fresh
        );
        drop(store);
        let _ = std::fs::remove_dir_all(dir);
    }
}
