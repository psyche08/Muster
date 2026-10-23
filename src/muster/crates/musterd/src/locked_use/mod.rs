//! One owner for the console lease, surviving cancellation of its RPC caller.
//! Plugin callbacks enter a separate authenticated socket, never normal RPC.
//!
//! This file is the owner's API; `actor` is the task that holds one lease
//! from challenge to relock.

use crate::{Daemon, Error};
use muster_core::DeviceId;
use muster_core::locked_use::{
    Challenge, Directive, Gate, MAX_TTL, Observation, Phase, PluginOutcome, StopReason,
};
use muster_platform::locked_use::{ConsoleGuard, LockedUseOs, UnlockCheck};
use muster_proto::v1::{self, request::Req, response::Resp};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{OwnedRwLockWriteGuard, RwLock, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

mod actor;
mod receipt;
use receipt::ReceiptWait;
#[cfg(test)]
mod tests;

use actor::*;

type Reply = oneshot::Sender<Result<Resp, Error>>;
#[derive(Clone)]
struct Current {
    receipt: Option<ReceiptWait>,
    tx: mpsc::Sender<Command>,
    scope: Option<(DeviceId, String)>,
    accepting: Arc<std::sync::atomic::AtomicBool>,
    terminal: Option<TerminalLease>,
    manual: Option<(DeviceId, Vec<u8>, Arc<std::sync::atomic::AtomicBool>)>,
}

/// A terminal session the console is kept unlocked for: which requester and
/// terminal, whether it is open now, and a barrier its terminal I/O holds
/// so the cover outlasts every read and write.
#[derive(Clone)]
pub struct TerminalLease {
    requester: DeviceId,
    session: String,
    live: Arc<std::sync::atomic::AtomicBool>,
    io: Arc<RwLock<()>>,
}

impl TerminalLease {
    pub fn live(&self) -> bool {
        self.live.load(std::sync::atomic::Ordering::SeqCst)
    }
    /// Held for one terminal read or write, or a whole attach stream. The
    /// owner releases the cover only once every holder has let go.
    pub fn io(&self) -> Option<tokio::sync::OwnedRwLockReadGuard<()>> {
        self.io
            .clone()
            .try_read_owned()
            .ok()
            .filter(|_| self.live())
    }
}
impl std::ops::Deref for Current {
    type Target = mpsc::Sender<Command>;
    fn deref(&self) -> &Self::Target {
        &self.tx
    }
}
struct Queued {
    request: Req,
    reply: Reply,
    cancel: CancellationToken,
}
enum Command {
    Enqueue(Queued),
    Execute {
        peer: DeviceId,
        challenge: Box<Challenge>,
        signature: Vec<u8>,
        reply: Reply,
        cancel: CancellationToken,
    },
    Cancel {
        peer: DeviceId,
        nonce: [u8; 32],
        reason: Option<muster_core::locked_use::CancelReason>,
    },
    Consume {
        uid: u32,
        reply: oneshot::Sender<Option<[u8; 32]>>,
    },
    Terminal {
        nonce: [u8; 32],
        allowed: bool,
    },
    /// A terminal session's next window, to be signed.
    Renew {
        peer: DeviceId,
        nonce: [u8; 32],
        reply: Reply,
    },
    /// The signed next window.
    Extend {
        peer: DeviceId,
        challenge: Box<Challenge>,
        signature: Vec<u8>,
        reply: Reply,
    },
    /// Terminal input in the session: it is not idle.
    Input {
        peer: DeviceId,
    },
    Shutdown,
}

pub struct Owner {
    pub(crate) writes: Arc<RwLock<()>>,
    current: std::sync::Mutex<Option<Current>>,
    os: Option<Arc<dyn LockedUseOs>>,
}

impl Owner {
    pub fn new(os: Option<Arc<dyn LockedUseOs>>) -> Self {
        Self {
            writes: Arc::new(RwLock::new(())),
            current: Default::default(),
            os,
        }
    }

    /// Display projection only. This nonce is not authority: cancellation
    /// always checks the authenticated transport peer against the live owner.
    pub fn manual_status(&self) -> (String, Vec<u8>, bool) {
        self.current
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|c| c.manual.as_ref())
            .map(|(peer, nonce, live)| {
                (
                    peer.to_hex(),
                    nonce.clone(),
                    live.load(std::sync::atomic::Ordering::SeqCst),
                )
            })
            .unwrap_or_default()
    }

    pub async fn prepare(
        self: &Arc<Self>,
        daemon: Arc<Daemon>,
        peer: DeviceId,
        operation: v1::LockedUseOperation,
    ) -> Result<Resp, Error> {
        let os = self.os.clone().ok_or_else(|| {
            Error::Unavailable("Locked Use is unsupported on this platform".into())
        })?;
        let explicit_device = matches!(
            operation.operation,
            Some(v1::locked_use_operation::Operation::Device(_))
        );
        if explicit_device && os.observe().screen == muster_core::locked_use::Screen::Unlocked {
            // Covering an unlocked console uses no Accessibility API or plugin.
            // Gate still binds and verifies the console observation and proof.
            os.update_ready().map_err(Error::Unavailable)?;
        } else {
            os.available().map_err(Error::Unavailable)?;
        }
        let write = self
            .writes
            .clone()
            .try_write_owned()
            .map_err(|_| Error::Conflict("the console is busy".into()))?;
        let turn = muster_proto::locked_use::turn(&operation).ok_or_else(|| {
            Error::Invalid("Locked Use requires a bounded operation and op_id".into())
        })?;
        let digest = muster_proto::locked_use::digest(&operation);
        let request = muster_proto::locked_use::request(&operation);
        let manual = matches!(
            operation.operation,
            Some(v1::locked_use_operation::Operation::Device(_))
        );
        if manual && !operation.provider_queue.is_empty() {
            return Err(Error::Invalid(
                "device lease cannot name a provider queue".into(),
            ));
        }
        let terminal = match &operation.operation {
            Some(v1::locked_use_operation::Operation::Device(_)) => Some(TerminalLease {
                requester: peer,
                session: String::new(),
                live: Default::default(),
                io: Arc::new(RwLock::new(())),
            }),
            Some(v1::locked_use_operation::Operation::Terminal(lease)) => {
                // Only a terminal whose screen needs an unlocked console.
                if !crate::native_terminals::resolve(&daemon, &lease.session)
                    .is_ok_and(|(t, _)| t.source() == "ghostty")
                {
                    return Err(Error::Invalid(
                        "this Mac's terminals need no Locked Use session".into(),
                    ));
                }
                if lease.session.is_empty() || !operation.provider_queue.is_empty() {
                    return Err(Error::Invalid(
                        "a terminal session names one terminal".into(),
                    ));
                }
                Some(TerminalLease {
                    requester: peer,
                    session: lease.session.clone(),
                    live: Default::default(),
                    io: Arc::new(RwLock::new(())),
                })
            }
            _ => None,
        };
        let Some(request) = request else {
            return self
                .prepare_terminal(daemon, os, write, peer, turn, digest, terminal, manual)
                .await;
        };
        let terminal_creation = matches!(&request, Req::TerminalCreate(_));
        let native_provider = if let Req::TerminalCreate(r) = &request {
            crate::handler::validate_locked_terminal_creation(&daemon, r)?;
            if !operation.provider_queue.is_empty() {
                return Err(Error::Invalid(
                    "terminal creation cannot authorize a provider queue".into(),
                ));
            }
            String::new()
        } else {
            crate::handler::require_native_operation(&daemon, &request)?
        };
        let provider = if operation.provider_queue.is_empty() {
            None
        } else {
            let actual = crate::handler::native_provider(&daemon, &request)?;
            if operation.provider_queue != actual {
                return Err(Error::Invalid(
                    "provider queue does not match operation".into(),
                ));
            }
            Some(actual)
        };
        let mut gate = Gate::new(daemon.device_id())?;
        let observed = observation(&*os, None, true);
        let issued_at = Instant::now();
        let issue = if matches!(request, Req::AgentInspect(_)) {
            if !operation.provider_queue.is_empty() {
                return Err(Error::Invalid(
                    "inspection cannot authorize a provider queue".into(),
                ));
            }
            Gate::issue_inspection
        } else {
            Gate::issue
        };
        let c = if terminal_creation {
            gate.issue_terminal(
                daemon.roster().read().unwrap().state(),
                &daemon.policy().read().unwrap(),
                peer,
                turn,
                digest,
                MAX_TTL,
                issued_at,
                observed,
            )?
        } else {
            issue(
                &mut gate,
                daemon.roster().read().unwrap().state(),
                &daemon.policy().read().unwrap(),
                peer,
                &native_provider,
                turn,
                digest,
                MAX_TTL,
                issued_at,
                observed,
            )?
        };
        if let Some(provider) = &provider {
            gate.bind_provider_queue(provider)?;
        }
        log::info!("Locked Use: challenge issued");
        let runtime = owner_runtime()?;
        let (tx, rx) = mpsc::channel(16);
        *self.current.lock().unwrap() = Some(Current {
            receipt: ReceiptWait::new(&daemon, peer, &request),
            tx,
            scope: provider.clone().map(|p| (peer, p)),
            accepting: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            terminal: None,
            manual: None,
        });
        self.spawn_owner(
            runtime,
            &daemon,
            os,
            gate,
            issued_at,
            c,
            peer,
            Some(request),
            digest,
            rx,
            write,
            provider,
            None,
        )
    }

    /// A terminal session: the first window unlocks the console like an
    /// operation, then the requester's signed renewals keep it open.
    #[allow(clippy::too_many_arguments)]
    async fn prepare_terminal(
        self: &Arc<Self>,
        daemon: Arc<Daemon>,
        os: Arc<dyn LockedUseOs>,
        write: OwnedRwLockWriteGuard<()>,
        peer: DeviceId,
        turn: &str,
        digest: [u8; 32],
        terminal: Option<TerminalLease>,
        manual: bool,
    ) -> Result<Resp, Error> {
        if terminal.is_none() && !manual {
            return Err(Error::Invalid("no operation".into()));
        }
        let mut gate = Gate::new(daemon.device_id())?;
        let observed = observation(&*os, None, true);
        let issued_at = Instant::now();
        let issue = if manual {
            Gate::issue_device
        } else {
            Gate::issue_terminal
        };
        let c = issue(
            &mut gate,
            daemon.roster().read().unwrap().state(),
            &daemon.policy().read().unwrap(),
            peer,
            turn,
            digest,
            MAX_TTL,
            issued_at,
            observed,
        )?;
        log::info!("Locked Use: terminal session challenge issued");
        let runtime = owner_runtime()?;
        let (tx, rx) = mpsc::channel(16);
        *self.current.lock().unwrap() = Some(Current {
            receipt: None,
            tx,
            scope: manual.then(|| (peer, String::new())),
            accepting: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            terminal: terminal.clone(),
            manual: manual.then(|| (peer, c.nonce().to_vec(), Arc::default())),
        });
        self.spawn_owner(
            runtime, &daemon, os, gate, issued_at, c, peer, None, digest, rx, write, None, terminal,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_owner(
        self: &Arc<Self>,
        runtime: tokio::runtime::Runtime,
        daemon: &Arc<Daemon>,
        os: Arc<dyn LockedUseOs>,
        gate: Gate,
        issued_at: Instant,
        c: Challenge,
        peer: DeviceId,
        request: Option<Req>,
        digest: [u8; 32],
        rx: mpsc::Receiver<Command>,
        write: OwnedRwLockWriteGuard<()>,
        provider: Option<String>,
        terminal: Option<TerminalLease>,
    ) -> Result<Resp, Error> {
        let owner = self.clone();
        let weak = Arc::downgrade(daemon);
        let challenge = c.clone();
        tokio::task::spawn_blocking(move || {
            runtime.block_on(async move {
                run(
                    owner.clone(),
                    weak,
                    os,
                    gate,
                    issued_at,
                    c,
                    peer,
                    request,
                    digest,
                    rx,
                    write,
                    muster_agent::control_guard::WorkQueue::default(),
                    provider,
                    terminal,
                )
                .await;
            })
        });
        Ok(Resp::LockedUseChallenge(v1::LockedUseChallenge {
            challenge: challenge.encode(),
        }))
    }

    /// The caller's open terminal session for `session`, if there is one.
    pub fn manual_terminal_lease(&self, peer: DeviceId) -> Option<TerminalLease> {
        let current = self.current.lock().unwrap();
        let active = current.as_ref()?;
        active.manual.as_ref()?;
        active
            .terminal
            .clone()
            .filter(|t| t.requester == peer && t.live())
    }

    pub fn terminal_lease(&self, peer: DeviceId, session: &str) -> Option<TerminalLease> {
        let current = self.current.lock().unwrap();
        let active = current.as_ref()?;
        active.terminal.clone().filter(|t| {
            t.requester == peer && (active.manual.is_some() || t.session == session) && t.live()
        })
    }

    /// Terminal input in the caller's session keeps it from going idle.
    pub fn terminal_input(&self, peer: DeviceId) {
        if let Some(current) = self.current.lock().unwrap().as_ref() {
            let _ = current.try_send(Command::Input { peer });
        }
    }

    async fn ask(&self, command: impl FnOnce(Reply) -> Command) -> Result<Resp, Error> {
        let tx = self
            .current
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| Error::Conflict("no Locked Use session".into()))?;
        let (reply, result) = oneshot::channel();
        tx.send(command(reply))
            .await
            .map_err(|_| Error::Conflict("the Locked Use session has ended".into()))?;
        result
            .await
            .unwrap_or_else(|_| Err(Error::Conflict("the Locked Use session has ended".into())))
    }

    pub async fn renew(&self, peer: DeviceId, nonce: &[u8]) -> Result<Resp, Error> {
        let nonce = nonce
            .try_into()
            .map_err(|_| Error::Invalid("nonce must be 32 bytes".into()))?;
        self.ask(|reply| Command::Renew { peer, nonce, reply })
            .await
    }

    pub async fn extend(
        &self,
        peer: DeviceId,
        r: v1::LockedUseExtendRequest,
    ) -> Result<Resp, Error> {
        let challenge = Box::new(Challenge::decode(&r.challenge)?);
        self.ask(|reply| Command::Extend {
            peer,
            challenge,
            signature: r.signature,
            reply,
        })
        .await
    }

    /// `muster debug unlock-probe`: the credential-free lock-screen Return
    /// with nothing armed, so the plugin cannot grant access.
    /// It shows whether Session Return reaches the authorization chain.
    pub async fn probe_unlock(&self) -> Result<String, Error> {
        let os = self.os.clone().ok_or_else(|| {
            Error::Unavailable("Locked Use is unsupported on this platform".into())
        })?;
        let _write = self
            .writes
            .clone()
            .try_write_owned()
            .map_err(|_| Error::Conflict("the console is busy".into()))?;
        let before = os.observe();
        if before.screen != muster_core::locked_use::Screen::Locked
            || before.console.is_none()
            || before.input_generation.is_none()
        {
            return Err(Error::Conflict(
                "probe requires a known locked console".into(),
            ));
        }
        let started = Instant::now();
        let check = unlock_check(
            os.clone(),
            before.console,
            before.input_generation,
            started + Duration::from_secs(4),
            CancellationToken::new(),
        );
        tokio::time::timeout(Duration::from_secs(4), os.request_unlock(check))
            .await
            .map_err(|_| Error::Unavailable("lock-screen probe timed out".into()))?
            .map_err(Error::Unavailable)?;
        tokio::time::sleep(Duration::from_millis(250)).await;
        let after = os.observe();
        Ok(format!(
            "Session Return submitted after {} ms; screen={:?} input_before={:?} input_after={:?}; submission is not unlock evidence",
            started.elapsed().as_millis(),
            after.screen,
            before.input_generation,
            after.input_generation
        ))
    }

    /// The local updater holds this stream across the platform stop callback.
    /// A queued writer blocks new console writes while existing work drains.
    pub async fn update_lease(&self, events: muster_rpc::EventSink) -> Result<Resp, Error> {
        let _write = self
            .update_permit(events.cancelled(), MAX_TTL + Duration::from_secs(3))
            .await?;
        log::info!("Desktop update: console operations drained; replacement lease held");
        events
            .send(v1::event::Ev::DesktopUpdateReady(v1::Ack {}))
            .await
            .map_err(|e| Error::Unavailable(e.to_string()))?;
        events.cancelled().cancelled().await;
        log::info!("Desktop update: replacement lease released");
        Ok(Resp::Ack(v1::Ack {}))
    }

    async fn update_permit(
        &self,
        cancel: &CancellationToken,
        timeout: Duration,
    ) -> Result<OwnedRwLockWriteGuard<()>, Error> {
        let write = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(Error::Conflict("Update cancelled before console drain".into())),
            result = tokio::time::timeout(timeout, self.writes.clone().write_owned()) =>
                result.map_err(|_| Error::Conflict("Mac protection has not finished; update was not started. Recover the local console before installing".into()))?,
        };
        if let Some(os) = &self.os {
            os.update_ready().map_err(Error::Unavailable)?;
        }
        Ok(write)
    }

    pub fn shutdown(&self) {
        if let Some(tx) = self.current.lock().unwrap().as_ref() {
            let _ = tx.try_send(Command::Shutdown);
        }
    }

    /// Route before ordinary console-write admission, even during temporary
    /// unlock. Only the same authenticated requester and signed provider enter.
    /// Enqueue and the owner's empty-queue close share `current` as a barrier.
    pub async fn enqueue(
        &self,
        daemon: &Arc<Daemon>,
        peer: DeviceId,
        request: Req,
        cancel: CancellationToken,
    ) -> Option<Result<Resp, Error>> {
        let receipt = ReceiptWait::new(daemon, peer, &request);
        let receipt_cancel = cancel.clone();
        let closing_same_requester = self.current.lock().unwrap().as_ref().is_some_and(|active| {
            active
                .scope
                .as_ref()
                .is_some_and(|(requester, _)| *requester == peer)
                && !active.accepting.load(std::sync::atomic::Ordering::SeqCst)
        });
        if closing_same_requester {
            // Nothing has been admitted yet. Let a normally finishing owner
            // drain, then use ordinary admission (and fresh Presence if locked).
            // Never retry an accepted operation or release a quarantined owner.
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Some(Err(Error::Conflict("Operation cancelled before admission".into()))),
                result = tokio::time::timeout(Duration::from_secs(2), self.writes.clone().read_owned()) => {
                    drop(result);
                }
            }
        }
        let result = {
            let current = self.current.lock().unwrap();
            let active = current.as_ref()?;
            let (reply, result) = oneshot::channel();
            let admitted = (|| {
                let Some((requester, provider)) = &active.scope else {
                    return Err(Error::Conflict(
                        "this Locked Use authorizes one operation only".into(),
                    ));
                };
                if *requester != peer {
                    log::warn!(
                        "Locked Use enqueue rejected: foreign requester={peer} owner={requester}; operation not started"
                    );
                    return Err(Error::Conflict(
                        "Another requester owns the Locked Use console; operation not started. End Locked Use on its owning device, then enable it from this device".into(),
                    ));
                }
                if !active.accepting.load(std::sync::atomic::Ordering::SeqCst) {
                    log::warn!(
                        "Locked Use enqueue rejected: owner is relocking; operation not started"
                    );
                    return Err(Error::Conflict(
                        "Previous Locked Use is still relocking; operation not started. If this persists, log out and log back in on the Mac to recover the protected console".into(),
                    ));
                }
                crate::handler::require_native_operation(daemon, &request)?;
                if active.manual.is_none()
                    && crate::handler::native_provider(daemon, &request)? != *provider
                {
                    return Err(Error::Conflict(
                        "another provider owns the Locked Use console".into(),
                    ));
                }
                let operation = muster_proto::locked_use::operation(&request)
                    .filter(|o| muster_proto::locked_use::turn(o).is_some())
                    .ok_or_else(|| {
                        Error::Invalid("queued operation needs a bounded op_id and payload".into())
                    })?;
                drop(operation);
                log::info!(
                    "Locked Use reuse admission: requester={peer} manual={} provider={provider}",
                    active.manual.is_some()
                );
                active
                    .try_send(Command::Enqueue(Queued {
                        request,
                        reply,
                        cancel,
                    }))
                    .map_err(|_| {
                        Error::Conflict(
                            "Locked Use queue is full or closing; operation not started".into(),
                        )
                    })
            })();
            if let Err(error) = admitted {
                return Some(Err(error));
            }
            result
        };
        let result = result.await.unwrap_or_else(|_| {
            Ok(unknown_result(
                "Locked Use owner stopped before the request receipt; check before retrying".into(),
            ))
        });
        Some(match receipt {
            Some(receipt) => receipt.settle(result, receipt_cancel).await,
            None => result,
        })
    }

    pub async fn execute(
        &self,
        peer: DeviceId,
        r: v1::LockedUseExecuteRequest,
        cancel: CancellationToken,
    ) -> Result<Resp, Error> {
        let challenge = Challenge::decode(&r.challenge)?;
        let tx = self
            .current
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| Error::Conflict("no outstanding Locked Use challenge".into()))?;
        let receipt = tx
            .receipt
            .clone()
            .filter(|receipt| receipt.peer_matches(peer));
        let receipt_cancel = cancel.clone();
        let (reply, result) = oneshot::channel();
        tx.send(Command::Execute {
            peer,
            challenge: Box::new(challenge),
            signature: r.signature,
            reply,
            cancel,
        })
        .await
        .map_err(|_| Error::Unavailable("console owner stopped".into()))?;
        let result = result.await.unwrap_or_else(|_| {
            Ok(unknown_result(
                "Locked Use owner stopped before the request receipt; check before retrying".into(),
            ))
        });
        match receipt {
            Some(receipt) => receipt.settle(result, receipt_cancel).await,
            None => result,
        }
    }

    pub async fn cancel(
        &self,
        peer: DeviceId,
        bytes: &[u8],
        reason: Option<muster_core::locked_use::CancelReason>,
    ) -> Result<Resp, Error> {
        let nonce = bytes
            .try_into()
            .map_err(|_| Error::Invalid("nonce must be 32 bytes".into()))?;
        let tx = self.current.lock().unwrap().clone();
        if let Some(tx) = tx {
            let _ = tx
                .send(Command::Cancel {
                    peer,
                    nonce,
                    reason,
                })
                .await;
        }
        Ok(Resp::Ack(v1::Ack {}))
    }

    pub async fn listen(self: &Arc<Self>, path: std::path::PathBuf) -> anyhow::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let Some(os) = self.os.clone() else {
            return Ok(());
        };
        if path.exists() {
            // Refuse a second daemon; never steal a live plugin endpoint.
            anyhow::ensure!(
                std::os::unix::net::UnixStream::connect(&path).is_err(),
                "plugin socket is already served"
            );
            std::fs::remove_file(&path)?;
        }
        let listener = tokio::net::UnixListener::bind(path)?;
        std::fs::set_permissions(
            listener.local_addr()?.as_pathname().unwrap(),
            std::fs::Permissions::from_mode(0o600),
        )?;
        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let Some(owner) = weak.upgrade() else { break };
                let Ok(raw) = stream.into_std() else { continue };
                if !os.plugin_peer(&raw) {
                    continue;
                }
                let Ok(mut stream) = tokio::net::UnixStream::from_std(raw) else {
                    continue;
                };
                let tx = owner.current.lock().unwrap().clone();
                tokio::spawn(async move {
                    // Fixed 9-byte request: "MLU1", verb, authorization UID (BE).
                    let Some(tx) = tx else { return };
                    let mut hello = [0; 9];
                    if !matches!(
                        tokio::time::timeout(Duration::from_secs(1), stream.read_exact(&mut hello))
                            .await,
                        Ok(Ok(_))
                    ) {
                        return;
                    }
                    if &hello[..5] != b"MLU1C" {
                        return;
                    }
                    let uid = u32::from_be_bytes(hello[5..9].try_into().unwrap());
                    let (reply, result) = oneshot::channel();
                    if tx.send(Command::Consume { uid, reply }).await.is_err() {
                        return;
                    }
                    let Ok(Ok(Some(nonce))) =
                        tokio::time::timeout(Duration::from_secs(1), result).await
                    else {
                        return;
                    };
                    if stream.write_all(&nonce).await.is_err() {
                        return;
                    }
                    // The connection stays held by this exact mechanism until
                    // its terminal. EOF/timeout is ambiguous, never a denial.
                    let mut terminal = [0; 33];
                    if !matches!(
                        tokio::time::timeout(
                            Duration::from_secs(30),
                            stream.read_exact(&mut terminal)
                        )
                        .await,
                        Ok(Ok(_))
                    ) {
                        return;
                    }
                    if terminal[1..] == nonce && matches!(terminal[0], b'A' | b'D') {
                        let _ = tx
                            .send(Command::Terminal {
                                nonce,
                                allowed: terminal[0] == b'A',
                            })
                            .await;
                    }
                });
            }
        });
        Ok(())
    }
}

