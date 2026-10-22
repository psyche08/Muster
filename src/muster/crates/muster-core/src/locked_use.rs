//! Admission and lifecycle for a temporary, covered console operation or provider queue.
//!
//! This module does not unlock a console. The daemon must supply fresh OS
//! observations and authenticated plugin receipts, and execute `Directive`s.
//! In particular, a requested cover is not a confirmed cover, and a successful
//! authorization callback is not an observed OS unlock. See docs/LOCKED_USE.md.

use std::time::{Duration, Instant};

use rand::RngCore;

use crate::canonical::{Reader, Writer};
use crate::identity::verify_domain;
use crate::{Decision, DeviceId, Error, LocalPolicy, Permission, State, TargetState, authorize};

pub const DOMAIN: &str = "MUSTER-LOCKED-USE-V1";
pub const MIN_TTL: Duration = Duration::from_secs(5);
pub const MAX_TTL: Duration = Duration::from_secs(15);
pub const MAX_OBSERVATION_AGE: Duration = Duration::from_millis(250);
/// A terminal session ends after this long without terminal input, even
/// while its requester keeps renewing it.
pub const TERMINAL_IDLE: Duration = Duration::from_secs(600);
const MAX_TURN_BYTES: usize = 128;

/// OS audit-session id, not an app's session identifier. A fast-user switch or
/// logout invalidates the lease even if the numeric user id stays the same.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Console {
    pub uid: u32,
    pub audit_session: u32,
}

/// Immutable, target-issued challenge. Sign `signing_message()` with the
/// enrolled remote presence key, or an enrolled phone's device key. The authenticated
/// transport principal must also match `requester` when the signature arrives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Challenge {
    epoch: [u8; 32],
    nonce: [u8; 32],
    network: [u8; 32],
    target: DeviceId,
    requester: DeviceId,
    console: Console,
    turn: String,
    operation: [u8; 32],
    ttl_ms: u64,
}

impl Challenge {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(1)
            .fixed(&self.epoch)
            .fixed(&self.nonce)
            .fixed(&self.network)
            .fixed(&self.target.0)
            .fixed(&self.requester.0)
            .u32(self.console.uid)
            .u32(self.console.audit_session)
            .str(&self.turn)
            .fixed(&self.operation)
            .u64(self.ttl_ms);
        w.finish()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > 512 {
            return Err(reject("challenge too large"));
        }
        let mut r = Reader::new(bytes);
        if r.u8()? != 1 {
            return Err(reject("unsupported challenge version"));
        }
        let value = Self {
            epoch: r.fixed()?,
            nonce: r.fixed()?,
            network: r.fixed()?,
            target: DeviceId(r.fixed()?),
            requester: DeviceId(r.fixed()?),
            console: Console {
                uid: r.u32()?,
                audit_session: r.u32()?,
            },
            turn: r.str()?.to_owned(),
            operation: r.fixed()?,
            ttl_ms: r.u64()?,
        };
        r.finish()?;
        validate_turn(&value.turn)?;
        validate_console(value.console)?;
        validate_ttl(Duration::from_millis(value.ttl_ms))?;
        Ok(value)
    }

    pub fn signing_message(&self) -> Vec<u8> {
        let mut message = DOMAIN.as_bytes().to_vec();
        message.push(0);
        message.extend(self.encode());
        message
    }

    pub fn console(&self) -> Console {
        self.console
    }

    pub fn nonce(&self) -> &[u8; 32] {
        &self.nonce
    }

    pub fn turn(&self) -> &str {
        &self.turn
    }

    pub fn operation(&self) -> &[u8; 32] {
        &self.operation
    }

    /// Remote app-core validates against the authenticated target and pending
    /// operation before asking the platform to sign these bytes.
    pub fn signing_message_for(
        &self,
        network: &[u8; 32],
        target: DeviceId,
        requester: DeviceId,
        turn: &str,
        operation: &[u8; 32],
    ) -> Result<Vec<u8>, Error> {
        if &self.network != network
            || self.target != target
            || self.requester != requester
            || self.turn != turn
            || &self.operation != operation
        {
            return Err(reject(
                "challenge does not describe the requested operation",
            ));
        }
        Ok(self.signing_message())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Locked,
    Unlocked,
    Unknown,
}

