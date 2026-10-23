//! One handler for both principals. Authorization happens here, once, from
//! the roster and the local policy (§3.4); the services below it trust the
//! decision.

use std::future::Future;
use std::sync::{Arc, Weak};
use std::time::Duration;

use muster_agent::desktop_lock::LockKind;
use muster_agent::model::Provider;
use muster_core::{Decision, DeviceId, Permission, TargetState, authorize};
use muster_proto::v1::{self, error::Code, op_result::Status, request::Req, response::Resp};
use muster_rpc::{Context, EventSink, Handler, HandlerFuture, Principal, err};
use muster_store::Begin;

use crate::{Daemon, Error};

pub struct MusterHandler {
    daemon: Weak<Daemon>,
}

impl MusterHandler {
    pub fn new(daemon: Weak<Daemon>) -> Self {
        Self { daemon }
    }
}

/// Who may make a request.
enum Access {
    /// The desktop's own user only.
    LocalOnly,
    /// Any active member.
    Member,
    Need(Permission),
    /// The provider of the agent session the request names decides; it is
    /// looked up only after the caller holds Agents for some provider.
    AgentReceipt,
    AgentSession {
        control: bool,
    },
}

impl Access {
    /// Requests that act on the console and so wait out a Locked Use window.
    fn writes_console(&self) -> bool {
        matches!(
            self,
            Access::AgentSession { control: true }
                | Access::Need(Permission::Agent { control: true, .. })
                | Access::Need(Permission::Terminal { control: true } | Permission::Browser { .. })
        )
    }
}

fn access_for(req: &Req) -> Access {
    let agents = |control| {
        Access::Need(Permission::Agent {
            provider: None,
            control,
        })
    };
    match req {
        Req::DevicePresence(r) if !r.public_key.is_empty() => Access::LocalOnly,
        Req::DevicePresence(_) | Req::DeviceInstallPresence(_) => Access::Member,
        Req::NetworkPresence(_) => Access::Need(Permission::NetworkAdmin),
        Req::BrowserSetup(r) => {
            if r.install {
                Access::LocalOnly
            } else {
                Access::Need(Permission::Browser { control: false })
            }
        }
        Req::BrowserCall(r) => crate::browser::parse(&r.json)
            .map(|r| Access::Need(crate::browser::permission(&r)))
            .unwrap_or(Access::LocalOnly),
        Req::DeviceInfo(_) | Req::DeviceWatch(_) | Req::NetworkStatus(_) => Access::Member,
        // Bounded, content-free files the daemon names; see `diagnostics`.
        Req::DiagnosticsUpload(_) => Access::Member,
        Req::NetworkCreate(_)
        | Req::NetworkJoin(_)
        | Req::DeviceSetRelay(_)
        | Req::DeviceRequestAccessibility(_)
        | Req::DeviceRequestAutomation(_)
        | Req::DeviceDebug(_)
        | Req::NetworkRefresh(_)
        | Req::DesktopUpdateLease(_) => Access::LocalOnly,
        Req::DeviceSetCover(_) => Access::Need(Permission::LockedControl),
        Req::LockedUsePrepare(_)
        | Req::LockedUseExecute(_)
        | Req::LockedUseCancel(_)
        | Req::LockedUseRenew(_)
        | Req::LockedUseExtend(_) => Access::Member,
        Req::NetworkInvite(_)
        | Req::NetworkPending(_)
        | Req::NetworkDecide(_)
        | Req::NetworkRemove(_)
        | Req::NetworkSetScopes(_)
        | Req::NetworkReaddress(_)
        | Req::PolicyGet(_)
        | Req::PolicySet(_) => Access::Need(Permission::NetworkAdmin),
        Req::TerminalSessions(_)
        | Req::TerminalAttach(_)
        | Req::TerminalHistory(_)
        | Req::TerminalRuns(_) => Access::Need(Permission::Terminal { control: false }),
        Req::TerminalInput(_)
        | Req::AgentTerminal(_)
        | Req::TerminalRun(_)
        | Req::TerminalCreate(_)
        | Req::TerminalResize(_)
        | Req::TerminalClose(_) => Access::Need(Permission::Terminal { control: true }),
        // Listings name no session: every provider the caller holds, filtered
        // per session in `dispatch`.
        Req::OrcaCreate(_) => Access::Need(Permission::Agent {
            provider: Some("orca".into()),
            control: true,
        }),
        Req::AgentList(r) if r.provider.is_empty() => agents(false),
        Req::AgentList(r) => Access::Need(Permission::Agent {
            provider: Some(r.provider.clone()),
            control: false,
        }),
        Req::AgentChats(r) => Access::Need(Permission::Agent {
            provider: Some(r.provider.clone()),
            control: r.full_history,
        }),
        Req::AgentChatOpen(r) => Access::Need(Permission::Agent {
            provider: Some(r.provider.clone()),
            control: true,
        }),
        Req::AgentWatch(r) if r.session.is_empty() => agents(false),
        Req::AgentWatch(_)
        | Req::AgentMessages(_)
        | Req::AgentFiles(_)
        | Req::AgentFile(_)
        | Req::AgentGit(_) => Access::AgentSession { control: false },
        Req::AgentSendStatus(_) => Access::AgentReceipt,
        Req::AgentSend(_)
        | Req::AgentRespond(_)
        | Req::AgentInterrupt(_)
        | Req::AgentChatOptions(_)
        | Req::AgentChatSetOptions(_)
        | Req::AgentComplete(_)
        | Req::AgentInspect(_) => Access::AgentSession { control: true },
    }
}

fn fail(error: Error) -> Resp {
    let code = match &error {
        Error::NoNetwork => Code::Unavailable,
        Error::Invalid(_) => Code::Invalid,
        Error::Unauthorized(_) => Code::Unauthorized,
        Error::NotFound(_) => Code::NotFound,
        Error::Unavailable(_) => Code::Unavailable,
        Error::Conflict(_) | Error::Unconfirmed { .. } | Error::UnknownOutcome(_) => Code::Conflict,
        Error::Busy(_) => Code::Busy,
        Error::Timeout(_) => Code::Timeout,
        Error::Net(muster_net::Error::Unauthorized(_)) => Code::Unauthorized,
        Error::Net(muster_net::Error::Timeout(_)) => Code::Timeout,
        Error::Net(_) => Code::Unavailable,
        Error::Core(_) => Code::Invalid,
        Error::Store(_) => Code::Unavailable,
    };
    err(code, error.to_string())
}

impl From<muster_agent::Error> for Error {
    fn from(error: muster_agent::Error) -> Self {
        match error {
            muster_agent::Error::NotFound(m) => Error::NotFound(m),
            muster_agent::Error::Invalid(m) => Error::Invalid(m),
            muster_agent::Error::Unavailable(m) => Error::Unavailable(m),
            muster_agent::Error::Conflict(m) => Error::Conflict(m),
            muster_agent::Error::UnknownOutcome(m) => Error::UnknownOutcome(m),
            muster_agent::Error::Unconfirmed { operation, message } => {
                Error::Unconfirmed { operation, message }
            }
            muster_agent::Error::Io(m) => Error::Unavailable(m),
        }
    }
}

impl From<muster_terminal::Error> for Error {
    fn from(error: muster_terminal::Error) -> Self {
        match error {
            muster_terminal::Error::Unavailable(m) => Error::Unavailable(m),
            muster_terminal::Error::NotFound(m) => Error::NotFound(m),
            muster_terminal::Error::Protocol(m) => Error::Invalid(m),
            muster_terminal::Error::Conflict(m) => Error::Conflict(m),
        }
    }
}

fn op_result(status: Status, message: impl Into<String>, result_id: impl Into<String>) -> Resp {
    Resp::OpResult(v1::OpResult {
        status: status as i32,
        message: message.into(),
        result_id: result_id.into(),
        unconfirmed: String::new(),
    })
}

fn rejected(message: String, unconfirmed: Option<String>) -> Resp {
    let mut result = op_result(Status::Rejected, message, "");
    if let (Resp::OpResult(r), Some(token)) = (&mut result, unconfirmed) {
        r.unconfirmed = token;
    }
    result
}

impl Handler for MusterHandler {
    fn forwarding_enrollment(&self) -> Vec<u8> {
        use muster_proto::Message;
        self.daemon
            .upgrade()
            .and_then(|daemon| daemon.export_presence())
            .map(|proof| proof.encode_to_vec())
            .unwrap_or_default()
    }
    /// The app addresses a member: open its "muster" service over the link.
    fn open_target(&self, target: &str) -> muster_rpc::TargetFuture<'_> {
        let daemon = self.daemon.upgrade();
        let target = target.to_string();
        Box::pin(async move {
            let Some(daemon) = daemon else {
                return Err(muster_rpc::Error::Closed);
            };
            let peer = muster_core::DeviceId::from_hex(&target)
                .map_err(|e| muster_rpc::Error::Protocol(format!("target: {e}")))?;
            let (send, recv) = daemon
                .node()
                .open(peer, crate::SERVICE, crate::SERVICE_VERSION)
                .await
                .map_err(|e| muster_rpc::Error::Io(e.to_string()))?;
            Ok(Box::new(tokio::io::join(recv, send)) as Box<dyn muster_rpc::Stream>)
        })
    }

    fn call(&self, ctx: Context, request: Req, events: EventSink) -> HandlerFuture<'_> {
        Box::pin(async move {
            let Some(daemon) = self.daemon.upgrade() else {
                return err(Code::Unavailable, "musterd is shutting down");
            };
            if let Principal::Remote(peer) = ctx.principal
                && !ctx.presence_enrollment.is_empty()
            {
                use muster_proto::Message;
                let result = v1::PresenceEnrollment::decode(ctx.presence_enrollment.as_slice())
                    .map_err(|error| Error::Invalid(error.to_string()))
                    .and_then(|proof| daemon.import_presence(peer, &proof));
                if let Err(error) = result {
                    return fail(error);
                }
            }
            let access = access_for(&request);
            if let Err(error) = authorize_request(&daemon, &ctx, &request, &access) {
                return fail(error);
            }
            if let Req::TerminalCreate(r) = &request
                && crate::native_terminals::source(&daemon, &r.source)
                    .is_ok_and(|t| t.source() == "ghostty")
                && daemon.platform().locked()
            {
                return err(
                    Code::LockedUseRequired,
                    "Ghostty must be created after an authorized unlock",
                );
            }
            // Creation has its own source-specific gate above. iTerm2 must
            // not enter an agent queue or require a Ghostty unlock.
            if let Principal::Remote(peer) = ctx.principal
                && !matches!(request, Req::TerminalCreate(_))
                && muster_proto::locked_use::operation(&request).is_some()
                && !independent_session(&daemon, &request)
                && let Some(result) = daemon
                    .locked_use
                    .enqueue(&daemon, peer, request.clone(), events.cancelled().clone())
                    .await
            {
                return result.unwrap_or_else(fail);
            }
            // A terminal whose screen only an unlocked console shows
            // (Ghostty): on a locked Mac only inside its requester's own
            // terminal session, which the console-write barrier below would
            // otherwise refuse.
            if let Principal::Remote(peer) = ctx.principal
                && matches!(
                    request,
                    Req::TerminalAttach(_)
                        | Req::TerminalInput(_)
                        | Req::TerminalHistory(_)
                        | Req::TerminalResize(_)
                        | Req::TerminalClose(_)
                )
                && !tmux_request(&request)
                && let Some(lease) = daemon.locked_use.manual_terminal_lease(peer)
            {
                return in_terminal_session(&daemon, &ctx, peer, lease, request, events)
                    .await
                    .unwrap_or_else(fail);
            }
            if let Principal::Remote(peer) = ctx.principal
                && let Some(session) = ghostty_session(&daemon, &request)
            {
                if let Some(lease) = daemon.locked_use.terminal_lease(peer, session) {
                    return match in_terminal_session(&daemon, &ctx, peer, lease, request, events)
                        .await
                    {
                        Ok(resp) => resp,
                        Err(error) => fail(error),
                    };
                }
                if daemon.locked_use.writes.clone().try_read_owned().is_err() {
                    return err(Code::Conflict, "a Locked Use transaction owns the console");
                }
                if daemon.platform().locked() {
                    return err(
                        Code::LockedUseRequired,
                        "Ghostty shows its terminals only on an unlocked Mac: open a terminal session",
                    );
                }
            }
            // Serialize all writes against a temporary-unlock transaction.
            // A normal request cannot slip in while its console is unlocked.
            let _console_write = if (access.writes_console()
                || matches!(request, Req::DeviceSetCover(_)))
                && !independent_session(&daemon, &request)
            {
                match daemon.locked_use.writes.clone().try_read_owned() {
                    Ok(guard) => Some(guard),
                    Err(_) => {
                        return err(Code::Conflict, "a Locked Use transaction owns the console");
                    }
                }
            } else {
                None
            };
            if matches!(ctx.principal, Principal::Remote(_))
                && daemon.platform().locked()
                && !matches!(request, Req::TerminalCreate(_))
                && muster_proto::locked_use::operation(&request).is_some()
                && !independent_session(&daemon, &request)
            {
                return err(
                    Code::LockedUseRequired,
                    "this native app requires a signed Locked Use operation",
                );
            }
            // Unlocked RPCs need the same OS-work lifetime as Locked Use.
            // Cancellation can drop the handler while a blocking AX call runs;
            // provider permits remain held until that work has actually drained.
            let locked_prepare = matches!(request, Req::LockedUsePrepare(_));
            let work = muster_agent::control_guard::WorkQueue::default();
            work.scope(async {
                match dispatch(&daemon, &ctx, request, events).await {
                    Ok(resp) => resp,
                    Err(error) => {
                        if locked_prepare {
                            log::warn!("Locked Use prepare rejected: requester={:?} rpc_id={} reason={error}",
                                ctx.principal, ctx.id);
                        }
                        fail(error)
                    }
                }
            })
            .await
        })
    }
}

