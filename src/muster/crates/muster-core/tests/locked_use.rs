use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use muster_core::identity::sign_domain;
use muster_core::locked_use::*;
use muster_core::{
    Author, DeviceId, DeviceKind, DeviceRecord, DeviceUpdate, HwSigner, LocalPolicy, Roster, Scope,
    SoftSigner,
};

struct Fixture {
    admin: SoftSigner,
    presence: SoftSigner,
    phone: SoftSigner,
    peer: DeviceId,
    roster: Roster,
    policy: LocalPolicy,
    gate: Gate,
    now: Instant,
    observation: Observation,
}

#[test]
fn paired_phone_signs_without_presence_but_desktop_device_key_cannot() {
    for kind in [DeviceKind::Phone, DeviceKind::Desktop] {
        let mut f = Fixture::with_kind(kind);
        let c = f.issue();
        let signature = f.phone.sign(&c.signing_message()).unwrap();
        let result = f.gate.arm(
            f.roster.state(),
            &f.policy,
            f.peer,
            &c,
            &signature,
            f.now,
            f.observation,
        );
        assert_eq!(result.is_ok(), kind == DeviceKind::Phone);
        if kind == DeviceKind::Phone {
            // One signed challenge remains one-shot.
            assert!(
                f.gate
                    .arm(
                        f.roster.state(),
                        &f.policy,
                        f.peer,
                        &c,
                        &signature,
                        f.now,
                        f.observation
                    )
                    .is_err()
            );
            f.policy.allow_locked_control = false;
            f.poll();
            assert_eq!(f.gate.last_stop(), Some(StopReason::AuthorizationChanged));
            assert!(f.action(f.peer, "turn-1", &[9; 32]).is_err());
        }
    }
}

#[test]
fn desktop_without_presence_key_fails_before_challenge_but_phone_remains_supported() {
    for kind in [DeviceKind::Desktop, DeviceKind::Phone] {
        let mut f = Fixture::with_enrollment(kind, false);
        let result = f.gate.issue(
            f.roster.state(),
            &f.policy,
            f.peer,
            "codex",
            "turn-1",
            [9; 32],
            Duration::from_secs(10),
            f.now,
            f.observation,
        );
        if kind == DeviceKind::Desktop {
            let error = result.unwrap_err().to_string();
            assert!(error.contains("no enrolled Presence key"), "{error}");
        } else {
            assert!(result.is_ok());
        }
    }
}

impl Fixture {
    fn new() -> Self {
        Self::with_kind(DeviceKind::Phone)
    }

    fn with_kind(kind: DeviceKind) -> Self {
        Self::with_enrollment(kind, true)
    }

    fn with_enrollment(kind: DeviceKind, enrolled: bool) -> Self {
        let admin = SoftSigner::generate();
        let phone = SoftSigner::generate();
        let presence = SoftSigner::generate();
        let peer = phone.public_key().device_id();
        let mut roster = Roster::new();
        let genesis = Author {
            roster: &roster,
            signer: &admin,
        }
        .genesis(
            "home",
            (Ipv4Addr::new(192, 168, 14, 0), 24),
            DeviceRecord::new(admin.public_key(), "mac", DeviceKind::Desktop),
        )
        .unwrap();
        roster.insert(genesis).unwrap();
        let mut record = DeviceRecord::new(phone.public_key(), "requester", kind);
        record.presence_key = enrolled.then(|| presence.public_key());
        record.scopes.extend([
            Scope::Agents,
            Scope::AgentProvider("codex".into()),
            Scope::LockedControl,
            Scope::Terminals,
        ]);
        let add = Author {
            roster: &roster,
            signer: &admin,
        }
        .add(record)
        .unwrap();
        roster.insert(add).unwrap();
        let now = Instant::now();
        Self {
            gate: Gate::new(admin.public_key().device_id()).unwrap(),
            admin,
            presence,
            phone,
            peer,
            roster,
            policy: LocalPolicy {
                allow_locked_control: true,
                ..Default::default()
            },
            now,
            observation: Observation {
                at: now,
                screen: Screen::Locked,
                console: Some(Console {
                    uid: 501,
                    audit_session: 100,
                }),
                input_generation: Some(7),
                cover_nonce: None,
                watchdog_nonce: None,
                connected: true,
            },
        }
    }

    fn tick(&mut self, millis: u64) {
        self.now += Duration::from_millis(millis);
        self.observation.at = self.now;
    }

    fn issue(&mut self) -> Challenge {
        self.gate
            .issue(
                self.roster.state(),
                &self.policy,
                self.peer,
                "codex",
                "turn-1",
                [9; 32],
                MAX_TTL,
                self.now,
                self.observation,
            )
            .unwrap()
    }

    fn arm(&mut self) -> Challenge {
        let challenge = self.issue();
        let signature = self.presence.sign(&challenge.signing_message()).unwrap();
        assert_eq!(
            self.gate
                .arm(
                    self.roster.state(),
                    &self.policy,
                    self.peer,
                    &challenge,
                    &signature,
                    self.now,
                    self.observation
                )
                .unwrap(),
            Directive::PrepareCoverAndWatchdog
        );
        self.observation.cover_nonce = Some(*challenge.nonce());
        self.observation.watchdog_nonce = Some(*challenge.nonce());
        challenge
    }

    fn consume(&mut self, c: &Challenge) -> Result<(), muster_core::Error> {
        self.gate.consume(
            c.nonce(),
            self.observation.console.unwrap(),
            self.roster.state(),
            &self.policy,
            self.now,
            self.observation,
        )
    }

    fn active(&mut self) -> Challenge {
        let c = self.arm();
        self.consume(&c).unwrap();
        self.gate
            .plugin_finished(c.nonce(), PluginOutcome::Allowed, self.now)
            .unwrap();
        self.tick(1);
        self.observation.screen = Screen::Unlocked;
        assert_eq!(self.poll(), Directive::ReadyForAction);
        c
    }

    fn poll(&mut self) -> Directive {
        self.gate.poll(
            self.roster.state(),
            &self.policy,
            self.now,
            self.observation,
        )
    }

