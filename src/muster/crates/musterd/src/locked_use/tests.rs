//! The owner and its task against a fake console and plugin.

use super::*;
use muster_core::locked_use::{Console, Screen};
use muster_core::{Author, DeviceKind, DeviceRecord, HwSigner, Scope, SoftSigner};
use muster_platform::locked_use::{ConsoleSample, GuardSample};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

pub(super) struct Os {
    creates: AtomicUsize,
    hold_create: AtomicBool,
    unlocked: AtomicBool,
    released: AtomicBool,
    locks: AtomicUsize,
    unlock_started: AtomicBool,
    unlock_submitted: AtomicBool,
    delay_unlock: AtomicBool,
    block_unlock: AtomicBool,
    input: AtomicU64,
    audit_session: AtomicUsize,
    fail_unlock: AtomicBool,
    guard_failed: AtomicBool,
    unlock_witness: std::sync::Mutex<Option<ConsoleSample>>,
    pending_recovery: AtomicBool,
    covered: AtomicBool,
    pulses: AtomicUsize,
    hold_unlock: AtomicBool,
    before_submit: tokio::sync::Notify,
    resume_submit: tokio::sync::Notify,
    work: muster_agent::control_guard::WorkQueue,
    extensions: std::sync::Mutex<Vec<Duration>>,
}
impl Os {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            creates: AtomicUsize::new(0),
            hold_create: AtomicBool::new(false),
            unlocked: AtomicBool::new(false),
            released: AtomicBool::new(false),
            locks: AtomicUsize::new(0),
            unlock_started: AtomicBool::new(false),
            unlock_submitted: AtomicBool::new(false),
            delay_unlock: AtomicBool::new(false),
            block_unlock: AtomicBool::new(false),
            input: AtomicU64::new(0),
            audit_session: AtomicUsize::new(42),
            fail_unlock: AtomicBool::new(false),
            guard_failed: AtomicBool::new(false),
            unlock_witness: Default::default(),
            pending_recovery: AtomicBool::new(false),
            covered: AtomicBool::new(true),
            pulses: AtomicUsize::new(0),
            hold_unlock: AtomicBool::new(false),
            before_submit: tokio::sync::Notify::new(),
            resume_submit: tokio::sync::Notify::new(),
            work: Default::default(),
            extensions: Default::default(),
        })
    }
}
struct Guard(Arc<Os>);
impl ConsoleGuard for Guard {
    fn extend(&mut self, ttl: Duration) -> Result<(), String> {
        self.0.extensions.lock().unwrap().push(ttl);
        Ok(())
    }
    fn pulse(&mut self) -> GuardSample {
        self.0.pulses.fetch_add(1, Ordering::SeqCst);
        GuardSample {
            covered: self.0.covered.load(Ordering::SeqCst),
            alive: true,
            failed: self.0.guard_failed.load(Ordering::SeqCst),
            observed_unlock: self.0.unlock_witness.lock().unwrap().take(),
            status_age: None,
        }
    }
    fn lock(&mut self) {}
    fn release(&mut self) -> Result<(), String> {
        self.0.released.store(true, Ordering::SeqCst);
        Ok(())
    }
}
struct Port(Arc<Os>);
#[async_trait::async_trait]
impl LockedUseOs for Port {
    fn available(&self) -> Result<(), String> {
        Ok(())
    }
    fn update_ready(&self) -> Result<(), String> {
        if self.0.pending_recovery.load(Ordering::SeqCst) {
            Err("pending protection".into())
        } else {
            Ok(())
        }
    }
    fn observe(&self) -> ConsoleSample {
        ConsoleSample {
            at: Instant::now(),
            console: Some(Console {
                uid: 501,
                audit_session: self.0.audit_session.load(Ordering::SeqCst) as u32,
            }),
            screen: if self.0.unlocked.load(Ordering::SeqCst) {
                Screen::Unlocked
            } else {
                Screen::Locked
            },
            input_generation: Some(self.0.input.load(Ordering::SeqCst)),
        }
    }
    fn guard(
        &self,
        _: [u8; 32],
        _: Duration,
        initial: Screen,
    ) -> Result<Box<dyn ConsoleGuard>, String> {
        assert_eq!(self.observe().screen, initial);
        Ok(Box::new(Guard(self.0.clone())))
    }
    async fn request_unlock(&self, check: UnlockCheck) -> Result<(), String> {
        check()?;
        self.0.unlock_started.store(true, Ordering::SeqCst);
        if self.0.block_unlock.load(Ordering::SeqCst) {
            self.0.before_submit.notify_one();
            std::thread::sleep(Duration::from_millis(400));
        }
        if self.0.delay_unlock.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        if self.0.hold_unlock.load(Ordering::SeqCst) {
            self.0.before_submit.notify_one();
            self.0.resume_submit.notified().await;
        }
        check()?;
        if self.0.fail_unlock.load(Ordering::SeqCst) {
            return Err("OS refused Session Return".into());
        }
        self.0.unlock_submitted.store(true, Ordering::SeqCst);
        Ok(())
    }
    fn lock(&self) {
        self.0.locks.fetch_add(1, Ordering::SeqCst);
        self.0.unlocked.store(false, Ordering::SeqCst);
    }
    fn plugin_peer(&self, _: &std::os::unix::net::UnixStream) -> bool {
        false
    }
}
#[tokio::test(flavor = "current_thread")]
async fn blocking_unlock_keeps_owner_timer_live_and_cancellation_drains_without_submission() {
    let os = Os::new();
    os.block_unlock.store(true, Ordering::SeqCst);
    let port = Arc::new(Port(os.clone()));
    let before = port.observe();
    let check = unlock_check(
        port.clone(),
        before.console,
        before.input_generation,
        Instant::now() + Duration::from_secs(4),
        CancellationToken::new(),
    );
    let mut job = Some(actor::start_unlock(port, check));
    os.before_submit.notified().await;
    let tick = Instant::now();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(tick.elapsed() < Duration::from_millis(250));
    assert!(!os.unlock_submitted.load(Ordering::SeqCst));
    actor::stop_unlock(&mut job).await;
    assert!(job.is_none());
    assert!(!os.unlock_submitted.load(Ordering::SeqCst));
}

#[tokio::test]
async fn diagnostic_never_submits_into_an_unlocked_or_owned_console() {
    let os = Os::new();
    let owner = Owner::new(Some(Arc::new(Port(os.clone()))));
    let held = owner.writes.clone().write_owned().await;
    assert!(matches!(
        owner.probe_unlock().await,
        Err(Error::Conflict(_))
    ));
    drop(held);
    os.unlocked.store(true, Ordering::SeqCst);
    assert!(matches!(
        owner.probe_unlock().await,
        Err(Error::Conflict(_))
    ));
    assert!(!os.unlock_started.load(Ordering::SeqCst));
    assert!(!os.unlock_submitted.load(Ordering::SeqCst));
}

#[tokio::test]
async fn submission_check_rejects_changes_after_an_os_wait() {
    for change in ["input", "console", "unlocked", "cancel"] {
        let os = Os::new();
        os.hold_unlock.store(true, Ordering::SeqCst);
        let port = Arc::new(Port(os.clone()));
        let before = port.observe();
        let cancel = CancellationToken::new();
        let check = unlock_check(
            port.clone(),
            before.console,
            before.input_generation,
            Instant::now() + Duration::from_secs(4),
            cancel.clone(),
        );
        let job = tokio::spawn(async move { port.request_unlock(check).await });
        tokio::time::timeout(Duration::from_secs(1), os.before_submit.notified())
            .await
            .unwrap();
        match change {
            "input" => os.input.store(1, Ordering::SeqCst),
            "console" => os.audit_session.store(43, Ordering::SeqCst),
            "unlocked" => os.unlocked.store(true, Ordering::SeqCst),
            "cancel" => cancel.cancel(),
            _ => unreachable!(),
        }
        os.resume_submit.notify_one();
        assert!(job.await.unwrap().is_err(), "{change}");
        assert!(!os.unlock_submitted.load(Ordering::SeqCst), "{change}");
    }
}

#[tokio::test]
async fn submission_check_does_not_rebase_when_the_os_task_starts_late() {
    let os = Os::new();
    let port = Arc::new(Port(os.clone()));
    let before = port.observe();
    let check = unlock_check(
        port.clone(),
        before.console,
        before.input_generation,
        Instant::now() + Duration::from_secs(4),
        CancellationToken::new(),
    );
    os.input.store(1, Ordering::SeqCst);
    assert!(port.request_unlock(check).await.is_err());
    assert!(!os.unlock_started.load(Ordering::SeqCst));
    assert!(!os.unlock_submitted.load(Ordering::SeqCst));
}