/// The Ghostty terminal a request reads or writes. Listing works without
/// a session: Ghostty's scripting answers on a locked Mac.
fn ghostty_session<'a>(daemon: &Daemon, request: &'a Req) -> Option<&'a str> {
    if tmux_request(request) {
        return None;
    }
    let id = match request {
        Req::TerminalAttach(r) => &r.session,
        Req::TerminalInput(r) => &r.session,
        Req::TerminalHistory(r) => &r.session,
        Req::TerminalResize(r) => &r.session,
        Req::TerminalClose(r) => &r.session,
        _ => return None,
    };
    crate::native_terminals::resolve(daemon, id)
        .ok()
        .filter(|(terminal, _)| terminal.source() == "ghostty")
        .map(|_| id.as_str())
}

/// A request of the open terminal session's requester. It holds the
/// session's I/O barrier, so the cover stays until it is done, and an
/// attach stream ends as soon as the session does.
async fn in_terminal_session(
    daemon: &Arc<Daemon>,
    ctx: &Context,
    peer: DeviceId,
    lease: crate::locked_use::TerminalLease,
    request: Req,
    events: EventSink,
) -> Result<Resp, Error> {
    let _io = lease
        .io()
        .ok_or_else(|| Error::Conflict("the terminal session has ended".into()))?;
    if matches!(request, Req::TerminalInput(_)) {
        daemon.locked_use.terminal_input(peer);
    }
    let access = access_for(&request);
    let checked_request = request.clone();
    let work = dispatch(daemon, ctx, request, events);
    tokio::pin!(work);
    let mut check = tokio::time::interval(Duration::from_millis(200));
    loop {
        tokio::select! {
            result = &mut work => return result,
            _ = check.tick() => {
                authorize_request(daemon, ctx, &checked_request, &access)?;
                if !lease.live() {
                    return Err(Error::Unavailable("The Mac locked again: the terminal session ended".into()));
                }
            }
        }
    }
}

fn authorize_request(
    daemon: &Daemon,
    ctx: &Context,
    request: &Req,
    access: &Access,
) -> Result<(), Error> {
    let Principal::Remote(peer) = ctx.principal else {
        return Ok(());
    };
    {
        let roster = daemon.roster().read().unwrap();
        let state = roster.state();
        let Some(member) = state.active(&peer) else {
            return Err(Error::Unauthorized("not an active member".into()));
        };
        if matches!(request, Req::NetworkInvite(_)) && !member.is_admin() {
            return Err(Error::Unauthorized("only an admin can invite".into()));
        }
        // Browser CDP works while the screen stays locked. Require a separate
        // operation-bound presence signature; never turn it into an unlock lease.
        if matches!(request, Req::BrowserCall(_)) && daemon.platform().locked() {
            let policy = daemon.policy().read().unwrap();
            if authorize(
                state,
                &peer,
                &Permission::LockedControl,
                &policy,
                &TargetState { locked: true },
            ) != Decision::NeedsPresence
            {
                return Err(Error::Unauthorized(
                    "browser while locked requires locked.control and local locked-control policy"
                        .into(),
                ));
            }
            let key = member.record.presence_key.as_ref().ok_or_else(|| {
                Error::Unauthorized("browser while locked requires a presence key".into())
            })?;
            key.verify(&ctx.presence_message(request), &ctx.presence)
                .map_err(|_| {
                    Error::Unauthorized(
                        "browser while locked requires本人认证; use Muster or --approve-in-app"
                            .into(),
                    )
                })?;
        }
    }
    if let Some(agent) = tmux_agent(request) {
        let session = agent_of(daemon)?
            .session(agent)
            .ok_or_else(|| Error::Invalid("agent session not found".into()))?;
        admit(
            daemon,
            ctx,
            &peer,
            request,
            &Permission::Agent {
                provider: Some(session.provider),
                control: false,
            },
        )?;
    }
    match access {
        Access::LocalOnly => Err(Error::Unauthorized(
            "only the device's own user may do this".into(),
        )),
        Access::Member => Ok(()),
        Access::Need(permission) => admit(daemon, ctx, &peer, request, permission),
        Access::AgentReceipt => {
            let allowed = |provider| {
                authorize(
                    daemon.roster().read().unwrap().state(),
                    &peer,
                    &Permission::Agent {
                        provider,
                        control: true,
                    },
                    &daemon.policy().read().unwrap(),
                    &TargetState { locked: false },
                ) == Decision::Allow
            };
            // Do not reveal whether a session exists to members without Agent access.
            if !allowed(None) {
                return Err(Error::Unauthorized("Send receipt access denied".into()));
            }
            let (_, provider) = session_provider(daemon, request)?;
            let provider =
                provider.ok_or_else(|| Error::NotFound("Receipt session unavailable".into()))?;
            if allowed(Some(provider)) {
                Ok(())
            } else {
                Err(Error::Unauthorized("Send receipt access denied".into()))
            }
        }
        Access::AgentSession { control } => {
            admit(
                daemon,
                ctx,
                &peer,
                request,
                &Permission::Agent {
                    provider: None,
                    control: *control,
                },
            )?;
            let for_provider = |provider: &str| Permission::Agent {
                provider: Some(provider.to_owned()),
                control: *control,
            };
            let (session, provider) = session_provider(daemon, request)?;
            match provider {
                Some(provider) => admit(daemon, ctx, &peer, request, &for_provider(&provider)),
                // No such session. A caller who could act on it whatever its
                // provider gets the service's own answer (a send is still
                // ledgered as rejected); anyone else only learns it is absent.
                None if Provider::ALL.iter().all(|p| {
                    admit(daemon, ctx, &peer, request, &for_provider(p.as_str())).is_ok()
                }) =>
                {
                    Ok(())
                }
                None => Err(Error::NotFound(format!("agent session {session}"))),
            }
        }
    }
}

/// Whether a remote caller must attach a presence proof to `req` because
/// its permission is high-risk. Clients decide whether to ask for local
/// authentication from this same table, so the two cannot drift apart.
pub fn needs_presence(req: &Req) -> bool {
    matches!(access_for(req), Access::Need(permission) if permission.high_risk())
}

/// One `authorize` decision for a remote caller, verifying the presence
/// proof a high-risk permission asks for.
fn admit(
    daemon: &Daemon,
    ctx: &Context,
    peer: &DeviceId,
    request: &Req,
    permission: &Permission,
) -> Result<(), Error> {
    let target = TargetState {
        locked: daemon.platform().locked(),
    };
    let roster = daemon.roster().read().unwrap();
    let state = roster.state();
    let policy = daemon.policy().read().unwrap();
    match authorize(state, peer, permission, &policy, &target) {
        Decision::Allow => Ok(()),
        Decision::Deny(reason) => Err(Error::Unauthorized(format!("{permission}: {reason}"))),
        Decision::NeedsPresence => {
            let Some(key) = state
                .active(peer)
                .and_then(|member| member.record.presence_key.as_ref())
            else {
                return Err(Error::Unauthorized(format!(
                    "{permission}: needs a presence proof and the device has no presence key"
                )));
            };
            if ctx.presence.is_empty() {
                return Err(Error::Unauthorized(format!(
                    "{permission}: presence proof required"
                )));
            }
            key.verify(&ctx.presence_message(request), &ctx.presence)
                .map_err(|_| Error::Unauthorized(format!("{permission}: presence proof invalid")))
        }
    }
}

/// The agent session a request names, and its provider if it exists.
fn session_provider(daemon: &Daemon, request: &Req) -> Result<(String, Option<String>), Error> {
    let session = match request {
        Req::AgentSendStatus(r) => r.session.clone(),
        Req::AgentWatch(r) => r.session.clone(),
        Req::AgentMessages(r) => r.session.clone(),
        Req::AgentFiles(r) => r.session.clone(),
        Req::AgentFile(r) => r.session.clone(),
        Req::AgentGit(r) => r.session.clone(),
        Req::AgentChatOptions(r) => r.session.clone(),
        Req::AgentChatSetOptions(r) => r.session.clone(),
        Req::AgentSend(_)
        | Req::AgentRespond(_)
        | Req::AgentInterrupt(_)
        | Req::AgentComplete(_)
        | Req::AgentInspect(_) => native_session(daemon, request)?,
        _ => return Err(Error::Invalid("not an agent session request".into())),
    };
    let provider = agent_of(daemon)?
        .session(&session)
        .map(|info| info.provider);
    Ok((session, provider))
}

/// Which agent providers the caller may see. Asked again on every use, so a
/// narrowed grant reaches streams already open; the lock state stays the
/// one the request was admitted under.
struct AgentVisibility<'a> {
    daemon: &'a Daemon,
    peer: Option<DeviceId>,
    target: TargetState,
}

impl<'a> AgentVisibility<'a> {
    fn new(daemon: &'a Daemon, ctx: &Context) -> Self {
        Self {
            daemon,
            peer: match ctx.principal {
                Principal::Remote(peer) => Some(peer),
                _ => None,
            },
            target: TargetState {
                locked: daemon.platform().locked(),
            },
        }
    }

    /// `None` asks whether any provider is still visible.
    fn allows(&self, provider: Option<&str>) -> bool {
        let Some(peer) = &self.peer else {
            return true;
        };
        let roster = self.daemon.roster().read().unwrap();
        let policy = self.daemon.policy().read().unwrap();
        authorize(
            roster.state(),
            peer,
            &Permission::Agent {
                provider: provider.map(str::to_owned),
                control: false,
            },
            &policy,
            &self.target,
        )
        .is_allow()
    }

    fn session(&self, agent: &muster_agent::AgentService, session: &str) -> bool {
        self.peer.is_none()
            || agent
                .session(session)
                .is_some_and(|info| self.allows(Some(&info.provider)))
    }

    /// Newest sessions first, as `AgentService::list` pages them, from the
    /// visible providers only.
    fn list(
        &self,
        agent: &muster_agent::AgentService,
        provider: Option<Provider>,
        limit: usize,
    ) -> Vec<v1::AgentSession> {
        // Filter every provider, including independent adapters, before limiting.
        let mut sessions: Vec<_> = agent
            .list(provider, usize::MAX)
            .into_iter()
            .filter(|session| self.allows(Some(&session.provider)))
            .collect();
        sessions.sort_by(|a, b| {
            b.updated_unix
                .cmp(&a.updated_unix)
                .then_with(|| a.id.cmp(&b.id))
        });
        sessions.truncate(if limit == 0 { 200 } else { limit });
        sessions
    }
}

