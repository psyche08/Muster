//! The standing local dispatch grant (ADR 0199).

use muster_core::dispatch_grant::*;
use muster_core::scheduler_grant::{Admission, Kind, OpState, Outcome};

const NOW: u64 = 1_000_000;
const DAY: u64 = 24 * 60 * 60 * 1000;

fn scope() -> Scope {
    Scope::new(
        "review-service".into(),
        vec!["codex".into(), "claude".into()],
        vec!["/work/b".into(), "/work/a".into(), "/work/a".into()],
    )
    .unwrap()
}

fn grant() -> Grant {
    Grant::new(scope(), NOW, DAY).unwrap()
}

fn create<'a>(op_id: &'a str, directory: &'a str) -> Request<'a> {
    Request {
        op_id,
        kind: Kind::Create,
        provider: "claude",
        directory,
        session: "",
        body: [1; 32],
    }
}

fn send<'a>(op_id: &'a str, session: &'a str) -> Request<'a> {
    Request {
        op_id,
        kind: Kind::Send,
        provider: "claude",
        directory: "",
        session,
        body: [2; 32],
    }
}

fn fresh(admission: Result<Admission, Refusal>) -> muster_core::scheduler_grant::Reservation {
    match admission.unwrap() {
        Admission::Fresh(reservation) => reservation,
        other => panic!("{other:?}"),
    }
}

/// A grant with one creation applied: its receipt `s1` is bound.
fn bound() -> Grant {
    let mut g = grant();
    fresh(g.reserve(&create("c1", "/work/a"), vec!["old".into()], true, NOW));
    g.settle("c1", Outcome::Applied("s1".into())).unwrap();
    g
}

#[test]
fn a_scope_names_its_caller_providers_and_roots_exactly() {
    let s = scope();
    assert_eq!(s.providers, ["claude", "codex"]);
    assert_eq!(s.roots, ["/work/a", "/work/b"]);
    let bad = [
        ("", vec!["claude"], vec!["/work"]),
        ("has space", vec!["claude"], vec!["/work"]),
        (&*"n".repeat(65), vec!["claude"], vec!["/work"]),
        ("svc", vec![], vec!["/work"]),
        ("svc", vec!["deepseek"], vec!["/work"]),
        ("svc", vec!["claude"], vec![]),
        ("svc", vec!["claude"], vec!["/"]),
        ("svc", vec!["claude"], vec!["relative"]),
        ("svc", vec!["claude"], vec!["/work/"]),
        ("svc", vec!["claude"], vec!["/work/../etc"]),
    ];
    for (name, providers, roots) in bad {
        let result = Scope::new(
            name.into(),
            providers.into_iter().map(String::from).collect(),
            roots.into_iter().map(String::from).collect(),
        );
        assert!(matches!(result, Err(Refusal::Invalid(_))), "{name:?}");
    }
    let many = (0..17).map(|i| format!("/work/{i}")).collect();
    assert!(Scope::new("svc".into(), vec!["claude".into()], many).is_err());
}

#[test]
fn a_creation_must_lie_in_or_below_a_granted_root() {
    let s = scope();
    for inside in ["/work/a", "/work/a/repo", "/work/b/x/y"] {
        assert!(s.covers(inside), "{inside}");
    }
    for outside in [
        "/work",
        "/work/ab",
        "/work/a/../c",
        "/work/a/",
        "relative",
        "/other",
    ] {
        assert!(!s.covers(outside), "{outside}");
    }
    let mut g = grant();
    assert_eq!(
        g.reserve(&create("c1", "/work/ab"), Vec::new(), false, NOW)
            .unwrap_err(),
        Refusal::OutOfScope("directory is under no granted root")
    );
    let other = Request {
        provider: "deepseek",
        ..create("c2", "/work/a")
    };
    assert_eq!(
        g.reserve(&other, Vec::new(), false, NOW).unwrap_err(),
        Refusal::OutOfScope("provider")
    );
    let named = Request {
        session: "s1",
        ..create("c3", "/work/a")
    };
    assert!(matches!(
        g.reserve(&named, Vec::new(), false, NOW),
        Err(Refusal::Invalid(_))
    ));
    assert!(g.ops.is_empty());
    let reserved = fresh(g.reserve(&create("c4", "/work/a/repo"), Vec::new(), false, NOW));
    assert_eq!(reserved.task_id(), "dispatch:review-service");
    assert_eq!(reserved.op_id(), "c4");
    assert_eq!(reserved.kind(), Kind::Create);
    assert_eq!(
        reserved.digest(),
        digest("review-service", &create("c4", "/work/a/repo"))
    );
}