#[tokio::test]
async fn diagnostic_rechecks_input_after_waiting_for_the_lock_screen() {
    let os = Os::new();
    os.hold_unlock.store(true, Ordering::SeqCst);
    let owner = Arc::new(Owner::new(Some(Arc::new(Port(os.clone())))));
    let probe_owner = owner.clone();
    let job = tokio::spawn(async move { probe_owner.probe_unlock().await });
    tokio::time::timeout(Duration::from_secs(1), os.before_submit.notified())
        .await
        .unwrap();
    os.input.store(1, Ordering::SeqCst);
    os.resume_submit.notify_one();
    assert!(job.await.unwrap().is_err());
    assert!(!os.unlock_submitted.load(Ordering::SeqCst));
    assert!(owner.writes.clone().try_read_owned().is_ok());
}

pub(super) async fn start() -> (
    tempfile::TempDir,
    Arc<Daemon>,
    Arc<Owner>,
    Arc<Os>,
    Challenge,
    SoftSigner,
    DeviceId,
    tokio::task::JoinHandle<()>,
) {
    start_with(false).await
}

/// `terminal`: a terminal session for Ghostty terminal "ghost-1" instead of
/// one agent send.
async fn start_with(
    terminal: bool,
) -> (
    tempfile::TempDir,
    Arc<Daemon>,
    Arc<Owner>,
    Arc<Os>,
    Challenge,
    SoftSigner,
    DeviceId,
    tokio::task::JoinHandle<()>,
) {
    start_fixture(terminal, false).await
}

struct CreationHost(Arc<Os>);
impl crate::Platform for CreationHost {
    fn locked(&self) -> bool {
        !self.0.unlocked.load(Ordering::SeqCst)
    }
}
impl muster_terminal::ghostty::GhosttyOs for CreationHost {
    fn running(&self) -> bool {
        true
    }
    fn terminals(&self) -> Result<Vec<muster_terminal::ghostty::Listed>, muster_terminal::Error> {
        Ok(vec![])
    }
    fn perform(&self, _: &str, _: &str) -> Result<(), muster_terminal::Error> {
        panic!("no existing terminal input")
    }
    fn open(
        &self,
        _: &str,
    ) -> Result<Box<dyn muster_terminal::ghostty::Surface>, muster_terminal::Error> {
        panic!("creation grants no terminal lease")
    }
    fn create(&self) -> Result<String, muster_terminal::Error> {
        assert!(self.0.unlocked.load(Ordering::SeqCst));
        assert!(self.0.covered.load(Ordering::SeqCst));
        assert!(!self.0.released.load(Ordering::SeqCst));
        self.0.creates.fetch_add(1, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.0.hold_create.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            !self.0.released.load(Ordering::SeqCst),
            "cover must outlive OS work"
        );
        Ok("new-window".into())
    }
    fn close(&self, _: &str) -> Result<(), muster_terminal::Error> {
        panic!("no automatic close")
    }
}

