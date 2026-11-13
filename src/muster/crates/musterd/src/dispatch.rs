//! A standing local dispatch grant (ADR 0199): a named local service creates
//! sessions under its granted roots and sends to the sessions its own
//! creations bound. On a locked Mac each operation runs under this Mac's own
//! scheduled Locked Use lease; on an unlocked one, on the scheduler's quiet
//! path. The rules are `muster_core::dispatch_grant`'s; this adds the store,
//! this Mac's facts and the desktop.

use crate::scheduler::{AFTER_RECEIPT, Holder, Job};
use crate::{Daemon, Error};
use muster_core::dispatch_grant::{self as grant, Refusal, Registry};
use muster_core::scheduler_grant::{Admission, Kind, OpState, Outcome};
use muster_proto::Message as _;
use muster_proto::v1::{self, op_result::Status, response::Resp};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const KEY: &str = "dispatch.grants";
const MAX_TEXT: usize = 1024 * 1024;

pub(crate) struct Dispatch {
    /// `Err`: the stored grants could not be read. Everything is refused and
    /// nothing is written, so no record or unknown operation is ever lost.
    registry: Mutex<Result<Registry, String>>,
    /// One dispatch operation at a time; the next waits its turn.
    running: tokio::sync::Mutex<()>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn refused(refusal: Refusal) -> Error {
    match refusal {
        Refusal::NotFound => Error::NotFound(refusal.message()),
        Refusal::Invalid(_) => Error::Invalid(refusal.message()),
        Refusal::Conflict(_) => Error::Conflict(refusal.message()),
        _ => Error::Unauthorized(refusal.message()),
    }
}

impl Dispatch {
    /// Loads the grants. An operation still reserved was interrupted by a
    /// restart: it becomes unknown and is never retried.
    pub(crate) fn load(store: &muster_store::Store) -> Self {
        let registry = match store.kv_get(KEY) {
            Ok(None) => Ok(Registry::default()),
            Ok(Some(bytes)) => serde_json::from_slice::<Registry>(&bytes)
                .map_err(|e| format!("stored dispatch grants are unreadable: {e}")),
            Err(e) => Err(format!("dispatch grants could not be read: {e}")),
        };
        let registry = registry.and_then(|mut registry| {
            let before = registry.clone();
            registry.recover();
            if registry != before {
                let bytes = serde_json::to_vec(&registry).map_err(|e| e.to_string())?;
                store.kv_set(KEY, &bytes).map_err(|e| e.to_string())?;
            }
            Ok(registry)
        });
        if let Err(error) = &registry {
            log::error!("dispatch: {error}; every dispatch request is refused");
        }
        Self {
            registry: Mutex::new(registry),
            running: tokio::sync::Mutex::new(()),
        }
    }

    /// Whether this grant still allows desktop operations. Every OS call of
    /// its operations checks this.
    pub(crate) fn live(&self, name: &str) -> bool {
        self.registry
            .lock()
            .unwrap()
            .as_ref()
            .ok()
            .and_then(|r| r.get(name))
            .is_some_and(|g| g.live(now_ms()))
    }

    fn read<T>(&self, f: impl FnOnce(&Registry) -> Result<T, Error>) -> Result<T, Error> {
        let guard = self.registry.lock().unwrap();
        let registry = guard
            .as_ref()
            .map_err(|e| Error::Unavailable(format!("{e}; refusing")))?;
        f(registry)
    }

