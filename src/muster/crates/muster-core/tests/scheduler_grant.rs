use muster_core::scheduler_grant::*;

const DIR: &str = "/work/project";

fn scope(task: &str) -> Scope {
    Scope {
        task_id: task.into(),
        contract_sha256: "ab".repeat(32),
        provider: "claude".into(),
        directory: DIR.into(),
        model: "Model A".into(),
        effort: "High".into(),
    }
}

fn request<'a>(op_id: &'a str, kind: Kind, role: Role, session: &'a str) -> Request<'a> {
    Request {
        op_id,
        kind,
        role,
        provider: "claude",
        directory: DIR,
        session,
        model: "Model A",
        effort: "High",
        body: [op_id.len() as u8; 32],
    }
}

fn fresh(grant: &mut Grant, request: &Request, now: u64) -> Reservation {
    match grant.reserve(request, Vec::new(), now).unwrap() {
        Admission::Fresh(r) => r,
        other => panic!("{other:?}"),
    }
}

/// A grant with both roles created and bound.
fn bound() -> Grant {
    let mut grant = Grant::new(scope("task-1"), 0, 60_000).unwrap();
    fresh(
        &mut grant,
        &request("c-producer", Kind::Create, Role::Producer, ""),
        1,
    );
    grant
        .settle("c-producer", Outcome::Applied("session-p".into()))
        .unwrap();
    fresh(
        &mut grant,
        &request("c-reviewer", Kind::Create, Role::Reviewer, ""),
        1,
    );
    grant
        .settle("c-reviewer", Outcome::Applied("session-r".into()))
        .unwrap();
    grant
}

#[test]
fn scope_validation_refuses_malformed_grants() {
    let changes: &[fn(&mut Scope)] = &[
        |s| s.task_id = String::new(),
        |s| s.task_id = "a/b".into(),
        |s| s.task_id = "x".repeat(97),
        |s| s.contract_sha256 = "AB".repeat(32),
        |s| s.contract_sha256 = "ab".into(),
        |s| s.provider = "orca".into(),
        |s| s.directory = "relative".into(),
        |s| s.directory = "/work/../etc".into(),
        |s| s.directory = "/work/".into(),
        |s| s.directory = "/work//x".into(),
        |s| s.model = String::new(),
        |s| s.effort = " High".into(),
    ];
    for change in changes {
        let mut s = scope("task-1");
        change(&mut s);
        assert!(Grant::new(s, 0, 1_000).is_err());
    }
    assert!(Grant::new(scope("task-1"), 0, 0).is_err());
    assert!(Grant::new(scope("task-1"), 0, MAX_TTL_MS + 1).is_err());
    assert!(Grant::new(scope("task-1"), 0, MAX_TTL_MS).is_ok());
}

#[test]
fn one_live_grant_and_a_task_id_is_never_granted_again() {
    let mut registry = Registry::default();
    registry.grant(scope("task-1"), 0, 1_000).unwrap();
    // Identical and live: idempotent, the deadline is not extended.
    assert_eq!(
        registry
            .grant(scope("task-1"), 500, 1_000)
            .unwrap()
            .expires_ms,
        1_000
    );
    let mut wider = scope("task-1");
    wider.directory = "/work".into();
    assert!(registry.grant(wider, 500, 1_000).is_err());
    assert!(registry.grant(scope("task-2"), 500, 1_000).is_err());
    registry.revoke("task-1", 600).unwrap();
    assert!(registry.grant(scope("task-1"), 700, 1_000).is_err());
    // Expired grants also stay ended.
    registry.grant(scope("task-2"), 700, 100).unwrap();
    assert!(registry.grant(scope("task-2"), 900, 1_000).is_err());
    registry.grant(scope("task-3"), 900, 1_000).unwrap();
    assert_eq!(registry.revoke("missing", 0), Err(Refusal::NotFound));
}