    fn action(
        &mut self,
        peer: DeviceId,
        turn: &str,
        digest: &[u8; 32],
    ) -> Result<(), muster_core::Error> {
        self.gate.take_action(
            peer,
            turn,
            digest,
            self.roster.state(),
            &self.policy,
            self.now,
            self.observation,
        )
    }

    /// A terminal session signed with the phone's device key, unlocked and
    /// opened.
    fn terminal_active(&mut self) -> Challenge {
        let c = self
            .gate
            .issue_terminal(
                self.roster.state(),
                &self.policy,
                self.peer,
                "term-1",
                [7; 32],
                MAX_TTL,
                self.now,
                self.observation,
            )
            .unwrap();
        let signature = self.phone.sign(&c.signing_message()).unwrap();
        self.gate
            .arm(
                self.roster.state(),
                &self.policy,
                self.peer,
                &c,
                &signature,
                self.now,
                self.observation,
            )
            .unwrap();
        self.observation.cover_nonce = Some(*c.nonce());
        self.observation.watchdog_nonce = Some(*c.nonce());
        self.consume(&c).unwrap();
        self.gate
            .plugin_finished(c.nonce(), PluginOutcome::Allowed, self.now)
            .unwrap();
        self.tick(1);
        self.observation.screen = Screen::Unlocked;
        assert_eq!(self.poll(), Directive::ReadyForAction);
        self.action(self.peer, "term-1", &[7; 32]).unwrap();
        c
    }

    fn renew_with(&mut self, signer: &SoftSigner) -> Result<Instant, muster_core::Error> {
        let next = self.gate.renewal(self.peer, self.now)?;
        let signature = signer.sign(&next.signing_message()).unwrap();
        self.gate.renew(
            self.roster.state(),
            &self.policy,
            self.peer,
            &next,
            &signature,
            self.now,
            self.observation,
        )
    }

    fn renew(&mut self) -> Result<Instant, muster_core::Error> {
        let phone = SoftSigner::from_bytes(&self.phone.to_bytes()).unwrap();
        self.renew_with(&phone)
    }

    fn remove(&mut self, id: DeviceId) {
        let entry = Author {
            roster: &self.roster,
            signer: &self.admin,
        }
        .remove(id)
        .unwrap();
        self.roster.insert(entry).unwrap();
    }
}

#[test]
fn one_signed_turn_one_action_then_confirmed_relock() {
    let mut f = Fixture::new();
    let c = f.active();
    assert_eq!(Challenge::decode(&c.encode()).unwrap(), c);
    assert_eq!(c.turn(), "turn-1");
    assert_eq!(c.operation(), &[9; 32]);
    f.action(f.peer, "turn-1", &[9; 32]).unwrap();
    assert_eq!(f.poll(), Directive::MonitorAction);
    assert!(f.action(f.peer, "turn-1", &[9; 32]).is_err());
    assert_eq!(
        f.gate.stop(StopReason::Completed, f.now),
        Directive::LockAndKeepCovered(StopReason::Completed)
    );
    assert_eq!(
        f.poll(),
        Directive::LockAndKeepCovered(StopReason::Completed)
    );
    f.tick(1);
    f.observation.screen = Screen::Locked;
    assert_eq!(f.poll(), Directive::ReleaseCover);
    assert_eq!(f.gate.phase(), Phase::Idle);
    assert_eq!(f.poll(), Directive::None);
    let next = f.issue();
    assert_ne!(next.nonce(), c.nonce());
}

#[test]
fn authority_is_not_a_cover_boolean() {
    let mut f = Fixture::new();
    let c = f.arm();
    f.observation.cover_nonce = None;
    assert!(f.consume(&c).is_err());
    assert_eq!(f.gate.phase(), Phase::Relocking);
    assert!(f.action(f.peer, "turn-1", &[9; 32]).is_err());
}

#[test]
fn previous_cover_or_watchdog_ack_cannot_authorize_new_transaction() {
    for cover in [true, false] {
        let mut f = Fixture::new();
        let c = f.arm();
        if cover {
            f.observation.cover_nonce = Some([42; 32]);
        } else {
            f.observation.watchdog_nonce = Some([42; 32]);
        }
        assert!(f.consume(&c).is_err());
    }
}

#[test]
fn no_action_before_plugin_terminal_and_later_unlock_sample() {
    let mut f = Fixture::new();
    let c = f.arm();
    assert!(f.action(f.peer, "turn-1", &[9; 32]).is_err());
    f.consume(&c).unwrap();
    f.observation.screen = Screen::Unlocked;
    assert_eq!(f.poll(), Directive::WaitForUnlock);
    assert!(f.action(f.peer, "turn-1", &[9; 32]).is_err());
    f.gate
        .plugin_finished(c.nonce(), PluginOutcome::Allowed, f.now)
        .unwrap();
    assert_eq!(f.poll(), Directive::WaitForUnlock);
    f.tick(1);
    assert_eq!(f.poll(), Directive::ReadyForAction);
}

#[test]
fn expired_grant_is_never_extended_by_signing_or_plugin_receipt() {
    for after_consume in [false, true] {
        let mut f = Fixture::new();
        let c = f.arm();
        if after_consume {
            f.consume(&c).unwrap();
        }
        f.tick(15_000);
        if after_consume {
            f.gate
                .plugin_finished(c.nonce(), PluginOutcome::Allowed, f.now)
                .unwrap();
        } else {
            assert!(f.consume(&c).is_err());
        }
        assert_eq!(
            f.poll(),
            if after_consume {
                Directive::LockAndKeepCovered(StopReason::Expired)
            } else {
                Directive::ReleaseCover
            }
        );
        assert!(f.action(f.peer, "turn-1", &[9; 32]).is_err());
    }
}

#[test]
fn delayed_presence_is_rejected_at_exact_deadline() {
    let mut f = Fixture::new();
    let c = f.issue();
    let sig = f.presence.sign(&c.signing_message()).unwrap();
    f.tick(15_000);
    assert!(
        f.gate
            .arm(
                f.roster.state(),
                &f.policy,
                f.peer,
                &c,
                &sig,
                f.now,
                f.observation
            )
            .is_err()
    );
    assert_eq!(f.gate.phase(), Phase::Relocking);
}

