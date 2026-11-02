//! One task's scheduler grant on this Mac (ADR 0095): its persistence, the
//! local `agent_scheduler` RPC, and each reserved desktop operation, run
//! under a scheduled Locked Use lease while the console is locked or on a
//! quiet unlocked console. The grant's rules are `muster_core::scheduler_grant`;
//! this module reserves before acting, settles from evidence, and never
//! re-runs an operation whose outcome is unknown.

use crate::{Daemon, Error};
use muster_core::scheduler_grant::{
    self as grant, Admission, Kind, Op, OpState, Outcome, Refusal, Registry, Role,
};
use muster_proto::Message as _;
use muster_proto::v1::{self, op_result::Status, request::Req, response::Resp};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const KEY: &str = "scheduler.grants";
/// Inside a scheduled lease: how long a creation looks for its receipt
/// before giving the console back. The model's answer is never waited for.
pub(crate) const IN_LEASE_RECEIPT: Duration = Duration::from_secs(3);
/// After the console is given back: how long `operate` looks for a receipt
/// read-only before answering unknown. No lease is held meanwhile.
const AFTER_RECEIPT: Duration = Duration::from_secs(45);
const MAX_TEXT: usize = 1024 * 1024;

pub(crate) struct Scheduler {
    /// `Err`: the stored grants could not be read. Everything is refused and
    /// nothing is written, so no count or unknown operation is ever reset.
    registry: Mutex<Result<Registry, String>>,
    /// One scheduler operation at a time on this Mac.
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

impl Scheduler {
    /// Loads the grants. An operation still reserved was interrupted by a
    /// restart: it becomes unknown and is never retried.
    pub(crate) fn load(store: &muster_store::Store) -> Self {
        let registry = match store.kv_get(KEY) {
            Ok(None) => Ok(Registry::default()),
            Ok(Some(bytes)) => serde_json::from_slice::<Registry>(&bytes)
                .map_err(|e| format!("stored scheduler grants are unreadable: {e}")),
            Err(e) => Err(format!("scheduler grants could not be read: {e}")),
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
            log::error!("scheduler: {error}; every scheduler request is refused");
        }
        Self {
            registry: Mutex::new(registry),
            running: tokio::sync::Mutex::new(()),
        }
    }