#[test]
fn out_of_scope_requests_are_refused_before_any_action() {
    let mut grant = bound();
    let mut wrong_provider = request("s1", Kind::Send, Role::Producer, "session-p");
    wrong_provider.provider = "codex";
    let mut wrong_dir = request("s1", Kind::Send, Role::Producer, "session-p");
    wrong_dir.directory = "/work";
    let refused = [
        wrong_provider,
        wrong_dir,
        // Not the session this role's receipt bound, nor the other role's.
        request("s1", Kind::Send, Role::Producer, "session-x"),
        request("s1", Kind::Send, Role::Producer, "session-r"),
        request("s1", Kind::Interrupt, Role::Reviewer, "session-p"),
        request("s1", Kind::Send, Role::Producer, ""),
    ];
    for r in &refused {
        assert!(matches!(
            grant.reserve(r, Vec::new(), 2),
            Err(Refusal::OutOfScope(_))
        ));
    }
    // Only the granted model and effort may be set.
    let mut other_model = request("o1", Kind::SetOptions, Role::Producer, "session-p");
    other_model.model = "Model B";
    assert!(matches!(
        grant.reserve(&other_model, Vec::new(), 2),
        Err(Refusal::OutOfScope(_))
    ));
    fresh(
        &mut grant,
        &request("o1", Kind::SetOptions, Role::Producer, "session-p"),
        2,
    );
    // Nothing was charged by the refusals.
    assert_eq!(grant.counts.packages, 2);
    assert_eq!(grant.counts.controls, 1);
}

#[test]
fn sessions_bind_only_by_creation_receipt() {
    let mut grant = Grant::new(scope("task-1"), 0, 60_000).unwrap();
    // No session yet: a send cannot name one.
    assert!(
        grant
            .reserve(
                &request("s1", Kind::Send, Role::Producer, "session-p"),
                Vec::new(),
                1
            )
            .is_err()
    );
    grant
        .reserve(
            &request("c1", Kind::Create, Role::Producer, ""),
            vec!["old-session".into()],
            1,
        )
        .unwrap();
    // A second creation for the same role, while the first is in flight.
    assert!(
        grant
            .reserve(
                &request("c2", Kind::Create, Role::Producer, ""),
                Vec::new(),
                1
            )
            .is_err()
    );
    // A pre-existing session is never a receipt.
    assert!(
        grant
            .settle("c1", Outcome::Applied("old-session".into()))
            .is_err()
    );
    grant
        .settle("c1", Outcome::Applied("session-p".into()))
        .unwrap();
    assert_eq!(grant.sessions[&Role::Producer], "session-p");
    // The reviewer cannot be bound to the producer's session.
    fresh(
        &mut grant,
        &request("c3", Kind::Create, Role::Reviewer, ""),
        2,
    );
    assert!(
        grant
            .settle("c3", Outcome::Applied("session-p".into()))
            .is_err()
    );
    assert_eq!(grant.ops["c3"].state, OpState::Reserved);
    // A creation naming a session is refused outright.
    let mut naming = request("c4", Kind::Create, Role::Reviewer, "");
    naming.session = "session-x";
    assert!(grant.reserve(&naming, Vec::new(), 2).is_err());
}

#[test]
fn expiry_and_revocation_stop_deliveries_but_not_reconciliation() {
    let mut grant = bound();
    fresh(
        &mut grant,
        &request("s1", Kind::Send, Role::Producer, "session-p"),
        2,
    );
    grant
        .settle("s1", Outcome::Unknown("no receipt".into()))
        .unwrap();
    assert_eq!(
        grant
            .reserve(
                &request("s2", Kind::Send, Role::Producer, "session-p"),
                Vec::new(),
                60_000
            )
            .unwrap_err(),
        Refusal::Expired
    );
    // Read-only evidence still resolves the earlier send after expiry.
    grant.reconcile("s1", "session-p".into()).unwrap();
    assert!(matches!(grant.ops["s1"].state, OpState::Applied { .. }));

    let mut grant = bound();
    grant.revoke(10);
    for kind in [Kind::Send, Kind::Interrupt, Kind::ReadOptions] {
        assert_eq!(
            grant
                .reserve(
                    &request("x", kind, Role::Producer, "session-p"),
                    Vec::new(),
                    11
                )
                .unwrap_err(),
            Refusal::Revoked
        );
    }
    assert!(!grant.live(11));
}