#[test]
fn wrong_signer_domain_principal_and_every_changed_claim_are_rejected() {
    let mut f = Fixture::new();
    let c = f.issue();
    let good = f.presence.sign(&c.signing_message()).unwrap();
    let other = SoftSigner::generate();
    let bad_signer = other.sign(&c.signing_message()).unwrap();
    let wrong_domain = sign_domain(&f.presence, "MUSTER-PRESENCE-V1", &c.encode()).unwrap();
    for sig in [&bad_signer, &wrong_domain, &vec![0; 81]] {
        assert!(
            f.gate
                .arm(
                    f.roster.state(),
                    &f.policy,
                    f.peer,
                    &c,
                    sig,
                    f.now,
                    f.observation
                )
                .is_err()
        );
    }
    assert!(
        f.gate
            .arm(
                f.roster.state(),
                &f.policy,
                other.public_key().device_id(),
                &c,
                &good,
                f.now,
                f.observation
            )
            .is_err()
    );
    // Includes epoch, nonce, network, target, caller, console, turn, digest,
    // TTL and codec framing; even a correctly re-signed alteration must fail.
    let bytes = c.encode();
    for index in 0..bytes.len() {
        let mut changed = bytes.clone();
        changed[index] ^= 1;
        if let Ok(changed) = Challenge::decode(&changed) {
            let sig = f.presence.sign(&changed.signing_message()).unwrap();
            assert!(
                f.gate
                    .arm(
                        f.roster.state(),
                        &f.policy,
                        f.peer,
                        &changed,
                        &sig,
                        f.now,
                        f.observation
                    )
                    .is_err(),
                "changed byte {index}"
            );
        }
    }
    f.gate
        .arm(
            f.roster.state(),
            &f.policy,
            f.peer,
            &c,
            &good,
            f.now,
            f.observation,
        )
        .unwrap();
}

#[test]
fn restart_and_another_target_cannot_restore_authority() {
    let mut f = Fixture::new();
    let old = f.issue();
    let signature = f.presence.sign(&old.signing_message()).unwrap();
    f.gate = Gate::new(f.admin.public_key().device_id()).unwrap();
    let next = f.issue();
    assert_ne!(old, next);
    assert!(
        f.gate
            .arm(
                f.roster.state(),
                &f.policy,
                f.peer,
                &old,
                &signature,
                f.now,
                f.observation
            )
            .is_err()
    );
    f.gate = Gate::new(f.peer).unwrap();
    assert!(
        f.gate
            .arm(
                f.roster.state(),
                &f.policy,
                f.peer,
                &old,
                &signature,
                f.now,
                f.observation
            )
            .is_err()
    );
}

#[test]
fn consume_is_single_use_and_checks_authorization_user_and_session() {
    let mut f = Fixture::new();
    let c = f.arm();
    for console in [
        Console {
            uid: 502,
            audit_session: 100,
        },
        Console {
            uid: 501,
            audit_session: 101,
        },
    ] {
        assert!(
            f.gate
                .consume(
                    c.nonce(),
                    console,
                    f.roster.state(),
                    &f.policy,
                    f.now,
                    f.observation
                )
                .is_err()
        );
    }
    assert!(
        f.gate
            .consume(
                &[1; 32],
                f.observation.console.unwrap(),
                f.roster.state(),
                &f.policy,
                f.now,
                f.observation
            )
            .is_err()
    );
    f.consume(&c).unwrap();
    assert!(f.consume(&c).is_err());
}

#[test]
fn wrong_turn_operation_or_requester_cannot_use_an_active_console() {
    let mut f = Fixture::new();
    f.active();
    assert!(f.action(f.peer, "turn-2", &[9; 32]).is_err());
    assert!(f.action(f.peer, "turn-1", &[8; 32]).is_err());
    assert!(f.action(DeviceId([0; 32]), "turn-1", &[9; 32]).is_err());
    f.action(f.peer, "turn-1", &[9; 32]).unwrap();
}

#[test]
fn live_observation_failures_revoke_action_authority() {
    type Change = fn(&mut Observation);
    let cases: &[(Change, StopReason)] = &[
        (|o| o.input_generation = Some(8), StopReason::LocalInput),
        (|o| o.input_generation = None, StopReason::ObservationLost),
        (|o| o.console = None, StopReason::ConsoleChanged),
        (
            |o| o.console.as_mut().unwrap().audit_session += 1,
            StopReason::ConsoleChanged,
        ),
        (|o| o.connected = false, StopReason::Disconnected),
        (|o| o.screen = Screen::Unknown, StopReason::ObservationLost),
        // A missing cover report is held for MAX_COVER_QUIET first; see
        // `a_briefly_unreported_cover_holds_the_transaction_instead_of_stopping_it`.
        (|o| o.watchdog_nonce = None, StopReason::WatchdogLost),
        (
            |o| o.screen = Screen::Locked,
            StopReason::UnexpectedScreenState,
        ),
    ];
    for (change, reason) in cases {
        let mut f = Fixture::new();
        f.active();
        change(&mut f.observation);
        assert_eq!(f.poll(), Directive::LockAndKeepCovered(*reason));
        assert!(f.action(f.peer, "turn-1", &[9; 32]).is_err());
    }
}

#[test]
fn a_briefly_unreported_cover_holds_the_transaction_instead_of_stopping_it() {
    let mut f = Fixture::new();
    let c = f.active();
    // The watchdog is alive and reports no loss; its ready status is only
    // older than one observation window, as every CoverLost on the owner's
    // Macs was. Nothing new is admitted while the cover is unreported.
    f.observation.cover_nonce = None;
    assert_eq!(f.poll(), Directive::WaitForUnlock);
    assert!(f.gate.cover_quiet());
    assert!(f.action(f.peer, "turn-1", &[9; 32]).is_err());
    assert_eq!(f.gate.phase(), Phase::Active);
    assert_eq!(f.gate.last_stop(), None);
    // A fresh report resumes the same transaction; nothing was replayed.
    f.tick(500);
    f.observation.cover_nonce = Some(*c.nonce());
    assert_eq!(f.poll(), Directive::ReadyForAction);
    assert!(!f.gate.cover_quiet());
    f.action(f.peer, "turn-1", &[9; 32]).unwrap();
}

