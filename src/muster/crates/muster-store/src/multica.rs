//! Durable task ownership, independent of provider catalog presentation.
//! Callers must validate native receipts and authorization before settlement.
//! A record/reference is evidence of ownership, never execution authority.
use super::{Error, Result, Store};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(crate) fn init(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS multica_owners (
            native_key TEXT PRIMARY KEY, task TEXT NOT NULL, reference TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS multica_reservations (
            reference TEXT PRIMARY KEY, requester TEXT NOT NULL, operation TEXT NOT NULL,
            native_key TEXT NOT NULL REFERENCES multica_owners(native_key),
            context BLOB NOT NULL, phase TEXT NOT NULL, receipt BLOB,
            task_key TEXT NOT NULL, run TEXT NOT NULL, role TEXT NOT NULL,
            UNIQUE(task_key, run, role),
            UNIQUE(requester, operation)
        );
        CREATE INDEX IF NOT EXISTS multica_native_reservations
            ON multica_reservations(native_key);
        UPDATE multica_reservations SET phase = 'unconfirmed' WHERE phase = 'reserved';",
    )?;
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub device: String,
    pub service: String,
    pub workspace: String,
    pub task: String,
}

/// Store/profile identity is supplied from the verified provider, not its title.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeSession {
    pub device: String,
    pub provider: String,
    pub store: String,
    pub session: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reservation {
    pub task: Task,
    pub run: String,
    pub role: String,
    pub profile: String,
    pub generation: u64,
    pub native: NativeSession,
    pub canonical_cwd: String,
    /// Original authenticated requester, retained across internal adapters.
    pub requester: String,
    pub operation: String,
    pub provider_request: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub native: NativeSession,
    pub canonical_cwd: String,
    pub provider_request: String,
    /// Native receipt/contract provenance; never credentials or prompt text.
    pub evidence: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Reserved,
    Unconfirmed,
    Bound,
    Ended,
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub reference: String,
    pub reservation: Reservation,
    pub phase: Phase,
    pub receipt: Option<Receipt>,
}