/// Runs an idempotent mutation under the ledger when it carries an op_id.
/// `tracked` operations run under Shared Rust delivery tracking: if they are
/// interrupted (Locked Use stop, RPC cancellation) before any submission
/// could have started, the ledger records them as rejected instead of
/// leaving an undecidable pending row. `subject` names the target in logs;
/// no request body is ever logged.
async fn ledgered<F>(
    daemon: &Daemon,
    op_id: &str,
    kind: &str,
    subject: &str,
    tracked: bool,
    body: &[u8],
    run: F,
) -> Result<Resp, Error>
where
    F: Future<Output = Result<String, Error>>,
{
    let result_subject = subject.to_owned();
    let subject: String = subject.chars().take(8).collect();
    let delivery = muster_agent::control_guard::current_delivery().unwrap_or_default();
    if op_id.is_empty() {
        let outcome = if tracked {
            delivery.scope(run).await
        } else {
            run.await
        };
        let result = match settle(outcome, tracked.then_some(&delivery)) {
            Settled::Applied(id) => op_result(Status::Applied, "", id),
            Settled::Rejected(message, unconfirmed) => rejected(message, unconfirmed),
            Settled::Unknown(message) => op_result(Status::Unknown, message, ""),
        };
        log_outcome(kind, "-", &subject, &result);
        return Ok(result);
    }
    let ledger = daemon.store().ledger();
    let result = match ledger.begin(op_id, kind, body)? {
        Begin::Fresh => {
            delivery.set_operation(op_id);
            if tracked && kind == "agent.send" {
                let store = daemon.store.clone();
                let operation = op_id.to_owned();
                let hash = muster_store::Ledger::body_hash(body);
                delivery.set_confirmation(Arc::new(move || {
                    store
                        .ledger()
                        .confirm(&operation, "agent.send", &hash, result_subject.as_bytes())
                        .map_err(|error| muster_agent::Error::Io(error.to_string()))
                }));
            }
            let mut interrupted = Interrupted {
                daemon,
                op_id,
                kind,
                subject: &subject,
                delivery: tracked.then(|| delivery.clone()),
                settled: false,
            };
            let outcome = if tracked {
                delivery.scope(run).await
            } else {
                run.await
            };
            interrupted.settled = true;
            match settle(outcome, tracked.then_some(&delivery)) {
                Settled::Applied(id) => {
                    ledger.confirm(
                        op_id,
                        kind,
                        &muster_store::Ledger::body_hash(body),
                        id.as_bytes(),
                    )?;
                    op_result(Status::Applied, "", id)
                }
                // Stays pending: undecidable, never replayed for this op_id.
                Settled::Unknown(message) => op_result(Status::Unknown, message, ""),
                Settled::Rejected(message, unconfirmed) => {
                    ledger.reject(op_id, message.as_bytes())?;
                    rejected(message, unconfirmed)
                }
            }
        }
        Begin::Duplicate(result) => {
            let text = result
                .map(|r| String::from_utf8_lossy(&r).into_owned())
                .unwrap_or_default();
            match ledger.status(op_id)?.as_deref() {
                Some("rejected") => op_result(Status::Rejected, text, ""),
                _ => op_result(Status::Duplicate, "", text),
            }
        }
        Begin::Conflict => op_result(Status::Rejected, "op_id reused with a different body", ""),
        Begin::Unknown => op_result(
            Status::Unknown,
            "outcome undecidable after a crash; not retried",
            "",
        ),
    };
    log_outcome(kind, op_id, &subject, &result);
    Ok(result)
}

enum Settled {
    Applied(String),
    /// Definitely not submitted; the requester may decide to send again.
    /// With the token of the unconfirmed earlier write that blocked it.
    Rejected(String, Option<String>),
    /// A submission may have happened; the op stays undecidable.
    Unknown(String),
}

/// One classification for every ledgered mutation, with or without op_id.
/// A tracked operation that never reached a delivery-capable press was not
/// sent, even when its controller reported an unknown outcome (e.g. an
/// earlier write to the same session is unconfirmed). Once a press may have
/// happened an unknown outcome is kept, never downgraded to "not sent".
fn settle(
    outcome: Result<String, Error>,
    delivery: Option<&muster_agent::control_guard::Delivery>,
) -> Settled {
    match outcome {
        Ok(id) => Settled::Applied(id),
        Err(Error::UnknownOutcome(message)) if delivery.is_some_and(|d| d.not_started()) => {
            Settled::Rejected(
                format!("{message}; this request reached no provider submission and was not sent"),
                None,
            )
        }
        Err(Error::UnknownOutcome(message)) => Settled::Unknown(message),
        Err(error) => {
            let unconfirmed = match &error {
                Error::Unconfirmed { operation, .. } => Some(operation.clone()),
                _ => None,
            };
            Settled::Rejected(error.to_string(), unconfirmed)
        }
    }
}

/// The failure reason otherwise reaches only the requester and the ledger.
fn log_outcome(kind: &str, op_id: &str, subject: &str, result: &Resp) {
    let Resp::OpResult(r) = result else { return };
    let message: String = r.message.chars().take(240).collect();
    match Status::try_from(r.status) {
        Ok(Status::Applied) => log::info!("ledger {kind} op={op_id} target={subject}: applied"),
        Ok(Status::Duplicate) => {
            log::info!("ledger {kind} op={op_id} target={subject}: duplicate of applied result")
        }
        Ok(Status::Unknown) => {
            log::warn!("ledger {kind} op={op_id} target={subject}: outcome unknown: {message}")
        }
        _ => log::warn!("ledger {kind} op={op_id} target={subject}: rejected: {message}"),
    }
}

/// Settles a ledger row whose operation future was dropped mid-flight.
struct Interrupted<'a> {
    daemon: &'a Daemon,
    op_id: &'a str,
    kind: &'a str,
    subject: &'a str,
    delivery: Option<muster_agent::control_guard::Delivery>,
    settled: bool,
}
impl Drop for Interrupted<'_> {
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        let (kind, op_id, subject) = (self.kind, self.op_id, self.subject);
        if self.delivery.as_ref().is_some_and(|d| d.not_started()) {
            let message = format!("{INTERRUPTED_NOT_STARTED}; not sent");
            match self
                .daemon
                .store()
                .ledger()
                .reject(op_id, message.as_bytes())
            {
                Ok(()) => log::warn!(
                    "ledger {kind} op={op_id} target={subject}: interrupted before any submission; recorded as rejected"
                ),
                Err(error) => log::warn!(
                    "ledger {kind} op={op_id} target={subject}: interrupted before any submission; ledger update failed: {error}"
                ),
            }
        } else {
            log::warn!(
                "ledger {kind} op={op_id} target={subject}: interrupted after a possible submission; outcome unknown"
            );
        }
    }
}

pub(crate) const INTERRUPTED_NOT_STARTED: &str =
    "operation interrupted before any provider submission";

fn agent_of(daemon: &Daemon) -> Result<&Arc<muster_agent::AgentService>, Error> {
    daemon
        .agent()
        .ok_or_else(|| Error::Unavailable("the agent service is off on this device".into()))
}

fn tmux_agent(request: &Req) -> Option<&str> {
    match request {
        Req::AgentTerminal(r) => Some(&r.session),
        Req::TerminalRun(r) => Some(&r.session),
        Req::TerminalRuns(r) => Some(&r.session),
        _ => None,
    }
}
fn tmux_request(request: &Req) -> bool {
    match request {
        Req::AgentTerminal(_) | Req::TerminalRun(_) | Req::TerminalRuns(_) => true,
        Req::TerminalAttach(r) => r.session.starts_with("tmux-"),
        Req::TerminalInput(r) => r.session.starts_with("tmux-"),
        Req::TerminalResize(r) => r.session.starts_with("tmux-"),
        Req::TerminalHistory(r) => r.session.starts_with("tmux-"),
        Req::TerminalClose(r) => r.session.starts_with("tmux-"),
        _ => false,
    }
}
fn terminal_for<'a>(
    daemon: &'a Daemon,
    session: &'a str,
) -> Result<(&'a Arc<muster_terminal::Terminal>, &'a str), Error> {
    if session.starts_with("tmux-") {
        #[cfg(unix)]
        {
            return Ok((&daemon.tmux, session));
        }
        #[cfg(not(unix))]
        {
            return Err(Error::Unavailable("tmux requires a Unix host".into()));
        }
    }
    crate::native_terminals::resolve(daemon, session)
}