#[test]
fn an_unreported_cover_that_outlasts_the_tolerance_stops_the_transaction() {
    let mut f = Fixture::new();
    f.active();
    f.observation.cover_nonce = None;
    assert_eq!(f.poll(), Directive::WaitForUnlock);
    f.tick(999);
    assert_eq!(f.poll(), Directive::WaitForUnlock);
    f.tick(1);
    assert_eq!(
        f.poll(),
        Directive::LockAndKeepCovered(StopReason::CoverLost)
    );
    assert!(f.action(f.peer, "turn-1", &[9; 32]).is_err());
}

#[test]
fn a_cover_that_never_reported_is_lost_at_once() {
    let mut f = Fixture::new();
    let c = f.arm();
    // Nothing ever carried this lease's nonce, so there is no stale evidence
    // to tolerate: the cover never came up.
    f.observation.cover_nonce = None;
    assert!(f.consume(&c).is_err());
    assert_eq!(f.gate.last_stop(), Some(StopReason::CoverLost));
}

#[test]
fn a_lost_watchdog_is_never_tolerated_as_a_quiet_cover() {
    let mut f = Fixture::new();
    f.active();
    f.observation.cover_nonce = None;
    f.observation.watchdog_nonce = None;
    assert_eq!(
        f.poll(),
        Directive::LockAndKeepCovered(StopReason::WatchdogLost)
    );
}

#[test]
fn stale_future_and_out_of_order_observations_fail_closed() {
    for offset in [-251_i64, 1, -1] {
        let mut f = Fixture::new();
        f.active();
        f.observation.at = if offset < 0 {
            f.now - Duration::from_millis((-offset) as u64)
        } else {
            f.now + Duration::from_millis(offset as u64)
        };
        assert_eq!(
            f.poll(),
            Directive::LockAndKeepCovered(StopReason::ObservationLost)
        );
    }
}

#[test]
fn scope_policy_and_member_revocation_apply_to_active_window() {
    for case in 0..7 {
        let mut f = Fixture::new();
        f.active();
        let narrowed = |scopes: Vec<Scope>| DeviceUpdate {
            scopes: Some(scopes.into_iter().collect()),
            ..Default::default()
        };
        let update = match case {
            0 => {
                f.policy.allow_locked_control = false;
                None
            }
            1 => {
                f.policy.offer_agent = false;
                None
            }
            2 => {
                f.remove(f.peer);
                None
            }
            3 => {
                f.remove(f.admin.public_key().device_id());
                None
            }
            // Locked control withdrawn.
            4 => Some(narrowed(vec![
                Scope::Agents,
                Scope::AgentProvider("codex".into()),
            ])),
            // The operation's provider switched off; another stays on.
            5 => Some(narrowed(vec![
                Scope::Agents,
                Scope::AgentProvider("claude".into()),
                Scope::LockedControl,
            ])),
            // The Agents switch off, provider choice kept.
            _ => Some(narrowed(vec![
                Scope::AgentProvider("codex".into()),
                Scope::LockedControl,
            ])),
        };
        if let Some(update) = update {
            let entry = Author {
                roster: &f.roster,
                signer: &f.admin,
            }
            .update(f.peer, update)
            .unwrap();
            f.roster.insert(entry).unwrap();
        }
        assert_eq!(
            f.poll(),
            Directive::LockAndKeepCovered(StopReason::AuthorizationChanged)
        );
    }
}

#[test]
fn cancellation_before_consume_withdraws_authority() {
    let mut f = Fixture::new();
    let c = f.arm();
    f.gate.stop(StopReason::Cancelled, f.now);
    assert!(f.consume(&c).is_err());
    // No plugin consumed authority: withdrawing must not lock a local user.
    assert_eq!(f.poll(), Directive::ReleaseCover);
    assert_eq!(f.poll(), Directive::None);
}

#[test]
fn an_unsigned_challenge_keeps_its_stop_cause_after_returning_to_idle() {
    let mut f = Fixture::new();
    f.issue();
    assert_eq!(f.gate.last_stop(), None);
    // Local input before any signature withdraws the challenge in one poll.
    f.observation.input_generation = Some(8);
    assert_eq!(f.poll(), Directive::ReleaseCover);
    assert_eq!(f.gate.phase(), Phase::Idle);
    assert_eq!(f.gate.last_stop(), Some(StopReason::LocalInput));
    // A later stop of the same lease keeps the first cause.
    f.gate.stop(StopReason::Expired, f.now);
    assert_eq!(f.gate.last_stop(), Some(StopReason::LocalInput));
}

#[test]
fn cancel_reasons_round_trip_and_unknown_text_is_not_a_reason() {
    for reason in [
        CancelReason::User,
        CancelReason::Authentication,
        CancelReason::Expired,
        CancelReason::Busy,
        CancelReason::Background,
        CancelReason::Terminating,
    ] {
        assert_eq!(CancelReason::parse(reason.as_str()), Some(reason));
    }
    for text in ["", "USER", "user ", "granted"] {
        assert_eq!(CancelReason::parse(text), None, "{text:?}");
    }
}

#[test]
fn late_allow_after_cancellation_requires_unlock_then_relock() {
    let mut f = Fixture::new();
    let c = f.arm();
    f.consume(&c).unwrap();
    f.gate.stop(StopReason::Cancelled, f.now);
    f.tick(1);
    assert_eq!(
        f.poll(),
        Directive::LockAndKeepCovered(StopReason::Cancelled)
    );
    f.gate
        .plugin_finished(c.nonce(), PluginOutcome::Allowed, f.now)
        .unwrap();
    f.tick(1);
    // Even after terminal, a still-locked sample can precede the late unlock.
    assert_eq!(
        f.poll(),
        Directive::LockAndKeepCovered(StopReason::Cancelled)
    );
    f.tick(1);
    f.observation.screen = Screen::Unlocked;
    assert_eq!(
        f.poll(),
        Directive::LockAndKeepCovered(StopReason::Cancelled)
    );
    assert!(f.action(f.peer, "turn-1", &[9; 32]).is_err());
    f.tick(1);
    f.observation.screen = Screen::Locked;
    assert_eq!(f.poll(), Directive::ReleaseCover);
}