#[test]
fn quotas_hold_and_only_a_proven_non_start_releases_budget() {
    let mut grant = bound();
    let ids: Vec<String> = (0..20).map(|i| format!("send-{i}")).collect();
    // Two creations already used two of the eighteen packages; the
    // reviewer's creation used one of six reviews.
    for id in &ids[..5] {
        fresh(
            &mut grant,
            &request(id, Kind::Send, Role::Reviewer, "session-r"),
            2,
        );
        grant.settle(id, Outcome::Applied("r".into())).unwrap();
    }
    assert_eq!(grant.counts.reviews, 6);
    assert_eq!(
        grant
            .reserve(
                &request("extra-review", Kind::Send, Role::Reviewer, "session-r"),
                Vec::new(),
                2
            )
            .unwrap_err(),
        Refusal::Quota("review packages")
    );
    // A proven non-start gives its budget back; an unknown keeps it.
    fresh(
        &mut grant,
        &request("p-not", Kind::Send, Role::Producer, "session-p"),
        2,
    );
    grant
        .settle("p-not", Outcome::NotStarted("console busy".into()))
        .unwrap();
    assert_eq!(grant.counts.packages, 7);
    fresh(
        &mut grant,
        &request("p-unknown", Kind::Send, Role::Producer, "session-p"),
        2,
    );
    grant
        .settle("p-unknown", Outcome::Unknown("lost receipt".into()))
        .unwrap();
    assert_eq!(grant.counts.packages, 8);
    for id in &ids[5..15] {
        fresh(
            &mut grant,
            &request(id, Kind::Send, Role::Producer, "session-p"),
            2,
        );
    }
    assert_eq!(grant.counts.packages, MAX_PACKAGES);
    assert_eq!(
        grant
            .reserve(
                &request("one-more", Kind::Send, Role::Producer, "session-p"),
                Vec::new(),
                2
            )
            .unwrap_err(),
        Refusal::Quota("work packages")
    );
    // Controls have their own bound.
    for i in 0..MAX_CONTROLS {
        let id = format!("ctl-{i}");
        fresh(
            &mut grant,
            &request(&id, Kind::ReadOptions, Role::Producer, "session-p"),
            2,
        );
    }
    assert!(
        grant
            .reserve(
                &request("ctl-x", Kind::Interrupt, Role::Producer, "session-p"),
                Vec::new(),
                2
            )
            .is_err()
    );
    // Two sessions at most, ever.
    assert!(
        grant
            .reserve(
                &request("c-third", Kind::Create, Role::Producer, ""),
                Vec::new(),
                2
            )
            .is_err()
    );
}

#[test]
fn an_op_id_is_admitted_once_and_never_replayed() {
    let mut grant = bound();
    let send = request("s1", Kind::Send, Role::Producer, "session-p");
    let first = fresh(&mut grant, &send, 2);
    assert_eq!(first.op_id(), "s1");
    // While in flight, after unknown, after a restart: the recorded state,
    // never a second admission.
    assert!(matches!(
        grant.reserve(&send, Vec::new(), 3).unwrap(),
        Admission::Recorded(OpState::Reserved)
    ));
    grant.recover();
    assert!(matches!(
        grant.reserve(&send, Vec::new(), 3).unwrap(),
        Admission::Recorded(OpState::Unknown { .. })
    ));
    // A settled unknown is not re-settled by a late report.
    assert!(grant.settle("s1", Outcome::Applied("x".into())).is_err());
    // The same op_id with another body is a conflict.
    let mut other = send.clone();
    other.body = [99; 32];
    assert!(matches!(
        grant.reserve(&other, Vec::new(), 3),
        Err(Refusal::Conflict(_))
    ));
    // Evidence resolves it; a duplicate then returns the applied receipt.
    grant.reconcile("s1", "receipt-1".into()).unwrap();
    assert_eq!(
        match grant.reserve(&send, Vec::new(), 3).unwrap() {
            Admission::Recorded(state) => state,
            other => panic!("{other:?}"),
        },
        OpState::Applied {
            result: "receipt-1".into()
        }
    );
    assert_eq!(grant.counts.packages, 3);
    // A rejection is never turned into success.
    fresh(
        &mut grant,
        &request("s2", Kind::Send, Role::Producer, "session-p"),
        4,
    );
    grant
        .settle("s2", Outcome::NotStarted("busy".into()))
        .unwrap();
    assert!(grant.reconcile("s2", "late".into()).is_err());
}