async fn dispatch(
    daemon: &Arc<Daemon>,
    ctx: &Context,
    request: Req,
    events: EventSink,
) -> Result<Resp, Error> {
    Ok(match request {
        Req::DevicePresence(r) => Resp::PresenceInfo(daemon.register_presence(&r.public_key)?),
        Req::NetworkPresence(r) => Resp::PresenceInfo(daemon.enroll_presence(r).await?),
        Req::DeviceInstallPresence(r) => {
            daemon.import_presence(
                daemon.device_id(),
                &r.enrollment
                    .ok_or_else(|| Error::Invalid("missing Presence enrollment".into()))?,
            )?;
            Resp::Ack(v1::Ack {})
        }
        // ------------------------------------------------------------ device
        Req::DesktopUpdateLease(_) => daemon.locked_use.update_lease(events).await?,
        Req::LockedUsePrepare(r) => {
            let Principal::Remote(peer) = ctx.principal else {
                return Err(Error::Unauthorized(
                    "use an enrolled remote device for Locked Use".into(),
                ));
            };
            daemon
                .locked_use
                .prepare(
                    daemon.clone(),
                    peer,
                    r.operation
                        .ok_or_else(|| Error::Invalid("no operation".into()))?,
                )
                .await?
        }
        Req::LockedUseExecute(r) => {
            let Principal::Remote(peer) = ctx.principal else {
                return Err(Error::Unauthorized("remote member required".into()));
            };
            daemon
                .locked_use
                .execute(peer, r, events.cancelled().clone())
                .await?
        }
        Req::LockedUseCancel(r) => {
            let Principal::Remote(peer) = ctx.principal else {
                return Err(Error::Unauthorized("remote member required".into()));
            };
            let reason = muster_core::locked_use::CancelReason::parse(&r.reason);
            daemon.locked_use.cancel(peer, &r.nonce, reason).await?
        }
        Req::LockedUseRenew(r) => {
            let Principal::Remote(peer) = ctx.principal else {
                return Err(Error::Unauthorized("remote member required".into()));
            };
            daemon.locked_use.renew(peer, &r.nonce).await?
        }
        Req::LockedUseExtend(r) => {
            let Principal::Remote(peer) = ctx.principal else {
                return Err(Error::Unauthorized("remote member required".into()));
            };
            daemon.locked_use.extend(peer, r).await?
        }
        Req::BrowserSetup(r) => {
            Resp::BrowserStatus(crate::browser::setup(daemon, r.install).await?)
        }
        Req::BrowserCall(r) => crate::browser::call(daemon, r).await?,
        Req::DeviceInfo(_) => Resp::DeviceInfo(daemon.device_info()),
        Req::DiagnosticsUpload(r) => {
            Resp::DiagnosticsSaved(crate::diagnostics::upload(daemon, ctx, r)?)
        }
        Req::DeviceWatch(_) => {
            let mut changes = daemon.subscribe_changes();
            let mut last_status = None;
            let mut last_policy = None;
            let mut last_device_state = None;
            loop {
                let status = daemon.network_status();
                if last_status.as_ref() != Some(&status) {
                    events
                        .send(v1::event::Ev::NetworkStatus(status.clone()))
                        .await?;
                    last_status = Some(status);
                }
                let policy = daemon.policy_view();
                if last_policy.as_ref() != Some(&policy) {
                    events.send(v1::event::Ev::Policy(policy.clone())).await?;
                    last_policy = Some(policy);
                }
                if last_device_state
                    .is_none_or(|at: std::time::Instant| at.elapsed() >= Duration::from_secs(2))
                {
                    events
                        .send(v1::event::Ev::DeviceState(daemon.device_state()))
                        .await?;
                    last_device_state = Some(std::time::Instant::now());
                }
                tokio::select! {
                    _ = events.cancelled().cancelled() => break,
                    _ = tokio::time::sleep(Duration::from_millis(250)) => {}
                    r = changes.recv() => { if matches!(r, Err(tokio::sync::broadcast::error::RecvError::Closed)) { break; } }
                }
            }
            Resp::Ack(v1::Ack {})
        }

        // ----------------------------------------------------------- network
        Req::NetworkStatus(_) => Resp::NetworkStatus(daemon.network_status()),
        Req::NetworkRefresh(_) => {
            daemon.refresh_presence();
            Resp::NetworkStatus(daemon.network_status())
        }
        Req::NetworkReaddress(r) => {
            daemon.readdress_network(&r.vnet_cidr, &r.addresses)?;
            Resp::NetworkStatus(daemon.network_status())
        }
        Req::NetworkCreate(r) => {
            daemon.create_network(&r.name, &r.vnet_cidr).await?;
            Resp::NetworkStatus(daemon.network_status())
        }
        Req::NetworkInvite(r) => {
            let invite = daemon.make_invite(r.ttl_secs)?;
            Resp::InviteText(v1::InviteText {
                text: invite.encode(),
                expires_unix: invite.expires_unix,
            })
        }
        Req::NetworkJoin(r) => {
            let sink = events.clone();
            let (tx, mut rx) =
                tokio::sync::mpsc::unbounded_channel::<(v1::join_progress::Phase, String)>();
            let forward = tokio::spawn(async move {
                while let Some((phase, message)) = rx.recv().await {
                    let _ = sink
                        .send(v1::event::Ev::JoinProgress(v1::JoinProgress {
                            phase: phase as i32,
                            message,
                        }))
                        .await;
                }
            });
            let progress = move |phase, message| {
                let _ = tx.send((phase, message));
            };
            let outcome = daemon
                .join(&r.invite_text, &r.device_name, &r.device_kind, &progress)
                .await;
            drop(progress);
            let _ = forward.await;
            match outcome {
                Ok(()) => Resp::NetworkStatus(daemon.network_status()),
                Err(error) => {
                    events
                        .send(v1::event::Ev::JoinProgress(v1::JoinProgress {
                            phase: v1::join_progress::Phase::Failed as i32,
                            message: error.to_string(),
                        }))
                        .await?;
                    return Err(error);
                }
            }
        }
        Req::NetworkPending(_) => Resp::PendingJoins(daemon.pending_joins()),
        Req::NetworkDecide(r) => {
            daemon.decide(&r.device_id, r.approve, &r.scopes, &r.reason)?;
            Resp::Ack(v1::Ack {})
        }
        Req::NetworkRemove(r) => {
            daemon.remove_member(&r.device_id)?;
            Resp::NetworkStatus(daemon.network_status())
        }
        Req::NetworkSetScopes(r) => {
            daemon.set_scopes(&r.device_id, &r.scopes, &r.role)?;
            Resp::NetworkStatus(daemon.network_status())
        }
        Req::DeviceSetCover(r) => {
            daemon.set_cover(r.on);
            Resp::DeviceInfo(daemon.device_info())
        }
        Req::DeviceDebug(r) => {
            let text = match r.what.as_str() {
                "unlock-probe" | "unlock-key-probe" => daemon.locked_use.probe_unlock().await?,
                "locked-inspect" => {
                    return crate::inspection::inspect(daemon, &r.arg, events.cancelled()).await;
                }
                "ax-question" => agent_of(daemon)?.question_diagnostic(&r.arg)?,
                "ax-chat-options" => {
                    let _write =
                        daemon
                            .locked_use
                            .writes
                            .clone()
                            .try_read_owned()
                            .map_err(|_| {
                                Error::Conflict("a Locked Use transaction owns the console".into())
                            })?;
                    let app = chat_app_named(&r.arg).ok_or_else(|| {
                        Error::Invalid("usage: ax-chat-options <Claude|ChatGPT>".into())
                    })?;
                    let _app = crate::agent_bridge::hold(
                        daemon,
                        app.profile(),
                        LockKind::Chat,
                        "inspect options",
                    )
                    .await?;
                    crate::agent_bridge::inspect_chat_options(daemon.platform().as_ref(), app)
                        .await?
                }
                "ax-chat-mode" => {
                    // Local diagnosis: `<Claude|ChatGPT> <chat|code>`. Like
                    // session selection it cannot navigate a locked or
                    // transaction-owned console.
                    let _write =
                        daemon
                            .locked_use
                            .writes
                            .clone()
                            .try_read_owned()
                            .map_err(|_| {
                                Error::Conflict("a Locked Use transaction owns the console".into())
                            })?;
                    let mut words = r.arg.split_whitespace();
                    let app = words.next().and_then(chat_app_named).ok_or_else(|| {
                        Error::Invalid("usage: ax-chat-mode <Claude|ChatGPT> <chat|code>".into())
                    })?;
                    let chat = match words.next() {
                        Some("chat") => true,
                        Some("code" | "codex") => false,
                        _ => {
                            return Err(Error::Invalid(
                                "usage: ax-chat-mode <Claude|ChatGPT> <chat|code>".into(),
                            ));
                        }
                    };
                    let kind = if chat {
                        LockKind::Chat
                    } else {
                        LockKind::Agent
                    };
                    let _app =
                        crate::agent_bridge::hold(daemon, app.profile(), kind, "switch mode")
                            .await?;
                    crate::agent_bridge::set_chat_mode(daemon.platform().as_ref(), app, chat)
                        .await?;
                    format!(
                        "{} switched to {} mode",
                        r.arg.split_whitespace().next().unwrap_or(""),
                        if chat { "Chat" } else { "its other" }
                    )
                }
                "ax-select-session" => {
                    // Local diagnosis uses the production selection path, but
                    // cannot navigate a locked or transaction-owned console.
                    let _write =
                        daemon
                            .locked_use
                            .writes
                            .clone()
                            .try_read_owned()
                            .map_err(|_| {
                                Error::Conflict("a Locked Use transaction owns the console".into())
                            })?;
                    if daemon.platform().locked() {
                        return Err(Error::Unavailable(
                            "AX selection needs an unlocked console".into(),
                        ));
                    }
                    let agent = agent_of(daemon)?;
                    let matches: Vec<_> = agent
                        .list(None, usize::MAX)
                        .into_iter()
                        .filter(|s| !r.arg.is_empty() && s.id.starts_with(&r.arg))
                        .collect();
                    if matches.len() != 1 {
                        return Err(Error::Invalid(
                            "session prefix must identify exactly one session".into(),
                        ));
                    }
                    let _app =
                        crate::agent_bridge::hold_session(daemon, agent, &matches[0].id, "select")
                            .await?;
                    crate::agent_bridge::ensure_native(
                        daemon.platform().as_ref(),
                        agent.clone(),
                        &matches[0].id,
                    )
                    .await?;
                    format!("AX selected {} ({})", matches[0].title, matches[0].id)
                }
                _ => daemon
                    .platform()
                    .debug(&r.what, &r.arg)
                    .map_err(Error::Invalid)?,
            };
            Resp::DebugText(v1::DebugText { text })
        }
        Req::DeviceRequestAutomation(_) => {
            #[cfg(target_os = "macos")]
            {
                let mut errors = Vec::new();
                for terminal in daemon.terminals() {
                    let source = terminal.source();
                    let permission = tokio::task::spawn_blocking(move || match source {
                        "ghostty" => muster_platform::macos::ghostty::permission(true),
                        "iterm2" => muster_platform::macos::automation::iterm_permission(true),
                        _ => "granted",
                    })
                    .await
                    .map_err(|e| Error::Unavailable(e.to_string()))?;
                    if permission != "granted" {
                        errors.push(format!("{source}: {permission}. Open the app and allow Automation in System Settings."));
                    } else if source == "iterm2" {
                        match tokio::time::timeout(Duration::from_secs(20), terminal.sessions()).await {
                            Ok(Ok(_)) => {},
                            _ => errors.push("iTerm2: enable Python API in Settings > General > Magic and approve the connection".into()),
                        }
                    }
                }
                if !errors.is_empty() {
                    return Err(Error::Unavailable(errors.join("\n")));
                }
            }
            #[cfg(not(target_os = "macos"))]
            return Err(Error::Unavailable("Requires macOS".into()));
            #[cfg(target_os = "macos")]
            Resp::DeviceInfo(daemon.device_info())
        }
        Req::DeviceRequestAccessibility(_) => {
            let granted = daemon.platform().request_accessibility();
            log::info!("accessibility requested: {granted:?}");
            Resp::DeviceInfo(daemon.device_info())
        }
        Req::DeviceSetRelay(r) => {
            let relay = r.relay.trim();
            if relay.is_empty() {
                return Err(Error::Invalid("relay is required".into()));
            }
            daemon.restart_node(relay).await?;
            Resp::DeviceInfo(daemon.device_info())
        }
        Req::PolicyGet(_) => Resp::Policy(daemon.policy_view()),
        Req::PolicySet(r) => {
            let Some(policy) = r.policy else {
                return Err(Error::Invalid("policy is required".into()));
            };
            daemon.set_policy(&policy)?;
            Resp::Policy(daemon.policy_view())
        }

        // ---------------------------------------------------------- terminal
        Req::TerminalSessions(r) => {
            if !r.watch {
                return Ok(Resp::TerminalSessions(
                    crate::native_terminals::list(daemon).await,
                ));
            }
            let (tx, mut rx) = tokio::sync::mpsc::channel(16);
            let watch = crate::native_terminals::watch(daemon, tx);
            tokio::pin!(watch);
            let mut finished = false;
            loop {
                tokio::select! {
                    _ = events.cancelled().cancelled() => break,
                    _ = &mut watch, if !finished => finished = true,
                    next = rx.recv() => match next {
                        Some(list) => events.send(v1::event::Ev::TerminalSessions(list)).await?,
                        None => break,
                    },
                }
            }
            Resp::Ack(v1::Ack {})
        }
        Req::TerminalAttach(r) => {
            let (terminal, raw) = terminal_for(daemon, &r.session)?;
            let terminal = terminal.clone();
            let session = raw.to_owned();
            let (tx, mut rx) = tokio::sync::mpsc::channel(64);
            let attach =
                tokio::spawn(async move { terminal.attach(&session, r.since_rev, tx).await });
            loop {
                tokio::select! {
                    _ = events.cancelled().cancelled() => break,
                    next = rx.recv() => match next {
                        Some(mut frame) => {
                            frame.session = r.session.clone();
                            if frame.full && !frame.desktop_attach_command.is_empty() {
                                frame.native_sources = crate::native_terminals::sources(daemon).into_iter()
                                    .filter(|s| matches!(s.source.as_str(), "iterm2" | "ghostty")).collect();
                            }
                            let ended = frame.ended;
                            events.send(v1::event::Ev::TerminalFrame(frame)).await?;
                            if ended { break; }
                        }
                        None => break,
                    }
                }
            }
            drop(rx);
            if let Ok(Err(error)) = attach.await {
                return Err(error.into());
            }
            Resp::Ack(v1::Ack {})
        }
        Req::TerminalInput(r) => {
            let (terminal, raw) = terminal_for(daemon, &r.session)?;
            let terminal = terminal.clone();
            let raw = raw.to_owned();
            let mut body = r.session.as_bytes().to_vec();
            body.push(0);
            body.extend_from_slice(&r.bytes);
            let subject = r.session.clone();
            ledgered(
                daemon,
                &r.op_id,
                "terminal.input",
                &subject,
                false,
                &body,
                async move {
                    terminal.input(&raw, &r.bytes).await?;
                    Ok(String::new())
                },
            )
            .await?
        }
        Req::TerminalRun(r) => {
            let agent = agent_of(daemon)?
                .session(&r.session)
                .ok_or_else(|| Error::Invalid("agent session not found".into()))?;
            if agent.read_only || agent.mode == "chat" {
                return Err(Error::Invalid(
                    "This session does not support command execution".into(),
                ));
            }
            if r.op_id.is_empty() {
                return Err(Error::Invalid("run requires operation id".into()));
            }
            #[cfg(unix)]
            {
                use muster_proto::Message as _;
                let muster_terminal::Terminal::Tmux(service) = daemon.tmux.as_ref() else {
                    unreachable!()
                };
                ledgered(
                    daemon,
                    &r.op_id,
                    "terminal.run",
                    &r.session,
                    false,
                    &r.encode_to_vec(),
                    async {
                        let mut delivered = Vec::new();
                        for run in service.run_list(&r.session).await? {
                            if daemon
                                .store()
                                .ledger()
                                .status(&muster_terminal::commands::return_id(&run.id))?
                                .as_deref()
                                == Some("applied")
                            {
                                delivered.push(run.id);
                            }
                        }
                        let owner = match ctx.principal {
                            Principal::Remote(id) => id,
                            _ => daemon.device_id(),
                        }
                        .to_string();
                        Ok(service
                            .start_run(&agent.cwd, &r, &delivered, &owner)
                            .await?)
                    },
                )
                .await?
            }
            #[cfg(not(unix))]
            {
                let _ = agent;
                return Err(Error::Unavailable("tmux requires a Unix host".into()));
            }
        }
        Req::TerminalRuns(r) => {
            #[cfg(unix)]
            {
                let muster_terminal::Terminal::Tmux(service) = daemon.tmux.as_ref() else {
                    unreachable!()
                };
                let mut runs = service.run_list(&r.session).await?;
                let agent = agent_of(daemon)?;
                // The final text may arrive before turn completion. Refresh the
                // CLI provider before allowing automatic result delivery.
                if muster_agent::orca::is_orca(&r.session) {
                    agent.orca_source()?.refresh(&r.session).await?;
                }
                let return_ready = agent
                    .session(&r.session)
                    .is_some_and(|s| s.state == "idle" && !s.read_only);
                for run in &mut runs {
                    run.return_ready = return_ready;
                    run.return_state = daemon
                        .store()
                        .ledger()
                        .status(&muster_terminal::commands::return_id(&run.id))?
                        .unwrap_or_default();
                }
                Resp::TerminalRuns(v1::TerminalRunList { runs })
            }
            #[cfg(not(unix))]
            {
                let _ = r;
                return Err(Error::Unavailable("tmux requires a Unix host".into()));
            }
        }
        Req::AgentTerminal(r) => {
            if r.op_id.is_empty() {
                return Err(Error::Invalid(
                    "terminal action requires operation id".into(),
                ));
            }
            let agent = agent_of(daemon)?
                .session(&r.session)
                .ok_or_else(|| Error::Invalid("agent session not found".into()))?;
            #[cfg(unix)]
            {
                use muster_proto::Message as _;
                let muster_terminal::Terminal::Tmux(service) = daemon.tmux.as_ref() else {
                    unreachable!()
                };
                let body = r.encode_to_vec();
                ledgered(
                    daemon,
                    &r.op_id,
                    "agent.terminal",
                    &r.session,
                    false,
                    &body,
                    async { Ok(service.action(&r.session, &agent.cwd, r.action).await?) },
                )
                .await?
            }
            #[cfg(not(unix))]
            {
                let _ = agent;
                return Err(Error::Unavailable("tmux requires a Unix host".into()));
            }
        }
        Req::TerminalCreate(r) => create_native_terminal(daemon, r).await?,
        Req::TerminalResize(r) => {
            let (terminal, raw) = terminal_for(daemon, &r.session)?;
            terminal.resize(raw, r.cols, r.rows).await?;
            Resp::Ack(v1::Ack {})
        }
        Req::TerminalClose(r) => {
            let (terminal, raw) = terminal_for(daemon, &r.session)?;
            terminal.close(raw).await?;
            Resp::Ack(v1::Ack {})
        }
        Req::TerminalHistory(r) => {
            let (terminal, raw) = terminal_for(daemon, &r.session)?;
            Resp::TerminalLines(v1::TerminalLines {
                lines: terminal.history(raw, r.lines).await?,
            })
        }

        // ------------------------------------------------------------- agent
        Req::OrcaCreate(r) => {
            use muster_proto::Message as _;
            if r.op_id.is_empty() {
                return Err(Error::Invalid(
                    "Orca creation requires an operation id".into(),
                ));
            }
            let source = agent_of(daemon)?.orca_source()?.clone();
            let body = r.encode_to_vec();
            ledgered(
                daemon,
                &r.op_id,
                "orca.create",
                "orca",
                false,
                &body,
                async {
                    source
                        .create(&r.directory, &r.agent, &r.op_id, &r.env)
                        .await
                        .map_err(Error::from)
                },
            )
            .await?
        }
        Req::AgentList(r) => {
            let agent = agent_of(daemon)?;
            let provider = if r.provider.is_empty() || r.provider == "orca" {
                None
            } else {
                Some(Provider::parse(&r.provider).ok_or_else(|| {
                    Error::Invalid(format!(
                        "provider must be one of {}",
                        muster_core::apps::agent_providers().join(", ")
                    ))
                })?)
            };
            Resp::AgentSessions(v1::AgentSessionPage {
                sessions: AgentVisibility::new(daemon, ctx)
                    .list(
                        agent,
                        provider,
                        if r.provider == "orca" {
                            usize::MAX
                        } else {
                            r.limit as usize
                        },
                    )
                    .into_iter()
                    .filter(|s| r.provider != "orca" || s.provider == "orca")
                    .take(if r.limit == 0 { 200 } else { r.limit as usize })
                    .collect(),
            })
        }
        Req::AgentWatch(r) => {
            let agent = agent_of(daemon)?.clone();
            let visible = AgentVisibility::new(daemon, ctx);
            let mut rx = agent.subscribe();
            let filter = r.session.clone();
            // The watched session's provider, fixed for the stream's life.
            let mut provider = None;
            let mut _watch = None;
            // Current state first so the watcher starts from a known view.
            if filter.is_empty() {
                events
                    .send(v1::event::Ev::AgentEvent(v1::AgentEvent {
                        ev: Some(v1::agent_event::Ev::ListChanged(v1::AgentSessionPage {
                            sessions: visible.list(&agent, None, 0),
                        })),
                    }))
                    .await?;
            } else {
                let session = agent
                    .session(&filter)
                    .ok_or_else(|| Error::NotFound(format!("agent session {filter}")))?;
                provider = Some(session.provider.clone());
                _watch = Some(agent.watch_session(&filter));
                events
                    .send(v1::event::Ev::AgentEvent(v1::AgentEvent {
                        ev: Some(v1::agent_event::Ev::State(session)),
                    }))
                    .await?;
                for interaction in agent.pending(&filter) {
                    events
                        .send(v1::event::Ev::AgentEvent(v1::AgentEvent {
                            ev: Some(v1::agent_event::Ev::Interaction(interaction)),
                        }))
                        .await?;
                }
            }
            loop {
                let event = tokio::select! {
                    _ = events.cancelled().cancelled() => break,
                    next = rx.recv() => match next {
                        Ok(event) => event,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            // A watcher cannot know which sessions/messages it
                            // missed. End the stream so reconnect can subscribe
                            // again and reconcile a fresh history snapshot.
                            return Err(Error::Unavailable(format!(
                                "agent event stream missed {skipped} events; reconnect and reload messages"
                            )));
                        }
                        Err(_) => break,
                    }
                };
                // A grant narrowed since the watch opened ends it, or hides
                // the sessions of providers switched off.
                if !visible.allows(provider.as_deref()) {
                    return Err(Error::Unauthorized(
                        "agents: access withdrawn while watching".into(),
                    ));
                }
                let shown = |session: &str| !filter.is_empty() || visible.session(&agent, session);
                let ev = match event {
                    muster_agent::Event::ListChanged if filter.is_empty() => {
                        v1::agent_event::Ev::ListChanged(v1::AgentSessionPage {
                            sessions: visible.list(&agent, None, 0),
                        })
                    }
                    muster_agent::Event::ListChanged => match agent.session(&filter) {
                        Some(session) => v1::agent_event::Ev::State(session),
                        None => continue,
                    },
                    muster_agent::Event::Message { session, message }
                        if (filter.is_empty() || session == filter) && shown(&session) =>
                    {
                        v1::agent_event::Ev::Message(message)
                    }
                    muster_agent::Event::State(session)
                        if (filter.is_empty() || session.id == filter)
                            && (!filter.is_empty() || visible.allows(Some(&session.provider))) =>
                    {
                        v1::agent_event::Ev::State(session)
                    }
                    muster_agent::Event::Interaction(interaction)
                        if (filter.is_empty() || interaction.session == filter)
                            && shown(&interaction.session) =>
                    {
                        v1::agent_event::Ev::Interaction(interaction)
                    }
                    muster_agent::Event::InteractionResolved {
                        session,
                        interaction,
                    } if (filter.is_empty() || session == filter) && shown(&session) => {
                        v1::agent_event::Ev::InteractionResolved(interaction)
                    }
                    muster_agent::Event::Reset { session } if session == filter => {
                        // Message ids were renumbered: the watcher reloads.
                        return Err(Error::Unavailable(
                            "desktop chat history changed; reconnect and reload messages".into(),
                        ));
                    }
                    _ => continue,
                };
                events
                    .send(v1::event::Ev::AgentEvent(v1::AgentEvent { ev: Some(ev) }))
                    .await?;
            }
            Resp::Ack(v1::Ack {})
        }
        Req::AgentSendStatus(r) => send_status(daemon, &r)?,
        Req::AgentMessages(r) => Resp::AgentMessages(agent_of(daemon)?.history_page(&r).await?),
        request @ (Req::AgentSend(_)
        | Req::AgentRespond(_)
        | Req::AgentInterrupt(_)
        | Req::AgentComplete(_)
        | Req::AgentInspect(_))
            if orca_session(daemon, &request) =>
        {
            orca_operation(daemon, request).await?
        }
        request @ (Req::AgentSend(_)
        | Req::AgentRespond(_)
        | Req::AgentInterrupt(_)
        | Req::AgentComplete(_)
        | Req::AgentInspect(_))
            if cloud_session(daemon, &request) =>
        {
            cloud_operation(daemon, request).await?
        }
        request @ (Req::AgentSend(_)
        | Req::AgentRespond(_)
        | Req::AgentInterrupt(_)
        | Req::AgentComplete(_)
        | Req::AgentInspect(_)) => locked_operation(daemon, request).await?,
        request @ (Req::AgentChats(_)
        | Req::AgentChatOpen(_)
        | Req::AgentChatOptions(_)
        | Req::AgentChatSetOptions(_)) => chat_control(daemon, request).await?,
        Req::AgentFiles(r) => Resp::FileList(agent_of(daemon)?.files(&r.session, &r.path).await?),
        Req::AgentFile(r) => Resp::FileContent(
            agent_of(daemon)?
                .file(&r.session, &r.path, r.max_bytes)
                .await?,
        ),
        Req::AgentGit(r) => Resp::GitLog(
            agent_of(daemon)?
                .git(&r.session, r.limit, &r.diff_commit)
                .await?,
        ),
    })
}