/// Facts from OS adapters and the daemon, never from a remote request or a
/// ViewModel. Cover/watchdog acknowledgements must belong to this nonce.
#[derive(Clone, Copy, Debug)]
pub struct Observation {
    pub at: Instant,
    pub screen: Screen,
    pub console: Option<Console>,
    pub input_generation: Option<u64>,
    pub cover_nonce: Option<[u8; 32]>,
    pub watchdog_nonce: Option<[u8; 32]>,
    pub connected: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    AwaitingSignature,
    Armed,
    Authorizing,
    AwaitingUnlock,
    Active,
    Relocking,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    Completed,
    Cancelled,
    Expired,
    AuthorizationChanged,
    ObservationLost,
    ConsoleChanged,
    LocalInput,
    Disconnected,
    CoverLost,
    WatchdogLost,
    UnexpectedScreenState,
    PluginDenied,
    /// A terminal session went `TERMINAL_IDLE` without input.
    Idle,
}

/// Why a requester withdrew its challenge. Diagnostic only: the target logs
/// it, and it never grants, denies or shortens anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelReason {
    /// The user cancelled on the requester.
    User,
    /// Presence (Face ID / Touch ID) failed or was cancelled.
    Authentication,
    /// The requester's own wait for the signature expired.
    Expired,
    /// Another authentication was already in progress.
    Busy,
    /// The requester app left the foreground.
    Background,
    /// The requester app is terminating.
    Terminating,
}

impl CancelReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Authentication => "authentication",
            Self::Expired => "expired",
            Self::Busy => "busy",
            Self::Background => "background",
            Self::Terminating => "terminating",
        }
    }
    /// Unknown or absent text from an older or foreign requester is `None`.
    pub fn parse(text: &str) -> Option<Self> {
        [
            Self::User,
            Self::Authentication,
            Self::Expired,
            Self::Busy,
            Self::Background,
            Self::Terminating,
        ]
        .into_iter()
        .find(|reason| reason.as_str() == text)
    }
}

/// The adapter must keep applying LockAndKeepCovered until ReleaseCover.
/// A lock API returning success cannot acknowledge the resulting OS state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Directive {
    None,
    PrepareCoverAndWatchdog,
    WaitForUnlock,
    ReadyForAction,
    MonitorAction,
    LockAndKeepCovered(StopReason),
    ReleaseCover,
}

/// Supplied only after the authenticated plugin transaction has ended. A lost
/// socket, timeout or deactivation is *not* Denied: its result is unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginOutcome {
    Allowed,
    Denied,
}

struct Lease {
    challenge: Challenge,
    deadline: Instant,
    input_generation: u64,
    keys: OperationKeys,
    inspection: bool,
    /// The agent provider the operation drives; its grant is re-checked.
    /// Empty for a terminal session.
    provider: String,
    /// A terminal session: Terminal control instead of Agents, renewable
    /// while its requester signs each next window, ended by idleness.
    terminal: bool,
    device: bool,
    initial_screen: Screen,
    // A signed manual lease can cover an already unlocked console. This is
    // distinct from plugin consumption and carries its own relock obligation.
    covered_unlocked: bool,
    /// The renewal challenge issued and not yet signed, with its issue time.
    renewal: Option<(Challenge, Instant)>,
    last_input: Instant,
    last_observation: Instant,
    consumed: bool,
    consumed_at: Option<Instant>,
    plugin_outcome: Option<PluginOutcome>,
    saw_unlock: bool,
    action_taken: bool,
    provider_queue: Option<String>,
    relock_after: Option<Instant>,
    stop: Option<StopReason>,
}

/// One outstanding transaction per console. Not Clone/Serialize: restarting
/// creates a fresh random epoch and restores no authority. Runtime crash
/// recovery is the independent watchdog's responsibility.
pub struct Gate {
    target: DeviceId,
    epoch: [u8; 32],
    phase: Phase,
    lease: Option<Lease>,
    /// The first stop cause of the latest lease; it outlives the lease so the
    /// adapter can log why a transaction returned to Idle.
    last_stop: Option<StopReason>,
}

impl Gate {
    pub fn new(target: DeviceId) -> Result<Self, Error> {
        Ok(Self {
            target,
            epoch: random()?,
            phase: Phase::Idle,
            lease: None,
            last_stop: None,
        })
    }

