//! The owner task: one console lease from challenge to relock, driven by
//! commands and a 50 ms tick. A lease runs one operation (and its provider
//! queue), or keeps a terminal session open while its requester renews it.

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) async fn run(
    owner: Arc<Owner>,
    weak: Weak<Daemon>,
    os: Arc<dyn LockedUseOs>,
    mut gate: Gate,
    started: Instant,
    challenge: Challenge,
    peer: DeviceId,
    request: Option<Req>,
    digest: [u8; 32],
    mut rx: mpsc::Receiver<Command>,
    _write: OwnedRwLockWriteGuard<()>,
    work: muster_agent::control_guard::WorkQueue,
    provider: Option<String>,
    terminal: Option<TerminalLease>,
) {
    let manual = gate.is_device();
    let mut last_maintenance = Instant::now();
    let close_terminal = || {
        if let Some((_, _, active)) = owner
            .current
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|c| c.manual.as_ref())
        {
            active.store(false, std::sync::atomic::Ordering::SeqCst);
        }
        if let Some(t) = &terminal {
            t.live.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    };
    // Terminal reads and writes hold this barrier; the cover outlasts them.
    let terminal_busy = || {
        terminal
            .as_ref()
            .is_some_and(|t| t.io.clone().try_write_owned().is_err())
    };
    let mut guard: Option<Box<dyn ConsoleGuard>> = None;
    let mut pending: Option<Reply> = None;
    let mut cancel = CancellationToken::new();
    let mut unlock_started = false;
    let mut unlock: Option<UnlockJob> = None;
    let mut action: Option<tokio::task::JoinHandle<Result<Resp, Error>>> = None;
    let mut action_delivery: Option<muster_agent::control_guard::Delivery> = None;
    // Operations stopped mid-flight. Their result is decided only after the OS
    // work drains, when no late press can still change it.
    let mut stopped: Vec<Stopped> = Vec::new();
    let mut outcome: Option<Result<Resp, Error>> = None;
    let mut action_finished = false;
    let mut release_pending = false;
    let mut queue = std::collections::VecDeque::<Queued>::new();
    let mut completed: Vec<(Reply, Result<Resp, Error>)> = Vec::new();
    let mut action_reply: Option<Reply> = None;
    let mut action_cancel: Option<CancellationToken> = None;
    let mut accepted = 1usize;
    let live = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut interval = tokio::time::interval(Duration::from_millis(50));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_pulse = Instant::now();
    // Content-free evidence for a cover or watchdog stop: the newest watchdog
    // status age, the owner's OS sampling time and its pulse gap.
    let mut guard_evidence: Option<(Option<Duration>, bool, bool, Duration, Duration)> = None;
    loop {
        let Some(daemon) = weak.upgrade() else {
            live.store(false, std::sync::atomic::Ordering::SeqCst);
            close_terminal();
            stop_unlock(&mut unlock).await;
            if let Some(job) = action.take() {
                job.abort();
                let _ = job.await;
            }
            if let Some(g) = guard.as_mut() {
                g.lock();
                if os.observe().console == Some(challenge.console()) {
                    os.lock();
                }
            }
            *owner.current.lock().unwrap() = None;
            return;
        };
        let command = tokio::select! {
            _ = interval.tick() => None,
            command = rx.recv() => command,
        };
        if unlock.as_ref().is_some_and(|job| job.is_finished()) {
            let result = unlock.take().unwrap().finish().await;
            if let Err(error) = result {
                log::warn!("Locked Use OS authorization: {error}");
                outcome.get_or_insert_with(|| Err(Error::Unavailable(error)));
                gate.stop(StopReason::ObservationLost, Instant::now());
            }
        }
        let sampling = Instant::now();
        let mut o = observation(&*os, None, !cancel.is_cancelled());
        let sampled_in = sampling.elapsed();
        if sampling.elapsed() > Duration::from_millis(100) {
            log::warn!(
                "Locked Use OS observation delayed {} ms phase={:?}",
                sampling.elapsed().as_millis(),
                gate.phase()
            );
        }
        if let Some(g) = guard.as_mut() {
            let gap = last_pulse.elapsed();
            if gap > Duration::from_millis(200) {
                log::warn!(
                    "Locked Use owner: pulse delayed {} ms phase={:?}",
                    gap.as_millis(),
                    gate.phase()
                );
            }
            last_pulse = Instant::now();
            let s = g.pulse();
            guard_evidence = Some((s.status_age, s.covered, s.alive, sampled_in, gap));
            if let Some(sample) = s.observed_unlock {
                let witness = Observation {
                    at: sample.at,
                    screen: sample.screen,
                    console: sample.console,
                    input_generation: sample.input_generation,
                    cover_nonce: None,
                    watchdog_nonce: None,
                    connected: false,
                };
                let accepted = gate
                    .witnessed_unlock(challenge.nonce(), witness, Instant::now())
                    .is_ok();
                log::info!(
                    "Locked Use: watchdog unlock witness accepted={accepted}; phase={:?}",
                    gate.phase()
                );
            }
            if s.failed && !release_pending && gate.phase() != Phase::Relocking {
                log::warn!("Locked Use: watchdog stopped; refusing authorization/execution");
                outcome.get_or_insert_with(|| {
                    Err(Error::Unavailable(
                        "Locked Use display watchdog stopped before completion".into(),
                    ))
                });
                gate.stop(StopReason::WatchdogLost, Instant::now());
            }
            if s.covered {
                o.cover_nonce = Some(*challenge.nonce());
            }
            if s.alive {
                o.watchdog_nonce = Some(*challenge.nonce());
            }
        }
        if let Some(command) = command {
            match command {
                Command::Enqueue(queued) => {
                    if (!manual && provider.is_none())
                        || (!manual && accepted >= 32)
                        || queue.len() >= 32
                        || release_pending
                        || gate.phase() == Phase::Relocking
                    {
                        let _ = queued
                            .reply
                            .send(Ok(not_started("Locked Use queue closed or full")));
                    } else {
                        accepted = accepted.saturating_add(1);
                        queue.push_back(queued);
                    }
                }
                Command::Execute {
                    peer: p,
                    challenge: c,
                    signature,
                    reply,
                    cancel: token,
                } => {
                    let result = gate.arm(
                        daemon.roster().read().unwrap().state(),
                        &daemon.policy().read().unwrap(),
                        p,
                        &c,
                        &signature,
                        Instant::now(),
                        o,
                    );
                    match result {
                        Ok(_) => {
                            log::info!(
                                "Locked Use: operation signature verified; preparing display guard"
                            );
                            cancel = token;
                            pending = Some(reply);
                            match os.guard(
                                *challenge.nonce(),
                                MAX_TTL.saturating_sub(started.elapsed()),
                                o.screen,
                            ) {
                                Ok(g) => {
                                    last_pulse = Instant::now();
                                    guard = Some(g);
                                }
                                Err(error) => {
                                    outcome = Some(Err(Error::Unavailable(error)));
                                    gate.stop(StopReason::WatchdogLost, Instant::now());
                                }
                            }
                        }
                        Err(e) => {
                            log::warn!("Locked Use: operation signature rejected: {e}");
                            let _ = reply.send(Err(e.into()));
                        }
                    }
                }
                Command::Cancel {
                    peer: p,
                    nonce,
                    reason,
                } if p == peer && &nonce == challenge.nonce() => {
                    log::info!(
                        "Locked Use: requester withdrew the challenge in {:?} ({})",
                        gate.phase(),
                        reason.map_or("reason not given", |r| r.as_str())
                    );
                    cancel.cancel();
                }
                Command::Cancel { .. } => {}
                Command::Shutdown => {
                    cancel.cancel();
                }
                Command::Renew {
                    peer: p,
                    nonce,
                    reply,
                } => {
                    let result = if terminal.is_none() || &nonce != challenge.nonce() {
                        Err(Error::Conflict("not this terminal session".into()))
                    } else {
                        gate.renewal(p, Instant::now())
                            .map(|next| {
                                Resp::LockedUseChallenge(v1::LockedUseChallenge {
                                    challenge: next.encode(),
                                })
                            })
                            .map_err(Error::from)
                    };
                    let _ = reply.send(result);
                }
                Command::Extend {
                    peer: p,
                    challenge: next,
                    signature,
                    reply,
                } => {
                    let renewed = gate.renew(
                        daemon.roster().read().unwrap().state(),
                        &daemon.policy().read().unwrap(),
                        p,
                        &next,
                        &signature,
                        Instant::now(),
                        o,
                    );
                    let result = match renewed {
                        Ok(deadline) => {
                            // The independent watchdog keeps its own deadline:
                            // move it too, or end the session.
                            let ttl = deadline.saturating_duration_since(Instant::now());
                            match guard.as_mut().map(|g| g.extend(ttl)) {
                                Some(Ok(())) => Ok(Resp::Ack(v1::Ack {})),
                                failed => {
                                    let why = match failed {
                                        Some(Err(e)) => e,
                                        _ => "no display guard".into(),
                                    };
                                    log::warn!("Locked Use: watchdog extension failed: {why}");
                                    gate.stop(StopReason::WatchdogLost, Instant::now());
                                    Err(Error::Unavailable(why))
                                }
                            }
                        }
                        Err(e) => {
                            log::warn!("Locked Use: terminal renewal refused: {e}");
                            Err(e.into())
                        }
                    };
                    let _ = reply.send(result);
                }
                Command::Input { peer: p } => gate.note_input(p, Instant::now()),
                Command::Consume { uid, reply } => {
                    o.connected = !cancel.is_cancelled();
                    let ok = o.console.filter(|c| c.uid == uid).is_some_and(|console| {
                        gate.consume(
                            challenge.nonce(),
                            console,
                            daemon.roster().read().unwrap().state(),
                            &daemon.policy().read().unwrap(),
                            Instant::now(),
                            o,
                        )
                        .is_ok()
                    });
                    log::info!("Locked Use: plugin consume admitted={ok}");
                    let _ = reply.send(ok.then_some(*challenge.nonce()));
                }
                Command::Terminal { nonce, allowed } => {
                    let accepted = gate
                        .plugin_finished(
                            &nonce,
                            if allowed {
                                PluginOutcome::Allowed
                            } else {
                                PluginOutcome::Denied
                            },
                            Instant::now(),
                        )
                        .is_ok();
                    log::info!("Locked Use: plugin terminal allowed={allowed} accepted={accepted}");
                }
            }
        }
        o.connected = !cancel.is_cancelled()
            && !action_cancel
                .as_ref()
                .is_some_and(CancellationToken::is_cancelled);
        if manual
            && gate.phase() == Phase::Active
            && last_maintenance.elapsed() >= Duration::from_secs(3)
        {
            let maintained = gate.maintain_device(
                daemon.roster().read().unwrap().state(),
                &daemon.policy().read().unwrap(),
                Instant::now(),
                o,
            );
            match maintained {
                Ok(deadline) => {
                    if !matches!(
                        guard
                            .as_mut()
                            .map(|g| g.extend(deadline.saturating_duration_since(Instant::now()))),
                        Some(Ok(()))
                    ) {
                        gate.stop(StopReason::WatchdogLost, Instant::now());
                    }
                }
                Err(_) => {
                    gate.stop(StopReason::AuthorizationChanged, Instant::now());
                }
            }
            last_maintenance = Instant::now();
        }
        let phase_before = gate.phase();
        let directive = if release_pending {
            Directive::ReleaseCover
        } else {
            gate.poll(
                daemon.roster().read().unwrap().state(),
                &daemon.policy().read().unwrap(),
                Instant::now(),
                o,
            )
        };
        if phase_before != gate.phase() {
            log::info!(
                "Locked Use: {:?} -> {:?}; directive={directive:?}; stop={}",
                phase_before,
                gate.phase(),
                gate.last_stop()
                    .map_or_else(|| "none".to_owned(), |r| format!("{r:?}"))
            );
            if let Directive::LockAndKeepCovered(
                reason @ (StopReason::CoverLost | StopReason::WatchdogLost),
            ) = directive
                && let Some((age, covered, alive, sampled, gap)) = guard_evidence
            {
                log::warn!(
                    "Locked Use: {reason:?} evidence: newest watchdog status age={:?} ms covered={covered} alive={alive}; owner OS sampling {} ms, pulse gap {} ms",
                    age.map(|a| a.as_millis()),
                    sampled.as_millis(),
                    gap.as_millis()
                );
            }
        }
        match directive {
            Directive::PrepareCoverAndWatchdog
                if o.cover_nonce.is_some() && o.watchdog_nonce.is_some() && !unlock_started =>
            {
                unlock_started = true;
                let os = os.clone();
                let check = unlock_check(
                    os.clone(),
                    o.console,
                    o.input_generation,
                    started + MAX_TTL,
                    cancel.clone(),
                );
                // Plugin invocation still needs the live owner to consume;
                // this OS task cannot manufacture authority after cancellation.
                unlock = Some(start_unlock(os, check));
            }
            Directive::ReadyForAction if action.is_none() && !action_finished => {
                let admitted = gate
                    .take_action(
                        peer,
                        challenge.turn(),
                        &digest,
                        daemon.roster().read().unwrap().state(),
                        &daemon.policy().read().unwrap(),
                        Instant::now(),
                        o,
                    )
                    .is_ok();
                if admitted && let Some(request) = &request {
                    log::info!("Locked Use: bound operation admitted after OS unlock");
                    live.store(true, std::sync::atomic::Ordering::SeqCst);
                    let (job, delivery) = start_action(
                        daemon.clone(),
                        request.clone(),
                        peer,
                        started,
                        os.clone(),
                        live.clone(),
                        o,
                        cancel.clone(),
                        work.clone(),
                        provider.clone(),
                    );
                    action = Some(job);
                    action_delivery = Some(delivery);
                } else if admitted && manual {
                    action_finished = true;
                    live.store(true, std::sync::atomic::Ordering::SeqCst);
                    if let Some(t) = &terminal {
                        t.live.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    cancel = CancellationToken::new();
                    if let Some((_, _, active)) = owner
                        .current
                        .lock()
                        .unwrap()
                        .as_ref()
                        .and_then(|c| c.manual.as_ref())
                    {
                        active.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    if let Some(reply) = pending.take() {
                        let _ = reply.send(Ok(Resp::OpResult(v1::OpResult::default())));
                    }
                } else if admitted && let Some(t) = &terminal {
                    log::info!("Locked Use: terminal session open under cover");
                    t.live.store(true, std::sync::atomic::Ordering::SeqCst);
                    // From here the session lives on signed renewals, not on
                    // the Execute call that opened it.
                    cancel = CancellationToken::new();
                    if let Some(reply) = pending.take() {
                        let _ = reply.send(Ok(Resp::OpResult(v1::OpResult::default())));
                    }
                }
            }
            Directive::LockAndKeepCovered(reason) => {
                if let Some(current) = owner.current.lock().unwrap().as_ref() {
                    current
                        .accepting
                        .store(false, std::sync::atomic::Ordering::SeqCst);
                }
                live.store(false, std::sync::atomic::Ordering::SeqCst);
                close_terminal();
                stop_unlock(&mut unlock).await;
                if let Some(job) = action.take() {
                    job.abort();
                    let _ = job.await;
                    log::warn!("Locked Use: stopping the running operation: {reason:?}");
                    stopped.push(Stopped {
                        reply: action_reply.take(),
                        delivery: action_delivery.take(),
                        reason,
                    });
                }
                for queued in queue.drain(..) {
                    completed.push((
                        queued.reply,
                        Ok(not_started(&format!("Locked Use stopped: {reason:?}"))),
                    ));
                }
                if o.console == Some(challenge.console()) {
                    os.lock();
                }
                if let Some(g) = guard.as_mut() {
                    g.lock();
                }
                if outcome.is_none() && !stopped.iter().any(|s| s.reply.is_none()) {
                    outcome = Some(Err(Error::Unavailable(format!(
                        "Locked Use stopped: {reason:?}"
                    ))));
                }
            }
            Directive::ReleaseCover => {
                release_pending = true;
                if let Some(current) = owner.current.lock().unwrap().as_ref() {
                    current
                        .accepting
                        .store(false, std::sync::atomic::Ordering::SeqCst);
                }
                rx.close();
                while let Ok(command) = rx.try_recv() {
                    match command {
                        Command::Enqueue(q) => {
                            completed.push((q.reply, Ok(not_started("Locked Use ended"))))
                        }
                        Command::Execute { reply, .. } => {
                            let _ = reply.send(Err(Error::Conflict("Locked Use ended".into())));
                        }
                        Command::Consume { reply, .. } => {
                            let _ = reply.send(None);
                        }
                        Command::Renew { reply, .. } | Command::Extend { reply, .. } => {
                            let _ = reply.send(Err(Error::Conflict("Locked Use ended".into())));
                        }
                        _ => {}
                    }
                }
                for q in queue.drain(..) {
                    completed.push((q.reply, Ok(not_started("Locked Use ended"))));
                }
                // Aborting an async task does not abort its blocking AX work.
                // Keep the console reservation and cover until every queued
                // OS call has actually exited, even after expiry/cancellation.
                close_terminal();
                if !work.is_idle() || terminal_busy() {
                    if o.console == Some(challenge.console()) {
                        os.lock();
                    }
                    if let Some(g) = guard.as_mut() {
                        g.lock();
                    }
                    continue;
                }
                resolve_stopped(&mut stopped, &mut outcome, &mut completed);
                // Join the OS task before releasing this transaction's cover
                // and console ownership. No delayed AX submit may outlive it.
                stop_unlock(&mut unlock).await;
                if let Some(g) = guard.as_mut() {
                    if let Err(e) = g.release() {
                        outcome.get_or_insert_with(|| Err(Error::Unavailable(e)));
                        log::warn!("Locked Use: display guard release failed");
                    } else {
                        log::info!("Locked Use: display guard released after gate cleanup");
                    }
                }
                if let Some(reply) = pending.take() {
                    let _ = reply.send(outcome.unwrap_or_else(|| {
                        Err(Error::Unavailable(
                            "Locked Use ended before execution".into(),
                        ))
                    }));
                }
                for (reply, result) in completed {
                    let _ = reply.send(result);
                }
                *owner.current.lock().unwrap() = None;
                drop(owner);
                return;
            }
            _ => {}
        }
        if action.as_ref().is_some_and(|a| a.is_finished()) {
            action_finished = true;
            action_cancel = None;
            action_delivery = None;
            let result = match action.take().unwrap().await {
                Ok(result) => result,
                Err(e) => Ok(unknown_result(format!("operation outcome unknown: {e}"))),
            };
            log::info!("Locked Use: one provider operation finished; waiting for queue drain");
            if let Some(reply) = action_reply.take() {
                completed.push((reply, result));
            } else {
                outcome = Some(result);
            }
        }
        if action_finished
            && action.is_none()
            && work.is_idle()
            && !release_pending
            && gate.phase() == Phase::Active
        {
            if let Some(next) = queue.pop_front() {
                if next.cancel.is_cancelled() {
                    completed.push((
                        next.reply,
                        Ok(not_started("request cancelled before execution")),
                    ));
                } else {
                    let actual = crate::handler::native_provider(&daemon, &next.request);
                    let provider = match actual {
                        Ok(p) => p,
                        Err(error) => {
                            completed.push((next.reply, Ok(not_started(&error.to_string()))));
                            continue;
                        }
                    };
                    match gate.take_queued_action(
                        peer,
                        &provider,
                        daemon.roster().read().unwrap().state(),
                        &daemon.policy().read().unwrap(),
                        Instant::now(),
                        o,
                    ) {
                        Ok(()) => {
                            action_cancel = Some(next.cancel.clone());
                            action_reply = Some(next.reply);
                            let (job, delivery) = start_action(
                                daemon.clone(),
                                next.request,
                                peer,
                                if manual { Instant::now() } else { started },
                                os.clone(),
                                live.clone(),
                                o,
                                next.cancel,
                                work.clone(),
                                Some(provider.clone()),
                            );
                            action = Some(job);
                            action_delivery = Some(delivery);
                        }
                        Err(error) => {
                            completed.push((next.reply, Ok(not_started(&error.to_string()))))
                        }
                    }
                }
            } else if !manual {
                // Atomic empty-queue barrier against RPC enqueue. Commands
                // already accepted into the mailbox must be handled first.
                let current = owner.current.lock().unwrap();
                if rx.is_empty() {
                    if let Some(current) = current.as_ref() {
                        current
                            .accepting
                            .store(false, std::sync::atomic::Ordering::SeqCst);
                    }
                    log::info!(
                        "Locked Use: provider queue and AX work drained; relocking under cover"
                    );
                    live.store(false, std::sync::atomic::Ordering::SeqCst);
                    gate.stop(StopReason::Completed, Instant::now());
                }
            }
        }
        // Return the failure promptly while the independent owner keeps
        // quarantining late authorization. Never abandon cleanup with the RPC.
        let idle = work.is_idle();
        if idle {
            resolve_stopped(&mut stopped, &mut outcome, &mut completed);
            if manual && gate.phase() == Phase::Active {
                for (reply, result) in completed.drain(..) {
                    let _ = reply.send(result);
                }
            }
        }
        if gate.phase() == Phase::Relocking && idle && outcome.is_some() && pending.is_some() {
            if let Some(Err(error)) = outcome.as_ref() {
                let _ = pending
                    .take()
                    .unwrap()
                    .send(Err(Error::Unavailable(error.to_string())));
            }
        }
    }
}

pub(super) struct Stopped {
    reply: Option<Reply>,
    delivery: Option<muster_agent::control_guard::Delivery>,
    reason: StopReason,
}

/// Call only once the WorkQueue is idle: a stopped operation that never
/// reached a delivery-capable press was not sent; otherwise it is unknown.
pub(super) fn resolve_stopped(
    stopped: &mut Vec<Stopped>,
    outcome: &mut Option<Result<Resp, Error>>,
    completed: &mut Vec<(Reply, Result<Resp, Error>)>,
) {
    for s in stopped.drain(..) {
        let not_started_proven = s.delivery.as_ref().is_some_and(|d| d.not_started());
        let result = if not_started_proven {
            log::warn!(
                "Locked Use: operation stopped by {:?} before any provider submission; reported as not sent",
                s.reason
            );
            Ok(not_started(&format!(
                "Locked Use stopped before any provider submission ({:?}); nothing was sent, retry is safe",
                s.reason
            )))
        } else {
            log::warn!(
                "Locked Use: operation stopped by {:?} after a possible submission; outcome unknown",
                s.reason
            );
            Ok(unknown_result(format!(
                "Locked Use stopped during execution: {:?}; check before retrying",
                s.reason
            )))
        };
        match s.reply {
            Some(reply) => completed.push((reply, result)),
            None => *outcome = Some(result),
        }
    }
}

pub(super) fn not_started(message: &str) -> Resp {
    Resp::OpResult(v1::OpResult {
        status: v1::op_result::Status::Rejected as i32,
        message: format!("{message}; operation not started"),
        ..Default::default()
    })
}
#[allow(clippy::too_many_arguments)]
pub(super) fn start_action(
    daemon: Arc<Daemon>,
    req: Req,
    peer: DeviceId,
    started: Instant,
    os: Arc<dyn LockedUseOs>,
    live: Arc<std::sync::atomic::AtomicBool>,
    observation: Observation,
    cancel: CancellationToken,
    work: muster_agent::control_guard::WorkQueue,
    provider: Option<String>,
) -> (
    tokio::task::JoinHandle<Result<Resp, Error>>,
    muster_agent::control_guard::Delivery,
) {
    let delivery = muster_agent::control_guard::Delivery::default();
    if matches!(&req, Req::AgentSend(r) if !r.op_id.is_empty() && muster_agent::desktop_chat::app_of(&r.session).is_none())
    {
        delivery.set_receipt_budget(Duration::from_millis(500));
    }
    let scoped = delivery.clone();
    let checked_daemon = daemon.clone();
    let checked_request = req.clone();
    let check = Arc::new(move || {
        let sample = os.observe();
        live.load(std::sync::atomic::Ordering::SeqCst)
            && !cancel.is_cancelled()
            && started.elapsed() < MAX_TTL
            && sample.console == observation.console
            && sample.input_generation == observation.input_generation
            && sample.screen == muster_core::locked_use::Screen::Unlocked
            && provider.as_ref().is_none_or(|p| {
                crate::handler::native_provider(&checked_daemon, &checked_request)
                    .as_ref()
                    .ok()
                    == Some(p)
            })
            && muster_core::authorize(
                checked_daemon.roster().read().unwrap().state(),
                &peer,
                &muster_core::Permission::LockedControl,
                &checked_daemon.policy().read().unwrap(),
                &muster_core::TargetState { locked: true },
            ) == muster_core::Decision::NeedsPresence
            && crate::handler::locked_operation_permission(&checked_daemon, &checked_request)
                .is_ok_and(|permission| {
                    muster_core::authorize(
                        checked_daemon.roster().read().unwrap().state(),
                        &peer,
                        &permission,
                        &checked_daemon.policy().read().unwrap(),
                        &muster_core::TargetState { locked: true },
                    ) == muster_core::Decision::Allow
                })
    });
    let job = tokio::spawn(async move {
        // The delivery scope is inherited by the blocking worker, so presses
        // there are visible to the owner and to the operation's ledger row.
        scoped
            .scope(work.scope(muster_agent::control_guard::scope(check, async move {
                // Aborting this async caller must also cancel readiness/receipt
                // waits on the OS worker. A synchronous AX call still drains first,
                // and its WorkQueue permit keeps the cover in place until then.
                let (_cancel_worker, cancelled) = oneshot::channel::<()>();
                muster_agent::control_guard::spawn_blocking_async(async move {
                    muster_agent::control_guard::check()?;
                    // Creation may be inside an OS call when cancellation arrives.
                    // Keep the work permit until it drains; the owner retains cover.
                    if matches!(&req, Req::TerminalCreate(_)) {
                        return crate::handler::locked_operation(&daemon, req).await;
                    }
                    tokio::select! {
                        biased;
                        _ = cancelled => Err(Error::Unavailable("native operation cancelled".into())),
                        result = crate::handler::locked_operation(&daemon, req) => result,
                    }
                })
                .await
                .map_err(|error| {
                    Error::Unavailable(format!("native operation worker failed: {error}"))
                })?
            })))
            .await
    });
    (job, delivery)
}

pub(super) struct UnlockJob {
    cancel: CancellationToken,
    job: Option<tokio::task::JoinHandle<Result<(), String>>>,
}
impl UnlockJob {
    fn is_finished(&self) -> bool {
        self.job.as_ref().is_none_or(|job| job.is_finished())
    }
    async fn finish(mut self) -> Result<(), String> {
        match self.job.take().unwrap().await {
            Ok(result) => result,
            Err(error) => Err(format!("lock-screen submission outcome unknown: {error}")),
        }
    }
}
impl Drop for UnlockJob {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// AX queries in request_unlock are synchronous, including on a cold lock
/// screen. They must not run on the owner's single-thread heartbeat runtime.
/// Cancellation revokes every subsequent OS check; cleanup joins the worker
/// instead of detaching an in-flight native operation.
pub(super) fn start_unlock(os: Arc<dyn LockedUseOs>, check: UnlockCheck) -> UnlockJob {
    let cancel = CancellationToken::new();
    let stopped = cancel.clone();
    let checked = cancel.clone();
    let check: UnlockCheck = Arc::new(move || {
        if checked.is_cancelled() {
            return Err("lock-screen submission cancelled".into());
        }
        check()?;
        if checked.is_cancelled() {
            return Err("lock-screen submission cancelled".into());
        }
        Ok(())
    });
    let job = tokio::task::spawn_blocking(move || {
        owner_runtime().map_err(|e| e.to_string())?.block_on(async {
            tokio::select! {
                biased;
                _ = stopped.cancelled() => Err("lock-screen submission cancelled".into()),
                result = os.request_unlock(check) => result,
            }
        })
    });
    UnlockJob {
        cancel,
        job: Some(job),
    }
}

pub(super) async fn stop_unlock(job: &mut Option<UnlockJob>) {
    if let Some(job) = job.take() {
        job.cancel.cancel();
        let _ = job.finish().await;
    }
}