    /// Whether this task's grant still allows desktop operations. Every OS
    /// call of its operations checks this.
    pub(crate) fn live(&self, task_id: &str) -> bool {
        self.registry
            .lock()
            .unwrap()
            .as_ref()
            .ok()
            .and_then(|r| r.get(task_id))
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

/// The desktop operation a reservation runs: the exact inner request.
#[derive(Clone)]
pub(crate) struct Job {
    provider: String,
    inner: Inner,
    /// Set when the operation itself begins. Unset afterwards proves it never
    /// reached the desktop (the lease or the console was refused first).
    started: Arc<AtomicBool>,
}

#[derive(Clone)]
enum Inner {
    Create(v1::AgentNativeCreateRequest),
    Operation(Req),
    /// Tests: waits while held, then checks the console and submits, as a
    /// native write does.
    #[cfg(test)]
    Probe(Arc<AtomicBool>),
}

impl Job {
    pub(crate) fn provider(&self) -> &str {
        &self.provider
    }

    #[cfg(test)]
    pub(crate) fn started(&self) -> bool {
        self.started.load(Ordering::SeqCst)
    }

    /// Runs the operation on a console the caller owns.
    pub(crate) async fn perform(
        &self,
        daemon: &Arc<Daemon>,
        receipt: Duration,
    ) -> Result<Resp, Error> {
        self.started.store(true, Ordering::SeqCst);
        match &self.inner {
            Inner::Create(r) => crate::native_create::scheduled(daemon, r.clone(), receipt).await,
            Inner::Operation(req) => crate::handler::locked_operation(daemon, req.clone()).await,
            #[cfg(test)]
            Inner::Probe(hold) => {
                let deadline = std::time::Instant::now() + Duration::from_secs(3);
                while hold.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                muster_agent::control_guard::check()
                    .map_err(|e| Error::UnknownOutcome(e.to_string()))?;
                muster_agent::control_guard::delivery_attempt();
                Ok(op_result(Status::Applied, "", "probe-session"))
            }
        }
    }
}

fn kind_of(text: &str) -> Result<Kind, Error> {
    Ok(match text {
        "create" => Kind::Create,
        "send" => Kind::Send,
        "read_options" => Kind::ReadOptions,
        "set_options" => Kind::SetOptions,
        "interrupt" => Kind::Interrupt,
        _ => return Err(Error::Invalid("unknown scheduler operation kind".into())),
    })
}

fn role_of(text: &str) -> Result<Role, Error> {
    Ok(match text {
        "producer" => Role::Producer,
        "reviewer" => Role::Reviewer,
        _ => return Err(Error::Invalid("role must be producer or reviewer".into())),
    })
}

/// One parsed scheduler operation and the inner request it stands for.
struct Parsed {
    kind: Kind,
    role: Role,
    body: [u8; 32],
    inner: Inner,
    /// The inner operation's ledger row, when it has one.
    ledger: Option<(&'static str, Vec<u8>)>,
}

fn parse(o: &v1::AgentSchedulerOperate, scope: &grant::Scope) -> Result<Parsed, Error> {
    let kind = kind_of(&o.kind)?;
    let role = role_of(&o.role)?;
    let content = matches!(kind, Kind::Create | Kind::Send);
    if content && (o.text.trim().is_empty() || o.text.len() > MAX_TEXT) {
        return Err(Error::Invalid(
            "a package needs text of at most 1 MiB".into(),
        ));
    }
    if !content && !o.text.is_empty() {
        return Err(Error::Invalid("only create and send carry text".into()));
    }
    if kind != Kind::SetOptions && (!o.model.is_empty() || !o.effort.is_empty()) {
        return Err(Error::Invalid(
            "only set_options names a model or effort".into(),
        ));
    }
    let (inner, ledger) = match kind {
        Kind::Create => {
            let r = v1::AgentNativeCreateRequest {
                op_id: o.op_id.clone(),
                provider: scope.provider.clone(),
                directory: scope.directory.clone(),
                prompt: o.text.clone(),
            };
            let body = r.encode_to_vec();
            (Inner::Create(r), Some(("agent.native_create", body)))
        }
        Kind::Send => {
            let r = v1::AgentSendRequest {
                op_id: o.op_id.clone(),
                session: o.session.clone(),
                text: o.text.clone(),
                ..Default::default()
            };
            let body = r.encode_to_vec();
            (
                Inner::Operation(Req::AgentSend(r)),
                Some(("agent.send", body)),
            )
        }
        Kind::Interrupt => (
            Inner::Operation(Req::AgentInterrupt(v1::AgentInterruptRequest {
                op_id: o.op_id.clone(),
                session: o.session.clone(),
            })),
            Some(("agent.interrupt", o.session.clone().into_bytes())),
        ),
        Kind::ReadOptions => (
            Inner::Operation(Req::AgentChatOptions(v1::AgentChatOptionsRequest {
                session: o.session.clone(),
                op_id: o.op_id.clone(),
            })),
            None,
        ),
        Kind::SetOptions => (
            Inner::Operation(Req::AgentChatSetOptions(v1::AgentChatSetOptionsRequest {
                session: o.session.clone(),
                model: o.model.clone(),
                effort: o.effort.clone(),
                op_id: o.op_id.clone(),
            })),
            None,
        ),
    };
    Ok(Parsed {
        kind,
        role,
        body: Sha256::digest(o.encode_to_vec()).into(),
        inner,
        ledger,
    })
}

fn request<'a>(o: &'a v1::AgentSchedulerOperate, p: &Parsed) -> grant::Request<'a> {
    grant::Request {
        op_id: &o.op_id,
        kind: p.kind,
        role: p.role,
        provider: &o.provider,
        directory: &o.directory,
        session: &o.session,
        model: &o.model,
        effort: &o.effort,
        body: p.body,
    }
}

pub(crate) async fn call(
    daemon: &Arc<Daemon>,
    r: v1::AgentSchedulerRequest,
) -> Result<Resp, Error> {
    use v1::agent_scheduler_request::Action;
    let scheduler = &daemon.scheduler;
    match r
        .action
        .ok_or_else(|| Error::Invalid("a scheduler action is required".into()))?
    {
        Action::Grant(g) => {
            let directory = std::fs::canonicalize(&g.directory)
                .ok()
                .filter(|p| p.is_dir())
                .and_then(|p| p.to_str().map(str::to_owned))
                .ok_or_else(|| Error::Invalid("the directory must exist".into()))?;
            let scope = grant::Scope {
                task_id: g.task_id,
                contract_sha256: g.contract_sha256,
                provider: g.provider,
                directory,
                model: g.model,
                effort: g.effort,
            };
            let ttl = g.ttl_seconds.saturating_mul(1000);
            let task = scope.task_id.clone();
            scheduler.update(daemon.store(), |r| {
                r.grant(scope, now_ms(), ttl).map(|_| ()).map_err(refused)
            })?;
            log::info!("scheduler: task {task:?} granted");
            status(daemon, &task, None)
        }
        Action::Revoke(t) => {
            scheduler.update(daemon.store(), |r| {
                r.revoke(&t.task_id, now_ms()).map_err(refused)
            })?;
            log::warn!("scheduler: task {:?} revoked", t.task_id);
            status(daemon, &t.task_id, None)
        }
        Action::Status(t) => status(daemon, &t.task_id, None),
        Action::Operate(o) => operate(daemon, o).await,
        Action::Reconcile(o) => reconcile(daemon, o).await,
        Action::Adopt(a) => adopt(daemon, a).await,
    }
}

/// Whether `id` is, in the provider's own registry, a session the scope may
/// adopt: this provider's code session in the granted directory, not
/// archived, read-only or a child session.
fn adoptable(sessions: &[v1::AgentSession], scope: &grant::Scope, id: &str) -> bool {
    sessions.iter().any(|s| {
        s.id == id
            && s.provider == scope.provider
            && s.mode == "code"
            && std::path::Path::new(&s.cwd) == std::path::Path::new(&scope.directory)
            && !s.archived
            && !s.read_only
            && s.parent_session_id.is_empty()
    })
}

/// Binds an existing session to a role instead of creating one (ADR 0095).
/// It reads the provider's session registry and never touches the desktop.
async fn adopt(daemon: &Arc<Daemon>, a: v1::AgentSchedulerAdopt) -> Result<Resp, Error> {
    let role = role_of(&a.role)?;
    // Liveness, the role, the quota and a repeat are decided on a copy before
    // the registry is read.
    let (scope, fresh) = daemon.scheduler.read(|r| {
        let g = r
            .get(&a.task_id)
            .ok_or_else(|| refused(Refusal::NotFound))?;
        let fresh = g
            .clone()
            .adopt(role, &a.session, now_ms())
            .map_err(refused)?;
        Ok((g.scope.clone(), fresh))
    })?;
    if a.provider != scope.provider {
        return Err(refused(Refusal::OutOfScope("provider")));
    }
    if a.directory != scope.directory {
        return Err(refused(Refusal::OutOfScope("directory")));
    }
    if !fresh {
        return status(daemon, &a.task_id, None);
    }
    let agent = daemon
        .agent()
        .ok_or_else(|| Error::Unavailable("Agent service unavailable".into()))?;
    agent.rescan().await;
    if !adoptable(&agent.list(None, usize::MAX), &scope, &a.session) {
        return Err(refused(Refusal::OutOfScope(
            "session is not this provider's code session in the granted directory",
        )));
    }
    daemon.scheduler.update(daemon.store(), |r| {
        r.get_mut(&a.task_id)
            .ok_or_else(|| refused(Refusal::NotFound))?
            .adopt(role, &a.session, now_ms())
            .map_err(refused)
    })?;
    log::info!(
        "scheduler: task {:?} adopted its {} session",
        a.task_id,
        role.as_str()
    );
    status(daemon, &a.task_id, None)
}

fn op_view(op_id: &str, op: &Op) -> v1::AgentSchedulerOp {
    let (result, message) = match &op.state {
        OpState::Applied { result } => (result.clone(), String::new()),
        OpState::Rejected { message } | OpState::Unknown { message } => {
            (String::new(), message.clone())
        }
        OpState::Reserved => (String::new(), String::new()),
    };
    v1::AgentSchedulerOp {
        op_id: op_id.to_owned(),
        kind: op.kind.as_str().into(),
        role: op.role.as_str().into(),
        session: op.session.clone(),
        state: op.state.name().into(),
        result,
        message,
        reserved_unix_ms: op.reserved_ms,
    }
}

fn status(daemon: &Daemon, task_id: &str, op: Option<&str>) -> Result<Resp, Error> {
    daemon.scheduler.read(|r| {
        let g = r.get(task_id).ok_or_else(|| refused(Refusal::NotFound))?;
        Ok(Resp::AgentScheduler(v1::AgentSchedulerStatus {
            task_id: g.scope.task_id.clone(),
            contract_sha256: g.scope.contract_sha256.clone(),
            provider: g.scope.provider.clone(),
            directory: g.scope.directory.clone(),
            model: g.scope.model.clone(),
            effort: g.scope.effort.clone(),
            issued_unix_ms: g.issued_ms,
            expires_unix_ms: g.expires_ms,
            revoked_unix_ms: g.revoked_ms.unwrap_or(0),
            live: g.live(now_ms()),
            producer_session: g.sessions.get(&Role::Producer).cloned().unwrap_or_default(),
            reviewer_session: g.sessions.get(&Role::Reviewer).cloned().unwrap_or_default(),
            creates: g.counts.creates,
            packages: g.counts.packages,
            reviews: g.counts.reviews,
            controls: g.counts.controls,
            ops: g.ops.iter().map(|(id, op)| op_view(id, op)).collect(),
            op: op.and_then(|id| g.ops.get(id).map(|o| op_view(id, o))),
            adopted: g.adopted.keys().map(|r| r.as_str().into()).collect(),
        }))
    })
}

fn op_result(status: Status, message: impl Into<String>, result_id: impl Into<String>) -> Resp {
    Resp::OpResult(v1::OpResult {
        status: status as i32,
        message: message.into(),
        result_id: result_id.into(),
        ..Default::default()
    })
}

/// What a recorded operation answers to the same request again. Never a
/// second attempt.
fn recorded(state: &OpState) -> Resp {
    match state {
        OpState::Applied { result } => op_result(Status::Duplicate, "", result.clone()),
        OpState::Rejected { message } => op_result(
            Status::Rejected,
            format!("{message}; this op_id was not started and is not retried"),
            "",
        ),
        OpState::Unknown { message } => op_result(
            Status::Unknown,
            format!("{message}; never replayed, reconcile it"),
            "",
        ),
        OpState::Reserved => op_result(
            Status::Unknown,
            "this operation is in flight; never replayed",
            "",
        ),
    }
}

async fn operate(daemon: &Arc<Daemon>, o: v1::AgentSchedulerOperate) -> Result<Resp, Error> {
    let scheduler = &daemon.scheduler;
    let scope = scheduler.read(|r| {
        r.get(&o.task_id)
            .map(|g| g.scope.clone())
            .ok_or_else(|| refused(Refusal::NotFound))
    })?;
    let parsed = parse(&o, &scope)?;
    let Ok(_running) = scheduler.running.try_lock() else {
        // Answer a repeat of a recorded request; anything else waits.
        return scheduler.read(|r| {
            let g = r
                .get(&o.task_id)
                .ok_or_else(|| refused(Refusal::NotFound))?;
            Ok(match g.lookup(&request(&o, &parsed)) {
                Ok(op) => recorded(&op.state),
                Err(Refusal::NotFound) => op_result(
                    Status::Rejected,
                    "another scheduler operation is running; nothing was reserved or sent",
                    "",
                ),
                Err(refusal) => return Err(refused(refusal)),
            })
        });
    };
    // Scope, liveness, quota and op_id are decided before anything else and
    // whatever the desktop's state; a repeat answers what was recorded.
    let dry_run = scheduler.read(|r| {
        let mut g = r
            .get(&o.task_id)
            .ok_or_else(|| refused(Refusal::NotFound))?
            .clone();
        g.reserve(&request(&o, &parsed), Vec::new(), now_ms())
            .map_err(refused)
    })?;
    if let Admission::Recorded(state) = dry_run {
        return Ok(recorded(&state));
    }
    preflight(daemon, &scope.provider)?;
    let baseline = if parsed.kind == Kind::Create {
        crate::native_create::baseline(daemon, &scope.provider, &scope.directory)
    } else {
        Vec::new()
    };
    // Reserved and stored before anything touches the desktop.
    let admission = scheduler.update(daemon.store(), |r| {
        let g = r
            .get_mut(&o.task_id)
            .ok_or_else(|| refused(Refusal::NotFound))?;
        g.reserve(&request(&o, &parsed), baseline, now_ms())
            .map_err(refused)
    })?;
    let reservation = match admission {
        Admission::Recorded(state) => return Ok(recorded(&state)),
        Admission::Fresh(reservation) => reservation,
    };
    let job = Job {
        provider: scope.provider.clone(),
        inner: parsed.inner.clone(),
        started: Arc::new(AtomicBool::new(false)),
    };
    let locked = daemon.platform().locked();
    log::info!(
        "scheduler: {} op={:?} role={} on a {} console",
        parsed.kind.as_str(),
        o.op_id,
        parsed.role.as_str(),
        if locked { "locked" } else { "unlocked" }
    );
    let result = if locked {
        daemon
            .locked_use
            .scheduled(daemon.clone(), reservation, job.clone())
            .await
    } else {
        unlocked(daemon, &o.task_id, &job).await
    };
    let outcome = classify(daemon, &o.op_id, &parsed, &job, &result);
    log::info!(
        "scheduler: op={:?} settled {:?}",
        o.op_id,
        match &outcome {
            Outcome::Applied(_) => "applied",
            Outcome::NotStarted(_) => "not started",
            Outcome::Unknown(_) => "unknown",
        }
    );
    scheduler.update(daemon.store(), |r| {
        r.get_mut(&o.task_id)
            .ok_or_else(|| refused(Refusal::NotFound))?
            .settle(&o.op_id, outcome.clone())
            .map_err(refused)
    })?;
    match outcome {
        Outcome::Applied(id) => Ok(match result {
            Ok(resp @ Resp::AgentChatOptions(_)) => resp,
            _ => op_result(Status::Applied, "", id),
        }),
        Outcome::NotStarted(message) => Ok(op_result(
            Status::Rejected,
            format!("{message}; not started, nothing was sent"),
            "",
        )),
        Outcome::Unknown(message) => {
            // The console is given back. Look for a receipt read-only, for
            // the operations that leave one.
            let deadline = tokio::time::Instant::now()
                + if matches!(parsed.kind, Kind::Create | Kind::Send) {
                    AFTER_RECEIPT
                } else {
                    Duration::ZERO
                };
            loop {
                if let Some(resolved) = resolve(daemon, &o, &parsed).await? {
                    return Ok(op_result(Status::Applied, "", resolved));
                }
                if tokio::time::Instant::now() >= deadline {
                    return Ok(op_result(
                        Status::Unknown,
                        format!("{message}; never replayed, reconcile it"),
                        "",
                    ));
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

/// Refusals that need no reservation: nothing would be attempted.
fn preflight(daemon: &Daemon, provider: &str) -> Result<(), Error> {
    if daemon.platform().accessibility() != Some(true) {
        return Err(Error::Unavailable(
            "Accessibility is not granted to musterd; nothing was reserved".into(),
        ));
    }
    let policy = daemon.policy().read().unwrap();
    let app = muster_core::apps::app(provider)
        .ok_or_else(|| Error::Invalid("unknown provider app".into()))?;
    if !policy.offer_agent || !policy.ax_allowlist.iter().any(|id| id == app.bundle_id) {
        return Err(Error::Unauthorized(
            "this Mac does not offer agent control of this app; nothing was reserved".into(),
        ));
    }
    if daemon.platform().locked() && !policy.allow_locked_control {
        return Err(Error::Unauthorized(
            "this Mac does not allow Locked Use; nothing was reserved".into(),
        ));
    }
    Ok(())
}

/// An unlocked console: only after local input has been quiet, while no
/// Locked Use owns the console, with the grant re-checked on every OS call.
async fn unlocked(daemon: &Arc<Daemon>, task_id: &str, job: &Job) -> Result<Resp, Error> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (daemon, task_id, job);
        Err(Error::Unavailable(
            "scheduler desktop control requires macOS".into(),
        ))
    }
    #[cfg(target_os = "macos")]
    {
        use muster_agent::idle_compact::{QUIET, QUIET_WAIT};
        use muster_platform::macos::input::user_input_idle;
        let checked = daemon.clone();
        let task = task_id.to_owned();
        let check = Arc::new(move || {
            !checked.platform().locked()
                && checked.platform().accessibility() == Some(true)
                && checked.scheduler.live(&task)
        });
        muster_agent::control_guard::scope(check, async {
            muster_agent::control_guard::wait_for_quiet(user_input_idle, QUIET, QUIET_WAIT).await?;
            let Ok(console) = daemon.locked_use.writes.clone().try_read_owned() else {
                return Err(Error::Conflict(
                    "a Locked Use transaction owns the console".into(),
                ));
            };
            let result = job.perform(daemon, IN_LEASE_RECEIPT).await;
            drop(console);
            result
        })
        .await
    }
}

/// Settles from evidence. An operation that never began was not sent. A
/// ledgered one is what its ledger row says: no row means it never reached
/// submission, a pending row is unknown. Reading options changes nothing;
/// setting them is unknown when it fails after it began.
fn classify(
    daemon: &Daemon,
    op_id: &str,
    parsed: &Parsed,
    job: &Job,
    result: &Result<Resp, Error>,
) -> Outcome {
    let failure = match result {
        Ok(Resp::OpResult(r)) => r.message.clone(),
        Ok(_) => String::new(),
        Err(e) => e.to_string(),
    };
    if !job.started.load(Ordering::SeqCst) {
        return Outcome::NotStarted(if failure.is_empty() {
            "the console was not available".into()
        } else {
            failure
        });
    }
    if let Some((kind, body)) = &parsed.ledger {
        let ledger = daemon.store().ledger();
        return match ledger.status(op_id) {
            Ok(None) => Outcome::NotStarted(failure),
            Ok(Some(_)) => match ledger.outcome(op_id, kind, body) {
                Ok(muster_store::Outcome::Applied(id)) => {
                    Outcome::Applied(String::from_utf8_lossy(&id).into_owned())
                }
                Ok(muster_store::Outcome::Rejected(reason)) => {
                    Outcome::NotStarted(String::from_utf8_lossy(&reason).into_owned())
                }
                Ok(muster_store::Outcome::Conflict) => {
                    Outcome::Unknown("the operation ledger holds a different request".into())
                }
                _ => Outcome::Unknown(if failure.is_empty() {
                    "no provider receipt yet".into()
                } else {
                    failure
                }),
            },
            Err(e) => Outcome::Unknown(format!("operation ledger unreadable: {e}")),
        };
    }
    match result {
        Ok(Resp::AgentChatOptions(options)) => Outcome::Applied(options_summary(options)),
        _ if parsed.kind == Kind::ReadOptions => Outcome::NotStarted(failure),
        _ => Outcome::Unknown(failure),
    }
}

fn options_summary(options: &v1::AgentChatOptions) -> String {
    let model = options
        .models
        .iter()
        .find(|m| m.selected)
        .map_or("", |m| m.name.as_str());
    format!("model={model};effort={}", options.effort)
}

/// Read-only: provider evidence for an operation that is reserved or
/// unknown. A creation's receipt is its unique new transcript; a send's or
/// an interrupt's is its confirmed ledger row. Binds nothing on ambiguity.
async fn resolve(
    daemon: &Arc<Daemon>,
    o: &v1::AgentSchedulerOperate,
    parsed: &Parsed,
) -> Result<Option<String>, Error> {
    let (state, baseline, others) = daemon.scheduler.read(|r| {
        let g = r
            .get(&o.task_id)
            .ok_or_else(|| refused(Refusal::NotFound))?;
        let op = g.lookup(&request(o, parsed)).map_err(refused)?;
        Ok((
            op.state.clone(),
            op.baseline.clone(),
            g.sessions.values().cloned().collect::<Vec<_>>(),
        ))
    })?;
    match state {
        OpState::Applied { result } => return Ok(Some(result)),
        OpState::Rejected { .. } => return Ok(None),
        OpState::Reserved | OpState::Unknown { .. } => {}
    }
    let Some((kind, body)) = &parsed.ledger else {
        return Ok(None);
    };
    let ledger = daemon.store().ledger();
    let evidence = match (&parsed.inner, ledger.outcome(&o.op_id, kind, body)?) {
        (_, muster_store::Outcome::Applied(id)) => String::from_utf8_lossy(&id).into_owned(),
        (_, muster_store::Outcome::Conflict | muster_store::Outcome::Rejected(_)) => {
            return Ok(None);
        }
        (Inner::Create(r), _) => {
            let mut exclude = baseline;
            exclude.extend(others);
            let Some(session) = crate::native_create::receipt(daemon, r, &exclude).await? else {
                return Ok(None);
            };
            // The ledger row (when the creation reached it) records the same receipt.
            if ledger.status(&o.op_id)?.is_some() {
                ledger.confirm(
                    &o.op_id,
                    kind,
                    &muster_store::Ledger::body_hash(body),
                    session.as_bytes(),
                )?;
            }
            session
        }
        _ => return Ok(None),
    };
    daemon.scheduler.update(daemon.store(), |r| {
        r.get_mut(&o.task_id)
            .ok_or_else(|| refused(Refusal::NotFound))?
            .reconcile(&o.op_id, evidence.clone())
            .map_err(refused)
    })?;
    log::info!(
        "scheduler: op={:?} reconciled from provider evidence",
        o.op_id
    );
    Ok(Some(evidence))
}

async fn reconcile(daemon: &Arc<Daemon>, o: v1::AgentSchedulerOperate) -> Result<Resp, Error> {
    let scope = daemon.scheduler.read(|r| {
        r.get(&o.task_id)
            .map(|g| g.scope.clone())
            .ok_or_else(|| refused(Refusal::NotFound))
    })?;
    let parsed = parse(&o, &scope)?;
    resolve(daemon, &o, &parsed).await?;
    status(daemon, &o.task_id, Some(&o.op_id))
}

/// Grants `task_id` (if needed) and reserves one creation in it, as `operate`
/// would before a scheduled lease. The job is a probe held while `hold` is set.
#[cfg(test)]
pub(crate) fn test_reservation(
    daemon: &Daemon,
    task_id: &str,
    op_id: &str,
    hold: Arc<AtomicBool>,
) -> (grant::Reservation, Job) {
    let scope = grant::Scope {
        task_id: task_id.into(),
        contract_sha256: "ab".repeat(32),
        provider: "claude".into(),
        directory: "/private/tmp".into(),
        model: "Model A".into(),
        effort: "High".into(),
    };
    let o = v1::AgentSchedulerOperate {
        task_id: task_id.into(),
        op_id: op_id.into(),
        role: "producer".into(),
        kind: "create".into(),
        provider: "claude".into(),
        directory: "/private/tmp".into(),
        text: "first package".into(),
        ..Default::default()
    };
    let parsed = parse(&o, &scope).unwrap();
    let scheduler = &daemon.scheduler;
    let reservation = scheduler
        .update(daemon.store(), |r| {
            if r.get(task_id).is_none() {
                r.grant(scope.clone(), now_ms(), 60_000).map_err(refused)?;
            }
            match r
                .get_mut(task_id)
                .unwrap()
                .reserve(&request(&o, &parsed), Vec::new(), now_ms())
                .map_err(refused)?
            {
                Admission::Fresh(reservation) => Ok(reservation),
                Admission::Recorded(_) => Err(Error::Conflict("recorded".into())),
            }
        })
        .unwrap();
    let job = Job {
        provider: "claude".into(),
        inner: Inner::Probe(hold),
        started: Arc::new(AtomicBool::new(false)),
    };
    (reservation, job)
}

#[cfg(test)]
pub(crate) fn test_revoke(daemon: &Daemon, task_id: &str) {
    daemon
        .scheduler
        .update(daemon.store(), |r| {
            r.revoke(task_id, now_ms()).map_err(refused)
        })
        .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use v1::agent_scheduler_request::Action;

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
        _dir: tempfile::TempDir,
        daemon: Arc<Daemon>,
        work: String,
    }

    async fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let work = dir.path().join("work");
        std::fs::create_dir(&work).unwrap();
        let work = std::fs::canonicalize(work)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let daemon = daemon(&dir.path().join("state")).await;
        Fixture {
            _dir: dir,
            daemon,
            work,
        }
    }

    fn grant(task: &str, directory: &str, ttl_seconds: u64) -> v1::AgentSchedulerRequest {
        v1::AgentSchedulerRequest {
            action: Some(Action::Grant(v1::AgentSchedulerGrant {
                task_id: task.into(),
                contract_sha256: "ab".repeat(32),
                provider: "claude".into(),
                directory: directory.into(),
                model: "Model A".into(),
                effort: "High".into(),
                ttl_seconds,
            })),
        }
    }

    fn task(action: fn(v1::AgentSchedulerTask) -> Action, id: &str) -> v1::AgentSchedulerRequest {
        v1::AgentSchedulerRequest {
            action: Some(action(v1::AgentSchedulerTask { task_id: id.into() })),
        }
    }

    fn op(
        f: &Fixture,
        op_id: &str,
        role: &str,
        kind: &str,
        session: &str,
    ) -> v1::AgentSchedulerOperate {
        let content = matches!(kind, "create" | "send");
        v1::AgentSchedulerOperate {
            task_id: "t1".into(),
            op_id: op_id.into(),
            role: role.into(),
            kind: kind.into(),
            provider: "claude".into(),
            directory: f.work.clone(),
            session: session.into(),
            text: if content {
                "package".into()
            } else {
                String::new()
            },
            ..Default::default()
        }
    }

    async fn operate(f: &Fixture, o: &v1::AgentSchedulerOperate) -> Result<Resp, Error> {
        call(
            &f.daemon,
            v1::AgentSchedulerRequest {
                action: Some(Action::Operate(o.clone())),
            },
        )
        .await
    }

    fn state(f: &Fixture) -> v1::AgentSchedulerStatus {
        match status(&f.daemon, "t1", None).unwrap() {
            Resp::AgentScheduler(s) => s,
            other => panic!("{other:?}"),
        }
    }

    /// Records `o` as an earlier operation that ended with `outcome`.
    fn record(f: &Fixture, o: &v1::AgentSchedulerOperate, outcome: Option<Outcome>) {
        let scope = f
            .daemon
            .scheduler
            .read(|r| Ok(r.get("t1").unwrap().scope.clone()))
            .unwrap();
        let parsed = parse(o, &scope).unwrap();
        f.daemon
            .scheduler
            .update(f.daemon.store(), |r| {
                let g = r.get_mut("t1").unwrap();
                assert!(matches!(
                    g.reserve(&request(o, &parsed), vec!["old".into()], now_ms()),
                    Ok(Admission::Fresh(_))
                ));
                if let Some(outcome) = outcome {
                    g.settle(&o.op_id, outcome).map_err(refused)?;
                }
                Ok(())
            })
            .unwrap();
    }

    fn op_status(resp: &Resp) -> (i32, String, String) {
        match resp {
            Resp::OpResult(r) => (r.status, r.message.clone(), r.result_id.clone()),
            other => panic!("{other:?}"),
        }
    }

    fn adoption(
        f: &Fixture,
        role: &str,
        provider: &str,
        directory: &str,
    ) -> v1::AgentSchedulerRequest {
        let _ = f;
        v1::AgentSchedulerRequest {
            action: Some(Action::Adopt(v1::AgentSchedulerAdopt {
                task_id: "t1".into(),
                role: role.into(),
                provider: provider.into(),
                directory: directory.into(),
                session: "s-existing".into(),
            })),
        }
    }

    #[tokio::test]
    async fn adoption_is_refused_outside_the_grant_and_without_the_registry() {
        let f = fixture().await;
        let work = f.work.clone();
        let error = call(&f.daemon, adoption(&f, "producer", "claude", &work))
            .await
            .unwrap_err();
        assert!(matches!(error, Error::NotFound(_)), "{error}");
        call(&f.daemon, grant("t1", &work, 3600)).await.unwrap();
        for (role, provider, directory) in [
            ("producer", "codex", work.as_str()),
            ("producer", "claude", "/private/tmp"),
            ("owner", "claude", work.as_str()),
        ] {
            let error = call(&f.daemon, adoption(&f, role, provider, directory))
                .await
                .unwrap_err();
            assert!(
                matches!(error, Error::Unauthorized(_) | Error::Invalid(_)),
                "{error}"
            );
        }
        // No provider registry to check it in: nothing is adopted.
        let error = call(&f.daemon, adoption(&f, "producer", "claude", &work))
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Unavailable(_)), "{error}");
        let s = state(&f);
        assert!(s.producer_session.is_empty() && s.adopted.is_empty());
        // A role whose creation may still bind a receipt cannot adopt.
        record(
            &f,
            &op(&f, "c1", "reviewer", "create", ""),
            Some(Outcome::Unknown("x".into())),
        );
        let error = call(&f.daemon, adoption(&f, "reviewer", "claude", &work))
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Conflict(_)), "{error}");
        f.daemon.shutdown().await;
    }

    #[tokio::test]
    async fn a_done_adoption_answers_again_until_the_grant_ends() {
        let f = fixture().await;
        let work = f.work.clone();
        call(&f.daemon, grant("t1", &work, 3600)).await.unwrap();
        f.daemon
            .scheduler
            .update(f.daemon.store(), |r| {
                r.get_mut("t1")
                    .unwrap()
                    .adopt(Role::Producer, "s-existing", now_ms())
                    .map_err(refused)
            })
            .unwrap();
        // A restarted caller asking again needs no registry and changes nothing.
        let Resp::AgentScheduler(s) = call(&f.daemon, adoption(&f, "producer", "claude", &work))
            .await
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(
            (s.producer_session.as_str(), s.adopted.clone()),
            ("s-existing", vec!["producer".to_owned()])
        );
        assert_eq!(s.creates, 0);
        // Sends must name it; the adoption is not an operation and costs no package.
        let error = operate(&f, &op(&f, "p1", "producer", "send", "s-other"))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("outside"), "{error}");
        let error = call(&f.daemon, adoption(&f, "reviewer", "claude", &work))
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Conflict(_)), "{error}");
        call(&f.daemon, task(Action::Revoke, "t1")).await.unwrap();
        let error = call(&f.daemon, adoption(&f, "producer", "claude", &work))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("revoked"), "{error}");
        f.daemon.shutdown().await;
    }

    #[test]
    fn only_this_scopes_code_session_in_its_directory_is_adoptable() {
        let scope = grant::Scope {
            task_id: "t1".into(),
            contract_sha256: "ab".repeat(32),
            provider: "claude".into(),
            directory: "/work".into(),
            model: "Model A".into(),
            effort: "High".into(),
        };
        let good = v1::AgentSession {
            id: "s".into(),
            provider: "claude".into(),
            cwd: "/work".into(),
            mode: "code".into(),
            ..Default::default()
        };
        assert!(adoptable(std::slice::from_ref(&good), &scope, "s"));
        assert!(!adoptable(std::slice::from_ref(&good), &scope, "other"));
        let variants: [fn(&mut v1::AgentSession); 6] = [
            |s| s.provider = "codex".into(),
            |s| s.cwd = "/work/sub".into(),
            |s| s.mode = "cloud".into(),
            |s| s.archived = true,
            |s| s.read_only = true,
            |s| s.parent_session_id = "p".into(),
        ];
        for change in variants {
            let mut session = good.clone();
            change(&mut session);
            assert!(!adoptable(&[session], &scope, "s"));
        }
    }

    #[tokio::test]
    async fn one_exact_task_is_granted_and_its_status_is_reported() {
        let f = fixture().await;
        let Resp::AgentScheduler(s) = call(&f.daemon, grant("t1", &f.work, 3600)).await.unwrap()
        else {
            panic!()
        };
        assert!(s.live);
        assert_eq!(s.directory, f.work);
        assert_eq!(s.expires_unix_ms - s.issued_unix_ms, 3_600_000);
        // The identical grant again is the same grant, never extended.
        let Resp::AgentScheduler(again) =
            call(&f.daemon, grant("t1", &f.work, 7200)).await.unwrap()
        else {
            panic!()
        };
        assert_eq!(again.expires_unix_ms, s.expires_unix_ms);
        // No second task, no other scope, no missing directory, no other provider.
        let mut other_contract = grant("t1", &f.work, 3600);
        if let Some(Action::Grant(g)) = &mut other_contract.action {
            g.contract_sha256 = "cd".repeat(32);
        }
        for bad in [
            grant("t2", &f.work, 3600),
            other_contract,
            grant("t1", "/nonexistent/muster", 3600),
            grant("t1", &f.work, 25 * 3600),
        ] {
            assert!(call(&f.daemon, bad).await.is_err());
        }
        let mut other = grant("t1", &f.work, 3600);
        if let Some(Action::Grant(g)) = &mut other.action {
            g.provider = "gemini".into();
        }
        assert!(call(&f.daemon, other).await.is_err());
        assert!(matches!(
            call(&f.daemon, task(Action::Status, "t9")).await,
            Err(Error::NotFound(_))
        ));
        f.daemon.shutdown().await;
    }

    #[tokio::test]
    async fn out_of_scope_requests_are_refused_before_anything_is_reserved() {
        let f = fixture().await;
        call(&f.daemon, grant("t1", &f.work, 3600)).await.unwrap();
        let other_dir = op(&f, "a", "producer", "create", "");
        let mut other_dir = other_dir;
        other_dir.directory = "/private/tmp".into();
        let mut other_provider = op(&f, "b", "producer", "create", "");
        other_provider.provider = "codex".into();
        let mut other_task = op(&f, "c", "producer", "create", "");
        other_task.task_id = "t2".into();
        let unbound = op(&f, "d", "producer", "send", "some-session");
        let mut model = op(&f, "e", "producer", "set_options", "some-session");
        model.model = "Other".into();
        model.effort = "High".into();
        let mut named = op(&f, "f", "producer", "create", "");
        named.session = "chosen-session".into();
        let mut text = op(&f, "g", "producer", "interrupt", "x");
        text.text = "not a package".into();
        let mut role = op(&f, "h", "producer", "create", "");
        role.role = "admin".into();
        for o in [
            other_dir,
            other_provider,
            other_task,
            unbound,
            model,
            named,
            text,
            role,
        ] {
            let error = operate(&f, &o).await.unwrap_err();
            assert!(
                matches!(
                    error,
                    Error::Unauthorized(_)
                        | Error::NotFound(_)
                        | Error::Invalid(_)
                        | Error::Conflict(_)
                ),
                "{}: {error}",
                o.op_id
            );
        }
        let s = state(&f);
        assert!(s.ops.is_empty());
        assert_eq!((s.creates, s.packages, s.controls), (0, 0, 0));
        f.daemon.shutdown().await;
    }

    #[tokio::test]
    async fn a_desktop_that_cannot_be_driven_reserves_nothing() {
        let f = fixture().await;
        call(&f.daemon, grant("t1", &f.work, 3600)).await.unwrap();
        // DefaultPlatform has no Accessibility.
        let error = operate(&f, &op(&f, "a", "producer", "create", ""))
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("nothing was reserved"),
            "{error}"
        );
        let s = state(&f);
        assert!(s.ops.is_empty());
        assert_eq!(s.creates, 0);
        f.daemon.shutdown().await;
    }

    #[tokio::test]
    async fn expiry_revocation_and_quota_stop_new_operations() {
        let f = fixture().await;
        call(&f.daemon, grant("t1", &f.work, 1)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(1100)).await;
        let error = operate(&f, &op(&f, "a", "producer", "create", ""))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("expired"), "{error}");
        assert!(!state(&f).live);

        let f = fixture().await;
        call(&f.daemon, grant("t1", &f.work, 3600)).await.unwrap();
        // A create already applied binds the producer; packages fill the quota.
        record(
            &f,
            &op(&f, "c1", "producer", "create", ""),
            Some(Outcome::Applied("s-p".into())),
        );
        for n in 0..(grant::MAX_PACKAGES - 1) {
            record(
                &f,
                &op(&f, &format!("p{n}"), "producer", "send", "s-p"),
                Some(Outcome::Applied("s-p".into())),
            );
        }
        let error = operate(&f, &op(&f, "p-over", "producer", "send", "s-p"))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("quota"), "{error}");
        call(&f.daemon, task(Action::Revoke, "t1")).await.unwrap();
        let error = operate(&f, &op(&f, "after", "producer", "interrupt", "s-p"))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("revoked"), "{error}");
        // A revoked task is never granted again.
        assert!(call(&f.daemon, grant("t1", &f.work, 3600)).await.is_err());
        f.daemon.shutdown().await;
    }

    #[tokio::test]
    async fn a_recorded_op_id_answers_its_record_and_never_runs_again() {
        let f = fixture().await;
        call(&f.daemon, grant("t1", &f.work, 3600)).await.unwrap();
        let create = op(&f, "c1", "producer", "create", "");
        record(&f, &create, Some(Outcome::Applied("s-p".into())));
        let (status, _, result) = op_status(&operate(&f, &create).await.unwrap());
        assert_eq!(status, Status::Duplicate as i32);
        assert_eq!(result, "s-p");
        // The same op_id for different content is a conflict, not a new run.
        let mut changed = create.clone();
        changed.text = "another package".into();
        assert!(matches!(
            operate(&f, &changed).await,
            Err(Error::Conflict(_))
        ));
        let send = op(&f, "s1", "producer", "send", "s-p");
        record(&f, &send, Some(Outcome::Unknown("no receipt".into())));
        let (status, message, _) = op_status(&operate(&f, &send).await.unwrap());
        assert_eq!(status, Status::Unknown as i32);
        assert!(message.contains("never replayed"), "{message}");
        let refused_send = op(&f, "s2", "producer", "send", "s-p");
        record(&f, &refused_send, Some(Outcome::NotStarted("busy".into())));
        let (status, _, _) = op_status(&operate(&f, &refused_send).await.unwrap());
        assert_eq!(status, Status::Rejected as i32);
        // Even after the grant is revoked, a repeat answers its record.
        call(&f.daemon, task(Action::Revoke, "t1")).await.unwrap();
        let (status, _, result) = op_status(&operate(&f, &create).await.unwrap());
        assert_eq!((status, result.as_str()), (Status::Duplicate as i32, "s-p"));
        let s = state(&f);
        assert_eq!(s.creates, 1);
        assert_eq!(s.packages, 2);
        f.daemon.shutdown().await;
    }

    #[tokio::test]
    async fn a_restart_turns_a_reserved_operation_unknown_and_keeps_its_count() {
        let f = fixture().await;
        call(&f.daemon, grant("t1", &f.work, 3600)).await.unwrap();
        let create = op(&f, "c1", "producer", "create", "");
        record(&f, &create, None);
        assert_eq!(state(&f).ops[0].state, "reserved");
        let reloaded = Scheduler::load(f.daemon.store());
        let g = reloaded.read(|r| Ok(r.get("t1").unwrap().clone())).unwrap();
        assert!(matches!(g.ops["c1"].state, OpState::Unknown { .. }));
        assert_eq!(g.counts.creates, 1);
        // Stored as unknown, so a further restart reads the same.
        let again = Scheduler::load(f.daemon.store());
        assert_eq!(again.read(|r| Ok(r.get("t1").unwrap().clone())).unwrap(), g);
        f.daemon.shutdown().await;
    }

    #[tokio::test]
    async fn unreadable_grants_fail_closed_and_are_never_overwritten() {
        let f = fixture().await;
        f.daemon.store().kv_set(KEY, b"{not json").unwrap();
        let broken = Scheduler::load(f.daemon.store());
        assert!(!broken.live("t1"));
        assert!(broken.read(|_| Ok(())).is_err());
        assert!(broken.update(f.daemon.store(), |_| Ok(())).is_err());
        assert_eq!(
            f.daemon.store().kv_get(KEY).unwrap().as_deref(),
            Some(&b"{not json"[..])
        );
        f.daemon.shutdown().await;
    }

    #[tokio::test]
    async fn outcomes_are_settled_from_evidence_only() {
        let f = fixture().await;
        call(&f.daemon, grant("t1", &f.work, 3600)).await.unwrap();
        let scope = f
            .daemon
            .scheduler
            .read(|r| Ok(r.get("t1").unwrap().scope.clone()))
            .unwrap();
        let job = |p: &Parsed, started: bool| Job {
            provider: "claude".into(),
            inner: p.inner.clone(),
            started: Arc::new(AtomicBool::new(started)),
        };
        let ledger = f.daemon.store().ledger();
        let failed: Result<Resp, Error> = Err(Error::UnknownOutcome("lost".into()));

        let send = op(&f, "s1", "producer", "send", "s-p");
        let p = parse(&send, &scope).unwrap();
        let (kind, body) = p.ledger.clone().unwrap();
        // Never began, or began without a ledger row: nothing was sent.
        assert!(matches!(
            classify(&f.daemon, "s1", &p, &job(&p, false), &failed),
            Outcome::NotStarted(_)
        ));
        assert!(matches!(
            classify(&f.daemon, "s1", &p, &job(&p, true), &failed),
            Outcome::NotStarted(_)
        ));
        // A pending row is unknown; a confirmed row is applied.
        ledger.begin("s1", kind, &body).unwrap();
        assert!(matches!(
            classify(&f.daemon, "s1", &p, &job(&p, true), &failed),
            Outcome::Unknown(_)
        ));
        ledger
            .confirm("s1", kind, &muster_store::Ledger::body_hash(&body), b"s-p")
            .unwrap();
        assert_eq!(
            classify(&f.daemon, "s1", &p, &job(&p, true), &failed),
            Outcome::Applied("s-p".into())
        );
        // A rejected row proves nothing was submitted.
        let send2 = op(&f, "s2", "producer", "send", "s-p");
        let p2 = parse(&send2, &scope).unwrap();
        let (kind2, body2) = p2.ledger.clone().unwrap();
        ledger.begin("s2", kind2, &body2).unwrap();
        ledger.reject("s2", b"refused").unwrap();
        assert!(matches!(
            classify(&f.daemon, "s2", &p2, &job(&p2, true), &failed),
            Outcome::NotStarted(_)
        ));
        // Options: reading changes nothing; a failed set after it began is unknown.
        let read = op(&f, "r1", "producer", "read_options", "s-p");
        let pr = parse(&read, &scope).unwrap();
        assert!(matches!(
            classify(&f.daemon, "r1", &pr, &job(&pr, true), &failed),
            Outcome::NotStarted(_)
        ));
        let mut set = op(&f, "o1", "producer", "set_options", "s-p");
        set.model = "Model A".into();
        set.effort = "High".into();
        let ps = parse(&set, &scope).unwrap();
        assert!(matches!(
            classify(&f.daemon, "o1", &ps, &job(&ps, true), &failed),
            Outcome::Unknown(_)
        ));
        let options = Ok(Resp::AgentChatOptions(v1::AgentChatOptions {
            models: vec![v1::AgentChatOption {
                name: "Model A".into(),
                selected: true,
                ..Default::default()
            }],
            effort: "High".into(),
            ..Default::default()
        }));
        assert_eq!(
            classify(&f.daemon, "o1", &ps, &job(&ps, true), &options),
            Outcome::Applied("model=Model A;effort=High".into())
        );
        f.daemon.shutdown().await;
    }

    #[tokio::test]
    async fn reconcile_binds_an_unknown_creation_only_from_its_ledger_receipt() {
        let f = fixture().await;
        call(&f.daemon, grant("t1", &f.work, 3600)).await.unwrap();
        let create = op(&f, "c1", "producer", "create", "");
        record(&f, &create, Some(Outcome::Unknown("no receipt".into())));
        let scope = state(&f);
        assert_eq!(scope.producer_session, "");
        let p = parse(
            &create,
            &f.daemon
                .scheduler
                .read(|r| Ok(r.get("t1").unwrap().scope.clone()))
                .unwrap(),
        )
        .unwrap();
        let (kind, body) = p.ledger.clone().unwrap();
        let ledger = f.daemon.store().ledger();
        ledger.begin("c1", kind, &body).unwrap();
        // Still pending: no agent service here, so no transcript evidence.
        let reconcile_req = v1::AgentSchedulerRequest {
            action: Some(Action::Reconcile(create.clone())),
        };
        assert!(call(&f.daemon, reconcile_req.clone()).await.is_err());
        assert_eq!(state(&f).producer_session, "");
        ledger
            .confirm(
                "c1",
                kind,
                &muster_store::Ledger::body_hash(&body),
                b"s-new",
            )
            .unwrap();
        let Resp::AgentScheduler(s) = call(&f.daemon, reconcile_req).await.unwrap() else {
            panic!()
        };
        assert_eq!(s.producer_session, "s-new");
        assert_eq!(s.op.unwrap().state, "applied");
        // Reconciling an applied operation again changes nothing.
        let again = v1::AgentSchedulerRequest {
            action: Some(Action::Reconcile(create)),
        };
        call(&f.daemon, again).await.unwrap();
        assert_eq!(state(&f).producer_session, "s-new");
        f.daemon.shutdown().await;
    }
}