impl From<muster_rpc::Error> for Error {
    fn from(error: muster_rpc::Error) -> Self {
        Error::Unavailable(error.to_string())
    }
}

/// Internal dispatch after the console owner has admitted this exact request.
/// Ordinary callers reach it only while holding the shared write reservation.
/// Whether the request acts on a cloud session (`muster_agent::cloud`).
fn independent_session(daemon: &Daemon, request: &Req) -> bool {
    tmux_request(request) || cloud_session(daemon, request) || orca_session(daemon, request)
}
fn orca_session(daemon: &Daemon, request: &Req) -> bool {
    native_session(daemon, request)
        .ok()
        .is_some_and(|s| muster_agent::orca::is_orca(&s))
}
async fn orca_operation(daemon: &Arc<Daemon>, request: Req) -> Result<Resp, Error> {
    use muster_agent::orca::Action;
    use muster_proto::Message as _;
    let source = agent_of(daemon)?.orca_source()?.clone();
    let (session, op, kind, body, action) = match request {
        Req::AgentSend(r) => {
            muster_proto::attachments::validate(&r.text, &r.attachments).map_err(Error::Invalid)?;
            if !r.attachments.is_empty() {
                return Err(Error::Invalid(
                    "Orca currently accepts text only; nothing was sent".into(),
                ));
            }
            let body = r.encode_to_vec();
            (r.session, r.op_id, "agent.send", body, Action::Send(r.text))
        }
        Req::AgentInterrupt(r) => {
            let body = r.encode_to_vec();
            (
                r.session,
                r.op_id,
                "agent.interrupt",
                body,
                Action::Interrupt,
            )
        }
        Req::AgentRespond(r) => {
            if !r.answer.is_empty() {
                return Err(Error::Invalid("Use the structured question card".into()));
            }
            let action = if !r.answers.is_empty() {
                Action::Answers {
                    interaction: r.interaction.clone(),
                    answers: r.answers.clone(),
                }
            } else {
                Action::Respond {
                    interaction: r.interaction.clone(),
                    option: r.option.clone(),
                }
            };
            let body = r.encode_to_vec();
            (r.session, r.op_id, "agent.respond", body, action)
        }
        _ => {
            return Err(Error::Unavailable(
                "Orca does not offer desktop inspection".into(),
            ));
        }
    };
    if op.is_empty() {
        return Err(Error::Invalid(
            "CLI operations require an operation id".into(),
        ));
    }
    let target = session.clone();
    let operation = op.clone();
    ledgered(daemon, &op, kind, &session, false, &body, async move {
        source
            .operation(&target, &operation, action)
            .await
            .map_err(Error::from)
    })
    .await
}