#[test]
fn only_a_session_the_grants_own_creation_bound_takes_a_send() {
    let mut g = grant();
    // Nothing is bound yet: no session the caller names is taken.
    assert_eq!(
        g.reserve(&send("s-op", "s1"), Vec::new(), true, NOW)
            .unwrap_err(),
        Refusal::OutOfScope("session was not created through this grant")
    );
    let mut g = bound();
    assert_eq!(g.sessions.get("s1").map(String::as_str), Some("claude"));
    fresh(g.reserve(&send("s-op", "s1"), Vec::new(), true, NOW));
    // Another session, or this one as another provider's, is not.
    assert!(
        g.reserve(&send("s-2", "user-session"), Vec::new(), true, NOW)
            .is_err()
    );
    let codex = Request {
        provider: "codex",
        ..send("s-3", "s1")
    };
    assert!(g.reserve(&codex, Vec::new(), true, NOW).is_err());
    let with_directory = Request {
        directory: "/work/a",
        ..send("s-4", "s1")
    };
    assert!(matches!(
        g.reserve(&with_directory, Vec::new(), true, NOW),
        Err(Refusal::Invalid(_))
    ));
}

#[test]
fn a_creation_receipt_binds_only_a_new_session() {
    let mut g = grant();
    fresh(g.reserve(&create("c1", "/work/a"), vec!["old".into()], false, NOW));
    assert!(g.settle("c1", Outcome::Applied("old".into())).is_err());
    assert!(g.settle("c1", Outcome::Applied(String::new())).is_err());
    assert!(g.sessions.is_empty());
    assert_eq!(g.ops["c1"].state, OpState::Reserved);
    g.settle("c1", Outcome::Applied("s1".into())).unwrap();
    assert!(g.settle("c1", Outcome::Applied("s1".into())).is_err());

    // Unknown first, then read-only evidence binds it; a rejection never
    // turns into success.
    fresh(g.reserve(&create("c2", "/work/b"), Vec::new(), false, NOW));
    g.settle("c2", Outcome::Unknown("lost".into())).unwrap();
    assert!(!g.sessions.contains_key("s2"));
    g.reconcile("c2", "s2".into()).unwrap();
    assert_eq!(
        g.ops["c2"].state,
        OpState::Applied {
            result: "s2".into()
        }
    );
    assert!(g.sessions.contains_key("s2"));
    fresh(g.reserve(&create("c3", "/work/b"), Vec::new(), false, NOW));
    g.settle("c3", Outcome::NotStarted("busy".into())).unwrap();
    assert!(g.reconcile("c3", "s3".into()).is_err());
    assert!(!g.sessions.contains_key("s3"));
}

