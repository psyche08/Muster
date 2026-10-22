//! Properties the trust core must hold under arbitrary input: parsers never
//! panic, encodings round-trip, and the roster fold is order-independent,
//! tamper-proof and permanent about removals.

use std::net::Ipv4Addr;
use std::sync::Arc;

use muster_core::roster::{Author, DeviceKind, DeviceRecord, DeviceUpdate, Entry, Role, Roster};
use muster_core::{HwSigner, Invite, Scope, SoftSigner};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn entry_decode_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..600)) {
        let _ = Entry::decode(&bytes);
    }

    #[test]
    fn record_decode_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..300)) {
        let _ = DeviceRecord::from_bytes(&bytes);
    }

    #[test]
    fn invite_decode_never_panics(text in "\\PC{0,200}") {
        let _ = Invite::decode(&text);
        let _ = Invite::decode(&format!("muster1:{text}"));
    }

    #[test]
    fn scope_parse_never_panics(text in "\\PC{0,40}") {
        let _ = text.parse::<Scope>();
    }

    #[test]
    fn scope_round_trips(proto in prop_oneof![Just("tcp"), Just("udp")], port in proptest::option::of(1u16..)) {
        let text = match port {
            Some(p) => format!("net:{proto}/{p}"),
            None => format!("net:{proto}/*"),
        };
        let scope: Scope = text.parse().unwrap();
        prop_assert_eq!(scope.to_string(), text);
        prop_assert_eq!("net:*".parse::<Scope>().unwrap().to_string(), "net:*");
    }

    #[test]
    fn a_flipped_byte_in_a_signed_entry_is_rejected(flip in 0usize..400, mask in 1u8..) {
        let admin = SoftSigner::generate();
        let mut roster = Roster::new();
        let genesis = author(&roster, &admin).genesis("p", (Ipv4Addr::new(10, 9, 0, 0), 24), record(&admin, "a")).unwrap();
        roster.insert(genesis).unwrap();
        let joiner = SoftSigner::generate();
        let add = author(&roster, &admin).add(record(&joiner, "b")).unwrap();
        let mut bytes = add.encode();
        let i = flip % bytes.len();
        bytes[i] ^= mask;
        // Either it no longer decodes, or the roster refuses it.
        if let Ok(entry) = Entry::decode(&bytes) {
            let mut r2 = roster.clone();
            prop_assert!(r2.insert(entry).is_err() || bytes == add.encode());
        }
    }
}

fn author<'a>(roster: &'a Roster, signer: &'a dyn HwSigner) -> Author<'a> {
    Author { roster, signer }
}

fn record(signer: &SoftSigner, name: &str) -> DeviceRecord {
    DeviceRecord::new(signer.public_key(), name, DeviceKind::Desktop)
}

/// A network with an admin, a second admin, two members, an update and a
/// removal, as a list of entries in authoring order.
fn history() -> (Vec<Entry>, Vec<Arc<SoftSigner>>) {
    let admin = Arc::new(SoftSigner::generate());
    let admin2 = Arc::new(SoftSigner::generate());
    let m1 = Arc::new(SoftSigner::generate());
    let m2 = Arc::new(SoftSigner::generate());
    let mut roster = Roster::new();
    let mut entries = Vec::new();
    let genesis = author(&roster, admin.as_ref())
        .genesis(
            "h",
            (Ipv4Addr::new(10, 8, 0, 0), 24),
            record(&admin, "admin"),
        )
        .unwrap();
    roster.insert(genesis.clone()).unwrap();
    entries.push(genesis);
    let mut r2 = record(&admin2, "admin2");
    r2.role = Role::Admin;
    let e = author(&roster, admin.as_ref()).add(r2).unwrap();
    roster.insert(e.clone()).unwrap();
    entries.push(e);
    let e = author(&roster, admin2.as_ref())
        .add(record(&m1, "m1"))
        .unwrap();
    roster.insert(e.clone()).unwrap();
    entries.push(e);
    let e = author(&roster, admin.as_ref())
        .add(record(&m2, "m2"))
        .unwrap();
    roster.insert(e.clone()).unwrap();
    entries.push(e);
    let e = author(&roster, admin2.as_ref())
        .update(
            m1.public_key().device_id(),
            DeviceUpdate {
                name: Some("m1-renamed".into()),
                role: None,
                scopes: Some([Scope::AgentView].into_iter().collect()),
                vip: None,
            },
        )
        .unwrap();
    roster.insert(e.clone()).unwrap();
    entries.push(e);
    let e = author(&roster, admin.as_ref())
        .remove(m2.public_key().device_id())
        .unwrap();
    roster.insert(e.clone()).unwrap();
    entries.push(e);
    (entries, vec![admin, admin2, m1, m2])
}