    /// Why the latest transaction stopped, if it has. Diagnostic only.
    pub fn last_stop(&self) -> Option<StopReason> {
        self.last_stop
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// The caller hashes the canonical *entire* operation, including provider,
    /// agent session, op_id and payload. No prompt content belongs in this type.
    /// `provider` is the agent provider of the operation's session: the
    /// requester must hold Agents for it as well as `locked.control`.
    #[allow(clippy::too_many_arguments)]
    pub fn issue(
        &mut self,
        state: &State,
        policy: &LocalPolicy,
        requester: DeviceId,
        provider: &str,
        turn: &str,
        operation: [u8; 32],
        ttl: Duration,
        now: Instant,
        observation: Observation,
    ) -> Result<Challenge, Error> {
        self.issue_with_proof(
            state,
            policy,
            requester,
            Scope::Agent {
                provider,
                inspection: false,
            },
            turn,
            operation,
            ttl,
            now,
            observation,
        )
    }

    /// Explicit host-maintained device cover, independent of Accessibility.
    #[allow(clippy::too_many_arguments)]
    pub fn issue_device(
        &mut self,
        state: &State,
        policy: &LocalPolicy,
        requester: DeviceId,
        turn: &str,
        operation: [u8; 32],
        ttl: Duration,
        now: Instant,
        observation: Observation,
    ) -> Result<Challenge, Error> {
        self.issue_with_proof(
            state,
            policy,
            requester,
            Scope::Device,
            turn,
            operation,
            ttl,
            now,
            observation,
        )
    }

    pub fn is_device(&self) -> bool {
        self.lease.as_ref().is_some_and(|l| l.device)
    }

    /// Only the explicit signed device scope authorizes host renewal. Ordinary
    /// operations and terminals can never become persistent this way.
    pub fn maintain_device(
        &mut self,
        state: &State,
        policy: &LocalPolicy,
        now: Instant,
        observation: Observation,
    ) -> Result<Instant, Error> {
        if !self.is_device()
            || self.phase != Phase::Active
            || !self.lease.as_ref().unwrap().action_taken
        {
            return Err(reject("no active manual device lease"));
        }
        self.check(state, policy, now, observation, true)?;
        if observation.screen != Screen::Unlocked {
            self.stop(StopReason::UnexpectedScreenState, now);
            return Err(reject("console is locked"));
        }
        let deadline = now + MAX_TTL;
        self.lease.as_mut().unwrap().deadline = deadline;
        Ok(deadline)
    }

    /// A terminal session: the requester holds `locked.control` and Terminal
    /// control. The first window unlocks the console like any operation; it
    /// then stays Active only while the requester signs each `renewal`, and
    /// ends after `TERMINAL_IDLE` without input.
    #[allow(clippy::too_many_arguments)]
    pub fn issue_terminal(
        &mut self,
        state: &State,
        policy: &LocalPolicy,
        requester: DeviceId,
        turn: &str,
        operation: [u8; 32],
        ttl: Duration,
        now: Instant,
        observation: Observation,
    ) -> Result<Challenge, Error> {
        self.issue_with_proof(
            state,
            policy,
            requester,
            Scope::Terminal,
            turn,
            operation,
            ttl,
            now,
            observation,
        )
    }

    /// A signed read-only inspection may use an enrolled device key. The issuer
    /// must bind an inspection-only operation; this lease can never grant a queue.
    #[allow(clippy::too_many_arguments)]
    pub fn issue_inspection(
        &mut self,
        state: &State,
        policy: &LocalPolicy,
        requester: DeviceId,
        provider: &str,
        turn: &str,
        operation: [u8; 32],
        ttl: Duration,
        now: Instant,
        observation: Observation,
    ) -> Result<Challenge, Error> {
        self.issue_with_proof(
            state,
            policy,
            requester,
            Scope::Agent {
                provider,
                inspection: true,
            },
            turn,
            operation,
            ttl,
            now,
            observation,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn issue_with_proof(
        &mut self,
        state: &State,
        policy: &LocalPolicy,
        requester: DeviceId,
        scope: Scope,
        turn: &str,
        operation: [u8; 32],
        ttl: Duration,
        now: Instant,
        observation: Observation,
    ) -> Result<Challenge, Error> {
        if self.phase != Phase::Idle {
            return Err(reject("another console transaction is outstanding"));
        }
        validate_turn(turn)?;
        validate_ttl(ttl)?;
        let keys = authorization(state, policy, self.target, requester, scope)?;
        let (provider, inspection, terminal, device) = match scope {
            Scope::Agent {
                provider,
                inspection,
            } => (provider, inspection, false, false),
            Scope::Terminal => ("", false, true, false),
            Scope::Device => ("", false, false, true),
        };
        if !fresh(now, observation.at)
            || (observation.screen != Screen::Locked
                && !(device && observation.screen == Screen::Unlocked))
        {
            return Err(reject("a freshly observed locked console is required"));
        }
        let console = observation.console.ok_or_else(|| reject("no console"))?;
        validate_console(console)?;
        let input_generation = observation
            .input_generation
            .ok_or_else(|| reject("no local input observation"))?;
        if !observation.connected {
            return Err(reject("requester is disconnected"));
        }
        let challenge = Challenge {
            epoch: self.epoch,
            nonce: random()?,
            network: state.network_id.ok_or_else(|| reject("no network"))?.0,
            target: self.target,
            requester,
            console,
            turn: turn.to_owned(),
            operation,
            ttl_ms: ttl.as_millis() as u64,
        };
        self.last_stop = None;
        self.lease = Some(Lease {
            challenge: challenge.clone(),
            deadline: now + ttl,
            input_generation,
            keys,
            inspection,
            provider: provider.to_owned(),
            terminal,
            device,
            initial_screen: observation.screen,
            covered_unlocked: false,
            renewal: None,
            last_input: now,
            last_observation: observation.at,
            consumed: false,
            consumed_at: None,
            plugin_outcome: None,
            saw_unlock: false,
            action_taken: false,
            provider_queue: None,
            relock_after: None,
            stop: None,
        });
        self.phase = Phase::AwaitingSignature;
        Ok(challenge)
    }

    /// Select the provider queue scope while issuing, before anyone signs.
    /// The issuer must include this exact scope in the operation digest that
    /// the requester independently validates. It cannot widen an armed lease.
    pub fn bind_provider_queue(&mut self, provider: &str) -> Result<(), Error> {
        if self.phase != Phase::AwaitingSignature || crate::apps::app(provider).is_none() {
            return Err(reject("invalid provider queue scope or already armed"));
        }
        let lease = self.lease.as_mut().ok_or_else(|| reject("no challenge"))?;
        if lease.inspection || lease.terminal || lease.device {
            return Err(reject(
                "read-only inspection cannot authorize a provider queue",
            ));
        }
        if lease.provider != provider {
            return Err(reject("queue scope differs from the authorized provider"));
        }
        if lease.provider_queue.is_some() {
            return Err(reject("provider queue scope already bound"));
        }
        lease.provider_queue = Some(provider.to_owned());
        Ok(())
    }

    /// Verifies possession of an enrolled operation key and the live transport
    /// principal. A replay from a different connection/device/target cannot arm
    /// this gate. All signed fields must match the target's outstanding nonce.
    #[allow(clippy::too_many_arguments)]
    pub fn arm(
        &mut self,
        state: &State,
        policy: &LocalPolicy,
        requester: DeviceId,
        challenge: &Challenge,
        signature: &[u8],
        now: Instant,
        observation: Observation,
    ) -> Result<Directive, Error> {
        let lease = self.lease.as_ref().ok_or_else(|| reject("no challenge"))?;
        if self.phase != Phase::AwaitingSignature
            || lease.challenge != *challenge
            || requester != challenge.requester
        {
            return Err(reject("challenge or principal mismatch"));
        }
        if signature.len() > 80 {
            return Err(reject("signature too large"));
        }
        lease.keys.verify(challenge, signature)?;
        self.check(state, policy, now, observation, false)?;
        if observation.screen != self.lease.as_ref().unwrap().initial_screen {
            self.stop(StopReason::UnexpectedScreenState, now);
            return Err(reject("console was unlocked outside this transaction"));
        }
        self.phase = Phase::Armed;
        Ok(Directive::PrepareCoverAndWatchdog)
    }

    /// Called only for an authenticated OS plugin invocation after validating
    /// its peer credentials/code identity and its authorization context. The
    /// nonce is burned before the adapter may return Allow. &mut serializes
    /// competing invocations; the daemon must hold the gate under one lock.
    #[allow(clippy::too_many_arguments)]
    pub fn consume(
        &mut self,
        nonce: &[u8; 32],
        console: Console,
        state: &State,
        policy: &LocalPolicy,
        now: Instant,
        observation: Observation,
    ) -> Result<(), Error> {
        let lease = self.lease.as_ref().ok_or_else(|| reject("no challenge"))?;
        if self.phase != Phase::Armed
            || &lease.challenge.nonce != nonce
            || lease.challenge.console != console
        {
            return Err(reject("plugin invocation does not match armed challenge"));
        }
        self.check(state, policy, now, observation, true)?;
        if observation.screen != Screen::Locked {
            self.stop(StopReason::UnexpectedScreenState, now);
            return Err(reject("console is already unlocked"));
        }
        self.lease.as_mut().unwrap().consumed = true;
        self.lease.as_mut().unwrap().consumed_at = Some(now);
        self.phase = Phase::Authorizing;
        Ok(())
    }

    /// Correlates the exact plugin transaction, including a late receipt while
    /// relocking. Failure/timeout never silently returns the gate to Idle.
    pub fn plugin_finished(
        &mut self,
        nonce: &[u8; 32],
        outcome: PluginOutcome,
        now: Instant,
    ) -> Result<(), Error> {
        let lease = self
            .lease
            .as_mut()
            .ok_or_else(|| reject("no transaction"))?;
        if !lease.consumed
            || &lease.challenge.nonce != nonce
            || lease.plugin_outcome.is_some()
            || now < lease.last_observation
        {
            return Err(reject("unexpected or duplicate plugin terminal"));
        }
        lease.plugin_outcome = Some(outcome);
        // A prior locked observation cannot prove cleanup of this terminal.
        lease.relock_after = Some(now);
        if self.phase == Phase::Relocking {
            return Ok(());
        }
        if outcome == PluginOutcome::Denied {
            self.stop(StopReason::PluginDenied, now);
        } else if now >= lease.deadline {
            self.stop(StopReason::Expired, now);
        } else {
            self.phase = Phase::AwaitingUnlock;
        }
        Ok(())
    }

    /// Historical OS evidence from this transaction's private watchdog pipe.
    /// This can settle cleanup, but never authorizes an action or substitutes
    /// for a fresh live observation. The watchdog may see and relock an unlock
    /// between two owner polls.
    pub fn witnessed_unlock(
        &mut self,
        nonce: &[u8; 32],
        observed: Observation,
        now: Instant,
    ) -> Result<(), Error> {
        let lease = self
            .lease
            .as_mut()
            .ok_or_else(|| reject("no transaction"))?;
        if nonce != lease.challenge.nonce()
            || observed.console != Some(lease.challenge.console())
            || observed.input_generation != Some(lease.input_generation)
            || observed.screen != Screen::Unlocked
            || observed.at > now
            || !lease.consumed_at.is_some_and(|at| observed.at >= at)
        {
            return Err(reject("invalid watchdog unlock witness"));
        }
        lease.saw_unlock = true;
        lease.relock_after = Some(
            lease
                .relock_after
                .map_or(observed.at, |at| at.max(observed.at)),
        );
        Ok(())
    }

    /// Poll with fresh observations, and immediately before every OS mutation.
    /// The watchdog must independently lock on loss of this polling process.
    pub fn poll(
        &mut self,
        state: &State,
        policy: &LocalPolicy,
        now: Instant,
        observation: Observation,
    ) -> Directive {
        if self.phase == Phase::Idle {
            return Directive::None;
        }
        if self.phase != Phase::Relocking {
            let require_cover = matches!(
                self.phase,
                Phase::Authorizing | Phase::AwaitingUnlock | Phase::Active
            );
            let _ = self.check(state, policy, now, observation, require_cover);
        }
        if self.phase == Phase::Armed
            && self
                .lease
                .as_ref()
                .is_some_and(|l| l.device && l.initial_screen == Screen::Unlocked)
            && observation.screen == Screen::Unlocked
            && observation.cover_nonce == self.lease.as_ref().map(|l| l.challenge.nonce)
            && observation.watchdog_nonce == self.lease.as_ref().map(|l| l.challenge.nonce)
            && self.check(state, policy, now, observation, true).is_ok()
        {
            self.lease.as_mut().unwrap().covered_unlocked = true;
            self.phase = Phase::Active;
        }
        let lease = self.lease.as_mut().unwrap();
        let fresh_sample = fresh(now, observation.at)
            && observation.at >= lease.last_observation
            && observation.console == Some(lease.challenge.console);
        if fresh_sample {
            lease.last_observation = observation.at;
            if lease.consumed && observation.screen == Screen::Unlocked {
                lease.saw_unlock = true;
                if self.phase == Phase::Relocking {
                    lease.relock_after = Some(observation.at);
                }
            }
        }
        if self.phase == Phase::Relocking {
            // Until consume succeeds there is no OS authorization in flight.
            // An unsigned challenge must never force a locally unlocked user
            // back to the lock screen. Withdraw and remove only our own cover.
            if !lease.consumed && !lease.covered_unlocked {
                self.lease = None;
                self.phase = Phase::Idle;
                return Directive::ReleaseCover;
            }
            // A consumed Allow can land *after* cancellation or a locked
            // sample. Require its terminal, an observed unlock, then a newer
            // locked sample; otherwise keep trying to lock under the cover.
            let settled = lease.covered_unlocked
                || !lease.consumed
                || lease.plugin_outcome == Some(PluginOutcome::Denied)
                || (lease.plugin_outcome == Some(PluginOutcome::Allowed) && lease.saw_unlock);
            if settled
                && fresh_sample
                && observation.screen == Screen::Locked
                && lease
                    .relock_after
                    .is_some_and(|after| observation.at > after)
            {
                self.lease = None;
                self.phase = Phase::Idle;
                return Directive::ReleaseCover;
            }
            return Directive::LockAndKeepCovered(lease.stop.unwrap());
        }
        if self.phase == Phase::AwaitingUnlock
            && observation.screen == Screen::Unlocked
            && lease
                .relock_after
                .is_some_and(|terminal| observation.at > terminal)
        {
            self.phase = Phase::Active;
        } else if (self.phase == Phase::Active && observation.screen != Screen::Unlocked)
            || (matches!(self.phase, Phase::AwaitingSignature | Phase::Armed)
                && observation.screen != lease.initial_screen)
        {
            let relock_required = lease.consumed || lease.covered_unlocked;
            self.stop(StopReason::UnexpectedScreenState, now);
            if !relock_required {
                self.lease = None;
                self.phase = Phase::Idle;
                return Directive::ReleaseCover;
            }
            return Directive::LockAndKeepCovered(StopReason::UnexpectedScreenState);
        }
        match self.phase {
            Phase::Active if self.lease.as_ref().unwrap().action_taken => Directive::MonitorAction,
            Phase::Active => Directive::ReadyForAction,
            Phase::Armed => Directive::PrepareCoverAndWatchdog,
            Phase::Authorizing | Phase::AwaitingUnlock => Directive::WaitForUnlock,
            _ => Directive::None,
        }
    }

    /// Single-use admission for the bound operation. A transport retry must
    /// use the operation ledger, not execute again under the same grant.
    #[allow(clippy::too_many_arguments)]
    pub fn take_action(
        &mut self,
        requester: DeviceId,
        turn: &str,
        operation: &[u8; 32],
        state: &State,
        policy: &LocalPolicy,
        now: Instant,
        observation: Observation,
    ) -> Result<(), Error> {
        let lease = self
            .lease
            .as_ref()
            .ok_or_else(|| reject("no transaction"))?;
        if lease.action_taken
            || lease.challenge.requester != requester
            || lease.challenge.turn != turn
            || &lease.challenge.operation != operation
        {
            return Err(reject("operation mismatch or already executed"));
        }
        if self.poll(state, policy, now, observation) != Directive::ReadyForAction {
            return Err(reject("console operation is not admitted"));
        }
        self.lease.as_mut().unwrap().action_taken = true;
        Ok(())
    }

    /// Admit another independently ledgered operation inside a signed provider
    /// scope. The transport peer, live policy, cover and original deadline still
    /// apply. This never renews the lease or admits another provider.
    pub fn take_queued_action(
        &mut self,
        requester: DeviceId,
        provider: &str,
        state: &State,
        policy: &LocalPolicy,
        now: Instant,
        observation: Observation,
    ) -> Result<(), Error> {
        let lease = self
            .lease
            .as_ref()
            .ok_or_else(|| reject("no transaction"))?;
        if lease.challenge.requester != requester
            || !lease.action_taken
            || (!lease.device && lease.provider_queue.as_deref() != Some(provider))
        {
            return Err(reject("requester or provider queue scope mismatch"));
        }
        if self.poll(state, policy, now, observation) != Directive::MonitorAction {
            return Err(reject("provider queue is no longer active"));
        }
        authorization(
            state,
            policy,
            self.target,
            requester,
            Scope::Agent {
                provider,
                inspection: false,
            },
        )?;
        Ok(())
    }

    /// A terminal session's next window: a fresh nonce bound to the same
    /// requester, console, turn and operation, signed like the first. Only
    /// while Active; it replaces a renewal not yet signed and extends nothing
    /// by itself.
    pub fn renewal(&mut self, requester: DeviceId, now: Instant) -> Result<Challenge, Error> {
        let lease = self
            .lease
            .as_mut()
            .ok_or_else(|| reject("no transaction"))?;
        if !lease.terminal
            || self.phase != Phase::Active
            || lease.challenge.requester != requester
            || lease.stop.is_some()
            || now >= lease.deadline
        {
            return Err(reject("no active terminal session for this requester"));
        }
        let challenge = Challenge {
            nonce: random()?,
            ttl_ms: MAX_TTL.as_millis() as u64,
            ..lease.challenge.clone()
        };
        lease.renewal = Some((challenge.clone(), now));
        Ok(challenge)
    }

    /// Extends a terminal session to the signed renewal's own deadline (its
    /// issue time plus its TTL), never further, after the same live checks
    /// as an action: membership, grants, console, local input, cover and
    /// watchdog. Returns the new deadline.
    #[allow(clippy::too_many_arguments)]
    pub fn renew(
        &mut self,
        state: &State,
        policy: &LocalPolicy,
        requester: DeviceId,
        challenge: &Challenge,
        signature: &[u8],
        now: Instant,
        observation: Observation,
    ) -> Result<Instant, Error> {
        let lease = self
            .lease
            .as_ref()
            .ok_or_else(|| reject("no transaction"))?;
        let Some((pending, issued)) = lease.renewal.clone() else {
            return Err(reject("no renewal outstanding"));
        };
        if self.phase != Phase::Active || pending != *challenge || requester != challenge.requester
        {
            return Err(reject("renewal or principal mismatch"));
        }
        if signature.len() > 80 {
            return Err(reject("signature too large"));
        }
        lease.keys.verify(challenge, signature)?;
        self.check(state, policy, now, observation, true)?;
        if observation.screen != Screen::Unlocked {
            self.stop(StopReason::UnexpectedScreenState, now);
            return Err(reject("console is no longer unlocked"));
        }
        let lease = self.lease.as_mut().unwrap();
        lease.renewal = None;
        lease.deadline = lease
            .deadline
            .max(issued + Duration::from_millis(challenge.ttl_ms));
        Ok(lease.deadline)
    }

    /// Terminal input from the session's requester: it keeps the session
    /// from going idle. Nothing else does.
    pub fn note_input(&mut self, requester: DeviceId, now: Instant) {
        if let Some(lease) = self.lease.as_mut()
            && lease.terminal
            && lease.challenge.requester == requester
            && lease.stop.is_none()
        {
            lease.last_input = lease.last_input.max(now);
        }
    }

    /// Called on completion, cancellation, socket loss, or adapter failure.
    /// Cleanup retains the first cause and cannot extend the deadline.
    pub fn stop(&mut self, reason: StopReason, now: Instant) -> Directive {
        let Some(lease) = self.lease.as_mut() else {
            return Directive::None;
        };
        if lease.stop.is_none() {
            lease.stop = Some(reason);
            lease.relock_after = Some(now);
            self.last_stop = Some(reason);
        }
        self.phase = Phase::Relocking;
        Directive::LockAndKeepCovered(lease.stop.unwrap())
    }

    fn check(
        &mut self,
        state: &State,
        policy: &LocalPolicy,
        now: Instant,
        o: Observation,
        require_cover: bool,
    ) -> Result<(), Error> {
        let lease = self.lease.as_mut().unwrap();
        let scope = if lease.device {
            Scope::Device
        } else if lease.terminal {
            Scope::Terminal
        } else {
            Scope::Agent {
                provider: &lease.provider,
                inspection: lease.inspection,
            }
        };
        let reason = if now >= lease.deadline {
            Some(StopReason::Expired)
        } else if lease.terminal && now >= lease.last_input + TERMINAL_IDLE {
            Some(StopReason::Idle)
        } else if state.network_id.map(|id| id.0) != Some(lease.challenge.network)
            || authorization(state, policy, self.target, lease.challenge.requester, scope)
                .map_or(true, |keys| keys != lease.keys)
        {
            Some(StopReason::AuthorizationChanged)
        } else if !fresh(now, o.at)
            || o.at < lease.last_observation
            || o.screen == Screen::Unknown
            || o.input_generation.is_none()
        {
            Some(StopReason::ObservationLost)
        } else if o.console != Some(lease.challenge.console) {
            Some(StopReason::ConsoleChanged)
        } else if o.input_generation != Some(lease.input_generation) {
            Some(StopReason::LocalInput)
        } else if !o.connected {
            Some(StopReason::Disconnected)
        } else if require_cover && o.cover_nonce != Some(lease.challenge.nonce) {
            Some(StopReason::CoverLost)
        } else if require_cover && o.watchdog_nonce != Some(lease.challenge.nonce) {
            Some(StopReason::WatchdogLost)
        } else {
            None
        };
        if let Some(reason) = reason {
            self.stop(reason, now);
            Err(reject("console lease no longer valid"))
        } else {
            lease.last_observation = o.at;
            Ok(())
        }
    }
}

/// Phone operations use the existing paired hardware identity without a
/// biometric prompt per message. Presence remains accepted for older clients;
/// desktop writes and all administrative RPCs still require Presence. A typed
/// read-only inspection may use any enrolled device key, without queue authority.
#[derive(Clone, PartialEq, Eq)]
struct OperationKeys {
    presence: Option<crate::PublicKey>,
    phone: Option<crate::PublicKey>,
}

impl OperationKeys {
    fn verify(&self, challenge: &Challenge, signature: &[u8]) -> Result<(), Error> {
        let encoded = challenge.encode();
        if self
            .presence
            .iter()
            .chain(self.phone.iter())
            .any(|key| verify_domain(key, DOMAIN, &encoded, signature).is_ok())
        {
            Ok(())
        } else {
            Err(reject("operation signature does not match an enrolled key"))
        }
    }
}

/// What a lease authorizes beyond `locked.control`.
#[derive(Clone, Copy)]
enum Scope<'a> {
    /// Agents control for one provider's session; `inspection` reads only.
    Agent {
        provider: &'a str,
        inspection: bool,
    },
    /// Terminal control: a session kept unlocked for one terminal.
    Terminal,
    Device,
}

fn authorization(
    state: &State,
    policy: &LocalPolicy,
    target: DeviceId,
    requester: DeviceId,
    scope: Scope,
) -> Result<OperationKeys, Error> {
    if target == requester || state.active(&target).is_none() {
        return Err(reject("target is not an active remote member"));
    }
    let locked = TargetState { locked: true };
    let (control, inspection) = match scope {
        Scope::Agent {
            provider,
            inspection,
        } => (
            Some(Permission::Agent {
                provider: Some(provider.to_owned()),
                control: true,
            }),
            inspection,
        ),
        Scope::Terminal => (Some(Permission::Terminal { control: true }), false),
        Scope::Device => (None, false),
    };
    if authorize(
        state,
        &requester,
        &Permission::LockedControl,
        policy,
        &locked,
    ) != Decision::NeedsPresence
        || control.as_ref().is_some_and(|control| {
            authorize(state, &requester, control, policy, &locked) != Decision::Allow
        })
    {
        return Err(reject(match scope {
            Scope::Agent { .. } => "locked.control and Agents for this provider are required",
            Scope::Terminal => "locked.control and Terminal control are required",
            Scope::Device => "locked.control is required",
        }));
    }
    let record = &state
        .active(&requester)
        .ok_or_else(|| reject("inactive requester"))?
        .record;
    let keys = OperationKeys {
        presence: record.presence_key.clone(),
        phone: (inspection || record.kind == crate::DeviceKind::Phone)
            .then(|| record.public_key.clone()),
    };
    if keys.presence.is_none() && keys.phone.is_none() {
        return Err(reject(
            "requesting desktop has no enrolled Presence key; changing locked.control alone cannot enable desktop Locked Use",
        ));
    }
    Ok(keys)
}

fn random() -> Result<[u8; 32], Error> {
    let mut value = [0; 32];
    rand::rngs::OsRng
        .try_fill_bytes(&mut value)
        .map_err(|_| reject("OS random source unavailable"))?;
    Ok(value)
}

fn fresh(now: Instant, at: Instant) -> bool {
    now.checked_duration_since(at)
        .is_some_and(|age| age <= MAX_OBSERVATION_AGE)
}

fn validate_turn(turn: &str) -> Result<(), Error> {
    if turn.is_empty()
        || turn.len() > MAX_TURN_BYTES
        || !turn
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
    {
        return Err(reject("invalid turn id"));
    }
    Ok(())
}

fn validate_console(console: Console) -> Result<(), Error> {
    if console.uid == 0 || console.audit_session == 0 || console.audit_session == u32::MAX {
        return Err(reject("invalid console identity"));
    }
    Ok(())
}

fn validate_ttl(ttl: Duration) -> Result<(), Error> {
    if !(MIN_TTL..=MAX_TTL).contains(&ttl) || ttl.subsec_nanos() % 1_000_000 != 0 {
        return Err(reject("TTL must be 5000..15000 whole milliseconds"));
    }
    Ok(())
}

fn reject(message: &str) -> Error {
    Error::Rejected(format!("locked use: {message}"))
}