#[test]
fn an_op_id_runs_once_and_answers_its_record() {
    let mut g = bound();
    assert!(matches!(
        g.reserve(&create("c1", "/work/a"), Vec::new(), true, NOW),
        Ok(Admission::Recorded(OpState::Applied { result })) if result == "s1"
    ));
    let altered = Request {
        body: [9; 32],
        ..create("c1", "/work/a")
    };
    assert!(matches!(
        g.reserve(&altered, Vec::new(), true, NOW),
        Err(Refusal::Conflict(_))
    ));
    assert!(matches!(
        g.reserve(&create("c1", "/work/b"), Vec::new(), true, NOW),
        Err(Refusal::Conflict(_))
    ));
    // A record still answers after revocation; nothing new is admitted.
    g.revoke(NOW + 1);
    assert!(matches!(
        g.reserve(&create("c1", "/work/a"), Vec::new(), true, NOW + 2),
        Ok(Admission::Recorded(_))
    ));
    assert_eq!(
        g.reserve(&create("c9", "/work/a"), Vec::new(), true, NOW + 2)
            .unwrap_err(),
        Refusal::Revoked
    );
    assert_eq!(
        g.reserve(&send("s9", "s1"), Vec::new(), true, NOW + 2)
            .unwrap_err(),
        Refusal::Revoked
    );
}

#[test]
fn an_expired_grant_admits_nothing_new() {
    let mut g = Grant::new(scope(), NOW, 1_000).unwrap();
    assert!(g.live(NOW + 999));
    assert!(!g.live(NOW + 1_000));
    assert_eq!(
        g.reserve(&create("c1", "/work/a"), Vec::new(), false, NOW + 1_000)
            .unwrap_err(),
        Refusal::Expired
    );
    assert!(Grant::new(scope(), NOW, 0).is_err());
    assert!(Grant::new(scope(), NOW, MAX_TTL_MS + 1).is_err());
    Grant::new(scope(), NOW, MAX_TTL_MS).unwrap();
}

#[test]
fn locked_leases_are_limited_per_rolling_hour_and_unlocked_runs_are_not() {
    let mut g = grant();
    for i in 0..MAX_LOCKED_PER_HOUR {
        let id = format!("c{i}");
        fresh(g.reserve(&create(&id, "/work/a"), Vec::new(), true, NOW + i as u64));
        // A non-start still counts: the console may have been unlocked.
        g.settle(&id, Outcome::NotStarted("input".into())).unwrap();
    }
    assert_eq!(g.locked_last_hour(NOW + 100), MAX_LOCKED_PER_HOUR);
    assert_eq!(
        g.reserve(&create("over", "/work/a"), Vec::new(), true, NOW + 100)
            .unwrap_err(),
        Refusal::Quota("locked leases this hour")
    );
    assert!(!g.ops.contains_key("over"));
    fresh(g.reserve(&create("unlocked", "/work/a"), Vec::new(), false, NOW + 100));
    // An hour after the first, one slot is free again; after the last, all are.
    let hour = 60 * 60 * 1000;
    assert_eq!(g.locked_last_hour(NOW + hour), MAX_LOCKED_PER_HOUR - 1);
    let later = NOW + hour + MAX_LOCKED_PER_HOUR as u64;
    fresh(g.reserve(&create("later", "/work/a"), Vec::new(), true, later));
    assert_eq!(g.locked_last_hour(later), 1);
}

#[test]
fn records_make_room_only_by_dropping_old_settled_ones() {
    let mut g = grant();
    let old = NOW;
    for i in 0..MAX_RECORDS {
        let id = format!("c{i}");
        fresh(g.reserve(&create(&id, "/work/a"), Vec::new(), false, old));
        let outcome = if i == 0 {
            Outcome::Unknown("lost".into())
        } else {
            Outcome::NotStarted("busy".into())
        };
        g.settle(&id, outcome).unwrap();
    }
    // All recent: nothing can be dropped.
    assert_eq!(
        g.reserve(&create("next", "/work/a"), Vec::new(), false, old + 1)
            .unwrap_err(),
        Refusal::Quota("operations on record")
    );
    // A week on the settled ones go; the unknown one stays, never replayed.
    let later = old + KEEP_SETTLED_MS;
    let mut g = Grant {
        expires_ms: later + DAY,
        ..g
    };
    fresh(g.reserve(&create("next", "/work/a"), Vec::new(), false, later));
    assert_eq!(g.ops.len(), 2);
    assert!(matches!(g.ops["c0"].state, OpState::Unknown { .. }));
}