fn cloud_session(daemon: &Daemon, request: &Req) -> bool {
    native_session(daemon, request)
        .ok()
        .is_some_and(|session| muster_agent::cloud::is_cloud(&session))
}

/// What a cloud session does not offer, by name: a prompt where the
/// profile withholds it, and always approvals, interrupts and inspection,
/// which stay with the provider's web app.
fn cloud_undeclared(cloud: &muster_agent::profile::Cloud, request: &Req) -> Option<&'static str> {
    let (offered, what) = match request {
        Req::AgentSend(_) => (cloud.capabilities.send, "sending prompts"),
        Req::AgentInterrupt(_) => (false, "interrupting a turn"),
        Req::AgentRespond(_) => (false, "answering prompts"),
        _ => (false, "inspection"),
    };
    (!offered).then_some(what)
}

/// An operation on a cloud session: the provider's API, with no desktop
/// app, Accessibility, desktop lock or Locked Use involved; the access
/// rules that admitted the request, the profile's cloud capabilities and
/// the idempotency ledger still apply.
pub(crate) async fn cloud_operation(daemon: &Arc<Daemon>, request: Req) -> Result<Resp, Error> {
    let agent = agent_of(daemon)?.clone();
    let session = native_session(daemon, &request)?;
    let info = agent
        .session(&session)
        .ok_or_else(|| Error::NotFound(format!("session {session}")))?;
    let cloud = muster_agent::profile::cloud(&info.provider)
        .ok_or_else(|| Error::Unavailable(format!("{} has no cloud sessions", info.provider)))?;
    if let Some(what) = cloud_undeclared(cloud, &request) {
        return Err(Error::Unavailable(format!(
            "{} does not offer {what}",
            cloud.label
        )));
    }
    let Req::AgentSend(r) = request else {
        return Err(Error::Unavailable(format!(
            "{} takes prompts only",
            cloud.label
        )));
    };
    use muster_proto::Message as _;
    muster_proto::attachments::validate(&r.text, &r.attachments).map_err(Error::Invalid)?;
    if !r.attachments.is_empty() {
        // Local file references mean nothing in the provider's cloud.
        return Err(Error::Invalid(
            "cloud sessions take text only; nothing was sent".into(),
        ));
    }
    let body = r.encode_to_vec();
    let subject = r.session.clone();
    ledgered(
        daemon,
        &r.op_id,
        "agent.send",
        &subject,
        false,
        &body,
        async move {
            agent
                .send_to(&r.session, &r.text)
                .await
                .map_err(Error::from)
        },
    )
    .await
}

/// UI reads navigate the desktop too; the Locked Use owner calls this only
/// after its signed operation is active, on the same guarded OS worker.
async fn chat_control(daemon: &Arc<Daemon>, request: Req) -> Result<Resp, Error> {
    Ok(match request {
        Req::AgentChats(r) => {
            let agent = agent_of(daemon)?.clone();
            let app = chat_app(&r.provider)?;
            let _provider = if r.full_history && app != muster_agent::desktop_chat::ChatApp::ChatGpt
            {
                desktop_ready(daemon)?;
                crate::agent_bridge::hold(daemon, app.profile(), LockKind::Chat, "load history")
                    .await?
            } else {
                None
            };
            let cache_key = format!("desktop-chat-history-v1:{}", r.provider);
            let saved = daemon.store().meta(&cache_key)?;
            let mut cache = muster_agent::chat_catalog::HistoryCache::restore(saved.as_deref());
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let cached_ax = r.full_history && app == muster_agent::desktop_chat::ChatApp::Claude;
            let full_history = cached_ax && cache.needs_full(now);
            if full_history {
                cache.attempted(now);
                daemon.store().set_meta(
                    &cache_key,
                    &serde_json::to_vec(&cache).map_err(|e| Error::Unavailable(e.to_string()))?,
                )?;
            }
            let mut catalog = crate::agent_bridge::chat_catalog(
                daemon.platform().as_ref(),
                agent.clone(),
                app,
                full_history,
            )
            .await?;
            if cached_ax {
                cache.merge(&mut catalog, now);
                daemon.store().set_meta(
                    &cache_key,
                    &serde_json::to_vec(&cache).map_err(|e| Error::Unavailable(e.to_string()))?,
                )?;
            }
            drop(_provider);
            // What a client weighs when it picks a Mac for its chats.
            catalog.busy = crate::agent_bridge::holder(daemon, app.profile())
                .map(|h| h.kind.as_str().to_string())
                .unwrap_or_default();
            catalog.agent_working = agent.list(None, usize::MAX).iter().any(|s| {
                s.provider == app.provider().as_str() && s.mode != "chat" && s.state == "running"
            });
            Resp::AgentChats(catalog)
        }
        Req::AgentChatOpen(r) => {
            desktop_ready(daemon)?;
            let agent = agent_of(daemon)?.clone();
            let app = chat_app(&r.provider)?;
            let _app =
                crate::agent_bridge::hold(daemon, app.profile(), LockKind::Chat, "open").await?;
            let chat = (!r.chat.is_empty()).then_some(r.chat.as_str());
            let session = crate::agent_bridge::open_chat(
                daemon.platform().as_ref(),
                agent.clone(),
                app,
                chat,
            )
            .await?;
            Resp::AgentSession(
                agent
                    .session(&session)
                    .ok_or_else(|| Error::NotFound(session.clone()))?,
            )
        }
        Req::AgentChatOptions(r) => {
            Resp::AgentChatOptions(session_options(daemon, &r.session, None).await?)
        }
        Req::AgentChatSetOptions(r) => {
            if r.model.is_empty() && r.effort.is_empty() {
                return Err(Error::Invalid("choose a model or an effort".into()));
            }
            Resp::AgentChatOptions(
                session_options(daemon, &r.session, Some((&r.model, &r.effort))).await?,
            )
        }
        _ => return Err(Error::Invalid("not a chat control request".into())),
    })
}

fn send_status(daemon: &Daemon, request: &v1::AgentSendRequest) -> Result<Resp, Error> {
    use muster_proto::Message;
    if request.op_id.is_empty() || request.session.is_empty() {
        return Err(Error::Invalid(
            "Receipt requires the original operation and session".into(),
        ));
    }
    let outcome =
        daemon
            .store()
            .ledger()
            .outcome(&request.op_id, "agent.send", &request.encode_to_vec())?;
    Ok(match outcome {
        muster_store::Outcome::Applied(id) => op_result(
            Status::Applied,
            "",
            String::from_utf8(id).map_err(|_| Error::Unavailable("Invalid receipt".into()))?,
        ),
        muster_store::Outcome::Rejected(reason) => {
            op_result(Status::Rejected, String::from_utf8_lossy(&reason), "")
        }
        muster_store::Outcome::Conflict => {
            return Err(Error::Conflict(
                "Receipt does not match the original send".into(),
            ));
        }
        muster_store::Outcome::Unknown => op_result(
            Status::Unknown,
            "Waiting for confirmation; the message will not be resent",
            "",
        ),
    })
}

pub(crate) async fn locked_operation(daemon: &Arc<Daemon>, request: Req) -> Result<Resp, Error> {
    if let Req::TerminalCreate(r) = request {
        validate_locked_terminal_creation(daemon, &r)?;
        return create_native_terminal(daemon, r).await;
    }

    // Fail before the operation ledger or attachment writes: missing OS
    // permission is a setup prerequisite, not an ambiguous attempted send.
    if daemon.platform().accessibility() == Some(false) {
        return Err(Error::Unavailable(format!(
            "{}: Accessibility is not granted to musterd. On that Mac enable /Applications/Muster.app/Contents/MacOS/musterd in System Settings > Privacy & Security > Accessibility, then open this session in its desktop app and retry.",
            daemon.name()
        )));
    }
    if matches!(
        request,
        Req::AgentChats(_)
            | Req::AgentChatOpen(_)
            | Req::AgentChatOptions(_)
            | Req::AgentChatSetOptions(_)
    ) {
        return chat_control(daemon, request).await;
    }
    let agent = agent_of(daemon)?;
    let session = native_session(daemon, &request)?;
    require_capability(agent, &session, &request)?;
    agent.require_desktop_input(&session)?;
    let what = match &request {
        Req::AgentSend(_) => "send",
        Req::AgentRespond(_) => "respond",
        Req::AgentInspect(_) => "inspect",
        Req::AgentComplete(_) => "complete",
        _ => "interrupt",
    };
    let op_id = match &request {
        Req::AgentSend(r) => r.op_id.as_str(),
        Req::AgentRespond(r) => r.op_id.as_str(),
        Req::AgentInterrupt(r) => r.op_id.as_str(),
        _ => "",
    };
    let preparation_started = std::time::Instant::now();
    log::info!("Agent {what} op={op_id:?} target={session:?}: waiting for provider queue");
    // FIFO for this provider, including navigation, input and its receipt.
    let _app = match crate::agent_bridge::hold_session(daemon, agent, &session, what).await {
        Ok(guard) => guard,
        Err(error @ Error::Timeout(_)) => {
            return Ok(op_result(Status::Rejected, error.to_string(), ""));
        }
        Err(error) => return Err(error),
    };
    log::info!(
        "Agent {what} op={op_id:?} target={session:?}: native preparation started queue_ms={}",
        preparation_started.elapsed().as_millis()
    );
    #[cfg(target_os = "macos")]
    if what == "send" && crate::agent_bridge::drives_desktop(daemon.platform().as_ref()) {
        muster_agent::control_guard::wait_for_quiet(
            muster_platform::macos::input::user_input_idle,
            Duration::from_millis(250),
            Duration::from_secs(3),
        )
        .await?;
    }
    crate::agent_bridge::ensure_native(daemon.platform().as_ref(), agent.clone(), &session)
        .await
        .inspect_err(|error| {
            // Pre-ledger failures used to leave only “operation finished” in
            // Locked Use logs. Record the stage and reason, never the request.
            log::warn!("Agent {what} op={op_id:?} target={session:?}: native preparation failed before ledger elapsed_ms={}: {error}", preparation_started.elapsed().as_millis());
        })?;
    log::info!(
        "Agent {what} op={op_id:?} target={session:?}: native preparation ready elapsed_ms={}",
        preparation_started.elapsed().as_millis()
    );
    Ok(match request {
        Req::AgentComplete(r) => Resp::AgentCompletion(v1::AgentCompletion {
            text: agent.complete(&r.session).await?,
            session: r.session,
        }),
        Req::AgentInspect(r) => {
            let agent = agent.clone();
            let text = muster_agent::control_guard::spawn_blocking(move || {
                agent.question_diagnostic(&r.session)
            })
            .await
            .map_err(|e| Error::Unavailable(e.to_string()))??;
            Resp::DebugText(v1::DebugText { text })
        }
        Req::AgentSend(r) => {
            let agent = agent_of(daemon)?.clone();
            use muster_proto::Message as _;
            muster_proto::attachments::validate(&r.text, &r.attachments).map_err(Error::Invalid)?;
            if muster_agent::desktop_chat::app_of(&r.session).is_some() && !r.attachments.is_empty()
            {
                // Local file references mean nothing to a claude.ai / ChatGPT chat.
                return Err(Error::Invalid(
                    "desktop chats take text only; nothing was sent".into(),
                ));
            }
            // Bind filenames, media types and all bytes to the idempotency ledger.
            let body = r.encode_to_vec();
            let root = daemon.config.dir.join("attachments");
            let subject = r.session.clone();
            ledgered(
                daemon,
                &r.op_id,
                "agent.send",
                &subject,
                true,
                &body,
                async move {
                    let text = tokio::task::spawn_blocking(move || {
                        muster_agent::attachments::prepare(&root, &r.text, &r.attachments)
                    })
                    .await
                    .map_err(|e| Error::Unavailable(e.to_string()))??;
                    // The user checked the Mac: the earlier write never arrived.
                    if !r.release_unconfirmed.is_empty() {
                        agent.release_unconfirmed(&r.session, &r.release_unconfirmed)?;
                    }
                    // A new desktop chat answers with the conversation it became.
                    Ok(agent.send_to(&r.session, &text).await?)
                },
            )
            .await?
        }
        Req::AgentRespond(r) => {
            let agent = agent_of(daemon)?.clone();
            let body = muster_proto::Message::encode_to_vec(&r);
            let subject = if r.session.is_empty() {
                r.interaction.clone()
            } else {
                r.session.clone()
            };
            let question_count = r.answers.len();
            let mut response = ledgered(
                daemon,
                &r.op_id,
                "agent.respond",
                &subject,
                true,
                &body,
                async move {
                    if !r.answers.is_empty() {
                        if r.session.is_empty() {
                            return Err(crate::Error::Invalid(
                                "Question answers require a session".into(),
                            ));
                        }
                        agent
                            .answer_questions(&r.session, &r.interaction, &r.answers)
                            .await?;
                    } else if r.session.is_empty() {
                        agent.respond(&r.interaction, &r.option, &r.answer).await?;
                    } else {
                        agent
                            .respond_in_session(&r.session, &r.interaction, &r.option, &r.answer)
                            .await?;
                    }
                    Ok(r.interaction)
                },
            )
            .await?;
            if question_count > 0
                && let Resp::OpResult(result) = &mut response
                && matches!(
                    Status::try_from(result.status),
                    Ok(Status::Applied | Status::Duplicate)
                )
            {
                result.message =
                    format!("已执行 {question_count} 道题的答题操作，请在原会话确认结果。");
            }
            response
        }
        Req::AgentInterrupt(r) => {
            let agent = agent_of(daemon)?.clone();
            let body = r.session.clone().into_bytes();
            let subject = r.session.clone();
            ledgered(
                daemon,
                &r.op_id,
                "agent.interrupt",
                &subject,
                true,
                &body,
                async move {
                    agent.interrupt(&r.session).await?;
                    Ok(r.session)
                },
            )
            .await?
        }
        _ => return Err(Error::Invalid("not a locked agent operation".into())),
    })
}