/// Inserts in the given order, retrying entries whose parents are missing
/// (what gossip does), until all are in.
fn fold(order: &[usize], entries: &[Entry]) -> Roster {
    let mut roster = Roster::new();
    let mut pending: Vec<Entry> = order.iter().map(|i| entries[*i].clone()).collect();
    let mut progress = true;
    while progress && !pending.is_empty() {
        progress = false;
        let mut next = Vec::new();
        for entry in pending {
            match roster.insert(entry.clone()) {
                Ok(_) => progress = true,
                Err(muster_core::Error::MissingParents(_)) => next.push(entry),
                Err(muster_core::Error::Rejected(m)) if m.contains("no genesis") => {
                    next.push(entry)
                }
                Err(e) => panic!("unexpected: {e}"),
            }
        }
        pending = next;
    }
    assert!(pending.is_empty(), "entries never became insertable");
    roster
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn fold_is_order_independent(perm in Just(6usize).prop_shuffle_indices()) {
        let (entries, _) = history();
        let reference = fold(&[0, 1, 2, 3, 4, 5], &entries);
        let shuffled = fold(&perm, &entries);
        prop_assert_eq!(shuffled.state(), reference.state());
        prop_assert_eq!(shuffled.heads(), reference.heads());
    }
}

trait ShuffleIndices {
    fn prop_shuffle_indices(self) -> BoxedStrategy<Vec<usize>>;
}

impl ShuffleIndices for Just<usize> {
    fn prop_shuffle_indices(self) -> BoxedStrategy<Vec<usize>> {
        self.prop_flat_map(|n| Just((0..n).collect::<Vec<usize>>()).prop_shuffle())
            .boxed()
    }
}

#[test]
fn removal_is_permanent_and_non_admins_cannot_author() {
    let (entries, signers) = history();
    let mut roster = Roster::from_entries(entries).unwrap();
    let (admin, admin2, m1, m2) = (&signers[0], &signers[1], &signers[2], &signers[3]);
    let m2_id = m2.public_key().device_id();
    assert!(roster.state().active(&m2_id).is_none());
    // Re-adding a removed device is refused, by either admin.
    for a in [admin, admin2] {
        if let Ok(entry) = author(&roster, a.as_ref()).add(record(m2, "m2-again")) {
            assert!(roster.insert(entry).is_err(), "removed device came back");
        }
    }
    // A member cannot add or remove anyone.
    let stranger = SoftSigner::generate();
    if let Ok(entry) = author(&roster, m1.as_ref()).add(record(&stranger, "s")) {
        assert!(roster.insert(entry).is_err(), "member added a device");
    }
    if let Ok(entry) = author(&roster, m1.as_ref()).remove(admin.public_key().device_id()) {
        assert!(roster.insert(entry).is_err(), "member removed the admin");
    }
    // A removed admin's later entries are void: remove admin2, then an entry
    // signed by admin2 whose causal past lacks the removal is rejected.
    let stale = roster.clone();
    let remove_admin2 = author(&roster, admin.as_ref())
        .remove(admin2.public_key().device_id())
        .unwrap();
    roster.insert(remove_admin2).unwrap();
    let late = author(&stale, admin2.as_ref())
        .add(record(&stranger, "late"))
        .unwrap();
    let outcome = roster.insert(late);
    assert!(
        outcome.is_err()
            || roster
                .state()
                .active(&stranger.public_key().device_id())
                .is_none()
    );
    assert_eq!(
        roster.state().active_members().count(),
        2,
        "admin and m1 remain"
    );
}

#[test]
fn invite_round_trips_and_expires() {
    let admin = SoftSigner::generate();
    let mut roster = Roster::new();
    let genesis = author(&roster, &admin)
        .genesis(
            "home",
            (Ipv4Addr::new(10, 7, 0, 0), 24),
            record(&admin, "a"),
        )
        .unwrap();
    roster.insert(genesis).unwrap();
    let invite = Invite::new(
        roster.network_id().unwrap(),
        "home",
        admin.public_key(),
        "relay:9443",
        60,
    );
    let text = invite.encode();
    assert!(text.starts_with("muster1:"));
    let back = Invite::decode(&text).unwrap();
    assert_eq!(back, invite);
    assert!(!invite.expired(invite.expires_unix - 1));
    assert!(invite.expired(invite.expires_unix + 1));
    let proof = invite.proof(b"exporter");
    assert!(invite.verify_proof(b"exporter", &proof));
    assert!(!invite.verify_proof(b"other", &proof));
    let mut wrong = invite.clone();
    wrong.secret[0] ^= 1;
    assert!(!wrong.verify_proof(b"exporter", &proof));
}
