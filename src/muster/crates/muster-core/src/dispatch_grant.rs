//! A standing local dispatch grant (ADR 0199): one named local caller may
//! create sessions under its granted roots and send to the sessions its own
//! creations bound, on a locked Mac too. Every rule here is decided before
//! any desktop action; the daemon only adds this Mac's facts.

use crate::scheduler_grant::{Admission, Kind, OpState, Outcome, Reservation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// A grant lasts at most 30 days; the owner grants it again to go on.
pub const MAX_TTL_MS: u64 = 30 * 24 * 60 * 60 * 1000;
pub const MAX_ROOTS: usize = 16;
pub const MAX_GRANTS: usize = 4;
/// Locked leases in any rolling hour, counted when reserved.
pub const MAX_LOCKED_PER_HOUR: usize = 30;
const HOUR_MS: u64 = 60 * 60 * 1000;
pub const MAX_RECORDS: usize = 2048;
/// A settled record older than this is dropped to make room; an unknown or
/// reserved one never is.
pub const KEEP_SETTLED_MS: u64 = 7 * 24 * 60 * 60 * 1000;
const MAX_NAME: usize = 64;
const MAX_OP_ID: usize = 128;
const DOMAIN: &[u8] = b"MUSTER-DISPATCH-OPERATION-V1\0";
/// The providers whose sessions are created through their app's windows.
const PROVIDERS: [&str; 2] = ["claude", "codex"];

/// What the owner granted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    pub name: String,
    /// Sorted, without repeats.
    pub providers: Vec<String>,
    /// Canonical absolute paths, sorted, without repeats; the daemon resolves
    /// them before granting.
    pub roots: Vec<String>,
}

impl Scope {
    /// Sorts and de-duplicates, then checks.
    pub fn new(name: String, providers: Vec<String>, roots: Vec<String>) -> Result<Self, Refusal> {
        let mut scope = Self {
            name,
            providers,
            roots,
        };
        scope.providers.sort();
        scope.providers.dedup();
        scope.roots.sort();
        scope.roots.dedup();
        scope.validate()?;
        Ok(scope)
    }

    pub fn validate(&self) -> Result<(), Refusal> {
        if !valid_name(&self.name) {
            return Err(Refusal::Invalid("name must be 1-64 of [A-Za-z0-9._:-]"));
        }
        if self.providers.is_empty()
            || self
                .providers
                .iter()
                .any(|p| !PROVIDERS.contains(&p.as_str()))
        {
            return Err(Refusal::Invalid("providers must be claude and/or codex"));
        }
        if self.roots.is_empty() || self.roots.len() > MAX_ROOTS {
            return Err(Refusal::Invalid("a grant names 1-16 roots"));
        }
        if self
            .roots
            .iter()
            .any(|root| root == "/" || !normalized_absolute(root))
        {
            return Err(Refusal::Invalid(
                "roots must be normalized absolute paths other than /",
            ));
        }
        Ok(())
    }