#[test]
fn an_unknown_creation_is_bound_only_by_later_receipt_evidence() {
    let mut grant = Grant::new(scope("task-1"), 0, 60_000).unwrap();
    fresh(
        &mut grant,
        &request("c1", Kind::Create, Role::Producer, ""),
        1,
    );
    grant
        .settle("c1", Outcome::Unknown("submitted; receipt pending".into()))
        .unwrap();
    // The role stays taken: an unknown creation is never re-attempted.
    assert!(
        grant
            .reserve(
                &request("c1-again", Kind::Create, Role::Producer, ""),
                Vec::new(),
                2
            )
            .is_err()
    );
    grant.reconcile("c1", "session-p".into()).unwrap();
    assert_eq!(grant.sessions[&Role::Producer], "session-p");
    // Re-reporting the same receipt is harmless; another one is refused.
    grant.reconcile("c1", "session-p".into()).unwrap();
    assert!(grant.reconcile("c1", "session-q".into()).is_err());
}

#[test]
fn digests_bind_the_grant_and_the_exact_operation() {
    let s = scope("task-1");
    let base = digest(&s, "op", Kind::Send, Role::Producer, "sess", &[1; 32]);
    let mut other_task = scope("task-2");
    assert_ne!(
        base,
        digest(
            &other_task,
            "op",
            Kind::Send,
            Role::Producer,
            "sess",
            &[1; 32]
        )
    );
    other_task = scope("task-1");
    other_task.directory = "/work".into();
    assert_ne!(
        base,
        digest(
            &other_task,
            "op",
            Kind::Send,
            Role::Producer,
            "sess",
            &[1; 32]
        )
    );
    assert_ne!(
        base,
        digest(&s, "op2", Kind::Send, Role::Producer, "sess", &[1; 32])
    );
    assert_ne!(
        base,
        digest(&s, "op", Kind::Interrupt, Role::Producer, "sess", &[1; 32])
    );
    assert_ne!(
        base,
        digest(&s, "op", Kind::Send, Role::Reviewer, "sess", &[1; 32])
    );
    assert_ne!(
        base,
        digest(&s, "op", Kind::Send, Role::Producer, "sess2", &[1; 32])
    );
    assert_ne!(
        base,
        digest(&s, "op", Kind::Send, Role::Producer, "sess", &[2; 32])
    );
}

#[test]
fn the_registry_persists_counts_and_unknowns_across_restarts() {
    let mut registry = Registry::default();
    registry.grant(scope("task-1"), 0, 60_000).unwrap();
    let grant = registry.get_mut("task-1").unwrap();
    fresh(grant, &request("c1", Kind::Create, Role::Producer, ""), 1);
    let saved = serde_json::to_vec(&registry).unwrap();
    let mut restored: Registry = serde_json::from_slice(&saved).unwrap();
    restored.recover();
    let grant = restored.get("task-1").unwrap();
    assert_eq!(grant.counts.packages, 1);
    assert!(matches!(grant.ops["c1"].state, OpState::Unknown { .. }));
}