fn invalid(message: &str) -> Error {
    Error::Other(format!("Multica ownership: {message}"))
}
fn field(value: &str) -> bool {
    !value.is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
fn cwd(value: &str) -> bool {
    use std::path::{Component, Path};
    field(value)
        && Path::new(value).is_absolute()
        && !Path::new(value)
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
}
impl Reservation {
    fn validate(&self) -> Result<()> {
        if ![
            &self.task.device,
            &self.task.service,
            &self.task.workspace,
            &self.task.task,
            &self.run,
            &self.role,
            &self.profile,
            &self.native.device,
            &self.native.provider,
            &self.native.store,
            &self.native.session,
            &self.requester,
            &self.operation,
            &self.provider_request,
        ]
        .into_iter()
        .all(|s| field(s))
            || self.task.device != self.native.device
            || self.generation == 0
            || !cwd(&self.canonical_cwd)
        {
            return Err(invalid("incomplete or inconsistent reservation identity"));
        }
        Ok(())
    }
}
fn read(conn: &Connection, reference: &str) -> Result<Option<Record>> {
    let row: Option<(Vec<u8>, String, Option<Vec<u8>>)> = conn
        .query_row(
            "SELECT context, phase, receipt FROM multica_reservations WHERE reference = ?1",
            [reference],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    row.map(|(context, phase, receipt)| {
        Ok(Record {
            reference: reference.into(),
            reservation: serde_json::from_slice(&context)?,
            phase: match phase.as_str() {
                "reserved" => Phase::Reserved,
                "unconfirmed" => Phase::Unconfirmed,
                "bound" => Phase::Bound,
                "ended" => Phase::Ended,
                "conflict" => Phase::Conflict,
                _ => return Err(invalid("unreadable reservation phase")),
            },
            receipt: receipt.map(|b| serde_json::from_slice(&b)).transpose()?,
        })
    })
    .transpose()
}

impl Store {
    /// Deny-only fallback when native discovery cannot prove a store identity.
    /// It must never authorize a session or associate an unknown store with a task.
    pub fn multica_has_session(&self, device: &str, session: &str) -> Result<bool> {
        Ok(self.conn.lock().unwrap().query_row(
            "SELECT EXISTS(SELECT 1 FROM multica_owners WHERE json_extract(native_key, '$.device') = ?1 AND json_extract(native_key, '$.session') = ?2)",
            params![device, session], |row| row.get(0),
        )?)
    }

    /// Ownership is append-only. Watches use this durable revision even when
    /// another Store connection commits adoption or a pre-create reservation.
    pub fn multica_ownership_revision(&self) -> Result<u64> {
        Ok(self.conn.lock().unwrap().query_row(
            "SELECT COUNT(*) FROM multica_owners",
            [],
            |row| row.get::<_, i64>(0),
        )? as u64)
    }

    /// Returns the existing exact operation without authorizing another create.
    /// The caller must inspect its phase; even Reserved is not replay permission.
    pub fn multica_reserve(&self, reservation: &Reservation) -> Result<Record> {
        reservation.validate()?;
        let context = serde_json::to_vec(reservation)?;
        let native = serde_json::to_string(&reservation.native)?;
        let task = serde_json::to_string(&reservation.task)?;
        let mut digest = Sha256::new();
        digest.update(b"MUSTER-MULTICA-RESERVATION-V1\0");
        for value in [&reservation.requester, &reservation.operation] {
            digest.update((value.len() as u64).to_be_bytes());
            digest.update(value.as_bytes());
        }
        let reference = hex::encode(digest.finalize());
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        if let Some(record) = read(&tx, &reference)? {
            if record.reservation != *reservation {
                return Err(invalid("operation context changed"));
            }
            return Ok(record);
        }
        let owner: Option<String> = tx
            .query_row(
                "SELECT task FROM multica_owners WHERE native_key = ?1",
                [&native],
                |r| r.get(0),
            )
            .optional()?;
        if owner.as_ref().is_some_and(|owner| owner != &task) {
            return Err(invalid("native session already belongs to another task"));
        }
        let active: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM multica_reservations WHERE native_key = ?1 AND phase != 'ended')",
            [&native], |r| r.get(0),
        )?;
        if active {
            return Err(invalid("native session has an unfinished reservation"));
        }
        {
            let mut query =
                tx.prepare("SELECT context FROM multica_reservations WHERE native_key = ?1")?;
            for context in query.query_map([&native], |r| r.get::<_, Vec<u8>>(0))? {
                let old: Reservation = serde_json::from_slice(&context?)?;
                if old.canonical_cwd != reservation.canonical_cwd
                    || old.profile != reservation.profile
                    || old.generation >= reservation.generation
                {
                    return Err(invalid("resume identity or generation does not match"));
                }
            }
        }
        tx.execute(
            "INSERT OR IGNORE INTO multica_owners(native_key, task, reference) VALUES (?1, ?2, ?3)",
            params![native, task, reference],
        )?;
        tx.execute("INSERT INTO multica_reservations(reference, requester, operation, native_key, context, phase, task_key, run, role)
            VALUES (?1, ?2, ?3, ?4, ?5, 'reserved', ?6, ?7, ?8)",
            params![reference, reservation.requester, reservation.operation, native, context, task, reservation.run, reservation.role])?;
        let record = read(&tx, &reference)?.ok_or_else(|| invalid("reservation disappeared"))?;
        tx.commit()?;
        Ok(record)
    }

    pub fn multica_record(&self, reference: &str) -> Result<Option<Record>> {
        read(&self.conn.lock().unwrap(), reference)
    }

    /// Includes reserved, unknown, conflicting and ended sessions, so neither
    /// publication before acknowledgement nor a restart makes them personal.
    pub fn multica_owner(&self, native: &NativeSession) -> Result<Option<Task>> {
        let value: Option<String> = self
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT task FROM multica_owners WHERE native_key = ?1",
                [serde_json::to_string(native)?],
                |r| r.get(0),
            )
            .optional()?;
        value
            .map(|v| serde_json::from_str(&v).map_err(Into::into))
            .transpose()
    }

    pub fn multica_bind(&self, reference: &str, receipt: &Receipt) -> Result<Record> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let record = read(&tx, reference)?.ok_or_else(|| invalid("unknown reservation"))?;
        if matches!(record.phase, Phase::Bound | Phase::Ended)
            && record.receipt.as_ref() == Some(receipt)
        {
            return Ok(record);
        }
        if !matches!(record.phase, Phase::Reserved | Phase::Unconfirmed) {
            return Err(invalid("reservation cannot accept another receipt"));
        }
        let expected = &record.reservation;
        let matches = receipt.native == expected.native
            && receipt.canonical_cwd == expected.canonical_cwd
            && receipt.provider_request == expected.provider_request
            && field(&receipt.evidence);
        let phase = if matches { "bound" } else { "conflict" };
        tx.execute(
            "UPDATE multica_reservations SET phase = ?2, receipt = ?3 WHERE reference = ?1",
            params![reference, phase, serde_json::to_vec(receipt)?],
        )?;
        let settled = read(&tx, reference)?.ok_or_else(|| invalid("reservation disappeared"))?;
        tx.commit()?;
        if !matches {
            return Err(invalid("receipt identity conflicts with reservation"));
        }
        Ok(settled)
    }

    pub fn multica_unconfirmed(&self, reference: &str) -> Result<()> {
        self.multica_transition(reference, Phase::Reserved, "unconfirmed")
    }
    /// Call only with a verified terminal outcome, never on disconnect or idle.
    pub fn multica_end(&self, reference: &str) -> Result<()> {
        self.multica_transition(reference, Phase::Bound, "ended")
    }
    fn multica_transition(&self, reference: &str, from: Phase, to: &str) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let record = read(&tx, reference)?.ok_or_else(|| invalid("unknown reservation"))?;
        if record.phase != from {
            return Err(invalid("invalid ownership transition"));
        }
        tx.execute(
            "UPDATE multica_reservations SET phase = ?2 WHERE reference = ?1",
            params![reference, to],
        )?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reservation() -> Reservation {
        Reservation {
            task: Task {
                device: "device-a".into(),
                service: "service-account-a".into(),
                workspace: "workspace-a".into(),
                task: "task-a".into(),
            },
            run: "attempt-1".into(),
            role: "executor".into(),
            profile: "desktop".into(),
            generation: 1,
            native: NativeSession {
                device: "device-a".into(),
                provider: "deepseek".into(),
                store: "store-a".into(),
                session: "session-a".into(),
            },
            canonical_cwd: "/work/task-a".into(),
            requester: "local-device-a".into(),
            operation: "create-1".into(),
            provider_request: "native-create-1".into(),
        }
    }
    fn receipt(r: &Reservation) -> Receipt {
        Receipt {
            native: r.native.clone(),
            canonical_cwd: r.canonical_cwd.clone(),
            provider_request: r.provider_request.clone(),
            evidence: "app-contract-v1:header".into(),
        }
    }
    #[test]
    fn reservation_is_owned_before_receipt_and_exact_retries_never_change_it() {
        let store = Store::open_memory().unwrap();
        let request = reservation();
        let record = store.multica_reserve(&request).unwrap();
        assert_eq!(record.phase, Phase::Reserved);
        assert!(
            store
                .multica_has_session(&request.native.device, &request.native.session)
                .unwrap()
        );
        assert!(
            !store
                .multica_has_session("other-device", &request.native.session)
                .unwrap()
        );
        assert_eq!(
            store.multica_owner(&request.native).unwrap(),
            Some(request.task.clone())
        );
        assert_eq!(store.multica_reserve(&request).unwrap(), record);
        let mut changed = request.clone();
        changed.canonical_cwd = "/work/other".into();
        assert!(store.multica_reserve(&changed).is_err());
        let bound = store
            .multica_bind(&record.reference, &receipt(&request))
            .unwrap();
        assert_eq!(bound.phase, Phase::Bound);
        assert_eq!(
            store
                .multica_bind(&record.reference, &receipt(&request))
                .unwrap(),
            bound
        );
        assert!(store.multica_unconfirmed(&record.reference).is_err());
    }
    #[test]
    fn restart_and_unknown_keep_identity_and_do_not_authorize_recreation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store.sqlite");
        let r = reservation();
        let id = {
            let store = Store::open(&path).unwrap();
            store.multica_reserve(&r).unwrap().reference
        };
        let store = Store::open(&path).unwrap();
        assert_eq!(store.multica_reserve(&r).unwrap().phase, Phase::Unconfirmed);
        assert!(store.multica_end(&id).is_err());
        assert_eq!(
            store.multica_owner(&r.native).unwrap(),
            Some(r.task.clone())
        );
        let mut retry = r.clone();
        retry.operation = "different-op".into();
        assert!(store.multica_reserve(&retry).is_err());
        assert_eq!(
            store.multica_bind(&id, &receipt(&r)).unwrap().phase,
            Phase::Bound
        );
    }
    #[test]
    fn receipt_conflict_is_retained_without_reassigning_or_releasing_session() {
        let store = Store::open_memory().unwrap();
        let r = reservation();
        let id = store.multica_reserve(&r).unwrap().reference;
        let mut wrong = receipt(&r);
        wrong.native.store = "another-profile-store".into();
        assert!(store.multica_bind(&id, &wrong).is_err());
        let conflict = store.multica_record(&id).unwrap().unwrap();
        assert_eq!(conflict.phase, Phase::Conflict);
        assert_eq!(conflict.receipt, Some(wrong));
        assert!(store.multica_bind(&id, &receipt(&r)).is_err());
        assert_eq!(store.multica_owner(&r.native).unwrap(), Some(r.task));
    }
    #[test]
    fn identities_and_requester_operations_do_not_collide() {
        let store = Store::open_memory().unwrap();
        let r = reservation();
        let first = store.multica_reserve(&r).unwrap();
        let mut other = r.clone();
        other.requester = "remote-device-b".into();
        // Reusing an op ID from another requester cannot acquire its session.
        assert!(store.multica_reserve(&other).is_err());
        other.native.store = "store-b".into();
        other.task.task = "task-b".into();
        assert_ne!(
            store.multica_reserve(&other).unwrap().reference,
            first.reference
        );
        assert_eq!(
            store.multica_owner(&other.native).unwrap(),
            Some(other.task.clone())
        );
        other.operation = "create-2".into();
        other.task.task = "task-c".into();
        other.native.provider = "codex".into();
        store.multica_reserve(&other).unwrap();
        other.operation = "create-3".into();
        other.task.task = "task-d".into();
        other.native.device = "device-b".into();
        other.task.device = "device-b".into();
        store.multica_reserve(&other).unwrap();
    }
    #[test]
    fn ended_history_is_owned_and_resume_requires_same_task_identity_and_new_generation() {
        let store = Store::open_memory().unwrap();
        let r = reservation();
        let id = store.multica_reserve(&r).unwrap().reference;
        store.multica_bind(&id, &receipt(&r)).unwrap();
        store.multica_end(&id).unwrap();
        assert_eq!(
            store.multica_owner(&r.native).unwrap(),
            Some(r.task.clone())
        );
        let mut next = r.clone();
        next.operation = "create-2".into();
        next.run = "attempt-2".into();
        next.task.task = "task-b".into();
        next.generation = 2;
        assert!(store.multica_reserve(&next).is_err());
        next.task = r.task.clone();
        next.canonical_cwd = "/work/other".into();
        assert!(store.multica_reserve(&next).is_err());
        next.canonical_cwd = r.canonical_cwd.clone();
        next.generation = 1;
        assert!(store.multica_reserve(&next).is_err());
        next.generation = 2;
        assert_eq!(store.multica_reserve(&next).unwrap().phase, Phase::Reserved);
        assert_eq!(
            store.multica_record(&id).unwrap().unwrap().phase,
            Phase::Ended
        );
    }
    #[test]
    fn incomplete_identity_and_second_session_for_the_same_run_are_refused() {
        let store = Store::open_memory().unwrap();
        let mut r = reservation();
        r.task.service.clear();
        assert!(store.multica_reserve(&r).is_err());
        r = reservation();
        r.canonical_cwd = "/work/../personal".into();
        assert!(store.multica_reserve(&r).is_err());
        r = reservation();
        store.multica_reserve(&r).unwrap();
        r.operation = "other-op".into();
        r.native.session = "session-b".into();
        assert!(store.multica_reserve(&r).is_err());
        // Failed transaction did not publish a second owner.
        assert!(store.multica_owner(&r.native).unwrap().is_none());
    }
}