#[test]
fn lost_plugin_terminal_remains_quarantined_even_after_expiry() {
    let mut f = Fixture::new();
    let c = f.arm();
    f.consume(&c).unwrap();
    f.tick(60_000);
    assert_eq!(f.poll(), Directive::LockAndKeepCovered(StopReason::Expired));
    f.tick(60_000);
    assert_eq!(f.poll(), Directive::LockAndKeepCovered(StopReason::Expired));
    assert!(
        f.gate
            .issue(
                f.roster.state(),
                &f.policy,
                f.peer,
                "codex",
                "new-turn",
                [9; 32],
                MAX_TTL,
                f.now,
                f.observation
            )
            .is_err()
    );
    assert!(
        f.gate
            .plugin_finished(&[0; 32], PluginOutcome::Denied, f.now)
            .is_err()
    );
    assert_eq!(f.gate.phase(), Phase::Relocking);
}

#[test]
fn known_plugin_denial_needs_new_lock_observation_and_duplicate_is_rejected() {
    let mut f = Fixture::new();
    let c = f.arm();
    f.consume(&c).unwrap();
    f.gate
        .plugin_finished(c.nonce(), PluginOutcome::Denied, f.now)
        .unwrap();
    assert!(
        f.gate
            .plugin_finished(c.nonce(), PluginOutcome::Allowed, f.now)
            .is_err()
    );
    assert_eq!(
        f.poll(),
        Directive::LockAndKeepCovered(StopReason::PluginDenied)
    );
    f.tick(1);
    assert_eq!(f.poll(), Directive::ReleaseCover);
}

#[test]
fn cleanup_never_releases_cover_for_another_console_or_unknown_state() {
    let mut f = Fixture::new();
    f.active();
    f.gate.stop(StopReason::Completed, f.now);
    f.tick(1);
    f.observation.screen = Screen::Unknown;
    assert_eq!(
        f.poll(),
        Directive::LockAndKeepCovered(StopReason::Completed)
    );
    f.tick(1);
    f.observation.screen = Screen::Locked;
    f.observation.console.as_mut().unwrap().uid = 502;
    assert_eq!(
        f.poll(),
        Directive::LockAndKeepCovered(StopReason::Completed)
    );
}

#[test]
fn outside_unlock_before_consume_withdraws_pending_grant() {
    let mut f = Fixture::new();
    let c = f.arm();
    f.observation.screen = Screen::Unlocked;
    assert!(f.consume(&c).is_err());
    assert_eq!(f.gate.phase(), Phase::Relocking);
}

#[test]
fn malformed_challenges_never_parse_and_ttl_is_bounded() {
    let mut f = Fixture::new();
    let bytes = f.issue().encode();
    for end in 0..bytes.len() {
        assert!(Challenge::decode(&bytes[..end]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(Challenge::decode(&trailing).is_err());
    assert!(Challenge::decode(&[0; 513]).is_err());
    for ttl in [0, 4999, 15001, u64::MAX] {
        let mut changed = bytes.clone();
        let end = changed.len();
        changed[end - 8..].copy_from_slice(&ttl.to_be_bytes());
        assert!(Challenge::decode(&changed).is_err());
    }
    for ttl in [
        Duration::from_secs(4),
        Duration::from_secs(16),
        Duration::from_nanos(5_000_000_001),
    ] {
        let mut f = Fixture::new();
        assert!(
            f.gate
                .issue(
                    f.roster.state(),
                    &f.policy,
                    f.peer,
                    "codex",
                    "turn-1",
                    [9; 32],
                    ttl,
                    f.now,
                    f.observation
                )
                .is_err()
        );
    }
}

#[test]
fn disabled_policy_missing_scopes_and_bad_console_cannot_issue_challenge() {
    for case in 0..7 {
        let mut f = Fixture::new();
        match case {
            0 => f.policy.allow_locked_control = false,
            1 => f.observation.screen = Screen::Unknown,
            2 => f.observation.screen = Screen::Unlocked,
            3 => f.observation.console.as_mut().unwrap().uid = 0,
            4 => f.observation.console.as_mut().unwrap().audit_session = u32::MAX,
            5 => f.observation.connected = false,
            _ => f.remove(f.peer),
        }
        assert!(
            f.gate
                .issue(
                    f.roster.state(),
                    &f.policy,
                    f.peer,
                    "codex",
                    "turn-1",
                    [9; 32],
                    MAX_TTL,
                    f.now,
                    f.observation
                )
                .is_err()
        );
        assert_eq!(f.gate.phase(), Phase::Idle);
    }
}

#[test]
fn phone_validates_challenge_before_requesting_presence_signature() {
    let mut f = Fixture::new();
    let c = f.issue();
    let network = f.roster.network_id().unwrap().0;
    let target = f.admin.public_key().device_id();
    assert_eq!(
        c.signing_message_for(&network, target, f.peer, "turn-1", &[9; 32])
            .unwrap(),
        c.signing_message()
    );
    for case in 0..5 {
        let result = c.signing_message_for(
            if case == 0 { &[0; 32] } else { &network },
            if case == 1 { f.peer } else { target },
            if case == 2 { target } else { f.peer },
            if case == 3 { "turn-2" } else { "turn-1" },
            if case == 4 { &[0; 32] } else { &[9; 32] },
        );
        assert!(result.is_err());
    }
}

#[test]
fn concurrent_plugin_invocations_have_exactly_one_winner() {
    let mut f = Fixture::new();
    let c = f.arm();
    let gate = std::sync::Mutex::new(f.gate);
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let call = || {
            barrier.wait();
            gate.lock()
                .unwrap()
                .consume(
                    c.nonce(),
                    f.observation.console.unwrap(),
                    f.roster.state(),
                    &f.policy,
                    f.now,
                    f.observation,
                )
                .is_ok()
        };
        let a = scope.spawn(call);
        let b = scope.spawn(call);
        assert_eq!(a.join().unwrap() as u8 + b.join().unwrap() as u8, 1);
    });
}

#[test]
fn either_missing_scope_refuses_admission() {
    for scopes in [
        vec![Scope::Agents, Scope::AgentProvider("codex".into())],
        vec![Scope::LockedControl],
        vec![
            Scope::Agents,
            Scope::AgentProvider("claude".into()),
            Scope::LockedControl,
        ],
        vec![Scope::AgentProvider("codex".into()), Scope::LockedControl],
        vec![],
    ] {
        let mut f = Fixture::new();
        let entry = Author {
            roster: &f.roster,
            signer: &f.admin,
        }
        .update(
            f.peer,
            DeviceUpdate {
                scopes: Some(scopes.into_iter().collect()),
                ..Default::default()
            },
        )
        .unwrap();
        f.roster.insert(entry).unwrap();
        assert!(
            f.gate
                .issue(
                    f.roster.state(),
                    &f.policy,
                    f.peer,
                    "codex",
                    "turn-1",
                    [9; 32],
                    MAX_TTL,
                    f.now,
                    f.observation
                )
                .is_err()
        );
    }
}

#[test]
fn regressed_plugin_timestamp_cannot_create_execution_or_cleanup_evidence() {
    let mut f = Fixture::new();
    let c = f.arm();
    f.consume(&c).unwrap();
    assert!(
        f.gate
            .plugin_finished(
                c.nonce(),
                PluginOutcome::Allowed,
                f.now - Duration::from_millis(1)
            )
            .is_err()
    );
    assert_eq!(f.gate.phase(), Phase::Authorizing);
}

proptest::proptest! {
    #[test]
    fn arbitrary_challenge_bytes_do_not_panic(bytes in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..1024)) {
        if let Ok(c) = Challenge::decode(&bytes) {
            proptest::prop_assert_eq!(c.encode(), bytes);
        }
    }
}