    /// Whether `directory` (canonical) is a root or lies below one.
    pub fn covers(&self, directory: &str) -> bool {
        normalized_absolute(directory)
            && self.roots.iter().any(|root| {
                directory == root
                    || directory
                        .strip_prefix(root.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
            })
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b':'))
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
    /// Same op_id with a different request, or a settled operation.
    Conflict(&'static str),
    NotFound,
}

impl Refusal {
    pub fn message(&self) -> String {
        match self {
            Refusal::Invalid(m) => format!("invalid dispatch request: {m}"),
            Refusal::OutOfScope(m) => format!("outside the dispatch grant: {m}"),
            Refusal::Expired => "the dispatch grant has expired".into(),
            Refusal::Revoked => "the dispatch grant was revoked".into(),
            Refusal::Quota(m) => format!("dispatch grant limit reached: {m}"),
            Refusal::Conflict(m) => format!("dispatch conflict: {m}"),
            Refusal::NotFound => "no dispatch grant by this name".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Op {
    pub kind: Kind,
    pub provider: String,
    /// A creation's canonical directory; empty for a send.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub directory: String,
    /// The bound session a send acts on; empty for a creation.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub session: String,
    /// Hex SHA-256 of the exact inner request.
    pub body: String,
    pub reserved_ms: u64,
    /// Whether it was reserved for a locked console.
    pub locked: bool,
    /// For a creation: the provider's sessions in the directory before it,
    /// none of which can be its receipt.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub baseline: Vec<String>,
    pub state: OpState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    pub scope: Scope,
    pub issued_ms: u64,
    pub expires_ms: u64,
    pub revoked_ms: Option<u64>,
    /// Sessions bound by this grant's creation receipts, with their provider.
    /// Nothing else binds one.
    pub sessions: BTreeMap<String, String>,
    pub ops: BTreeMap<String, Op>,
    /// When each locked lease of the last hour was reserved.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locked: Vec<u64>,
}

/// One operation as the caller asks for it.
#[derive(Clone, Debug)]
pub struct Request<'a> {
    pub op_id: &'a str,
    pub kind: Kind,
    pub provider: &'a str,
    /// A creation's canonical directory; empty for a send.
    pub directory: &'a str,
    /// A send's session; empty for a creation.
    pub session: &'a str,
    /// SHA-256 of the exact inner request.
    pub body: [u8; 32],
}

fn check_ttl(ttl_ms: u64) -> Result<(), Refusal> {
    if ttl_ms == 0 || ttl_ms > MAX_TTL_MS {
        Err(Refusal::Invalid("grant lifetime must be within 30 days"))
    } else {
        Ok(())
    }
}

impl Grant {
    pub fn new(scope: Scope, now_ms: u64, ttl_ms: u64) -> Result<Self, Refusal> {
        scope.validate()?;
        check_ttl(ttl_ms)?;
        Ok(Self {
            scope,
            issued_ms: now_ms,
            expires_ms: now_ms.saturating_add(ttl_ms),
            revoked_ms: None,
            sessions: BTreeMap::new(),
            ops: BTreeMap::new(),
            locked: Vec::new(),
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

    /// Locked leases reserved in the hour before `now_ms`.
    pub fn locked_last_hour(&self, now_ms: u64) -> usize {
        self.locked
            .iter()
            .filter(|at| now_ms.saturating_sub(**at) < HOUR_MS)
            .count()
    }

    /// The recorded operation this exact request made, read-only.
    pub fn lookup(&self, request: &Request) -> Result<&Op, Refusal> {
        let op = self.ops.get(request.op_id).ok_or(Refusal::NotFound)?;
        if op.kind == request.kind
            && op.provider == request.provider
            && op.directory == request.directory
            && op.session == request.session
            && op.body == hex::encode(request.body)
        {
            Ok(op)
        } else {
            Err(Refusal::Conflict("op_id reused with a different request"))
        }
    }

    /// Admits one operation and records it before any desktop action, or
    /// returns what an earlier attempt with this op_id recorded. `locked`
    /// says whether it runs under a Locked Use lease.
    pub fn reserve(
        &mut self,
        request: &Request,
        baseline: Vec<String>,
        locked: bool,
        now_ms: u64,
    ) -> Result<Admission, Refusal> {
        if request.op_id.is_empty() || request.op_id.len() > MAX_OP_ID {
            return Err(Refusal::Invalid("op_id must be 1-128 bytes"));
        }
        if self.ops.contains_key(request.op_id) {
            return self
                .lookup(request)
                .map(|op| Admission::Recorded(op.state.clone()));
        }
        self.liveness(now_ms)?;
        if !self.scope.providers.iter().any(|p| p == request.provider) {
            return Err(Refusal::OutOfScope("provider"));
        }
        match request.kind {
            Kind::Create => {
                if !request.session.is_empty() {
                    return Err(Refusal::Invalid("a creation names no session"));
                }
                if !self.scope.covers(request.directory) {
                    return Err(Refusal::OutOfScope("directory is under no granted root"));
                }
            }
            Kind::Send => {
                if !request.directory.is_empty() {
                    return Err(Refusal::Invalid("a send names no directory"));
                }
                if request.session.is_empty()
                    || self.sessions.get(request.session).map(String::as_str)
                        != Some(request.provider)
                {
                    return Err(Refusal::OutOfScope(
                        "session was not created through this grant",
                    ));
                }
            }
            _ => return Err(Refusal::Invalid("only create and send are dispatched")),
        }
        if locked {
            self.locked
                .retain(|at| now_ms.saturating_sub(*at) < HOUR_MS);
            if self.locked.len() >= MAX_LOCKED_PER_HOUR {
                return Err(Refusal::Quota("locked leases this hour"));
            }
        }
        if self.ops.len() >= MAX_RECORDS {
            self.ops.retain(|_, op| {
                matches!(op.state, OpState::Reserved | OpState::Unknown { .. })
                    || now_ms.saturating_sub(op.reserved_ms) < KEEP_SETTLED_MS
            });
            if self.ops.len() >= MAX_RECORDS {
                return Err(Refusal::Quota("operations on record"));
            }
        }
        if locked {
            self.locked.push(now_ms);
        }
        self.ops.insert(
            request.op_id.to_owned(),
            Op {
                kind: request.kind,
                provider: request.provider.to_owned(),
                directory: request.directory.to_owned(),
                session: request.session.to_owned(),
                body: hex::encode(request.body),
                reserved_ms: now_ms,
                locked,
                baseline: if request.kind == Kind::Create {
                    baseline
                } else {
                    Vec::new()
                },
                state: OpState::Reserved,
            },
        );
        Ok(Admission::Fresh(Reservation::from_grant(
            format!("dispatch:{}", self.scope.name),
            request.op_id.to_owned(),
            request.provider.to_owned(),
            request.kind,
            request.session.to_owned(),
            digest(&self.scope.name, request),
        )))
    }

    /// Records how a reserved operation ended. A creation's receipt binds its
    /// session to this grant.
    pub fn settle(&mut self, op_id: &str, outcome: Outcome) -> Result<(), Refusal> {
        let op = self.ops.get(op_id).ok_or(Refusal::NotFound)?;
        if op.state != OpState::Reserved {
            return Err(Refusal::Conflict("operation already settled"));
        }
        let kind = op.kind;
        match outcome {
            Outcome::Applied(result) if kind == Kind::Create => return self.bind(op_id, result),
            Outcome::Applied(result) => {
                self.ops.get_mut(op_id).unwrap().state = OpState::Applied { result }
            }
            Outcome::NotStarted(message) => {
                self.ops.get_mut(op_id).unwrap().state = OpState::Rejected { message }
            }
            Outcome::Unknown(message) => {
                self.ops.get_mut(op_id).unwrap().state = OpState::Unknown { message }
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
        let op = &self.ops[op_id];
        if session.is_empty() || session.len() > 256 || op.baseline.contains(&session) {
            return Err(Refusal::Conflict("creation receipt is not a new session"));
        }
        if self
            .sessions
            .get(&session)
            .is_some_and(|provider| *provider != op.provider)
        {
            return Err(Refusal::Conflict("creation receipt is another provider's"));
        }
        let provider = op.provider.clone();
        self.sessions.insert(session.clone(), provider);
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

/// The digest a lease binds: the grant's name and the exact operation,
/// domain-separated from every task-scheduled, signed and unattended one.
pub fn digest(name: &str, request: &Request) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(DOMAIN);
    for part in [
        name,
        request.op_id,
        request.kind.as_str(),
        request.provider,
        request.directory,
        request.session,
    ] {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    hash.update(request.body);
    hash.finalize().into()
}

/// Every dispatch grant on this Mac, by name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registry {
    pub grants: BTreeMap<String, Grant>,
}

impl Registry {
    /// Grants `scope` for `ttl_ms`. A name granted before gets the new scope
    /// and lifetime and is live again; its bound sessions and its record of
    /// operations stay, so no op_id ever runs twice.
    pub fn grant(&mut self, scope: Scope, now_ms: u64, ttl_ms: u64) -> Result<&Grant, Refusal> {
        scope.validate()?;
        check_ttl(ttl_ms)?;
        let name = scope.name.clone();
        match self.grants.get_mut(&name) {
            Some(grant) => {
                grant.scope = scope;
                grant.issued_ms = now_ms;
                grant.expires_ms = now_ms.saturating_add(ttl_ms);
                grant.revoked_ms = None;
            }
            None => {
                if self.grants.len() >= MAX_GRANTS {
                    return Err(Refusal::Quota("dispatch grants on this Mac"));
                }
                self.grants
                    .insert(name.clone(), Grant::new(scope, now_ms, ttl_ms)?);
            }
        }
        Ok(&self.grants[&name])
    }

    pub fn get(&self, name: &str) -> Option<&Grant> {
        self.grants.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Grant> {
        self.grants.get_mut(name)
    }

    pub fn revoke(&mut self, name: &str, now_ms: u64) -> Result<(), Refusal> {
        self.get_mut(name).ok_or(Refusal::NotFound)?.revoke(now_ms);
        Ok(())
    }

    pub fn recover(&mut self) {
        for grant in self.grants.values_mut() {
            grant.recover();
        }
    }
}