async fn start_fixture(
    terminal: bool,
    creation: bool,
) -> (
    tempfile::TempDir,
    Arc<Daemon>,
    Arc<Owner>,
    Arc<Os>,
    Challenge,
    SoftSigner,
    DeviceId,
    tokio::task::JoinHandle<()>,
) {
    let dir = tempfile::tempdir().unwrap();
    let mut config = crate::DaemonConfig::new(dir.path());
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.listen = false;
    config.agent = Some(muster_agent::AgentConfig {
        codex_home: Some(dir.path().join("codex")),
        claude_home: None,
        catdesk_home: None,
        deepseek_home: None,
        claude_desktop: None,
        ..Default::default()
    });
    let os = Os::new();
    config.terminal = creation.then(|| {
        muster_terminal::TerminalKind::Ghostty(muster_terminal::GhosttyPlatform(Arc::new(
            CreationHost(os.clone()),
        )))
    });
    let platform: Arc<dyn crate::Platform> = if creation {
        Arc::new(CreationHost(os.clone()))
    } else {
        Arc::new(crate::DefaultPlatform)
    };
    let daemon = Daemon::start_with(config, platform, None).await.unwrap();
    let presence = SoftSigner::generate();
    let peer_key = SoftSigner::generate();
    let peer = peer_key.public_key().device_id();
    {
        let mut roster = daemon.roster().write().unwrap();
        let genesis = Author {
            roster: &roster,
            signer: &**daemon.signer(),
        }
        .genesis(
            "test",
            ("192.168.14.0".parse().unwrap(), 24),
            daemon.own_record(),
        )
        .unwrap();
        roster.insert(genesis).unwrap();
        let mut record = DeviceRecord::new(peer_key.public_key(), "phone", DeviceKind::Phone);
        record.presence_key = Some(presence.public_key());
        record
            .scopes
            .extend([Scope::LockedControl, Scope::Terminals]);
        if !creation {
            record
                .scopes
                .extend([Scope::Agents, Scope::AgentProvider("codex".into())]);
        }
        let add = Author {
            roster: &roster,
            signer: &**daemon.signer(),
        }
        .add(record)
        .unwrap();
        roster.insert(add).unwrap();
    }
    daemon.policy().write().unwrap().allow_locked_control = true;
    let port = Arc::new(Port(os.clone()));
    let owner = Arc::new(Owner::new(Some(port.clone())));
    let mut gate = Gate::new(daemon.device_id()).unwrap();
    let observed = observation(&*port, None, true);
    let c = if terminal || creation {
        gate.issue_terminal(
            daemon.roster().read().unwrap().state(),
            &daemon.policy().read().unwrap(),
            peer,
            "op-1",
            [9; 32],
            MAX_TTL,
            Instant::now(),
            observed,
        )
    } else {
        gate.issue(
            daemon.roster().read().unwrap().state(),
            &daemon.policy().read().unwrap(),
            peer,
            "codex",
            "op-1",
            [9; 32],
            MAX_TTL,
            Instant::now(),
            observed,
        )
    }
    .unwrap();
    let lease = terminal.then(|| TerminalLease {
        requester: peer,
        session: "ghost-1".into(),
        live: Default::default(),
        io: Arc::new(RwLock::new(())),
    });
    let (tx, rx) = mpsc::channel(16);
    *owner.current.lock().unwrap() = Some(Current {
        receipt: None,
        tx,
        scope: None,
        accepting: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        terminal: lease.clone(),
        manual: None,
    });
    let write = owner.writes.clone().write_owned().await;
    let runtime = owner_runtime().unwrap();
    let task = run(
        owner.clone(),
        Arc::downgrade(&daemon),
        port,
        gate,
        Instant::now(),
        c.clone(),
        peer,
        (!terminal).then(|| {
            Req::AgentSend(v1::AgentSendRequest {
                attachments: Vec::new(),
                session: "missing".into(),
                text: "hello".into(),
                op_id: "op-1".into(),
                release_unconfirmed: String::new(),
            })
        }),
        [9; 32],
        rx,
        write,
        os.work.clone(),
        None,
        lease,
    );
    let job = tokio::task::spawn_blocking(move || runtime.block_on(task));
    (dir, daemon, owner, os, c, presence, peer, job)
}
async fn execute(
    owner: Arc<Owner>,
    c: &Challenge,
    key: &SoftSigner,
    peer: DeviceId,
) -> tokio::task::JoinHandle<Result<Resp, Error>> {
    let request = v1::LockedUseExecuteRequest {
        challenge: c.encode(),
        signature: key.sign(&c.signing_message()).unwrap(),
    };
    let job =
        tokio::spawn(async move { owner.execute(peer, request, CancellationToken::new()).await });
    tokio::time::sleep(Duration::from_millis(120)).await;
    job
}
async fn consume(owner: &Owner) -> [u8; 32] {
    let tx = owner.current.lock().unwrap().clone().unwrap();
    let (reply, result) = oneshot::channel();
    tx.send(Command::Consume { uid: 501, reply }).await.unwrap();
    result.await.unwrap().unwrap()
}
#[tokio::test]
async fn updater_waits_without_stopping_owner_and_fences_new_writes() {
    let (_dir, _daemon, owner, _os, _c, _key, _peer, job) = start().await;
    let task_owner = owner.clone();
    let update = tokio::spawn(async move {
        task_owner
            .update_permit(&CancellationToken::new(), Duration::from_secs(2))
            .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!update.is_finished());
    assert!(owner.current.lock().unwrap().is_some());
    owner.shutdown();
    job.await.unwrap();
    let lease = update.await.unwrap().unwrap();
    assert!(owner.writes.clone().try_read_owned().is_err());
    assert!(owner.writes.clone().try_write_owned().is_err());
    drop(lease);
    assert!(owner.writes.clone().try_read_owned().is_ok());
}

#[tokio::test]
async fn updater_cancel_timeout_and_orphan_protection_never_stop_owner() {
    let (_dir, _daemon, owner, os, _c, _key, _peer, job) = start().await;
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(
        owner
            .update_permit(&cancel, Duration::from_secs(1))
            .await
            .is_err()
    );
    assert!(
        owner
            .update_permit(&CancellationToken::new(), Duration::from_millis(30))
            .await
            .is_err()
    );
    assert!(owner.current.lock().unwrap().is_some());
    assert!(!os.released.load(Ordering::SeqCst));
    owner.shutdown();
    job.await.unwrap();
    os.pending_recovery.store(true, Ordering::SeqCst);
    assert!(
        owner
            .update_permit(&CancellationToken::new(), Duration::from_secs(1))
            .await
            .is_err()
    );
    assert!(owner.writes.clone().try_write_owned().is_ok());
}

#[tokio::test]
async fn watchdog_witness_settles_unlock_missed_by_owner() {
    let (_dir, _daemon, owner, os, c, key, peer, job) = start().await;
    let reply = execute(owner.clone(), &c, &key, peer).await;
    let nonce = consume(&owner).await;
    owner
        .current
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .try_send(Command::Terminal {
            nonce,
            allowed: true,
        })
        .unwrap();
    let mut witness = Port(os.clone()).observe();
    witness.screen = Screen::Unlocked;
    *os.unlock_witness.lock().unwrap() = Some(witness);
    os.guard_failed.store(true, Ordering::SeqCst);
    tokio::time::timeout(Duration::from_secs(2), job)
        .await
        .unwrap()
        .unwrap();
    assert!(reply.await.unwrap().is_err());
    assert!(os.released.load(Ordering::SeqCst));
    assert!(owner.current.lock().unwrap().is_none());
}

#[tokio::test]
async fn same_requester_waits_for_closing_owner_before_fresh_admission() {
    let (_dir, daemon, owner, _os, _c, _key, peer, job) = start().await;
    {
        let mut current = owner.current.lock().unwrap();
        let active = current.as_mut().unwrap();
        active.scope = Some((peer, "codex".into()));
        active.accepting.store(false, Ordering::SeqCst);
    }
    let request_owner = owner.clone();
    let request = tokio::spawn(async move {
        request_owner
            .enqueue(
                &daemon,
                peer,
                Req::AgentRespond(Default::default()),
                CancellationToken::new(),
            )
            .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!request.is_finished());
    owner.shutdown();
    job.await.unwrap();
    assert!(request.await.unwrap().is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn owner_keeps_pulsing_when_the_rpc_executor_is_blocked() {
    let (_dir, _daemon, owner, os, c, key, peer, job) = start().await;
    let reply = execute(owner.clone(), &c, &key, peer).await;
    let before = os.pulses.load(Ordering::SeqCst);
    // Reproduce a synchronous scan longer than the watchdog's 350 ms.
    std::thread::sleep(Duration::from_millis(550));
    assert!(os.pulses.load(Ordering::SeqCst) >= before + 5);
    owner.cancel(peer, c.nonce(), None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), job)
        .await
        .unwrap()
        .unwrap();
    assert!(reply.await.unwrap().is_err());
    assert!(os.released.load(Ordering::SeqCst));
    assert!(owner.current.lock().unwrap().is_none());
}

#[tokio::test]
async fn closed_queue_reports_recovery_without_claiming_a_different_requester() {
    let (_dir, daemon, owner, _os, _c, _key, peer, job) = start().await;
    {
        let mut current = owner.current.lock().unwrap();
        let active = current.as_mut().unwrap();
        active.scope = Some((peer, "codex".into()));
        active.accepting.store(false, Ordering::SeqCst);
    }
    let error = owner
        .enqueue(
            &daemon,
            peer,
            Req::AgentSend(Default::default()),
            CancellationToken::new(),
        )
        .await
        .unwrap()
        .unwrap_err()
        .to_string();
    assert!(error.contains("still relocking"));
    assert!(error.contains("operation not started"));
    assert!(!error.contains("another requester"));
    let other = SoftSigner::generate().public_key().device_id();
    let error = owner
        .enqueue(
            &daemon,
            other,
            Req::AgentSend(Default::default()),
            CancellationToken::new(),
        )
        .await
        .unwrap()
        .unwrap_err()
        .to_string();
    assert!(error.contains("Another requester"));
    owner.shutdown();
    tokio::time::timeout(Duration::from_secs(1), job)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn failed_watchdog_rejects_before_unlock_without_waiting_for_ttl() {
    let (_dir, _daemon, owner, os, c, key, peer, job) = start().await;
    os.guard_failed.store(true, Ordering::SeqCst);
    let reply = execute(owner.clone(), &c, &key, peer).await;
    let error = tokio::time::timeout(Duration::from_secs(1), reply)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("watchdog stopped"));
    assert!(!os.unlock_started.load(Ordering::SeqCst));
    assert!(!os.unlock_submitted.load(Ordering::SeqCst));
    assert!(os.released.load(Ordering::SeqCst));
    assert!(owner.writes.clone().try_read_owned().is_ok());
    job.await.unwrap();
}

#[tokio::test]
async fn unlock_waits_for_actual_cover_readiness() {
    let (_dir, daemon, owner, os, c, key, peer, job) = start().await;
    os.covered.store(false, Ordering::SeqCst);
    let reply = execute(owner.clone(), &c, &key, peer).await;
    assert!(!os.unlock_started.load(Ordering::SeqCst));
    os.covered.store(true, Ordering::SeqCst);
    tokio::time::timeout(Duration::from_secs(1), async {
        while !os.unlock_started.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    owner.cancel(peer, c.nonce(), None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(1), job)
        .await
        .unwrap()
        .unwrap();
    assert!(reply.await.unwrap().is_err());
    assert!(os.released.load(Ordering::SeqCst));
    daemon.shutdown().await;
}

#[tokio::test]
async fn cancellation_joins_pending_unlock_before_releasing_console() {
    let (_dir, daemon, owner, os, c, key, peer, job) = start().await;
    os.delay_unlock.store(true, Ordering::SeqCst);
    let reply = execute(owner.clone(), &c, &key, peer).await;
    assert!(os.unlock_started.load(Ordering::SeqCst));
    owner.cancel(peer, c.nonce(), None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), job)
        .await
        .unwrap()
        .unwrap();
    assert!(reply.await.unwrap().is_err());
    assert!(os.released.load(Ordering::SeqCst));
    assert_eq!(os.locks.load(Ordering::SeqCst), 0);
    assert!(owner.writes.clone().try_read_owned().is_ok());
    tokio::time::sleep(Duration::from_millis(550)).await;
    assert!(!os.unlock_submitted.load(Ordering::SeqCst));
    daemon.shutdown().await;
}

#[tokio::test]
async fn owner_joins_submission_on_local_input_or_console_change() {
    for console_changed in [false, true] {
        let (_dir, daemon, owner, os, c, key, peer, job) = start().await;
        os.delay_unlock.store(true, Ordering::SeqCst);
        let reply = execute(owner.clone(), &c, &key, peer).await;
        assert!(os.unlock_started.load(Ordering::SeqCst));
        if console_changed {
            os.audit_session.store(43, Ordering::SeqCst);
        } else {
            os.input.store(1, Ordering::SeqCst);
        }
        tokio::time::timeout(Duration::from_secs(1), job)
            .await
            .unwrap()
            .unwrap();
        assert!(reply.await.unwrap().is_err());
        assert!(os.released.load(Ordering::SeqCst));
        assert_eq!(os.locks.load(Ordering::SeqCst), 0);
        assert!(owner.writes.clone().try_read_owned().is_ok());
        tokio::time::sleep(Duration::from_millis(550)).await;
        assert!(!os.unlock_submitted.load(Ordering::SeqCst));
        daemon.shutdown().await;
    }
}

#[tokio::test]
async fn os_submission_failure_releases_an_unconsumed_lease_promptly() {
    let (_dir, daemon, owner, os, c, key, peer, job) = start().await;
    os.fail_unlock.store(true, Ordering::SeqCst);
    let reply = execute(owner.clone(), &c, &key, peer).await;
    let error = tokio::time::timeout(Duration::from_secs(1), reply)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("OS refused Session Return"));
    tokio::time::timeout(Duration::from_secs(1), job)
        .await
        .unwrap()
        .unwrap();
    assert!(os.released.load(Ordering::SeqCst));
    assert!(!os.unlock_submitted.load(Ordering::SeqCst));
    assert_eq!(os.locks.load(Ordering::SeqCst), 0);
    daemon.shutdown().await;
}

#[tokio::test]
async fn os_failure_after_consume_keeps_cover_until_plugin_terminal() {
    let (_dir, daemon, owner, os, c, key, peer, job) = start().await;
    os.hold_unlock.store(true, Ordering::SeqCst);
    os.fail_unlock.store(true, Ordering::SeqCst);
    let reply = execute(owner.clone(), &c, &key, peer).await;
    tokio::time::timeout(Duration::from_secs(1), os.before_submit.notified())
        .await
        .unwrap();
    let nonce = consume(&owner).await;
    os.resume_submit.notify_one();
    let error = tokio::time::timeout(Duration::from_secs(1), reply)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("OS refused Session Return"));
    assert!(!os.released.load(Ordering::SeqCst));
    assert!(owner.writes.clone().try_read_owned().is_err());
    let tx = owner.current.lock().unwrap().clone().unwrap();
    tx.send(Command::Terminal {
        nonce,
        allowed: false,
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(1), job)
        .await
        .unwrap()
        .unwrap();
    assert!(os.released.load(Ordering::SeqCst));
    daemon.shutdown().await;
}
#[tokio::test]
async fn failed_native_action_still_relocks_and_releases_only_after_lock() {
    let (_dir, daemon, owner, os, c, key, peer, job) = start().await;
    let reply = execute(owner.clone(), &c, &key, peer).await;
    let nonce = consume(&owner).await;
    assert!(owner.writes.clone().try_read_owned().is_err());
    let tx = owner.current.lock().unwrap().clone().unwrap();
    tx.send(Command::Terminal {
        nonce,
        allowed: true,
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    os.unlocked.store(true, Ordering::SeqCst);
    assert!(
        tokio::time::timeout(Duration::from_secs(3), reply)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    tokio::time::timeout(Duration::from_secs(3), job)
        .await
        .unwrap()
        .unwrap();
    assert!(os.released.load(Ordering::SeqCst));
    assert!(!os.unlocked.load(Ordering::SeqCst));
    assert!(os.locks.load(Ordering::SeqCst) > 0);
    daemon.shutdown().await;
}
#[tokio::test]
async fn completion_and_cancellation_keep_cover_and_console_until_ax_queue_drains() {
    for stop in ["completion", "cancel", "cover lost"] {
        let (_dir, daemon, owner, os, c, key, peer, job) = start().await;
        let reply = execute(owner.clone(), &c, &key, peer).await;
        let nonce = consume(&owner).await;
        let (finish, wait) = std::sync::mpsc::channel();
        // Deliberately return the unjoined OS task to simulate an async
        // caller ending while its blocking AX call remains queued/running.
        #[allow(clippy::async_yields_async)]
        let queued = os
            .work
            .scope(async {
                muster_agent::control_guard::spawn_blocking(move || wait.recv().unwrap())
            })
            .await;
        let tx = owner.current.lock().unwrap().clone().unwrap();
        tx.send(Command::Terminal {
            nonce,
            allowed: true,
        })
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(60)).await;
        os.unlocked.store(true, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(120)).await;
        if stop != "completion" {
            if stop == "cancel" {
                owner.cancel(peer, &nonce, None).await.unwrap();
                tokio::time::sleep(Duration::from_millis(120)).await;
                assert!(
                    !os.unlocked.load(Ordering::SeqCst),
                    "cancel locks promptly under cover"
                );
            } else {
                // The guard reports no cover, and no loss. The transaction is
                // held while the gap can still be a stalled sample, admitting
                // nothing new, and relocks once it outlasts MAX_COVER_QUIET.
                os.covered.store(false, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(120)).await;
                assert!(
                    os.unlocked.load(Ordering::SeqCst),
                    "a briefly unreported cover is held, not stopped"
                );
                tokio::time::sleep(
                    muster_core::locked_use::MAX_COVER_QUIET + Duration::from_millis(300),
                )
                .await;
                assert!(
                    !os.unlocked.load(Ordering::SeqCst),
                    "an unreported cover that outlasts the tolerance locks under cover"
                );
            }
        } else {
            assert!(
                os.unlocked.load(Ordering::SeqCst),
                "normal completion waits for all AX work"
            );
        }
        assert!(!os.released.load(Ordering::SeqCst));
        assert!(!reply.is_finished());
        assert!(owner.writes.clone().try_read_owned().is_err());
        finish.send(()).unwrap();
        queued.await.unwrap();
        tokio::time::timeout(Duration::from_secs(3), job)
            .await
            .unwrap()
            .unwrap();
        assert!(reply.await.unwrap().is_err()); // Deliberately missing fixture session.
        assert!(os.released.load(Ordering::SeqCst));
        assert!(!os.unlocked.load(Ordering::SeqCst));
        assert!(owner.writes.clone().try_read_owned().is_ok());
        daemon.shutdown().await;
    }
}
#[tokio::test]
async fn cancellation_quarantines_a_consumed_grant_until_its_late_unlock() {
    let (_dir, daemon, owner, os, c, key, peer, job) = start().await;
    let reply = execute(owner.clone(), &c, &key, peer).await;
    let nonce = consume(&owner).await;
    owner.cancel(peer, &nonce, None).await.unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(!os.released.load(Ordering::SeqCst));
    assert!(!job.is_finished());
    let tx = owner.current.lock().unwrap().clone().unwrap();
    tx.send(Command::Terminal {
        nonce,
        allowed: true,
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert!(!os.released.load(Ordering::SeqCst));
    os.unlocked.store(true, Ordering::SeqCst);
    tokio::time::timeout(Duration::from_secs(3), job)
        .await
        .unwrap()
        .unwrap();
    assert!(os.released.load(Ordering::SeqCst));
    assert!(reply.await.unwrap().is_err());
    daemon.shutdown().await;
}
struct QueueRun {
    entered: std::sync::Mutex<Vec<String>>,
    changed: tokio::sync::Notify,
    permits: tokio::sync::Semaphore,
}
struct QueueController {
    session: String,
    run: Arc<QueueRun>,
    tracked: bool,
}
#[async_trait::async_trait]
impl muster_agent::controller::Controller for QueueController {
    fn kind(&self) -> muster_agent::model::ControllerKind {
        muster_agent::model::ControllerKind::NativeApp
    }
    fn session_id(&self) -> String {
        self.session.clone()
    }
    fn state(&self) -> muster_agent::model::SessionState {
        muster_agent::model::SessionState::Idle
    }
    fn process_identity(&self) -> muster_agent::controller::ProcessIdentity {
        muster_agent::controller::ProcessIdentity { pid: 1, start: 1 }
    }
    fn process_state(&self) -> muster_agent::controller::ProcessState {
        muster_agent::controller::ProcessState::Running
    }
    fn pending(&self) -> Vec<muster_agent::model::PendingInteraction> {
        Vec::new()
    }
    fn reports_delivery(&self) -> bool {
        self.tracked
    }
    async fn send(&self, text: &str) -> Result<(), muster_agent::Error> {
        muster_agent::control_guard::check()?;
        if matches!(text, "pressed" | "lost") {
            muster_agent::control_guard::delivery_attempt();
        }
        self.run.entered.lock().unwrap().push(text.into());
        self.run.changed.notify_one();
        self.run.permits.acquire().await.unwrap().forget();
        muster_agent::control_guard::check()?;
        match text {
            "reject" => Err(muster_agent::Error::Conflict("fixture rejection".into())),
            // Refused before input because an earlier write is unconfirmed.
            "blocked" => Err(muster_agent::Error::UnknownOutcome(
                "an earlier send has an unknown outcome".into(),
            )),
            // Pressed, then no receipt.
            "lost" => Err(muster_agent::Error::UnknownOutcome(
                "no provider receipt".into(),
            )),
            _ => Ok(()),
        }
    }
    async fn model_options(
        &self,
        set: Option<(&str, &str)>,
    ) -> Result<v1::AgentChatOptions, muster_agent::Error> {
        muster_agent::control_guard::check()?;
        self.run.entered.lock().unwrap().push("options".into());
        self.run.changed.notify_one();
        self.run.permits.acquire().await.unwrap().forget();
        muster_agent::control_guard::check()?;
        Ok(v1::AgentChatOptions {
            session: self.session.clone(),
            effort: set.map(|(_, effort)| effort).unwrap_or("Medium").into(),
            ..Default::default()
        })
    }
    async fn respond(&self, _: &str, _: &str, _: &str) -> Result<(), muster_agent::Error> {
        unreachable!()
    }
    async fn interrupt(&self) -> Result<(), muster_agent::Error> {
        unreachable!()
    }
    async fn stop(&self) {}
}
async fn wait_calls(run: &QueueRun, count: usize) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if run.entered.lock().unwrap().len() >= count {
                break;
            }
            run.changed.notified().await;
        }
    })
    .await
    .unwrap();
}
fn send_request(session: &str, text: &str, id: &str) -> Req {
    Req::AgentSend(v1::AgentSendRequest {
        session: session.into(),
        text: text.into(),
        op_id: id.into(),
        attachments: Vec::new(),
        release_unconfirmed: String::new(),
    })
}
fn status(reply: Resp) -> v1::op_result::Status {
    let Resp::OpResult(result) = reply else {
        panic!("missing per-request receipt")
    };
    v1::op_result::Status::try_from(result.status).unwrap()
}

#[tokio::test]
async fn one_presence_drains_same_provider_fifo_with_independent_receipts_and_replay_protection() {
    queue_case("complete").await;
}
#[tokio::test]
async fn provider_queue_cancel_preserves_unknown_active_and_rejects_not_started_requests() {
    queue_case("cancel").await;
}
#[tokio::test]
async fn later_cancellation_preserves_already_confirmed_request_result() {
    queue_case("cancel-later").await;
}
#[tokio::test]
async fn later_watchdog_failure_preserves_already_confirmed_request_result() {
    queue_case("watchdog-later").await;
}
#[tokio::test]
async fn provider_queue_deadline_is_not_extended_by_pending_requests() {
    queue_case("expire").await;
}
#[tokio::test]
async fn stopped_operation_is_not_sent_only_before_its_delivery_press() {
    let (status, ledger, message) = delivery_case("drafting").await;
    assert_eq!(status, v1::op_result::Status::Rejected);
    assert!(message.contains("nothing was sent"), "{message}");
    assert_eq!(ledger.as_deref(), Some("rejected"));

    let (status, ledger, _) = delivery_case("pressed").await;
    assert_eq!(status, v1::op_result::Status::Unknown);
    assert_eq!(
        ledger.as_deref(),
        Some("pending"),
        "a possible submission stays undecidable"
    );
}
#[tokio::test]
async fn unknown_before_any_press_is_recorded_as_not_sent() {
    let (status, ledger, message, replay) = outcome_case("blocked").await;
    assert_eq!(status, v1::op_result::Status::Rejected);
    assert_eq!(ledger.as_deref(), Some("rejected"));
    assert!(message.contains("was not sent"), "{message}");
    assert!(matches!(replay, muster_store::Begin::Duplicate(_)));
}
#[tokio::test]
async fn unknown_after_a_possible_press_stays_undecided_and_is_never_replayed() {
    let (status, ledger, message, replay) = outcome_case("lost").await;
    assert_eq!(status, v1::op_result::Status::Unknown);
    assert_eq!(ledger.as_deref(), Some("pending"));
    assert!(!message.contains("not sent"), "{message}");
    assert!(matches!(replay, muster_store::Begin::Unknown));
}
/// One request completes normally inside a Locked Use transaction; the
/// controller's own result decides the receipt and the ledger row.
async fn outcome_case(
    text: &str,
) -> (
    v1::op_result::Status,
    Option<String>,
    String,
    muster_store::Begin,
) {
    use muster_agent::controller::Controller;
    use muster_agent::desktop_chat::{ChatApp, ChatKey};
    let (_dir, daemon, owner, os, old, key, peer, old_job) = start().await;
    owner.cancel(peer, old.nonce(), None).await.unwrap();
    old_job.await.unwrap();
    os.released.store(false, Ordering::SeqCst);
    let run = Arc::new(QueueRun {
        entered: Default::default(),
        changed: Default::default(),
        permits: tokio::sync::Semaphore::new(1),
    });
    let agent = daemon.agent().unwrap();
    let session = agent
        .chat_observed(
            ChatApp::ChatGpt,
            None,
            ChatKey::Id("outcome".into()),
            "Outcome",
            Vec::new(),
        )
        .unwrap();
    agent.attach(Arc::new(QueueController {
        session: session.clone(),
        run: run.clone(),
        tracked: true,
    }) as Arc<dyn Controller>);
    let request = send_request(&session, text, "outcome-1");
    let operation = muster_proto::locked_use::operation(&request).unwrap();
    let Resp::LockedUseChallenge(prepared) = owner
        .prepare(daemon.clone(), peer, operation)
        .await
        .unwrap()
    else {
        panic!()
    };
    let c = Challenge::decode(&prepared.challenge).unwrap();
    let reply = execute(owner.clone(), &c, &key, peer).await;
    let nonce = consume(&owner).await;
    let tx = owner.current.lock().unwrap().clone().unwrap();
    tx.send(Command::Terminal {
        nonce,
        allowed: true,
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    os.unlocked.store(true, Ordering::SeqCst);
    let Resp::OpResult(result) = tokio::time::timeout(Duration::from_secs(17), reply)
        .await
        .unwrap()
        .unwrap()
        .unwrap()
    else {
        panic!("missing operation result")
    };
    assert_eq!(run.entered.lock().unwrap().len(), 1);
    let ledger = daemon.store().ledger();
    let status = ledger.status("outcome-1").unwrap();
    // The same op_id again: the recorded result, never a second attempt.
    let body = muster_proto::Message::encode_to_vec(match &request {
        Req::AgentSend(r) => r,
        _ => unreachable!(),
    });
    let replay = ledger.begin("outcome-1", "agent.send", &body).unwrap();
    daemon.shutdown().await;
    (
        v1::op_result::Status::try_from(result.status).unwrap(),
        status,
        result.message,
        replay,
    )
}
async fn delivery_case(text: &str) -> (v1::op_result::Status, Option<String>, String) {
    use muster_agent::controller::Controller;
    use muster_agent::desktop_chat::{ChatApp, ChatKey};
    let (_dir, daemon, owner, os, old, key, peer, old_job) = start().await;
    owner.cancel(peer, old.nonce(), None).await.unwrap();
    old_job.await.unwrap();
    os.released.store(false, Ordering::SeqCst);
    let run = Arc::new(QueueRun {
        entered: Default::default(),
        changed: Default::default(),
        permits: tokio::sync::Semaphore::new(0),
    });
    let agent = daemon.agent().unwrap();
    let session = agent
        .chat_observed(
            ChatApp::ChatGpt,
            None,
            ChatKey::Id("tracked".into()),
            "Tracked",
            Vec::new(),
        )
        .unwrap();
    agent.attach(Arc::new(QueueController {
        session: session.clone(),
        run: run.clone(),
        tracked: true,
    }) as Arc<dyn Controller>);
    let request = send_request(&session, text, "delivery-1");
    let operation = muster_proto::locked_use::operation(&request).unwrap();
    let Resp::LockedUseChallenge(prepared) = owner
        .prepare(daemon.clone(), peer, operation)
        .await
        .unwrap()
    else {
        panic!()
    };
    let c = Challenge::decode(&prepared.challenge).unwrap();
    let reply = execute(owner.clone(), &c, &key, peer).await;
    let nonce = consume(&owner).await;
    let tx = owner.current.lock().unwrap().clone().unwrap();
    tx.send(Command::Terminal {
        nonce,
        allowed: true,
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    os.unlocked.store(true, Ordering::SeqCst);
    // The controller is entered and blocked; then the watchdog stops.
    wait_calls(&run, 1).await;
    os.guard_failed.store(true, Ordering::SeqCst);
    let Resp::OpResult(result) = tokio::time::timeout(Duration::from_secs(17), reply)
        .await
        .unwrap()
        .unwrap()
        .unwrap()
    else {
        panic!("missing operation result")
    };
    assert!(os.released.load(Ordering::SeqCst));
    assert!(!os.unlocked.load(Ordering::SeqCst));
    let ledger = daemon.store().ledger().status("delivery-1").unwrap();
    daemon.shutdown().await;
    (
        v1::op_result::Status::try_from(result.status).unwrap(),
        ledger,
        result.message,
    )
}
async fn queue_case(stop: &str) {
    use muster_agent::controller::Controller;
    use muster_agent::desktop_chat::{ChatApp, ChatKey};
    let (_dir, daemon, owner, os, old, key, peer, old_job) = start().await;
    owner.cancel(peer, old.nonce(), None).await.unwrap();
    old_job.await.unwrap();
    os.released.store(false, Ordering::SeqCst);
    let run = Arc::new(QueueRun {
        entered: Default::default(),
        changed: Default::default(),
        permits: tokio::sync::Semaphore::new(0),
    });
    let agent = daemon.agent().unwrap();
    let first_session = agent
        .chat_observed(
            ChatApp::ChatGpt,
            None,
            ChatKey::Id("first".into()),
            "First",
            Vec::new(),
        )
        .unwrap();
    let second_session = agent
        .chat_observed(
            ChatApp::ChatGpt,
            None,
            ChatKey::Id("second".into()),
            "Second",
            Vec::new(),
        )
        .unwrap();
    let claude = agent
        .chat_observed(
            ChatApp::Claude,
            None,
            ChatKey::Id("other".into()),
            "Other",
            Vec::new(),
        )
        .unwrap();
    for session in [&first_session, &second_session, &claude] {
        agent.attach(Arc::new(QueueController {
            session: session.clone(),
            run: run.clone(),
            tracked: false,
        }) as Arc<dyn Controller>);
    }
    let first = send_request(&first_session, "first", "fifo-1");
    let mut operation = muster_proto::locked_use::operation(&first).unwrap();
    operation.provider_queue = "codex".into();
    let Resp::LockedUseChallenge(prepared) = owner
        .prepare(daemon.clone(), peer, operation)
        .await
        .unwrap()
    else {
        panic!()
    };
    let c = Challenge::decode(&prepared.challenge).unwrap();
    // Reject cross-provider and cross-requester access before any OS work.
    assert!(
        owner
            .enqueue(
                &daemon,
                peer,
                send_request(&claude, "other", "wrong-provider"),
                CancellationToken::new()
            )
            .await
            .unwrap()
            .is_err()
    );
    assert!(
        owner
            .enqueue(
                &daemon,
                daemon.device_id(),
                first.clone(),
                CancellationToken::new()
            )
            .await
            .unwrap()
            .is_err()
    );
    let mut second = Box::pin(owner.enqueue(
        &daemon,
        peer,
        send_request(&second_session, "reject", "fifo-2"),
        CancellationToken::new(),
    ));
    assert!(futures_util::poll!(second.as_mut()).is_pending());
    let mut third = Box::pin(owner.enqueue(
        &daemon,
        peer,
        send_request(&first_session, "third", "fifo-3"),
        CancellationToken::new(),
    ));
    assert!(futures_util::poll!(third.as_mut()).is_pending());
    let mut replay = Box::pin(owner.enqueue(&daemon, peer, first, CancellationToken::new()));
    assert!(futures_util::poll!(replay.as_mut()).is_pending());
    let reply = execute(owner.clone(), &c, &key, peer).await;
    let nonce = consume(&owner).await;
    let tx = owner.current.lock().unwrap().clone().unwrap();
    tx.send(Command::Terminal {
        nonce,
        allowed: true,
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    os.unlocked.store(true, Ordering::SeqCst);
    if stop != "complete" {
        wait_calls(&run, 1).await;
        if stop.ends_with("-later") {
            run.permits.add_permits(1);
            wait_calls(&run, 2).await;
        }
        if stop.starts_with("cancel") {
            owner.cancel(peer, c.nonce(), None).await.unwrap();
        } else if stop == "watchdog-later" {
            os.guard_failed.store(true, Ordering::SeqCst);
        }
        assert_eq!(
            status(
                tokio::time::timeout(Duration::from_secs(17), reply)
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap()
            ),
            if stop.ends_with("-later") {
                v1::op_result::Status::Applied
            } else {
                v1::op_result::Status::Unknown
            }
        );
        assert_eq!(
            status(second.await.unwrap().unwrap()),
            if stop.ends_with("-later") {
                v1::op_result::Status::Unknown
            } else {
                v1::op_result::Status::Rejected
            }
        );
        assert_eq!(
            status(third.await.unwrap().unwrap()),
            v1::op_result::Status::Rejected
        );
        assert_eq!(
            status(replay.await.unwrap().unwrap()),
            v1::op_result::Status::Rejected
        );
        assert_eq!(
            *run.entered.lock().unwrap(),
            if stop.ends_with("-later") {
                vec!["first", "reject"]
            } else {
                vec!["first"]
            }
        );
        assert!(os.released.load(Ordering::SeqCst));
        assert!(!os.unlocked.load(Ordering::SeqCst));
        assert!(owner.current.lock().unwrap().is_none());
        daemon.shutdown().await;
        return;
    }
    for count in 1..=3 {
        wait_calls(&run, count).await;
        assert_eq!(run.entered.lock().unwrap().len(), count);
        assert!(!os.released.load(Ordering::SeqCst));
        assert_eq!(os.locks.load(Ordering::SeqCst), 0);
        assert!(!reply.is_finished());
        run.permits.add_permits(1);
    }
    assert_eq!(
        status(
            tokio::time::timeout(Duration::from_secs(3), reply)
                .await
                .unwrap()
                .unwrap()
                .unwrap()
        ),
        v1::op_result::Status::Applied
    );
    assert_eq!(
        status(second.await.unwrap().unwrap()),
        v1::op_result::Status::Rejected
    );
    assert_eq!(
        status(third.await.unwrap().unwrap()),
        v1::op_result::Status::Applied
    );
    assert_eq!(
        status(replay.await.unwrap().unwrap()),
        v1::op_result::Status::Duplicate
    );
    assert_eq!(
        *run.entered.lock().unwrap(),
        vec!["first", "reject", "third"]
    );
    assert!(os.released.load(Ordering::SeqCst));
    assert!(!os.unlocked.load(Ordering::SeqCst));
    assert!(owner.current.lock().unwrap().is_none());
    daemon.shutdown().await;
}

/// Opens the terminal session `start_with(true)` prepared: signed, consumed,
/// allowed and unlocked.
async fn open_terminal(
    owner: &Arc<Owner>,
    os: &Os,
    c: &Challenge,
    key: &SoftSigner,
    peer: DeviceId,
) -> TerminalLease {
    let reply = execute(owner.clone(), c, key, peer).await;
    let nonce = consume(owner).await;
    let tx = owner.current.lock().unwrap().clone().unwrap();
    tx.send(Command::Terminal {
        nonce,
        allowed: true,
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    os.unlocked.store(true, Ordering::SeqCst);
    let opened = tokio::time::timeout(Duration::from_secs(3), reply)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(status(opened), v1::op_result::Status::Applied);
    owner
        .terminal_lease(peer, "ghost-1")
        .expect("the session is open")
}

#[tokio::test]
async fn a_terminal_session_renews_under_its_cover_and_relocks_after_its_io() {
    let (_dir, daemon, owner, os, c, key, peer, job) = start_with(true).await;
    let lease = open_terminal(&owner, &os, &c, &key, peer).await;
    assert!(owner.terminal_lease(peer, "ghost-2").is_none());
    // Two renewals, each signed by the requester, move the watchdog too.
    for _ in 0..2 {
        let Resp::LockedUseChallenge(next) = owner.renew(peer, c.nonce()).await.unwrap() else {
            panic!("no renewal challenge")
        };
        let next_challenge = Challenge::decode(&next.challenge).unwrap();
        assert_ne!(next_challenge.nonce(), c.nonce());
        let extended = owner
            .extend(
                peer,
                v1::LockedUseExtendRequest {
                    challenge: next.challenge,
                    signature: key.sign(&next_challenge.signing_message()).unwrap(),
                },
            )
            .await
            .unwrap();
        assert!(matches!(extended, Resp::Ack(_)));
    }
    let extensions = os.extensions.lock().unwrap().clone();
    assert_eq!(extensions.len(), 2);
    assert!(extensions.iter().all(|ttl| *ttl <= MAX_TTL));
    // Someone else, or a stale session nonce, gets no renewal.
    let stranger = SoftSigner::generate().public_key().device_id();
    assert!(owner.renew(stranger, c.nonce()).await.is_err());
    assert!(owner.renew(peer, &[1; 32]).await.is_err());
    // Terminal I/O in flight keeps the cover after the session ends.
    let io = lease.io().unwrap();
    owner.cancel(peer, c.nonce(), None).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!lease.live());
    assert!(lease.io().is_none());
    assert!(!os.released.load(Ordering::SeqCst));
    assert!(owner.terminal_lease(peer, "ghost-1").is_none());
    drop(io);
    tokio::time::timeout(Duration::from_secs(3), job)
        .await
        .unwrap()
        .unwrap();
    assert!(os.released.load(Ordering::SeqCst));
    assert!(os.locks.load(Ordering::SeqCst) > 0);
    daemon.shutdown().await;
}

#[tokio::test]
async fn local_input_ends_a_terminal_session() {
    let (_dir, daemon, owner, os, c, key, peer, job) = start_with(true).await;
    let lease = open_terminal(&owner, &os, &c, &key, peer).await;
    os.input.fetch_add(1, Ordering::SeqCst);
    tokio::time::timeout(Duration::from_secs(3), job)
        .await
        .unwrap()
        .unwrap();
    assert!(!lease.live());
    assert!(os.released.load(Ordering::SeqCst));
    assert!(owner.renew(peer, c.nonce()).await.is_err());
    daemon.shutdown().await;
}

#[tokio::test]
async fn chat_options_wait_for_signed_unlock_and_reply_only_after_relock() {
    for set in [false, true] {
        let (dir, daemon, owner, os, old, key, peer, old_job) = start().await;
        owner.cancel(peer, old.nonce(), None).await.unwrap();
        old_job.await.unwrap();
        os.released.store(false, Ordering::SeqCst);
        let session = "01a0e659-2de9-7f63-b80b-3b53fc7754e0";
        let history = dir.path().join("codex/sessions");
        std::fs::create_dir_all(&history).unwrap();
        std::fs::write(
            history.join("rollout-options.jsonl"),
            format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{session}\"}}}}\n"),
        )
        .unwrap();
        let agent = daemon.agent().unwrap();
        agent.rescan().await;
        let run = Arc::new(QueueRun {
            entered: Default::default(),
            changed: Default::default(),
            permits: tokio::sync::Semaphore::new(0),
        });
        agent.attach_native(Arc::new(QueueController {
            session: session.into(),
            run: run.clone(),
            tracked: false,
        }));
        let request = if set {
            Req::AgentChatSetOptions(v1::AgentChatSetOptionsRequest {
                session: session.into(),
                effort: "High".into(),
                op_id: "set-options".into(),
                ..Default::default()
            })
        } else {
            Req::AgentChatOptions(v1::AgentChatOptionsRequest {
                session: session.into(),
                op_id: "load-options".into(),
            })
        };
        let operation = muster_proto::locked_use::operation(&request).unwrap();
        daemon.policy().write().unwrap().allow_locked_control = false;
        assert!(
            owner
                .prepare(daemon.clone(), peer, operation.clone())
                .await
                .is_err()
        );
        assert!(!os.unlock_started.load(Ordering::SeqCst));
        daemon.policy().write().unwrap().allow_locked_control = true;
        let Resp::LockedUseChallenge(prepared) = owner
            .prepare(daemon.clone(), peer, operation)
            .await
            .unwrap()
        else {
            panic!()
        };
        let c = Challenge::decode(&prepared.challenge).unwrap();
        let reply = execute(owner.clone(), &c, &key, peer).await;
        assert!(run.entered.lock().unwrap().is_empty());
        let nonce = consume(&owner).await;
        let owner_tx = owner.current.lock().unwrap().clone().unwrap();
        owner_tx
            .send(Command::Terminal {
                nonce,
                allowed: true,
            })
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(run.entered.lock().unwrap().is_empty());
        os.unlocked.store(true, Ordering::SeqCst);
        wait_calls(&run, 1).await;
        assert!(!reply.is_finished());
        run.permits.add_permits(1);
        let Resp::AgentChatOptions(options) = tokio::time::timeout(Duration::from_secs(3), reply)
            .await
            .unwrap()
            .unwrap()
            .unwrap()
        else {
            panic!("typed options response")
        };
        assert_eq!(options.session, session);
        assert_eq!(options.effort, if set { "High" } else { "Medium" });
        assert!(!os.unlocked.load(Ordering::SeqCst));
        assert!(os.released.load(Ordering::SeqCst));
        assert!(os.locks.load(Ordering::SeqCst) > 0);
        daemon.shutdown().await;
    }
}

#[tokio::test]
async fn manual_cover_returns_each_receipt_without_relocking_and_drains_io_on_cancel() {
    use muster_agent::controller::Controller;
    use muster_agent::desktop_chat::{ChatApp, ChatKey};
    for initially_unlocked in [false, true] {
        let (_dir, daemon, owner, os, old, key, peer, old_job) = start().await;
        owner.cancel(peer, old.nonce(), None).await.unwrap();
        old_job.await.unwrap();
        os.released.store(false, Ordering::SeqCst);
        os.unlocked.store(initially_unlocked, Ordering::SeqCst);
        let run = Arc::new(QueueRun {
            entered: Default::default(),
            changed: Default::default(),
            permits: tokio::sync::Semaphore::new(2),
        });
        let agent = daemon.agent().unwrap();
        let session = agent
            .chat_observed(
                ChatApp::ChatGpt,
                None,
                ChatKey::Id("manual".into()),
                "Manual",
                Vec::new(),
            )
            .unwrap();
        agent.attach(Arc::new(QueueController {
            session: session.clone(),
            run: run.clone(),
            tracked: false,
        }) as Arc<dyn Controller>);
        let operation = v1::LockedUseOperation {
            provider_queue: String::new(),
            operation: Some(v1::locked_use_operation::Operation::Device(
                v1::DeviceLeaseRequest {
                    op_id: "manual".into(),
                },
            )),
        };
        let Resp::LockedUseChallenge(prepared) = owner
            .prepare(daemon.clone(), peer, operation)
            .await
            .unwrap()
        else {
            panic!()
        };
        let c = Challenge::decode(&prepared.challenge).unwrap();
        let lease = if initially_unlocked {
            let reply = execute(owner.clone(), &c, &key, peer).await;
            assert_eq!(
                status(
                    tokio::time::timeout(Duration::from_secs(3), reply)
                        .await
                        .unwrap()
                        .unwrap()
                        .unwrap()
                ),
                v1::op_result::Status::Applied
            );
            assert!(!os.unlock_started.load(Ordering::SeqCst));
            owner.terminal_lease(peer, "any-session").unwrap()
        } else {
            open_terminal(&owner, &os, &c, &key, peer).await
        };
        assert_eq!(
            owner.manual_status(),
            (peer.to_hex(), c.nonce().to_vec(), true)
        );
        tokio::time::sleep(Duration::from_millis(3200)).await;
        assert!(!os.extensions.lock().unwrap().is_empty());
        for index in 0..2 {
            let response = tokio::time::timeout(
                Duration::from_secs(3),
                owner.enqueue(
                    &daemon,
                    peer,
                    send_request(&session, "manual-send", &format!("manual-{index}")),
                    CancellationToken::new(),
                ),
            )
            .await
            .unwrap()
            .unwrap()
            .unwrap();
            assert_eq!(status(response), v1::op_result::Status::Applied);
            assert!(lease.live());
            assert!(!os.released.load(Ordering::SeqCst));
            assert_eq!(
                owner.manual_status(),
                (peer.to_hex(), c.nonce().to_vec(), true)
            );
            let foreign = owner
                .enqueue(
                    &daemon,
                    daemon.device_id(),
                    send_request(&session, "foreign", &format!("foreign-{index}")),
                    CancellationToken::new(),
                )
                .await
                .unwrap()
                .unwrap_err();
            assert!(foreign.to_string().contains("Another requester"));
            assert!(
                lease.live(),
                "foreign rejection must not end the real owner's lease"
            );
        }
        assert_eq!(run.entered.lock().unwrap().len(), 2);
        assert!(owner.manual_terminal_lease(peer).is_some());
        assert!(owner.manual_terminal_lease(daemon.device_id()).is_none());
        let io = lease.io().unwrap();
        owner.cancel(peer, c.nonce(), None).await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!lease.live());
        assert!(!os.released.load(Ordering::SeqCst));
        drop(io);
        tokio::time::timeout(Duration::from_secs(3), async {
            while owner.current.lock().unwrap().is_some() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert!(os.released.load(Ordering::SeqCst));
        assert!(!os.unlocked.load(Ordering::SeqCst));
        daemon.shutdown().await;
    }
}

#[tokio::test]
async fn locked_creation_is_one_shot_and_drains_before_releasing_cover() {
    for cancel in [false, true] {
        let (_dir, daemon, owner, os, old, key, peer, old_job) = start_fixture(false, true).await;
        owner.cancel(peer, old.nonce(), None).await.unwrap();
        old_job.await.unwrap();
        os.released.store(false, Ordering::SeqCst);
        let request = Req::TerminalCreate(v1::TerminalCreateRequest {
            source: "ghostty".into(),
            op_id: "create-once".into(),
            ..Default::default()
        });
        let (app, server) = tokio::io::duplex(65536);
        tokio::spawn(muster_rpc::serve(
            server,
            muster_rpc::Principal::Local,
            Arc::new(crate::MusterHandler::new(Arc::downgrade(&daemon))),
        ));
        let client = muster_rpc::Client::new(app);
        assert!(matches!(
            client.call(request.clone()).await,
            Err(muster_rpc::Error::Remote {
                code: v1::error::Code::LockedUseRequired,
                ..
            })
        ));
        assert_eq!(os.creates.load(Ordering::SeqCst), 0);
        let (app, server) = tokio::io::duplex(65536);
        tokio::spawn(muster_rpc::serve(
            server,
            muster_rpc::Principal::Remote(peer),
            Arc::new(crate::MusterHandler::new(Arc::downgrade(&daemon))),
        ));
        let remote = muster_rpc::Client::new(app);
        // Unlike local calls, the remote route used to incorrectly send all
        // TerminalCreate requests through the generic native unlock gate.
        let error = remote
            .call(Req::TerminalCreate(v1::TerminalCreateRequest {
                source: "iterm2".into(),
                op_id: "remote-iterm".into(),
                ..Default::default()
            }))
            .await
            .unwrap_err();
        assert!(
            matches!(error, muster_rpc::Error::Remote { code: v1::error::Code::Invalid, ref message, .. }
            if message.contains("not configured")),
            "iTerm2 reaches source dispatch without an unlock: {error}"
        );
        assert!(matches!(
            remote.call(request.clone()).await,
            Err(muster_rpc::Error::Remote {
                code: v1::error::Code::LockedUseRequired,
                ..
            })
        ));
        let operation = muster_proto::locked_use::operation(&request).unwrap();
        let mut queue = operation.clone();
        queue.provider_queue = "codex".into();
        assert!(owner.prepare(daemon.clone(), peer, queue).await.is_err());
        let mut wrong = v1::TerminalCreateRequest {
            source: "iterm2".into(),
            op_id: "bad".into(),
            ..Default::default()
        };
        assert!(crate::handler::validate_locked_terminal_creation(&daemon, &wrong).is_err());
        wrong.source = "ghostty".into();
        wrong.op_id.clear();
        assert!(crate::handler::validate_locked_terminal_creation(&daemon, &wrong).is_err());
        daemon.policy().write().unwrap().allow_locked_control = false;
        assert!(
            owner
                .prepare(daemon.clone(), peer, operation.clone())
                .await
                .is_err()
        );
        daemon.policy().write().unwrap().allow_locked_control = true;
        let Resp::LockedUseChallenge(prepared) = owner
            .prepare(daemon.clone(), peer, operation)
            .await
            .unwrap()
        else {
            panic!()
        };
        let c = Challenge::decode(&prepared.challenge).unwrap();
        assert!(owner.renew(peer, c.nonce()).await.is_err());
        assert!(owner.terminal_lease(peer, "ghostty:new-window").is_none());
        os.hold_create.store(true, Ordering::SeqCst);
        let reply = execute(owner.clone(), &c, &key, peer).await;
        let nonce = consume(&owner).await;
        let owner_tx = owner.current.lock().unwrap().clone().unwrap();
        owner_tx
            .send(Command::Terminal {
                nonce,
                allowed: true,
            })
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert_eq!(os.creates.load(Ordering::SeqCst), 0);
        os.unlocked.store(true, Ordering::SeqCst);
        tokio::time::timeout(Duration::from_secs(3), async {
            while os.creates.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(!reply.is_finished());
        if cancel {
            owner.cancel(peer, c.nonce(), None).await.unwrap();
            tokio::time::sleep(Duration::from_millis(250)).await;
            assert!(!os.released.load(Ordering::SeqCst));
        }
        os.hold_create.store(false, Ordering::SeqCst);
        let response = tokio::time::timeout(Duration::from_secs(3), reply)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        if cancel {
            assert!(matches!(response, Resp::OpResult(_)));
        } else {
            assert!(
                matches!(response, Resp::TerminalSession(ref s) if s.id == "ghostty:new-window")
            );
        }
        assert!(!os.unlocked.load(Ordering::SeqCst));
        assert!(os.released.load(Ordering::SeqCst));
        // Replay only after an explicit local unlock; the ledger returns the same window.
        os.unlocked.store(true, Ordering::SeqCst);
        let replay = client.call(request).await.unwrap();
        assert!(matches!(replay, Resp::TerminalSession(ref s) if s.id == "ghostty:new-window"));
        assert_eq!(os.creates.load(Ordering::SeqCst), 1);
        daemon.shutdown().await;
    }
}

#[tokio::test]
async fn late_agent_receipt_returns_after_relock_without_another_submission() {
    use muster_proto::Message;
    let (dir, daemon, owner, os, old, key, peer, old_job) = start().await;
    owner.cancel(peer, old.nonce(), None).await.unwrap();
    old_job.await.unwrap();
    os.released.store(false, Ordering::SeqCst);
    let session = "11111111-2222-7333-8444-555555555555";
    let history = dir.path().join("codex/sessions");
    std::fs::create_dir_all(&history).unwrap();
    std::fs::write(
        history.join("rollout-receipt.jsonl"),
        format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{session}\"}}}}\n"),
    )
    .unwrap();
    let agent = daemon.agent().unwrap();
    agent.rescan().await;
    let run = Arc::new(QueueRun {
        entered: Default::default(),
        changed: Default::default(),
        permits: tokio::sync::Semaphore::new(1),
    });
    agent.attach_native(Arc::new(QueueController {
        session: session.into(),
        run: run.clone(),
        tracked: true,
    }));
    let request = send_request(session, "lost", "late-receipt");
    let Req::AgentSend(send) = &request else {
        unreachable!()
    };
    let hash = muster_store::Ledger::body_hash(&send.encode_to_vec());
    let Resp::LockedUseChallenge(prepared) = owner
        .prepare(
            daemon.clone(),
            peer,
            muster_proto::locked_use::operation(&request).unwrap(),
        )
        .await
        .unwrap()
    else {
        panic!()
    };
    let c = Challenge::decode(&prepared.challenge).unwrap();
    let reply = execute(owner.clone(), &c, &key, peer).await;
    let nonce = consume(&owner).await;
    let tx = owner.current.lock().unwrap().clone().unwrap();
    tx.send(Command::Terminal {
        nonce,
        allowed: true,
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    os.unlocked.store(true, Ordering::SeqCst);
    tokio::time::timeout(Duration::from_secs(3), async {
        while owner.current.lock().unwrap().is_some() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert!(!os.unlocked.load(Ordering::SeqCst));
    assert!(os.released.load(Ordering::SeqCst));
    assert!(owner.writes.clone().try_write_owned().is_ok());
    assert!(
        !reply.is_finished(),
        "caller awaits receipt after the console has been released"
    );
    assert_eq!(run.entered.lock().unwrap().as_slice(), ["lost"]);
    daemon
        .store()
        .ledger()
        .confirm("late-receipt", "agent.send", &hash, session.as_bytes())
        .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(2), reply)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(status(result), v1::op_result::Status::Applied);
    assert_eq!(run.entered.lock().unwrap().as_slice(), ["lost"]);
    daemon.shutdown().await;
}