#[test]
fn unsigned_challenge_never_relocks_a_local_unlock() {
    let mut f = Fixture::new();
    f.issue();
    f.tick(1);
    f.observation.screen = Screen::Unlocked;
    assert_eq!(f.poll(), Directive::ReleaseCover);
    assert_eq!(f.gate.phase(), Phase::Idle);
}

#[test]
fn provider_queue_scope_cannot_be_widened_and_each_admission_rechecks_authority() {
    let mut f = Fixture::new();
    let c = f.issue();
    assert!(f.gate.bind_provider_queue("claude").is_err());
    f.gate.bind_provider_queue("codex").unwrap();
    assert!(f.gate.bind_provider_queue("claude").is_err());
    f.gate
        .arm(
            f.roster.state(),
            &f.policy,
            f.peer,
            &c,
            &f.presence.sign(&c.signing_message()).unwrap(),
            f.now,
            f.observation,
        )
        .unwrap();
    assert!(f.gate.bind_provider_queue("claude").is_err());
    f.observation.cover_nonce = Some(*c.nonce());
    f.observation.watchdog_nonce = Some(*c.nonce());
    f.consume(&c).unwrap();
    f.gate
        .plugin_finished(c.nonce(), PluginOutcome::Allowed, f.now)
        .unwrap();
    f.tick(1);
    f.observation.screen = Screen::Unlocked;
    f.gate
        .take_action(
            f.peer,
            "turn-1",
            &[9; 32],
            f.roster.state(),
            &f.policy,
            f.now,
            f.observation,
        )
        .unwrap();
    for _ in 0..3 {
        f.tick(1);
        f.gate
            .take_queued_action(
                f.peer,
                "codex",
                f.roster.state(),
                &f.policy,
                f.now,
                f.observation,
            )
            .unwrap();
    }
    assert!(
        f.gate
            .take_queued_action(
                f.peer,
                "claude",
                f.roster.state(),
                &f.policy,
                f.now,
                f.observation
            )
            .is_err()
    );
    assert!(
        f.gate
            .take_queued_action(
                f.admin.public_key().device_id(),
                "codex",
                f.roster.state(),
                &f.policy,
                f.now,
                f.observation
            )
            .is_err()
    );
    f.policy.allow_locked_control = false;
    assert!(
        f.gate
            .take_queued_action(
                f.peer,
                "codex",
                f.roster.state(),
                &f.policy,
                f.now,
                f.observation
            )
            .is_err()
    );
    assert_eq!(f.gate.phase(), Phase::Relocking);
}

#[test]
fn ordinary_single_operation_does_not_grant_a_provider_queue() {
    let mut f = Fixture::new();
    let c = f.active();
    f.gate
        .take_action(
            f.peer,
            c.turn(),
            c.operation(),
            f.roster.state(),
            &f.policy,
            f.now,
            f.observation,
        )
        .unwrap();
    assert!(
        f.gate
            .take_queued_action(
                f.peer,
                "codex",
                f.roster.state(),
                &f.policy,
                f.now,
                f.observation
            )
            .is_err()
    );
}

#[test]
fn historical_watchdog_unlock_only_settles_cleanup_after_terminal_and_new_lock() {
    for terminal in [false, true] {
        let mut f = Fixture::new();
        let c = f.arm();
        f.consume(&c).unwrap();
        f.tick(1);
        let witness = Observation {
            screen: Screen::Unlocked,
            ..f.observation
        };
        f.gate.stop(StopReason::WatchdogLost, f.now);
        f.tick(500); // Even old evidence can settle cleanup, never authorize.
        f.gate.witnessed_unlock(c.nonce(), witness, f.now).unwrap();
        assert!(f.action(f.peer, "turn-1", &[9; 32]).is_err());
        assert_eq!(
            f.poll(),
            Directive::LockAndKeepCovered(StopReason::WatchdogLost)
        );
        if terminal {
            f.gate
                .plugin_finished(c.nonce(), PluginOutcome::Allowed, f.now)
                .unwrap();
            f.tick(1);
            assert_eq!(f.poll(), Directive::ReleaseCover);
        } else {
            f.tick(60_000);
            assert_eq!(
                f.poll(),
                Directive::LockAndKeepCovered(StopReason::WatchdogLost)
            );
        }
    }
}