/// Rejects an operation the session's app does not declare in its profile
/// (`[capabilities]`, ARCHITECTURE §3.6.4), before any queue, lock, unlock
/// or ledger entry. A declared operation still passes every engine check,
/// and so does a session with no profile to consult (one the catalog does
/// not list, driven by a controller attached in-process): the engine's own
/// checks decide those.
fn require_capability(
    agent: &muster_agent::AgentService,
    session: &str,
    request: &Req,
) -> Result<(), Error> {
    let Some(app) = agent
        .session(session)
        .and_then(|info| muster_agent::profile::app(&info.provider))
    else {
        return Ok(());
    };
    match undeclared(&app.capabilities, request) {
        None => Ok(()),
        Some(what) => Err(Error::Unavailable(format!(
            "{} does not offer {what}",
            app.app.agent_label
        ))),
    }
}

/// What `request` needs that `caps` does not declare, if anything: a send,
/// an interrupt, a question-card answer (`answers`), a queued message
/// (`queued:` interactions) or an ordinary prompt answer.
fn undeclared(caps: &muster_agent::profile::Capabilities, request: &Req) -> Option<&'static str> {
    let (offered, what) = match request {
        Req::AgentChatOptions(_) | Req::AgentChatSetOptions(_) => {
            (caps.model_options, "model or effort choices")
        }
        Req::AgentSend(_) => (caps.send, "sending prompts"),
        Req::AgentInterrupt(_) => (caps.interrupt, "interrupting a turn"),
        Req::AgentRespond(r) if !r.answers.is_empty() => (caps.questions, "question cards"),
        Req::AgentRespond(r) if r.interaction.starts_with("queued:") => {
            (caps.queue, "queued messages")
        }
        Req::AgentRespond(_) => (caps.respond, "answering prompts"),
        _ => (true, ""),
    };
    (!offered).then_some(what)
}

/// Model and effort choices exist only where the app's profile declares
/// them.
async fn session_options(
    daemon: &Daemon,
    session: &str,
    set: Option<(&str, &str)>,
) -> Result<v1::AgentChatOptions, Error> {
    desktop_ready(daemon)?;
    let agent = agent_of(daemon)?.clone();
    let chat = muster_agent::desktop_chat::app_of(session);
    let app = if let Some(chat) = chat {
        chat.profile()
    } else {
        let target = agent
            .session(session)
            .ok_or_else(|| Error::NotFound(session.into()))?;
        if target.read_only || target.mode == "cloud" {
            return Err(Error::Unavailable(
                "This session cannot change model or effort".into(),
            ));
        }
        muster_agent::profile::app(&target.provider).ok_or_else(|| {
            Error::Unavailable("This provider offers no model or effort choices".into())
        })?
    };
    if !app.capabilities.model_options {
        return Err(Error::Unavailable(
            "This provider offers no model or effort choices".into(),
        ));
    }
    let _guard =
        crate::agent_bridge::hold_session(daemon, &agent, session, "model and effort").await?;
    if let Some(chat) = chat {
        return crate::agent_bridge::chat_options(
            daemon.platform().as_ref(),
            agent,
            chat,
            session,
            set,
        )
        .await;
    }
    crate::agent_bridge::ensure_native(daemon.platform().as_ref(), agent.clone(), session).await?;
    Ok(agent.model_options(session, set).await?)
}

/// Desktop chat navigation needs Accessibility and an unlocked console.
fn desktop_ready(daemon: &Daemon) -> Result<(), Error> {
    if daemon.platform().accessibility() == Some(false) {
        return Err(Error::Unavailable(format!(
            "{}: Accessibility is not granted to musterd. On that Mac enable /Applications/Muster.app/Contents/MacOS/musterd in System Settings > Privacy & Security > Accessibility.",
            daemon.name()
        )));
    }
    if daemon.platform().locked() {
        return Err(Error::Unavailable(format!(
            "{} is locked; unlock it to use its desktop chats",
            daemon.name()
        )));
    }
    Ok(())
}

fn chat_app(provider: &str) -> Result<muster_agent::desktop_chat::ChatApp, Error> {
    muster_agent::desktop_chat::ChatApp::from_provider(provider).ok_or_else(|| {
        Error::Invalid(format!(
            "provider must be one with a Chat mode ({})",
            muster_core::apps::chat_providers().join(", ")
        ))
    })
}

/// For local diagnostics: a desktop app with a Chat mode, by any name a
/// person types for it (`Claude`, `ChatGPT`, `codex`).
fn chat_app_named(name: &str) -> Option<muster_agent::desktop_chat::ChatApp> {
    muster_core::apps::chat_app(name)
        .and_then(|app| muster_agent::desktop_chat::ChatApp::from_provider(app.provider))
}

pub(crate) fn native_provider(daemon: &Daemon, request: &Req) -> Result<String, Error> {
    let direct = match request {
        Req::AgentChats(r) => Some(r.provider.as_str()),
        Req::AgentChatOpen(r) => Some(r.provider.as_str()),
        _ => None,
    };
    if let Some(provider) = direct {
        return Ok(chat_app(provider)?.provider().as_str().into());
    }
    let session = native_session(daemon, request)?;
    if let Some(app) = muster_agent::desktop_chat::app_of(&session) {
        return Ok(app.provider().as_str().into());
    }
    let provider = agent_of(daemon)?
        .session(&session)
        .ok_or_else(|| Error::NotFound(session))?
        .provider;
    if muster_core::apps::app(&provider).is_none() {
        return Err(Error::Invalid("unsupported native provider".into()));
    }
    Ok(provider)
}

pub(crate) fn native_session(daemon: &Daemon, request: &Req) -> Result<String, Error> {
    let agent = agent_of(daemon)?;
    match request {
        Req::AgentChats(r) => Ok(muster_agent::desktop_chat::new_session_id(chat_app(
            &r.provider,
        )?)),
        Req::AgentChatOpen(r) => {
            let app = chat_app(&r.provider)?;
            if r.chat.is_empty() {
                Ok(muster_agent::desktop_chat::new_session_id(app))
            } else if muster_agent::desktop_chat::app_of(&r.chat) == Some(app) {
                Ok(r.chat.clone())
            } else {
                Err(Error::Invalid(
                    "conversation does not belong to this provider".into(),
                ))
            }
        }
        Req::AgentChatOptions(r) => Ok(r.session.clone()),
        Req::AgentChatSetOptions(r) => Ok(r.session.clone()),
        Req::AgentSend(r) => Ok(r.session.clone()),
        Req::AgentInspect(r) => Ok(r.session.clone()),
        Req::AgentComplete(r) => Ok(r.session.clone()),
        Req::AgentInterrupt(r) => Ok(r.session.clone()),
        Req::AgentRespond(r) if !r.session.is_empty() => {
            if !agent
                .pending(&r.session)
                .iter()
                .any(|i| i.id == r.interaction)
            {
                return Err(Error::NotFound(
                    "interaction is no longer pending in this session".into(),
                ));
            }
            Ok(r.session.clone())
        }
        Req::AgentRespond(r) => agent
            .list(None, 0)
            .iter()
            .find(|session| {
                agent
                    .pending(&session.id)
                    .iter()
                    .any(|i| i.id == r.interaction)
            })
            .map(|s| s.id.clone())
            .ok_or_else(|| Error::NotFound("interaction is no longer pending".into())),
        _ => Err(Error::Invalid(
            "only native Agent operations support Locked Use".into(),
        )),
    }
}

/// Checks a Locked Use operation targets a native desktop app this device
/// allows, and returns the agent provider of its session.
pub(crate) fn require_native_operation(daemon: &Daemon, request: &Req) -> Result<String, Error> {
    if matches!(request, Req::AgentRespond(r) if r.session.is_empty()) {
        return Err(Error::Invalid(
            "Locked Use approvals require an explicit session".into(),
        ));
    }
    let session = native_session(daemon, request)?;
    let provider = native_provider(daemon, request)?;
    let info = agent_of(daemon)?.session(&session);
    let bundle = muster_core::apps::app(&provider)
        .ok_or_else(|| Error::Invalid("provider has no supported native app".into()))?
        .bundle_id;
    if !daemon
        .policy()
        .read()
        .unwrap()
        .ax_allowlist
        .iter()
        .any(|x| x == bundle)
    {
        return Err(Error::Unauthorized(
            "native app is not in this device's allowlist".into(),
        ));
    }
    if info.as_ref().is_some_and(|info| {
        info.read_only || !matches!(info.controller.as_str(), "native_app" | "none")
    }) {
        return Err(Error::Invalid(
            "Locked Use only controls a native desktop app".into(),
        ));
    }
    // Before any unlock: a session no desktop app shows has no input channel.
    agent_of(daemon)?.require_desktop_input(&session)?;
    require_capability(agent_of(daemon)?, &session, request)?;
    Ok(provider)
}

