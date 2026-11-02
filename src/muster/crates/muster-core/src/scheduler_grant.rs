//! A task-scoped scheduler grant (ADR 0095). This Mac's local owner lets one
//! named task drive at most two desktop sessions of one provider app in one
//! directory, within fixed quotas and a deadline, including under Locked Use
//! while the console is locked. Pure state: the daemon persists it, reserves
//! every desktop operation here before it acts, re-checks liveness on every
//! OS call, and reports how each operation ended.
//!
//! The grant is a bound on what a scheduler may ask for, not a boundary
//! against other processes of the same user (docs/ARCHITECTURE.md threat
//! model). It carries no GPU, data-egress or spending authority.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// The longest a grant may live.
pub const MAX_TTL_MS: u64 = 24 * 60 * 60 * 1000;
/// A producer and one independent reviewer.
pub const MAX_SESSIONS: u32 = 2;
/// Content-bearing operations: creations (whose first prompt is a package)
/// and sends.
pub const MAX_PACKAGES: u32 = 18;
/// Of those, the reviewer's.
pub const MAX_REVIEWS: u32 = 6;
/// Option reads and sets and interrupts: bounded so a loop cannot drive the
/// console without end.
pub const MAX_CONTROLS: u32 = 72;
const MAX_TASK_ID: usize = 96;
const MAX_OPTION: usize = 64;
const DOMAIN: &[u8] = b"MUSTER-SCHEDULED-OPERATION-V1\0";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Producer,
    Reviewer,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Producer => "producer",
            Role::Reviewer => "reviewer",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Create,
    Send,
    ReadOptions,
    SetOptions,
    Interrupt,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Create => "create",
            Kind::Send => "send",
            Kind::ReadOptions => "read_options",
            Kind::SetOptions => "set_options",
            Kind::Interrupt => "interrupt",
        }
    }

    fn package(self) -> bool {
        matches!(self, Kind::Create | Kind::Send)
    }
}

/// What the owner granted. Immutable once issued.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    pub task_id: String,
    /// Lowercase hex SHA-256 of the task contract the owner approved.
    pub contract_sha256: String,
    pub provider: String,
    /// Canonical absolute path; the daemon resolves it before granting.
    pub directory: String,
    /// The only model and effort an option set may choose, as the app names them.
    pub model: String,
    pub effort: String,
}

impl Scope {
    pub fn validate(&self) -> Result<(), Refusal> {
        let id_ok = !self.task_id.is_empty()
            && self.task_id.len() <= MAX_TASK_ID
            && self
                .task_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b':'));
        if !id_ok {
            return Err(Refusal::Invalid("task id must be 1-96 of [A-Za-z0-9._:-]"));
        }
        if self.contract_sha256.len() != 64
            || !self
                .contract_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Refusal::Invalid(
                "contract digest must be 64 lowercase hex characters",
            ));
        }
        if !matches!(self.provider.as_str(), "claude" | "codex") {
            return Err(Refusal::Invalid("provider has no native desktop creation"));
        }
        if !normalized_absolute(&self.directory) {
            return Err(Refusal::Invalid(
                "directory must be a normalized absolute path",
            ));
        }
        for option in [&self.model, &self.effort] {
            if option.trim().is_empty() || option.len() > MAX_OPTION || option.trim() != option {
                return Err(Refusal::Invalid("model and effort must be named exactly"));
            }
        }
        Ok(())
    }
}

fn normalized_absolute(path: &str) -> bool {
    path.starts_with('/')
        && (path == "/"
            || (!path.ends_with('/')
                && path[1..]
                    .split('/')
                    .all(|part| !part.is_empty() && part != "." && part != "..")))
        && !path.contains('\0')
}