#[test]
fn an_existing_session_is_adopted_once_and_counts_as_a_session() {
    let mut grant = Grant::new(scope("task-1"), 0, 60_000).unwrap();
    assert_eq!(
        grant.adopt(Role::Producer, "", 1),
        Err(Refusal::Invalid("adoption names one session"))
    );
    assert_eq!(grant.adopt(Role::Producer, "session-p", 1), Ok(true));
    // A restarted caller asking again changes nothing.
    assert_eq!(grant.adopt(Role::Producer, "session-p", 2), Ok(false));
    assert_eq!(grant.counts.adopted, 1);
    assert_eq!(grant.adopted[&Role::Producer], 1);
    // One session per role, never replaced, never shared between roles.
    assert!(matches!(
        grant.adopt(Role::Producer, "session-x", 2),
        Err(Refusal::Conflict(_))
    ));
    assert!(matches!(
        grant.adopt(Role::Reviewer, "session-p", 2),
        Err(Refusal::Conflict(_))
    ));
    assert!(matches!(
        grant.reserve(
            &request("c-producer", Kind::Create, Role::Producer, ""),
            Vec::new(),
            2
        ),
        Err(Refusal::Quota(_))
    ));
    // The adopted session is the producer's: sends must name it.
    fresh(
        &mut grant,
        &request("s1", Kind::Send, Role::Producer, "session-p"),
        3,
    );
    assert!(matches!(
        grant.reserve(
            &request("s2", Kind::Send, Role::Producer, "session-x"),
            Vec::new(),
            3
        ),
        Err(Refusal::OutOfScope(_))
    ));
    // The reviewer is still created, and its receipt cannot be the adopted one.
    fresh(
        &mut grant,
        &request("c-reviewer", Kind::Create, Role::Reviewer, ""),
        4,
    );
    assert!(matches!(
        grant.settle("c-reviewer", Outcome::Applied("session-p".into())),
        Err(Refusal::Conflict(_))
    ));
    grant
        .settle("c-reviewer", Outcome::Applied("session-r".into()))
        .unwrap();
    assert_eq!(grant.counts.creates + grant.counts.adopted, MAX_SESSIONS);
    assert!(matches!(
        grant.adopt(Role::Reviewer, "session-y", 5),
        Err(Refusal::Conflict(_))
    ));
}

#[test]
fn adoption_needs_a_live_grant_and_a_role_without_a_creation() {
    let mut grant = Grant::new(scope("task-1"), 0, 60_000).unwrap();
    fresh(
        &mut grant,
        &request("c-producer", Kind::Create, Role::Producer, ""),
        1,
    );
    // A creation in flight or of unknown outcome may still bind its receipt.
    assert!(matches!(
        grant.adopt(Role::Producer, "session-p", 2),
        Err(Refusal::Conflict(_))
    ));
    grant
        .settle("c-producer", Outcome::Unknown("no receipt".into()))
        .unwrap();
    assert!(matches!(
        grant.adopt(Role::Producer, "session-p", 2),
        Err(Refusal::Conflict(_))
    ));
    // A creation proven not started leaves the role free.
    let mut grant = Grant::new(scope("task-1"), 0, 60_000).unwrap();
    fresh(
        &mut grant,
        &request("c-producer", Kind::Create, Role::Producer, ""),
        1,
    );
    grant
        .settle("c-producer", Outcome::NotStarted("cover failed".into()))
        .unwrap();
    assert_eq!(grant.adopt(Role::Producer, "session-p", 2), Ok(true));
    // Expiry and revocation end adoption, including a repeat of a done one.
    assert_eq!(
        grant.adopt(Role::Producer, "session-p", 60_000),
        Err(Refusal::Expired)
    );
    grant.revoke(3);
    assert_eq!(
        grant.adopt(Role::Producer, "session-p", 4),
        Err(Refusal::Revoked)
    );
    assert_eq!(
        grant.adopt(Role::Reviewer, "session-r", 4),
        Err(Refusal::Revoked)
    );
}

#[test]
fn adoptions_persist_and_older_records_read_as_none() {
    let mut registry = Registry::default();
    registry.grant(scope("task-1"), 0, 60_000).unwrap();
    registry
        .get_mut("task-1")
        .unwrap()
        .adopt(Role::Producer, "session-p", 1)
        .unwrap();
    let restored: Registry =
        serde_json::from_slice(&serde_json::to_vec(&registry).unwrap()).unwrap();
    assert_eq!(restored, registry);
    let mut old = serde_json::to_value(Grant::new(scope("task-2"), 0, 60_000).unwrap()).unwrap();
    old["counts"].as_object_mut().unwrap().remove("adopted");
    let old: Grant = serde_json::from_value(old).unwrap();
    assert_eq!((old.counts.adopted, old.adopted.len()), (0, 0));
}