fn observation(
    os: &dyn LockedUseOs,
    guard: Option<&mut Box<dyn ConsoleGuard>>,
    connected: bool,
) -> Observation {
    let s = os.observe();
    // Ack fields are filled by the owner with its nonce, never caller bytes.
    let _ = guard;
    Observation {
        at: s.at,
        screen: s.screen,
        console: s.console,
        input_generation: s.input_generation,
        cover_nonce: None,
        watchdog_nonce: None,
        connected,
    }
}

// The baseline is the observation already admitted by the owner, not a new
// snapshot taken when an independently scheduled OS task eventually starts.
fn unlock_check(
    os: Arc<dyn LockedUseOs>,
    console: Option<muster_core::locked_use::Console>,
    input: Option<u64>,
    deadline: Instant,
    cancel: CancellationToken,
) -> UnlockCheck {
    Arc::new(move || {
        if cancel.is_cancelled() || Instant::now() >= deadline {
            return Err("lock-screen submission cancelled or expired".into());
        }
        let sample = os.observe();
        if console.is_none()
            || input.is_none()
            || sample.console != console
            || sample.input_generation != input
            || sample.screen != muster_core::locked_use::Screen::Locked
        {
            return Err("console, lock state or local input changed before submission".into());
        }
        if cancel.is_cancelled() || Instant::now() >= deadline {
            return Err("lock-screen submission cancelled or expired".into());
        }
        Ok(())
    })
}

// Catalog scans and AX observers can block the main Tokio executor. The
// safety owner needs its own timer driver, not just another task on that pool.
fn owner_runtime() -> Result<tokio::runtime::Runtime, Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::Unavailable(format!("Cannot start Locked Use owner: {e}")))
}

fn unknown_result(message: String) -> Resp {
    Resp::OpResult(v1::OpResult {
        status: v1::op_result::Status::Unknown as i32,
        message,
        ..Default::default()
    })
}