/// The bound request runs once after authorization; OS work stays in its work queue.
async fn create_native_terminal(
    daemon: &Arc<Daemon>,
    r: v1::TerminalCreateRequest,
) -> Result<Resp, Error> {
    use muster_proto::Message as _;
    if crate::native_terminals::source(daemon, &r.source)?.source() == "ghostty"
        && daemon.platform().locked()
    {
        return Err(Error::Unavailable(
            "Ghostty creation requires an authorized unlock before creating its renderer".into(),
        ));
    }
    let terminal = crate::native_terminals::source(daemon, &r.source)?;
    let profile = (!r.profile.is_empty()).then_some(r.profile.as_str());
    let command = if r.managed_session.is_empty() {
        None
    } else {
        #[cfg(unix)]
        {
            let muster_terminal::Terminal::Tmux(service) = daemon.tmux.as_ref() else {
                unreachable!()
            };
            Some(service.native_command(&r.managed_session).await?)
        }
        #[cfg(not(unix))]
        {
            return Err(Error::Unavailable("tmux requires Unix".into()));
        }
    };
    // New clients supply operation IDs. Preserve legacy creation, but
    // managed launches always require a ledger to prevent replay.
    if r.op_id.is_empty() && (!r.source.is_empty() || command.is_some()) {
        return Err(Error::Invalid(
            "Native terminal creation requires an operation ID".into(),
        ));
    }
    let create = async {
        crate::native_terminals::prepare(terminal).await?;
        muster_agent::control_guard::check()?;
        muster_agent::control_guard::delivery_attempt();
        let session = match &command {
        Some(command) => terminal.create_command(profile, r.background, command).await,
        None => terminal.create(profile, r.background).await,
    }.map_err(|error| Error::UnknownOutcome(format!("Native creation may have reached the application; inspect its windows before creating again: {error}")))?;
        if session.id.is_empty() {
            return Err(Error::UnknownOutcome("Native application returned no session ID; inspect its windows before creating again".into()));
        }
        Ok::<_, Error>(crate::native_terminals::qualify(session, terminal.source()))
    };
    Ok(if r.op_id.is_empty() {
        Resp::TerminalSession(create.await?)
    } else {
        let body = r.encode_to_vec();
        let result = ledgered(
            daemon,
            &r.op_id,
            "terminal.create",
            terminal.source(),
            false,
            &body,
            async { Ok(create.await?.id) },
        )
        .await?;
        match result {
            Resp::OpResult(result) if matches!(result.status, 0 | 1) => {
                Resp::TerminalSession(v1::TerminalSession {
                    id: result.result_id,
                    source: terminal.source().into(),
                    ..Default::default()
                })
            }
            other => other,
        }
    })
}

/// Only an exact Ghostty creation may use the terminal-scoped one-shot unlock.
pub(crate) fn validate_locked_terminal_creation(
    daemon: &Daemon,
    request: &v1::TerminalCreateRequest,
) -> Result<(), Error> {
    if request.op_id.is_empty()
        || request.op_id.len() > 128
        || crate::native_terminals::source(daemon, &request.source)?.source() != "ghostty"
    {
        return Err(Error::Invalid(
            "Locked terminal creation requires Ghostty and an operation ID".into(),
        ));
    }
    Ok(())
}

pub(crate) fn locked_operation_permission(
    daemon: &Daemon,
    request: &Req,
) -> Result<Permission, Error> {
    if let Req::TerminalCreate(r) = request {
        validate_locked_terminal_creation(daemon, r)?;
        Ok(Permission::Terminal { control: true })
    } else {
        Ok(Permission::Agent {
            provider: Some(require_native_operation(daemon, request)?),
            control: true,
        })
    }
}

#[cfg(test)]
mod setup_permission_tests {
    use super::*;
    #[tokio::test]
    async fn secondary_ghostty_ids_still_require_the_console_lease() {
        struct Fixture;
        impl muster_terminal::ghostty::GhosttyOs for Fixture {
            fn running(&self) -> bool {
                false
            }
            fn terminals(
                &self,
            ) -> Result<Vec<muster_terminal::ghostty::Listed>, muster_terminal::Error> {
                unreachable!()
            }
            fn perform(&self, _: &str, _: &str) -> Result<(), muster_terminal::Error> {
                unreachable!()
            }
            fn open(
                &self,
                _: &str,
            ) -> Result<Box<dyn muster_terminal::ghostty::Surface>, muster_terminal::Error>
            {
                unreachable!()
            }
            fn create(&self) -> Result<String, muster_terminal::Error> {
                unreachable!()
            }
            fn close(&self, _: &str) -> Result<(), muster_terminal::Error> {
                unreachable!()
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let mut config = crate::DaemonConfig::new(dir.path());
        config.listen = false;
        config.agent = None;
        config.cloud = false;
        config.bind = "127.0.0.1:0".parse().unwrap();
        config.terminal = Some(muster_terminal::TerminalKind::Iterm2(
            muster_terminal::TerminalConfig::default(),
        ));
        config.additional_terminals = vec![muster_terminal::TerminalKind::Ghostty(
            muster_terminal::GhosttyPlatform(Arc::new(Fixture)),
        )];
        let daemon = Daemon::start_with(config, Arc::new(crate::DefaultPlatform), None)
            .await
            .unwrap();
        let request = Req::TerminalAttach(v1::TerminalAttachRequest {
            session: "ghostty:opaque:one".into(),
            since_rev: 0,
        });
        assert_eq!(
            ghostty_session(&daemon, &request),
            Some("ghostty:opaque:one")
        );
        for id in ["iterm2:opaque:one", "legacy:iterm-id", "tmux-owned"] {
            let request = Req::TerminalAttach(v1::TerminalAttachRequest {
                session: id.into(),
                since_rev: 0,
            });
            assert_eq!(ghostty_session(&daemon, &request), None);
        }
        let (host, raw) = terminal_for(&daemon, "legacy:iterm-id").unwrap();
        assert_eq!(host.source(), "iterm2");
        assert_eq!(raw, "legacy:iterm-id");
        assert_eq!(
            terminal_for(&daemon, "ghostty:opaque:one").unwrap().1,
            "opaque:one"
        );
        assert!(terminal_for(&daemon, "ghostty:").is_err());
        daemon.shutdown().await;
    }

    #[test]
    fn automation_prompt_is_local_only() {
        assert!(matches!(
            access_for(&Req::DeviceRequestAutomation(
                v1::DeviceRequestAutomationRequest {}
            )),
            Access::LocalOnly
        ));
    }

    /// Every admin request needs a presence proof, readdressing included;
    /// ordinary member requests do not.
    #[test]
    fn presence_follows_high_risk_permissions() {
        assert!(needs_presence(&Req::NetworkReaddress(
            v1::NetworkReaddressRequest::default()
        )));
        assert!(needs_presence(&Req::DeviceSetCover(
            v1::DeviceSetCoverRequest::default()
        )));
        assert!(!needs_presence(&Req::DeviceInfo(v1::DeviceInfoRequest {})));
        assert!(!needs_presence(&Req::AgentList(
            v1::AgentListRequest::default()
        )));
    }

    /// A cloud session takes a prompt only where its profile says so, and
    /// never an approval, an interrupt or an inspection: those stay with
    /// the provider's web app. Only cloud ids route to the cloud path.
    #[test]
    fn cloud_sessions_take_prompts_only_where_the_profile_says() {
        let claude = muster_agent::profile::cloud("claude").unwrap();
        let codex = muster_agent::profile::cloud("codex").unwrap();
        let send = Req::AgentSend(v1::AgentSendRequest::default());
        let interrupt = Req::AgentInterrupt(v1::AgentInterruptRequest::default());
        let respond = Req::AgentRespond(v1::AgentRespondRequest::default());
        let inspect = Req::AgentInspect(v1::AgentInspectRequest::default());
        assert_eq!(cloud_undeclared(claude, &send), None);
        assert_eq!(cloud_undeclared(codex, &send), Some("sending prompts"));
        for cloud in [claude, codex] {
            assert_eq!(
                cloud_undeclared(cloud, &interrupt),
                Some("interrupting a turn")
            );
            assert_eq!(cloud_undeclared(cloud, &respond), Some("answering prompts"));
            assert_eq!(cloud_undeclared(cloud, &inspect), Some("inspection"));
        }
        assert!(muster_agent::cloud::is_cloud("cloud:claude:session_1"));
        assert!(!muster_agent::cloud::is_cloud(
            "01a0e659-2de9-7f63-b80b-3b53fc7754e0"
        ));
    }

    /// The roster's agent provider scopes name exactly the providers the
    /// agent service reports on sessions.
    #[test]
    fn agent_provider_scopes_match_the_agent_service() {
        assert_eq!(
            Provider::ALL
                .iter()
                .map(|p| p.as_str())
                .chain(std::iter::once("orca"))
                .collect::<Vec<_>>(),
            muster_core::apps::agent_providers()
        );
    }

    /// An operation is refused by name when the app's profile does not
    /// declare it: CatDesk has no queue and no question cards, while the
    /// Codex profile declares both. Inspection is never gated.
    #[test]
    fn undeclared_capabilities_are_named() {
        use muster_agent::profile::{CATDESK_APP, CODEX_APP};
        let respond = |interaction: &str, answers: usize| {
            Req::AgentRespond(v1::AgentRespondRequest {
                interaction: interaction.into(),
                session: "s".into(),
                answers: vec![v1::QuestionAnswer::default(); answers],
                ..Default::default()
            })
        };
        let send = Req::AgentSend(v1::AgentSendRequest::default());
        let interrupt = Req::AgentInterrupt(v1::AgentInterruptRequest::default());
        let inspect = Req::AgentInspect(v1::AgentInspectRequest::default());
        let catdesk = &CATDESK_APP.capabilities;
        assert_eq!(
            undeclared(catdesk, &respond("queued:1", 0)),
            Some("queued messages")
        );
        assert_eq!(
            undeclared(catdesk, &respond("question:1", 2)),
            Some("question cards")
        );
        assert_eq!(undeclared(catdesk, &respond("approval:1", 0)), None);
        assert_eq!(undeclared(catdesk, &send), None);
        assert_eq!(undeclared(catdesk, &interrupt), None);
        assert_eq!(undeclared(catdesk, &inspect), None);
        let codex = &CODEX_APP.capabilities;
        for request in [
            respond("queued:1", 0),
            respond("question:1", 2),
            send,
            interrupt,
        ] {
            assert_eq!(undeclared(codex, &request), None);
        }
        let none = muster_agent::profile::Capabilities {
            send: false,
            respond: false,
            interrupt: false,
            questions: false,
            queue: false,
            files: false,
            chat: false,
            model_options: false,
        };
        assert_eq!(
            undeclared(&none, &Req::AgentSend(v1::AgentSendRequest::default())),
            Some("sending prompts")
        );
        assert_eq!(
            undeclared(
                &none,
                &Req::AgentInterrupt(v1::AgentInterruptRequest::default())
            ),
            Some("interrupting a turn")
        );
        assert_eq!(
            undeclared(&none, &respond("approval:1", 0)),
            Some("answering prompts")
        );
    }
    #[test]
    fn agent_terminals_require_control_and_only_owned_ids_route_to_tmux() {
        for request in [
            Req::TerminalRun(v1::TerminalRunRequest::default()),
            Req::AgentTerminal(v1::AgentTerminalRequest::default()),
        ] {
            assert!(matches!(
                access_for(&request),
                Access::Need(Permission::Terminal { control: true })
            ));
            assert!(tmux_request(&request));
            assert!(tmux_agent(&request).is_some());
        }
        let read = Req::TerminalRuns(v1::TerminalRunsRequest::default());
        assert!(matches!(
            access_for(&read),
            Access::Need(Permission::Terminal { control: false })
        ));
        assert!(tmux_request(&read));
        assert!(tmux_agent(&read).is_some());
        let request = Req::AgentTerminal(v1::AgentTerminalRequest::default());
        assert!(matches!(
            access_for(&request),
            Access::Need(Permission::Terminal { control: true })
        ));
        assert!(tmux_request(&request));
        for (session, owned) in [
            ("tmux-123-1", true),
            ("pty-1", false),
            ("ghostty-window", false),
        ] {
            let request = Req::TerminalInput(v1::TerminalInputRequest {
                session: session.into(),
                ..Default::default()
            });
            assert_eq!(tmux_request(&request), owned);
            assert!(matches!(
                access_for(&request),
                Access::Need(Permission::Terminal { control: true })
            ));
        }
    }
}