#[test]
fn watchdog_witness_rejects_other_transaction_console_input_and_times() {
    let mut f = Fixture::new();
    let c = f.arm();
    let before = Observation {
        screen: Screen::Unlocked,
        ..f.observation
    };
    assert!(f.gate.witnessed_unlock(c.nonce(), before, f.now).is_err());
    f.tick(1);
    f.consume(&c).unwrap();
    assert!(f.gate.witnessed_unlock(c.nonce(), before, f.now).is_err());
    f.tick(1);
    let witness = Observation {
        screen: Screen::Unlocked,
        ..f.observation
    };
    assert!(f.gate.witnessed_unlock(&[0; 32], witness, f.now).is_err());
    for invalid in [
        Observation {
            console: None,
            ..witness
        },
        Observation {
            input_generation: Some(8),
            ..witness
        },
        Observation {
            screen: Screen::Locked,
            ..witness
        },
        Observation {
            at: f.now + Duration::from_millis(1),
            ..witness
        },
    ] {
        assert!(f.gate.witnessed_unlock(c.nonce(), invalid, f.now).is_err());
    }
    let phase = f.gate.phase();
    f.gate.witnessed_unlock(c.nonce(), witness, f.now).unwrap();
    assert_eq!(f.gate.phase(), phase);
    assert!(f.action(f.peer, "turn-1", &[9; 32]).is_err());
}

#[test]
fn desktop_inspection_is_signed_scoped_single_use_and_revocable() {
    let mut f = Fixture::with_kind(DeviceKind::Desktop);
    let c = f
        .gate
        .issue_inspection(
            f.roster.state(),
            &f.policy,
            f.peer,
            "codex",
            "inspect-1",
            [7; 32],
            MAX_TTL,
            f.now,
            f.observation,
        )
        .unwrap();
    assert!(f.gate.bind_provider_queue("codex").is_err());
    let wrong = SoftSigner::generate().sign(&c.signing_message()).unwrap();
    assert!(
        f.gate
            .arm(
                f.roster.state(),
                &f.policy,
                f.peer,
                &c,
                &wrong,
                f.now,
                f.observation
            )
            .is_err()
    );
    let signature = f.phone.sign(&c.signing_message()).unwrap();
    f.gate
        .arm(
            f.roster.state(),
            &f.policy,
            f.peer,
            &c,
            &signature,
            f.now,
            f.observation,
        )
        .unwrap();
    assert!(
        f.gate
            .arm(
                f.roster.state(),
                &f.policy,
                f.peer,
                &c,
                &signature,
                f.now,
                f.observation
            )
            .is_err()
    );
    // Inspection cannot turn into a write with a different signed digest.
    assert!(f.action(f.peer, "inspect-1", &[9; 32]).is_err());
    f.policy.allow_locked_control = false;
    f.poll();
    assert_eq!(f.gate.last_stop(), Some(StopReason::AuthorizationChanged));
}

#[test]
fn a_terminal_session_stays_unlocked_only_while_its_requester_renews_it() {
    let mut f = Fixture::new();
    let first = f.terminal_active();
    assert_eq!(f.poll(), Directive::MonitorAction);
    // Each signed window ends 15 s after its own issue, never later.
    for _ in 0..4 {
        f.tick(10_000);
        f.renew().unwrap();
        assert_eq!(f.poll(), Directive::MonitorAction);
    }
    // 40 s in: far past the first window, still one session.
    assert_eq!(f.gate.phase(), Phase::Active);
    let next = f.gate.renewal(f.peer, f.now).unwrap();
    assert_ne!(next.nonce(), first.nonce());
    assert_eq!((next.turn(), next.operation()), ("term-1", &[7; 32]));
    // Nobody signs: the window lapses and the console relocks.
    f.tick(15_001);
    assert_eq!(f.poll(), Directive::LockAndKeepCovered(StopReason::Expired));
    assert!(f.renew().is_err());
}

#[test]
fn a_renewal_runs_from_its_issue_and_is_signed_once_by_the_requester() {
    let mut f = Fixture::new();
    f.terminal_active();
    f.tick(5_000);
    let next = f.gate.renewal(f.peer, f.now).unwrap();
    // A slow signature does not move the deadline past issue + TTL.
    f.tick(9_000);
    let signature = f.phone.sign(&next.signing_message()).unwrap();
    let renew = |f: &mut Fixture, c: &Challenge, s: &[u8]| {
        f.gate.renew(
            f.roster.state(),
            &f.policy,
            f.peer,
            c,
            s,
            f.now,
            f.observation,
        )
    };
    let deadline = renew(&mut f, &next, &signature).unwrap();
    assert_eq!(deadline, f.now - Duration::from_millis(9_000) + MAX_TTL);
    // Used once; another signer, another requester or another session
    // cannot renew.
    assert!(renew(&mut f, &next, &signature).is_err());
    let stranger = SoftSigner::generate();
    assert!(f.renew_with(&stranger).is_err());
    let other = stranger.public_key().device_id();
    assert!(f.gate.renewal(other, f.now).is_err());
    assert_eq!(f.gate.phase(), Phase::Active);
    // Local input on the Mac ends the session at the renewal check.
    f.observation.input_generation = Some(8);
    assert!(f.renew().is_err());
    assert_eq!(f.gate.last_stop(), Some(StopReason::LocalInput));
}

#[test]
fn an_agent_operation_cannot_be_renewed() {
    let mut f = Fixture::new();
    f.active();
    f.action(f.peer, "turn-1", &[9; 32]).unwrap();
    assert!(f.gate.renewal(f.peer, f.now).is_err());
}

#[test]
fn a_terminal_session_ends_after_ten_minutes_without_input() {
    let mut f = Fixture::new();
    f.terminal_active();
    let mut elapsed = 0;
    while elapsed < 300_000 {
        f.tick(10_000);
        elapsed += 10_000;
        f.renew().unwrap();
    }
    // Input at five minutes restarts the ten.
    f.gate.note_input(f.peer, f.now);
    // Someone else's input does not.
    f.gate.note_input(
        SoftSigner::generate().public_key().device_id(),
        f.now + Duration::from_secs(500),
    );
    while elapsed < 890_000 {
        f.tick(10_000);
        elapsed += 10_000;
        f.renew().unwrap();
        assert_eq!(f.poll(), Directive::MonitorAction);
    }
    f.tick(10_000);
    assert!(f.renew().is_err());
    assert_eq!(f.gate.last_stop(), Some(StopReason::Idle));
}