    /// Applies a change durably: stored first, then made current.
    fn update<T>(
        &self,
        store: &muster_store::Store,
        f: impl FnOnce(&mut Registry) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let mut guard = self.registry.lock().unwrap();
        let registry = guard
            .as_mut()
            .map_err(|e| Error::Unavailable(format!("{e}; refusing")))?;
        let mut next = registry.clone();
        let value = f(&mut next)?;
        if next != *registry {
            let bytes = serde_json::to_vec(&next).map_err(|e| Error::Unavailable(e.to_string()))?;
            store.kv_set(KEY, &bytes)?;
            *registry = next;
        }
        Ok(value)
    }
}

/// `agent_dispatch`, local socket only.
pub(crate) async fn call(daemon: &Arc<Daemon>, r: v1::AgentDispatchRequest) -> Result<Resp, Error> {
    use v1::agent_dispatch_request::Action;
    match r
        .action
        .ok_or_else(|| Error::Invalid("a dispatch action is required".into()))?
    {
        Action::Grant(g) => {
            let mut roots = Vec::with_capacity(g.roots.len());
            for root in &g.roots {
                roots.push(
                    std::fs::canonicalize(root)
                        .ok()
                        .filter(|p| p.is_dir())
                        .and_then(|p| p.to_str().map(str::to_owned))
                        .ok_or_else(|| Error::Invalid("every root must be a directory".into()))?,
                );
            }
            let scope = grant::Scope::new(g.name, g.providers, roots).map_err(refused)?;
            let name = scope.name.clone();
            let ttl = g.ttl_seconds.saturating_mul(1000);
            daemon.dispatch.update(daemon.store(), |r| {
                r.grant(scope, now_ms(), ttl).map(|_| ()).map_err(refused)
            })?;
            log::warn!("dispatch: {name:?} granted for {} s", g.ttl_seconds);
            status(daemon, &name)
        }
        Action::Revoke(n) => {
            daemon.dispatch.update(daemon.store(), |r| {
                r.revoke(&n.name, now_ms()).map_err(refused)
            })?;
            log::warn!("dispatch: {:?} revoked", n.name);
            status(daemon, &n.name)
        }
        Action::Status(n) => status(daemon, &n.name),
        // The operation settles even when its caller goes away: it runs in a
        // task of its own, with its own OS-work lifetime.
        Action::Create(c) => {
            let daemon = daemon.clone();
            tokio::spawn(async move {
                let work = muster_agent::control_guard::WorkQueue::default();
                work.scope(create(&daemon, c)).await
            })
            .await
            .map_err(|e| Error::Unavailable(format!("dispatch operation failed: {e}")))?
        }
        Action::Send(s) => {
            let daemon = daemon.clone();
            tokio::spawn(async move {
                let work = muster_agent::control_guard::WorkQueue::default();
                work.scope(send(&daemon, s)).await
            })
            .await
            .map_err(|e| Error::Unavailable(format!("dispatch operation failed: {e}")))?
        }
    }
}

fn status(daemon: &Daemon, name: &str) -> Result<Resp, Error> {
    daemon.dispatch.read(|r| {
        let g = r.get(name).ok_or_else(|| refused(Refusal::NotFound))?;
        let now = now_ms();
        Ok(Resp::AgentDispatch(v1::AgentDispatchStatus {
            name: g.scope.name.clone(),
            providers: g.scope.providers.clone(),
            roots: g.scope.roots.clone(),
            issued_unix_ms: g.issued_ms,
            expires_unix_ms: g.expires_ms,
            revoked_unix_ms: g.revoked_ms.unwrap_or(0),
            live: g.live(now),
            sessions: g.sessions.len() as u32,
            locked_last_hour: g.locked_last_hour(now) as u32,
            records: g.ops.len() as u32,
            unknown: g
                .ops
                .values()
                .filter(|op| matches!(op.state, OpState::Unknown { .. }))
                .count() as u32,
        }))
    })
}

/// One dispatched operation: what the grant checks, and the exact inner
/// request the desktop runs and the ledger records.
struct Operation {
    grant: String,
    op_id: String,
    kind: Kind,
    provider: String,
    directory: String,
    session: String,
    /// SHA-256 of the inner request.
    body: [u8; 32],
    ledger: (&'static str, Vec<u8>),
    /// A creation's request, for its receipt.
    create: Option<v1::AgentNativeCreateRequest>,
    job: Job,
}

impl Operation {
    fn request(&self) -> grant::Request<'_> {
        grant::Request {
            op_id: &self.op_id,
            kind: self.kind,
            provider: &self.provider,
            directory: &self.directory,
            session: &self.session,
            body: self.body,
        }
    }
}

async fn create(daemon: &Arc<Daemon>, c: v1::AgentDispatchCreate) -> Result<Resp, Error> {
    let mut r = c
        .request
        .ok_or_else(|| Error::Invalid("a creation request is required".into()))?;
    crate::native_create::validate(&r)?;
    // The grant's roots are canonical, and so is the session's cwd that its
    // receipt is matched against.
    r.directory = std::fs::canonicalize(&r.directory)
        .ok()
        .filter(|p| p.is_dir())
        .and_then(|p| p.to_str().map(str::to_owned))
        .ok_or_else(|| Error::Invalid("the directory must exist".into()))?;
    let body = r.encode_to_vec();
    let op = Operation {
        op_id: r.op_id.clone(),
        kind: Kind::Create,
        provider: r.provider.clone(),
        directory: r.directory.clone(),
        session: String::new(),
        body: Sha256::digest(&body).into(),
        ledger: ("agent.native_create", body),
        job: Job::create(Holder::Dispatch(c.grant.clone()), r.clone()),
        create: Some(r),
        grant: c.grant,
    };
    operate(daemon, op).await
}

async fn send(daemon: &Arc<Daemon>, s: v1::AgentDispatchSend) -> Result<Resp, Error> {
    let r = s
        .request
        .ok_or_else(|| Error::Invalid("a send request is required".into()))?;
    if r.op_id.is_empty() || r.session.is_empty() {
        return Err(Error::Invalid(
            "a dispatched send needs an op_id and a session".into(),
        ));
    }
    if r.text.trim().is_empty() || r.text.len() > MAX_TEXT {
        return Err(Error::Invalid(
            "a dispatched send needs text of at most 1 MiB".into(),
        ));
    }
    if !r.attachments.is_empty() {
        return Err(Error::Invalid(
            "a dispatched send is text only; nothing was sent".into(),
        ));
    }
    // The provider is the one the session's creation receipt bound.
    let provider = daemon.dispatch.read(|reg| {
        let g = reg
            .get(&s.grant)
            .ok_or_else(|| refused(Refusal::NotFound))?;
        g.sessions.get(&r.session).cloned().ok_or_else(|| {
            refused(Refusal::OutOfScope(
                "session was not created through this grant",
            ))
        })
    })?;
    let body = r.encode_to_vec();
    let op = Operation {
        op_id: r.op_id.clone(),
        kind: Kind::Send,
        provider: provider.clone(),
        directory: String::new(),
        session: r.session.clone(),
        body: Sha256::digest(&body).into(),
        ledger: ("agent.send", body),
        job: Job::send(Holder::Dispatch(s.grant.clone()), provider, r),
        create: None,
        grant: s.grant,
    };
    operate(daemon, op).await
}

fn op_result(status: Status, message: impl Into<String>, result_id: impl Into<String>) -> Resp {
    Resp::OpResult(v1::OpResult {
        status: status as i32,
        message: message.into(),
        result_id: result_id.into(),
        ..Default::default()
    })
}

async fn operate(daemon: &Arc<Daemon>, op: Operation) -> Result<Resp, Error> {
    let dispatch = &daemon.dispatch;
    // A repeat answers its record at once, even while another runs.
    if let Some(state) = recorded(daemon, &op)? {
        return answer(daemon, &op, state).await;
    }
    let _running = dispatch.running.lock().await;
    let locked = daemon.platform().locked();
    // Scope, liveness, limits and op_id are decided first, on a copy and
    // whatever the desktop's state.
    let dry_run = dispatch.read(|r| {
        let mut g = r
            .get(&op.grant)
            .ok_or_else(|| refused(Refusal::NotFound))?
            .clone();
        g.reserve(&op.request(), Vec::new(), locked, now_ms())
            .map_err(refused)
    })?;
    if let Admission::Recorded(state) = dry_run {
        return answer(daemon, &op, state).await;
    }
    // The ledger is shared with the plain RPCs: an op_id already on it (a
    // plain request refused on the locked Mac, say) would only answer its old
    // row inside the lease. Refuse it before anything unlocks.
    if daemon.store().ledger().status(&op.op_id)?.is_some() {
        return Err(Error::Conflict(
            "this op_id is already on the operation ledger; use a new op_id. Nothing was reserved"
                .into(),
        ));
    }
    crate::scheduler::preflight(daemon, &op.provider)?;
    let baseline = if op.kind == Kind::Create {
        crate::native_create::baseline(daemon, &op.provider, &op.directory)
    } else {
        Vec::new()
    };
    // Recorded and stored before anything touches the desktop.
    let admission = dispatch.update(daemon.store(), |r| {
        r.get_mut(&op.grant)
            .ok_or_else(|| refused(Refusal::NotFound))?
            .reserve(&op.request(), baseline, locked, now_ms())
            .map_err(refused)
    })?;
    let reservation = match admission {
        Admission::Recorded(state) => return answer(daemon, &op, state).await,
        Admission::Fresh(reservation) => reservation,
    };
    log::info!(
        "dispatch {:?}: {} op={:?} on a {} console",
        op.grant,
        op.kind.as_str(),
        op.op_id,
        if locked { "locked" } else { "unlocked" }
    );
    let result = if locked {
        daemon
            .locked_use
            .scheduled(daemon.clone(), reservation, op.job.clone())
            .await
    } else {
        crate::scheduler::unlocked(daemon, &op.job).await
    };
    let outcome = classify(daemon, &op, &result);
    let settled = dispatch.update(daemon.store(), |r| {
        r.get_mut(&op.grant)
            .ok_or_else(|| refused(Refusal::NotFound))?
            .settle(&op.op_id, outcome.clone())
            .map_err(refused)
    });
    // A receipt the grant cannot bind is no evidence of success: keep it
    // unknown, never replayed.
    let outcome = match settled {
        Ok(()) => outcome,
        Err(error) => {
            let unknown = Outcome::Unknown(error.to_string());
            dispatch.update(daemon.store(), |r| {
                r.get_mut(&op.grant)
                    .ok_or_else(|| refused(Refusal::NotFound))?
                    .settle(&op.op_id, unknown.clone())
                    .map_err(refused)
            })?;
            unknown
        }
    };
    log::info!(
        "dispatch {:?}: op={:?} settled {}",
        op.grant,
        op.op_id,
        match &outcome {
            Outcome::Applied(_) => "applied",
            Outcome::NotStarted(_) => "not started",
            Outcome::Unknown(_) => "unknown",
        }
    );
    match outcome {
        Outcome::Applied(id) => Ok(op_result(Status::Applied, "", id)),
        Outcome::NotStarted(message) => Ok(op_result(
            Status::Rejected,
            format!("{message}; not started, nothing was sent"),
            "",
        )),
        Outcome::Unknown(message) => {
            // The console is given back. Look for a receipt read-only.
            let deadline = tokio::time::Instant::now() + AFTER_RECEIPT;
            loop {
                if let Some(id) = resolve(daemon, &op).await? {
                    return Ok(op_result(Status::Applied, "", id));
                }
                if tokio::time::Instant::now() >= deadline {
                    return Ok(op_result(
                        Status::Unknown,
                        format!("{message}; never replayed"),
                        "",
                    ));
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

/// What an earlier request with this op_id recorded, when it was this same
/// request; a different request under the same op_id is a conflict.
fn recorded(daemon: &Daemon, op: &Operation) -> Result<Option<OpState>, Error> {
    daemon.dispatch.read(|r| {
        let g = r.get(&op.grant).ok_or_else(|| refused(Refusal::NotFound))?;
        match g.lookup(&op.request()) {
            Ok(recorded) => Ok(Some(recorded.state.clone())),
            Err(Refusal::NotFound) => Ok(None),
            Err(refusal) => Err(refused(refusal)),
        }
    })
}

/// A repeat's answer: its record, or read-only evidence for one that is
/// still undecided. Nothing runs again.
async fn answer(daemon: &Arc<Daemon>, op: &Operation, state: OpState) -> Result<Resp, Error> {
    Ok(match state {
        OpState::Applied { result } => op_result(Status::Duplicate, "", result),
        OpState::Rejected { message } => op_result(Status::Rejected, message, ""),
        OpState::Reserved => op_result(
            Status::Unknown,
            "this operation is still running; never replayed",
            "",
        ),
        OpState::Unknown { message } => match resolve(daemon, op).await? {
            Some(id) => op_result(Status::Duplicate, "", id),
            None => op_result(Status::Unknown, format!("{message}; never replayed"), ""),
        },
    })
}

/// Settles from evidence, as the task scheduler does: an operation that never
/// began was not sent; one that began is what its ledger row says.
fn classify(daemon: &Daemon, op: &Operation, result: &Result<Resp, Error>) -> Outcome {
    let failure = match result {
        Ok(Resp::OpResult(r)) => r.message.clone(),
        Ok(_) => String::new(),
        Err(e) => e.to_string(),
    };
    if !op.job.started() {
        return Outcome::NotStarted(if failure.is_empty() {
            "the console was not available".into()
        } else {
            failure
        });
    }
    crate::scheduler::from_ledger(daemon, &op.op_id, op.ledger.0, &op.ledger.1, failure)
}

/// Read-only: provider evidence for an operation that is reserved or
/// unknown. A creation's receipt is its unique new transcript outside the
/// baseline and the grant's bound sessions; a send's is its confirmed ledger
/// row. Binds nothing on ambiguity.
async fn resolve(daemon: &Arc<Daemon>, op: &Operation) -> Result<Option<String>, Error> {
    let (state, baseline, bound) = daemon.dispatch.read(|r| {
        let g = r.get(&op.grant).ok_or_else(|| refused(Refusal::NotFound))?;
        let recorded = g.lookup(&op.request()).map_err(refused)?;
        Ok((
            recorded.state.clone(),
            recorded.baseline.clone(),
            g.sessions.keys().cloned().collect::<Vec<_>>(),
        ))
    })?;
    match state {
        OpState::Applied { result } => return Ok(Some(result)),
        OpState::Rejected { .. } => return Ok(None),
        OpState::Reserved | OpState::Unknown { .. } => {}
    }
    let (kind, body) = (op.ledger.0, op.ledger.1.as_slice());
    let ledger = daemon.store().ledger();
    let evidence = match (&op.create, ledger.outcome(&op.op_id, kind, body)?) {
        (_, muster_store::Outcome::Applied(id)) => String::from_utf8_lossy(&id).into_owned(),
        (_, muster_store::Outcome::Conflict | muster_store::Outcome::Rejected(_)) => {
            return Ok(None);
        }
        (Some(r), _) => {
            let mut exclude = baseline;
            exclude.extend(bound);
            let Some(session) = crate::native_create::receipt(daemon, r, &exclude).await? else {
                return Ok(None);
            };
            // The ledger row (when the creation reached it) records the same receipt.
            if ledger.status(&op.op_id)?.is_some() {
                ledger.confirm(
                    &op.op_id,
                    kind,
                    &muster_store::Ledger::body_hash(body),
                    session.as_bytes(),
                )?;
            }
            session
        }
        (None, _) => return Ok(None),
    };
    daemon.dispatch.update(daemon.store(), |r| {
        r.get_mut(&op.grant)
            .ok_or_else(|| refused(Refusal::NotFound))?
            .reconcile(&op.op_id, evidence.clone())
            .map_err(refused)
    })?;
    log::info!(
        "dispatch {:?}: op={:?} reconciled from provider evidence",
        op.grant,
        op.op_id
    );
    Ok(Some(evidence))
}

/// Grants `name` (if needed) over `/private/tmp` and reserves one locked
/// creation in it, as `operate` would before a scheduled lease. The job is a
/// probe held while `hold` is set.
#[cfg(test)]
pub(crate) fn test_reservation(
    daemon: &Daemon,
    name: &str,
    op_id: &str,
    hold: Arc<std::sync::atomic::AtomicBool>,
) -> (muster_core::scheduler_grant::Reservation, Job) {
    let scope = grant::Scope::new(
        name.into(),
        vec!["claude".into()],
        vec!["/private/tmp".into()],
    )
    .unwrap();
    let request = grant::Request {
        op_id,
        kind: Kind::Create,
        provider: "claude",
        directory: "/private/tmp",
        session: "",
        body: [7; 32],
    };
    let reservation = daemon
        .dispatch
        .update(daemon.store(), |r| {
            if r.get(name).is_none() {
                r.grant(scope.clone(), now_ms(), 60_000).map_err(refused)?;
            }
            match r
                .get_mut(name)
                .unwrap()
                .reserve(&request, Vec::new(), true, now_ms())
                .map_err(refused)?
            {
                Admission::Fresh(reservation) => Ok(reservation),
                Admission::Recorded(_) => Err(Error::Conflict("recorded".into())),
            }
        })
        .unwrap();
    (reservation, Job::probe(Holder::Dispatch(name.into()), hold))
}

#[cfg(test)]
pub(crate) fn test_revoke(daemon: &Daemon, name: &str) {
    daemon
        .dispatch
        .update(daemon.store(), |r| {
            r.revoke(name, now_ms()).map_err(refused)
        })
        .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use v1::agent_dispatch_request::Action;

    async fn daemon(dir: &std::path::Path) -> Arc<Daemon> {
        let mut config = crate::DaemonConfig::new(dir);
        config.listen = false;
        config.agent = None;
        config.cloud = false;
        config.bind = "127.0.0.1:0".parse().unwrap();
        Daemon::start_with(config, Arc::new(crate::DefaultPlatform), None)
            .await
            .unwrap()
    }

    struct Fixture {
        dir: tempfile::TempDir,
        daemon: Arc<Daemon>,
        /// The granted root, canonical.
        work: String,
    }

    async fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        for d in ["work/repo", "elsewhere"] {
            std::fs::create_dir_all(dir.path().join(d)).unwrap();
        }
        let work = std::fs::canonicalize(dir.path().join("work"))
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let daemon = daemon(&dir.path().join("state")).await;
        Fixture { dir, daemon, work }
    }

    fn grant(name: &str, root: &str, ttl_seconds: u64) -> v1::AgentDispatchRequest {
        v1::AgentDispatchRequest {
            action: Some(Action::Grant(v1::AgentDispatchGrant {
                name: name.into(),
                providers: vec!["claude".into()],
                roots: vec![root.into()],
                ttl_seconds,
            })),
        }
    }

    fn named(action: fn(v1::AgentDispatchName) -> Action, name: &str) -> v1::AgentDispatchRequest {
        v1::AgentDispatchRequest {
            action: Some(action(v1::AgentDispatchName { name: name.into() })),
        }
    }

    fn creation(op_id: &str, provider: &str, directory: &str) -> v1::AgentNativeCreateRequest {
        v1::AgentNativeCreateRequest {
            op_id: op_id.into(),
            provider: provider.into(),
            directory: directory.into(),
            prompt: "Synthetic first message".into(),
            ..Default::default()
        }
    }

    fn sending(op_id: &str, session: &str) -> v1::AgentSendRequest {
        v1::AgentSendRequest {
            op_id: op_id.into(),
            session: session.into(),
            text: "/compact".into(),
            ..Default::default()
        }
    }

    async fn create_in(
        f: &Fixture,
        name: &str,
        r: v1::AgentNativeCreateRequest,
    ) -> Result<Resp, Error> {
        let action = Action::Create(v1::AgentDispatchCreate {
            grant: name.into(),
            request: Some(r),
        });
        call(
            &f.daemon,
            v1::AgentDispatchRequest {
                action: Some(action),
            },
        )
        .await
    }

    async fn send_in(f: &Fixture, name: &str, r: v1::AgentSendRequest) -> Result<Resp, Error> {
        let action = Action::Send(v1::AgentDispatchSend {
            grant: name.into(),
            request: Some(r),
        });
        call(
            &f.daemon,
            v1::AgentDispatchRequest {
                action: Some(action),
            },
        )
        .await
    }

    fn state(f: &Fixture, name: &str) -> v1::AgentDispatchStatus {
        match status(&f.daemon, name).unwrap() {
            Resp::AgentDispatch(s) => s,
            other => panic!("{other:?}"),
        }
    }

    fn op_status(resp: &Resp) -> (i32, String, String) {
        match resp {
            Resp::OpResult(r) => (r.status, r.message.clone(), r.result_id.clone()),
            other => panic!("{other:?}"),
        }
    }

    /// Records an earlier operation of `name` that ended with `outcome`, under
    /// the exact digest a request through the RPC carries.
    fn record(f: &Fixture, name: &str, op: Operation, outcome: Option<Outcome>) {
        f.daemon
            .dispatch
            .update(f.daemon.store(), |r| {
                let g = r.get_mut(name).unwrap();
                assert!(matches!(
                    g.reserve(&op.request(), vec!["old".into()], true, now_ms()),
                    Ok(Admission::Fresh(_))
                ));
                if let Some(outcome) = outcome {
                    g.settle(&op.op_id, outcome).map_err(refused)?;
                }
                Ok(())
            })
            .unwrap();
    }

    fn created(name: &str, r: &v1::AgentNativeCreateRequest) -> Operation {
        let body = r.encode_to_vec();
        Operation {
            grant: name.into(),
            op_id: r.op_id.clone(),
            kind: Kind::Create,
            provider: r.provider.clone(),
            directory: r.directory.clone(),
            session: String::new(),
            body: Sha256::digest(&body).into(),
            ledger: ("agent.native_create", body),
            create: Some(r.clone()),
            job: Job::create(Holder::Dispatch(name.into()), r.clone()),
        }
    }

    fn sent(name: &str, provider: &str, r: &v1::AgentSendRequest) -> Operation {
        let body = r.encode_to_vec();
        Operation {
            grant: name.into(),
            op_id: r.op_id.clone(),
            kind: Kind::Send,
            provider: provider.into(),
            directory: String::new(),
            session: r.session.clone(),
            body: Sha256::digest(&body).into(),
            ledger: ("agent.send", body),
            create: None,
            job: Job::send(Holder::Dispatch(name.into()), provider.into(), r.clone()),
        }
    }

    #[tokio::test]
    async fn a_grant_stores_canonical_roots_and_reports_its_status() {
        let f = fixture().await;
        // A path through a symlink or `..` is stored as what it names.
        let indirect = f.dir.path().join("elsewhere/../work");
        call(&f.daemon, grant("svc", indirect.to_str().unwrap(), 3600))
            .await
            .unwrap();
        let s = state(&f, "svc");
        assert_eq!(s.roots, vec![f.work.clone()]);
        assert_eq!(s.providers, ["claude"]);
        assert!(s.live);
        assert_eq!((s.sessions, s.records, s.unknown), (0, 0, 0));
        assert_eq!(s.expires_unix_ms - s.issued_unix_ms, 3_600_000);

        let missing = f.dir.path().join("missing");
        for refused in [
            grant("svc-2", missing.to_str().unwrap(), 3600),
            grant("svc-2", "/", 3600),
            grant("svc-2", &f.work, 0),
            grant("svc-2", &f.work, 31 * 24 * 3600),
            grant("bad name", &f.work, 3600),
        ] {
            assert!(call(&f.daemon, refused).await.is_err());
        }
        assert!(matches!(
            status(&f.daemon, "svc-2"),
            Err(Error::NotFound(_))
        ));

        call(&f.daemon, named(Action::Revoke, "svc")).await.unwrap();
        assert!(!state(&f, "svc").live);
        assert!(!f.daemon.dispatch.live("svc"));
        // The owner may grant it again.
        call(&f.daemon, grant("svc", &f.work, 60)).await.unwrap();
        assert!(f.daemon.dispatch.live("svc"));
        f.daemon.shutdown().await;
    }

    #[tokio::test]
    async fn out_of_scope_requests_are_refused_before_anything_is_reserved() {
        let f = fixture().await;
        call(&f.daemon, grant("svc", &f.work, 3600)).await.unwrap();
        call(&f.daemon, grant("other", &f.work, 3600))
            .await
            .unwrap();
        let elsewhere = f.dir.path().join("elsewhere");
        let error = create_in(
            &f,
            "svc",
            creation("c1", "claude", elsewhere.to_str().unwrap()),
        )
        .await
        .unwrap_err();
        assert!(
            error.to_string().contains("outside the dispatch grant"),
            "{error}"
        );
        let error = create_in(&f, "svc", creation("c2", "codex", &f.work))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("provider"), "{error}");
        assert!(
            create_in(&f, "missing", creation("c3", "claude", &f.work))
                .await
                .is_err()
        );
        // A session that no creation of this grant bound: the owner's own, or
        // another grant's.
        let r = creation("c4", "claude", &f.work);
        let mut r = r;
        r.directory = format!("{}/repo", f.work);
        record(
            &f,
            "other",
            created("other", &r),
            Some(Outcome::Applied("s-other".into())),
        );
        for session in ["s-user", "s-other"] {
            let error = send_in(&f, "svc", sending("s1", session))
                .await
                .unwrap_err();
            assert!(error.to_string().contains("not created through"), "{error}");
        }
        let mut with_file = sending("s2", "s-other");
        with_file.attachments.push(Default::default());
        assert!(matches!(
            send_in(&f, "other", with_file).await,
            Err(Error::Invalid(_))
        ));
        assert_eq!(state(&f, "svc").records, 0);
        assert_eq!(state(&f, "other").records, 1);
        f.daemon.shutdown().await;
    }

    #[tokio::test]
    async fn an_op_id_the_plain_rpc_already_used_is_refused_before_anything_unlocks() {
        let f = fixture().await;
        call(&f.daemon, grant("svc", &f.work, 3600)).await.unwrap();
        let earlier = creation("c1", "claude", &f.work);
        f.daemon
            .store()
            .ledger()
            .begin("c1", "agent.native_create", &earlier.encode_to_vec())
            .unwrap();
        let error = create_in(&f, "svc", earlier).await.unwrap_err();
        assert!(matches!(error, Error::Conflict(_)), "{error}");
        assert!(error.to_string().contains("new op_id"), "{error}");
        let s = state(&f, "svc");
        assert_eq!((s.records, s.locked_last_hour), (0, 0));
        f.daemon.shutdown().await;
    }

    #[tokio::test]
    async fn a_desktop_that_cannot_be_driven_reserves_nothing() {
        let f = fixture().await;
        call(&f.daemon, grant("svc", &f.work, 3600)).await.unwrap();
        // DefaultPlatform has no Accessibility.
        let error = create_in(&f, "svc", creation("c1", "claude", &f.work))
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("nothing was reserved"),
            "{error}"
        );
        let s = state(&f, "svc");
        assert_eq!((s.records, s.locked_last_hour), (0, 0));
        f.daemon.shutdown().await;
    }

    #[tokio::test]
    async fn a_recorded_op_id_answers_its_record_and_never_runs_again() {
        let f = fixture().await;
        call(&f.daemon, grant("svc", &f.work, 3600)).await.unwrap();
        let create = creation("c1", "claude", &f.work);
        record(
            &f,
            "svc",
            created("svc", &create),
            Some(Outcome::Applied("s1".into())),
        );
        let (status, _, result) = op_status(&create_in(&f, "svc", create.clone()).await.unwrap());
        assert_eq!((status, result.as_str()), (Status::Duplicate as i32, "s1"));
        let mut changed = create.clone();
        changed.prompt = "Another first message".into();
        assert!(matches!(
            create_in(&f, "svc", changed).await,
            Err(Error::Conflict(_))
        ));

        let lost = sending("s-1", "s1");
        record(
            &f,
            "svc",
            sent("svc", "claude", &lost),
            Some(Outcome::Unknown("lost".into())),
        );
        let (status, message, _) = op_status(&send_in(&f, "svc", lost).await.unwrap());
        assert_eq!(status, Status::Unknown as i32);
        assert!(message.contains("never replayed"), "{message}");
        let busy = sending("s-2", "s1");
        record(
            &f,
            "svc",
            sent("svc", "claude", &busy),
            Some(Outcome::NotStarted("busy".into())),
        );
        let (status, _, _) = op_status(&send_in(&f, "svc", busy).await.unwrap());
        assert_eq!(status, Status::Rejected as i32);

        // A repeat still answers after revocation; nothing new is admitted.
        call(&f.daemon, named(Action::Revoke, "svc")).await.unwrap();
        let (status, _, result) = op_status(&create_in(&f, "svc", create).await.unwrap());
        assert_eq!((status, result.as_str()), (Status::Duplicate as i32, "s1"));
        let error = send_in(&f, "svc", sending("s-3", "s1")).await.unwrap_err();
        assert!(error.to_string().contains("revoked"), "{error}");
        let s = state(&f, "svc");
        assert_eq!((s.sessions, s.records, s.unknown), (1, 3, 1));
        f.daemon.shutdown().await;
    }

    #[tokio::test]
    async fn a_restart_turns_a_reserved_operation_unknown() {
        let f = fixture().await;
        call(&f.daemon, grant("svc", &f.work, 3600)).await.unwrap();
        record(
            &f,
            "svc",
            created("svc", &creation("c1", "claude", &f.work)),
            None,
        );
        let reloaded = Dispatch::load(f.daemon.store());
        let g = reloaded
            .read(|r| Ok(r.get("svc").unwrap().clone()))
            .unwrap();
        assert!(matches!(g.ops["c1"].state, OpState::Unknown { .. }));
        assert_eq!(g.locked_last_hour(now_ms()), 1);
        let again = Dispatch::load(f.daemon.store());
        assert_eq!(
            again.read(|r| Ok(r.get("svc").unwrap().clone())).unwrap(),
            g
        );
        f.daemon.shutdown().await;
    }

    #[tokio::test]
    async fn unreadable_grants_fail_closed_and_are_never_overwritten() {
        let f = fixture().await;
        f.daemon.store().kv_set(KEY, b"{not json").unwrap();
        let broken = Dispatch::load(f.daemon.store());
        assert!(!broken.live("svc"));
        assert!(broken.read(|_| Ok(())).is_err());
        assert!(broken.update(f.daemon.store(), |_| Ok(())).is_err());
        assert_eq!(
            f.daemon.store().kv_get(KEY).unwrap().as_deref(),
            Some(&b"{not json"[..])
        );
        f.daemon.shutdown().await;
    }
}