#[test]
fn a_restart_turns_a_reserved_operation_unknown() {
    let mut registry = Registry::default();
    registry.grant(scope(), NOW, DAY).unwrap();
    let g = registry.get_mut("review-service").unwrap();
    fresh(g.reserve(&create("c1", "/work/a"), Vec::new(), true, NOW));
    registry.recover();
    let state = &registry.get("review-service").unwrap().ops["c1"].state;
    assert!(matches!(state, OpState::Unknown { .. }));
    let g = registry.get_mut("review-service").unwrap();
    assert!(matches!(
        g.reserve(&create("c1", "/work/a"), Vec::new(), true, NOW),
        Ok(Admission::Recorded(OpState::Unknown { .. }))
    ));
    let bytes = serde_json::to_vec(&registry).unwrap();
    assert_eq!(
        serde_json::from_slice::<Registry>(&bytes).unwrap(),
        registry
    );
}

#[test]
fn granting_again_renews_the_scope_and_keeps_sessions_and_records() {
    let mut registry = Registry::default();
    registry.grant(scope(), NOW, DAY).unwrap();
    *registry.get_mut("review-service").unwrap() = Grant {
        scope: scope(),
        ..bound()
    };
    registry.revoke("review-service", NOW + 1).unwrap();
    assert!(!registry.get("review-service").unwrap().live(NOW + 2));
    let narrower = Scope::new(
        "review-service".into(),
        vec!["claude".into()],
        vec!["/work/b".into()],
    )
    .unwrap();
    let g = registry.grant(narrower.clone(), NOW + 2, DAY).unwrap();
    assert!(g.live(NOW + 3));
    assert_eq!(g.scope, narrower);
    assert_eq!(g.expires_ms, NOW + 2 + DAY);
    assert!(g.sessions.contains_key("s1"));
    assert!(g.ops.contains_key("c1"));
    // The old root is no longer granted; the op_id still answers its record.
    let g = registry.get_mut("review-service").unwrap();
    assert!(
        g.reserve(&create("c2", "/work/a"), Vec::new(), false, NOW + 3)
            .is_err()
    );
    assert!(matches!(
        g.reserve(&create("c1", "/work/a"), Vec::new(), false, NOW + 3),
        Ok(Admission::Recorded(_))
    ));
    assert_eq!(
        registry.revoke("missing", NOW).unwrap_err(),
        Refusal::NotFound
    );
    assert!(registry.grant(scope(), NOW, MAX_TTL_MS + 1).is_err());
}

#[test]
fn a_mac_holds_a_few_named_grants() {
    let mut registry = Registry::default();
    for i in 0..MAX_GRANTS {
        let s = Scope::new(
            format!("svc-{i}"),
            vec!["claude".into()],
            vec!["/work".into()],
        )
        .unwrap();
        registry.grant(s, NOW, DAY).unwrap();
    }
    let extra = Scope::new("svc-x".into(), vec!["claude".into()], vec!["/work".into()]).unwrap();
    assert!(matches!(
        registry.grant(extra, NOW, DAY),
        Err(Refusal::Quota(_))
    ));
}

#[test]
fn the_digest_binds_the_grant_and_every_part_of_the_operation() {
    let base = create("c1", "/work/a");
    let d = digest("svc", &base);
    assert_ne!(d, digest("svc-2", &base));
    for changed in [
        Request {
            op_id: "c2",
            ..base.clone()
        },
        Request {
            provider: "codex",
            ..base.clone()
        },
        Request {
            directory: "/work/b",
            ..base.clone()
        },
        Request {
            kind: Kind::Send,
            ..base.clone()
        },
        Request {
            session: "s",
            ..base.clone()
        },
        Request {
            body: [3; 32],
            ..base.clone()
        },
    ] {
        assert_ne!(d, digest("svc", &changed));
    }
}