#[test]
fn a_terminal_session_needs_terminal_control() {
    let mut f = Fixture::new();
    f.policy.offer_terminal = false;
    let refused = f.gate.issue_terminal(
        f.roster.state(),
        &f.policy,
        f.peer,
        "term-1",
        [7; 32],
        MAX_TTL,
        f.now,
        f.observation,
    );
    assert!(
        refused
            .unwrap_err()
            .to_string()
            .contains("Terminal control")
    );
}

impl Fixture {
    fn manual_active(&mut self, unlocked: bool) -> Challenge {
        if unlocked {
            self.observation.screen = Screen::Unlocked;
        }
        let c = self
            .gate
            .issue_device(
                self.roster.state(),
                &self.policy,
                self.peer,
                "manual-1",
                [8; 32],
                MAX_TTL,
                self.now,
                self.observation,
            )
            .unwrap();
        assert!(
            self.gate
                .maintain_device(
                    self.roster.state(),
                    &self.policy,
                    self.now,
                    self.observation
                )
                .is_err()
        );
        let signature = self.phone.sign(&c.signing_message()).unwrap();
        self.gate
            .arm(
                self.roster.state(),
                &self.policy,
                self.peer,
                &c,
                &signature,
                self.now,
                self.observation,
            )
            .unwrap();
        assert_eq!(self.poll(), Directive::PrepareCoverAndWatchdog);
        self.observation.cover_nonce = Some(*c.nonce());
        assert_eq!(self.poll(), Directive::PrepareCoverAndWatchdog);
        self.observation.watchdog_nonce = Some(*c.nonce());
        if !unlocked {
            self.consume(&c).unwrap();
            self.gate
                .plugin_finished(c.nonce(), PluginOutcome::Allowed, self.now)
                .unwrap();
            self.tick(1);
            self.observation.screen = Screen::Unlocked;
        }
        assert_eq!(self.poll(), Directive::ReadyForAction);
        self.action(self.peer, "manual-1", &[8; 32]).unwrap();
        c
    }
}

#[test]
fn explicit_manual_cover_survives_idle_and_requires_lock_before_uncover() {
    for unlocked in [false, true] {
        let mut f = Fixture::new();
        f.policy.offer_terminal = false;
        let c = f.manual_active(unlocked);
        for _ in 0..300 {
            f.tick(3_000);
            assert!(
                f.gate
                    .maintain_device(f.roster.state(), &f.policy, f.now, f.observation)
                    .is_ok()
            );
            assert_eq!(f.poll(), Directive::MonitorAction);
        }
        // It does not become a terminal renewal or permit replay of its proof.
        assert!(f.gate.renewal(f.peer, f.now).is_err());
        assert!(f.action(f.peer, "manual-1", &[8; 32]).is_err());
        assert!(
            f.gate
                .arm(
                    f.roster.state(),
                    &f.policy,
                    f.peer,
                    &c,
                    &f.phone.sign(&c.signing_message()).unwrap(),
                    f.now,
                    f.observation
                )
                .is_err()
        );
        f.gate.stop(StopReason::Completed, f.now);
        assert_eq!(
            f.poll(),
            Directive::LockAndKeepCovered(StopReason::Completed)
        );
        assert!(
            f.gate
                .maintain_device(f.roster.state(), &f.policy, f.now, f.observation)
                .is_err()
        );
        f.tick(1);
        f.observation.screen = Screen::Locked;
        assert_eq!(f.poll(), Directive::ReleaseCover);
    }
}

#[test]
fn manual_lease_keeps_provider_and_owner_authorization_separate() {
    let mut f = Fixture::new();
    f.manual_active(false);
    assert!(
        f.gate
            .take_queued_action(
                f.peer,
                "codex",
                f.roster.state(),
                &f.policy,
                f.now,
                f.observation
            )
            .is_ok()
    );
    assert!(
        f.gate
            .take_queued_action(
                f.admin.public_key().device_id(),
                "codex",
                f.roster.state(),
                &f.policy,
                f.now,
                f.observation
            )
            .is_err()
    );
    assert!(
        f.gate
            .take_queued_action(
                f.peer,
                "claude",
                f.roster.state(),
                &f.policy,
                f.now,
                f.observation
            )
            .is_err()
    );
    f.policy.allow_locked_control = false;
    assert!(
        f.gate
            .maintain_device(f.roster.state(), &f.policy, f.now, f.observation)
            .is_err()
    );
    assert_eq!(f.gate.last_stop(), Some(StopReason::AuthorizationChanged));
}

#[test]
fn manual_maintenance_cannot_revive_expired_or_uncovered_console() {
    for failure in ["expired", "cover", "watchdog", "input", "console", "stale"] {
        let mut f = Fixture::new();
        f.manual_active(true);
        match failure {
            "expired" => f.tick(15_000),
            "cover" => {
                // Held on the first quiet tick, stopped once the gap outlasts
                // MAX_COVER_QUIET. Neither tick revives anything.
                f.observation.cover_nonce = None;
                f.gate
                    .maintain_device(f.roster.state(), &f.policy, f.now, f.observation)
                    .unwrap();
                assert!(f.gate.cover_quiet());
                f.tick(1_000);
            }
            "watchdog" => f.observation.watchdog_nonce = None,
            "input" => f.observation.input_generation = Some(8),
            "console" => f.observation.console.as_mut().unwrap().audit_session += 1,
            "stale" => f.now += Duration::from_secs(3),
            _ => unreachable!(),
        }
        assert!(
            f.gate
                .maintain_device(f.roster.state(), &f.policy, f.now, f.observation)
                .is_err(),
            "{failure}"
        );
        assert_eq!(f.gate.phase(), Phase::Relocking, "{failure}");
    }
    let mut f = Fixture::new();
    f.active();
    assert!(
        f.gate
            .maintain_device(f.roster.state(), &f.policy, f.now, f.observation)
            .is_err()
    );
    let mut f = Fixture::new();
    f.terminal_active();
    assert!(
        f.gate
            .maintain_device(f.roster.state(), &f.policy, f.now, f.observation)
            .is_err()
    );
}