/// Why a grant or an operation was refused. Every refusal happens before any
/// desktop action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    Invalid(&'static str),
    /// The request names a provider, directory or session outside the grant.
    OutOfScope(&'static str),
    Expired,
    Revoked,
    Quota(&'static str),
    /// Same op_id with a different request, or a second grant.
    Conflict(&'static str),
    NotFound,
}

impl Refusal {
    pub fn message(&self) -> String {
        match self {
            Refusal::Invalid(m) => format!("invalid scheduler request: {m}"),
            Refusal::OutOfScope(m) => format!("outside the task grant: {m}"),
            Refusal::Expired => "the task grant has expired".into(),
            Refusal::Revoked => "the task grant was revoked".into(),
            Refusal::Quota(m) => format!("task grant quota exhausted: {m}"),
            Refusal::Conflict(m) => format!("scheduler conflict: {m}"),
            Refusal::NotFound => "no grant for this task".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum OpState {
    /// Reserved before the desktop action; budget held.
    Reserved,
    Applied {
        result: String,
    },
    /// Proven not started; its budget was released.
    Rejected {
        message: String,
    },
    /// A submission may have happened. Budget stays spent; never replayed.
    Unknown {
        message: String,
    },
}

impl OpState {
    pub fn name(&self) -> &'static str {
        match self {
            OpState::Reserved => "reserved",
            OpState::Applied { .. } => "applied",
            OpState::Rejected { .. } => "rejected",
            OpState::Unknown { .. } => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Op {
    pub kind: Kind,
    pub role: Role,
    /// The bound session it acts on; empty for a creation.
    pub session: String,
    /// Hex SHA-256 of the exact inner request.
    pub body: String,
    pub reserved_ms: u64,
    /// For a creation: the provider's sessions in the directory before it,
    /// none of which can be its receipt.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub baseline: Vec<String>,
    pub state: OpState,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    pub creates: u32,
    pub packages: u32,
    pub reviews: u32,
    pub controls: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    pub scope: Scope,
    pub issued_ms: u64,
    pub expires_ms: u64,
    pub revoked_ms: Option<u64>,
    /// Sessions bound by creation receipts, never by recency.
    pub sessions: BTreeMap<Role, String>,
    pub ops: BTreeMap<String, Op>,
    pub counts: Counts,
}

/// One desktop operation as the scheduler asks for it. Provider, directory
/// and session are the caller's statement of intent; each must equal the
/// grant's own binding.
#[derive(Clone, Debug)]
pub struct Request<'a> {
    pub op_id: &'a str,
    pub kind: Kind,
    pub role: Role,
    pub provider: &'a str,
    pub directory: &'a str,
    /// Empty for a creation; the role's bound session otherwise.
    pub session: &'a str,
    pub model: &'a str,
    pub effort: &'a str,
    /// SHA-256 of the exact inner request.
    pub body: [u8; 32],
}

/// A reserved operation: the only way to open a scheduled Locked Use lease
/// (`locked_use::Gate::issue_scheduled`). Not Clone; only `Grant::reserve`
/// makes one.
#[derive(Debug)]
pub struct Reservation {
    task_id: String,
    op_id: String,
    provider: String,
    kind: Kind,
    role: Role,
    session: String,
    digest: [u8; 32],
}

impl Reservation {
    pub fn task_id(&self) -> &str {
        &self.task_id
    }
    pub fn op_id(&self) -> &str {
        &self.op_id
    }
    pub fn provider(&self) -> &str {
        &self.provider
    }
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub fn role(&self) -> Role {
        self.role
    }
    pub fn session(&self) -> &str {
        &self.session
    }
    /// The operation digest a scheduled lease binds.
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
}

#[derive(Debug)]
pub enum Admission {
    /// Run it now; report the outcome with `settle`.
    Fresh(Reservation),
    /// This op_id was seen before with the same request: its recorded state.
    /// Never a second attempt.
    Recorded(OpState),
}

/// How a reserved operation ended, as the daemon proved it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Applied(String),
    NotStarted(String),
    Unknown(String),
}

impl Grant {
    pub fn new(scope: Scope, now_ms: u64, ttl_ms: u64) -> Result<Self, Refusal> {
        scope.validate()?;
        if ttl_ms == 0 || ttl_ms > MAX_TTL_MS {
            return Err(Refusal::Invalid("grant lifetime must be within 24 hours"));
        }
        Ok(Self {
            scope,
            issued_ms: now_ms,
            expires_ms: now_ms.saturating_add(ttl_ms),
            revoked_ms: None,
            sessions: BTreeMap::new(),
            ops: BTreeMap::new(),
            counts: Counts::default(),
        })
    }

    pub fn live(&self, now_ms: u64) -> bool {
        self.revoked_ms.is_none() && now_ms < self.expires_ms
    }

    fn liveness(&self, now_ms: u64) -> Result<(), Refusal> {
        if self.revoked_ms.is_some() {
            Err(Refusal::Revoked)
        } else if now_ms >= self.expires_ms {
            Err(Refusal::Expired)
        } else {
            Ok(())
        }
    }

    pub fn revoke(&mut self, now_ms: u64) {
        self.revoked_ms.get_or_insert(now_ms);
    }

    /// The recorded operation this exact request made, read-only.
    pub fn lookup(&self, request: &Request) -> Result<&Op, Refusal> {
        let op = self.ops.get(request.op_id).ok_or(Refusal::NotFound)?;
        let session = match request.kind {
            Kind::Create => "",
            _ => request.session,
        };
        if op.kind == request.kind
            && op.role == request.role
            && op.session == session
            && op.body == hex::encode(request.body)
        {
            Ok(op)
        } else {
            Err(Refusal::Conflict("op_id reused with a different request"))
        }
    }

    /// Admits one operation and holds its budget before any desktop action,
    /// or returns what an earlier attempt with this op_id recorded.
    pub fn reserve(
        &mut self,
        request: &Request,
        baseline: Vec<String>,
        now_ms: u64,
    ) -> Result<Admission, Refusal> {
        if request.op_id.is_empty() || request.op_id.len() > 128 {
            return Err(Refusal::Invalid("op_id must be 1-128 bytes"));
        }
        let session = match request.kind {
            Kind::Create => "",
            _ => request.session,
        };
        let body = hex::encode(request.body);
        if self.ops.contains_key(request.op_id) {
            return self
                .lookup(request)
                .map(|op| Admission::Recorded(op.state.clone()));
        }
        self.liveness(now_ms)?;
        if request.provider != self.scope.provider {
            return Err(Refusal::OutOfScope("provider"));
        }
        if request.directory != self.scope.directory {
            return Err(Refusal::OutOfScope("directory"));
        }
        match request.kind {
            Kind::Create => {
                if !request.session.is_empty() {
                    return Err(Refusal::OutOfScope("a creation names no session"));
                }
                if self.sessions.contains_key(&request.role)
                    || self.ops.values().any(|op| {
                        op.kind == Kind::Create
                            && op.role == request.role
                            && !matches!(op.state, OpState::Rejected { .. })
                    })
                {
                    return Err(Refusal::Quota("this role already has its session"));
                }
                if self.counts.creates >= MAX_SESSIONS {
                    return Err(Refusal::Quota("sessions"));
                }
            }
            _ => {
                if self.sessions.get(&request.role).map(String::as_str) != Some(request.session)
                    || request.session.is_empty()
                {
                    return Err(Refusal::OutOfScope(
                        "session is not the one this role's creation receipt bound",
                    ));
                }
            }
        }
        if request.kind == Kind::SetOptions
            && (request.model != self.scope.model || request.effort != self.scope.effort)
        {
            return Err(Refusal::OutOfScope(
                "only the granted model and effort may be set",
            ));
        }
        if request.kind.package() {
            if self.counts.packages >= MAX_PACKAGES {
                return Err(Refusal::Quota("work packages"));
            }
            if request.role == Role::Reviewer && self.counts.reviews >= MAX_REVIEWS {
                return Err(Refusal::Quota("review packages"));
            }
        } else if self.counts.controls >= MAX_CONTROLS {
            return Err(Refusal::Quota("option and interrupt operations"));
        }
        self.charge(request.kind, request.role, 1);
        self.ops.insert(
            request.op_id.to_owned(),
            Op {
                kind: request.kind,
                role: request.role,
                session: session.to_owned(),
                body,
                reserved_ms: now_ms,
                baseline: if request.kind == Kind::Create {
                    baseline
                } else {
                    Vec::new()
                },
                state: OpState::Reserved,
            },
        );
        Ok(Admission::Fresh(Reservation {
            task_id: self.scope.task_id.clone(),
            op_id: request.op_id.to_owned(),
            provider: self.scope.provider.clone(),
            kind: request.kind,
            role: request.role,
            session: session.to_owned(),
            digest: digest(
                &self.scope,
                request.op_id,
                request.kind,
                request.role,
                session,
                &request.body,
            ),
        }))
    }

    fn charge(&mut self, kind: Kind, role: Role, sign: i32) {
        let apply = |n: &mut u32| {
            *n = if sign > 0 {
                n.saturating_add(1)
            } else {
                n.saturating_sub(1)
            }
        };
        if kind == Kind::Create {
            apply(&mut self.counts.creates);
        }
        if kind.package() {
            apply(&mut self.counts.packages);
            if role == Role::Reviewer {
                apply(&mut self.counts.reviews);
            }
        } else {
            apply(&mut self.counts.controls);
        }
    }

    /// Records how a reserved operation ended. Only a proven non-start frees
    /// its budget; an unknown outcome keeps it spent. A creation's receipt
    /// binds its role's session.
    pub fn settle(&mut self, op_id: &str, outcome: Outcome) -> Result<(), Refusal> {
        let op = self.ops.get(op_id).ok_or(Refusal::NotFound)?;
        if op.state != OpState::Reserved {
            return Err(Refusal::Conflict("operation already settled"));
        }
        let (kind, role) = (op.kind, op.role);
        match outcome {
            Outcome::Applied(result) => {
                if kind == Kind::Create {
                    return self.bind(op_id, result);
                }
                self.ops.get_mut(op_id).unwrap().state = OpState::Applied { result };
            }
            Outcome::NotStarted(message) => {
                self.charge(kind, role, -1);
                self.ops.get_mut(op_id).unwrap().state = OpState::Rejected { message };
            }
            Outcome::Unknown(message) => {
                self.ops.get_mut(op_id).unwrap().state = OpState::Unknown { message };
            }
        }
        Ok(())
    }

    /// Read-only evidence after the fact (a transcript receipt or a ledger
    /// confirmation) resolves a reserved or unknown operation to applied. It
    /// never re-runs anything and never turns a rejection into success.
    pub fn reconcile(&mut self, op_id: &str, result: String) -> Result<(), Refusal> {
        let op = self.ops.get(op_id).ok_or(Refusal::NotFound)?;
        match &op.state {
            OpState::Applied { result: earlier } if *earlier == result => Ok(()),
            OpState::Reserved | OpState::Unknown { .. } if op.kind == Kind::Create => {
                self.bind(op_id, result)
            }
            OpState::Reserved | OpState::Unknown { .. } => {
                self.ops.get_mut(op_id).unwrap().state = OpState::Applied { result };
                Ok(())
            }
            _ => Err(Refusal::Conflict("operation is settled differently")),
        }
    }

    fn bind(&mut self, op_id: &str, session: String) -> Result<(), Refusal> {
        let role = self.ops[op_id].role;
        if session.is_empty()
            || self.ops[op_id].baseline.contains(&session)
            || self
                .sessions
                .iter()
                .any(|(r, s)| *r != role && *s == session)
            || self.sessions.get(&role).is_some_and(|s| *s != session)
        {
            return Err(Refusal::Conflict(
                "creation receipt is not a new, unbound session",
            ));
        }
        self.sessions.insert(role, session.clone());
        self.ops.get_mut(op_id).unwrap().state = OpState::Applied { result: session };
        Ok(())
    }

    /// After a daemon restart nothing is in flight: an operation still
    /// reserved may have been submitted, so it becomes unknown, never retried.
    pub fn recover(&mut self) {
        for op in self.ops.values_mut() {
            if op.state == OpState::Reserved {
                op.state = OpState::Unknown {
                    message: "musterd restarted during the operation; never replayed".into(),
                };
            }
        }
    }
}

/// The digest a scheduled lease binds: the grant's scope and the exact
/// operation, domain-separated from every signed and unattended operation.
pub fn digest(
    scope: &Scope,
    op_id: &str,
    kind: Kind,
    role: Role,
    session: &str,
    body: &[u8; 32],
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(DOMAIN);
    for part in [
        scope.task_id.as_str(),
        scope.contract_sha256.as_str(),
        scope.provider.as_str(),
        scope.directory.as_str(),
        op_id,
        kind.as_str(),
        role.as_str(),
        session,
    ] {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    hash.update(body);
    hash.finalize().into()
}

/// Every grant this Mac issued. At most one is live; a task id is never
/// granted twice, so an ended grant cannot be revived with fresh quotas.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registry {
    pub grants: Vec<Grant>,
}

impl Registry {
    /// Issues a grant, or returns the identical live one unchanged.
    /// Grants `scope` once. The identical scope again (a restarted caller)
    /// answers the existing grant unchanged: its lifetime is never extended.
    pub fn grant(&mut self, scope: Scope, now_ms: u64, ttl_ms: u64) -> Result<&Grant, Refusal> {
        scope.validate()?;
        if ttl_ms == 0 || ttl_ms > MAX_TTL_MS {
            return Err(Refusal::Invalid("grant lifetime must be within 24 hours"));
        }
        if let Some(i) = self
            .grants
            .iter()
            .position(|g| g.scope.task_id == scope.task_id)
        {
            let existing = &self.grants[i];
            return if existing.scope == scope && existing.live(now_ms) {
                Ok(&self.grants[i])
            } else if existing.live(now_ms) {
                Err(Refusal::Conflict(
                    "this task is granted with a different scope",
                ))
            } else {
                Err(Refusal::Conflict(
                    "a task id is never granted again once ended",
                ))
            };
        }
        if self.grants.iter().any(|g| g.live(now_ms)) {
            return Err(Refusal::Conflict(
                "another task holds the live scheduler grant",
            ));
        }
        self.grants.push(Grant::new(scope, now_ms, ttl_ms)?);
        Ok(self.grants.last().unwrap())
    }

    pub fn get(&self, task_id: &str) -> Option<&Grant> {
        self.grants.iter().find(|g| g.scope.task_id == task_id)
    }

    pub fn get_mut(&mut self, task_id: &str) -> Option<&mut Grant> {
        self.grants.iter_mut().find(|g| g.scope.task_id == task_id)
    }

    pub fn revoke(&mut self, task_id: &str, now_ms: u64) -> Result<(), Refusal> {
        self.get_mut(task_id)
            .ok_or(Refusal::NotFound)?
            .revoke(now_ms);
        Ok(())
    }

    pub fn recover(&mut self) {
        for grant in &mut self.grants {
            grant.recover();
        }
    }
}
