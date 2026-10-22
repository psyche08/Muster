use muster_core::presence::Binding;
use muster_core::{
    Author, Body, DeviceKind, DeviceRecord, DeviceUpdate, Entry, HwSigner, Roster, Scope,
    SoftSigner,
};

struct Fixture {
    roster: Roster,
    admin: SoftSigner,
    member: SoftSigner,
    presence: SoftSigner,
}
impl Fixture {
    fn new() -> Self {
        let admin = SoftSigner::generate();
        let member = SoftSigner::generate();
        let presence = SoftSigner::generate();
        let mut roster = Roster::new();
        let genesis = Author {
            roster: &roster,
            signer: &admin,
        }
        .genesis(
            "test",
            ("192.168.13.0".parse().unwrap(), 24),
            DeviceRecord::new(admin.public_key(), "admin", DeviceKind::Desktop),
        )
        .unwrap();
        roster.insert(genesis).unwrap();
        let mut record = DeviceRecord::new(member.public_key(), "desktop", DeviceKind::Desktop);
        record.scopes.insert(Scope::LockedControl);
        let add = Author {
            roster: &roster,
            signer: &admin,
        }
        .add(record)
        .unwrap();
        roster.insert(add).unwrap();
        Self {
            roster,
            admin,
            member,
            presence,
        }
    }
    fn epoch(&mut self) -> Entry {
        let entry = Author {
            roster: &self.roster,
            signer: &self.admin,
        }
        .update(
            self.member.public_key().device_id(),
            DeviceUpdate::default(),
        )
        .unwrap();
        self.roster.insert(entry.clone()).unwrap();
        entry
    }
    fn enroll(&mut self) -> Binding {
        let epoch = self.epoch();
        let binding = Binding::issue(
            self.roster.network_id().unwrap(),
            self.member.public_key().device_id(),
            epoch.hash(),
            self.presence.public_key(),
            &self.admin,
        )
        .unwrap();
        self.roster
            .install_presence_binding(binding.clone())
            .unwrap();
        binding
    }
}
#[test]
fn enrollment_keeps_signed_records_unchanged_and_scopes_do_not_revoke_key() {
    let mut f = Fixture::new();
    let before: Vec<_> = f.roster.entries().map(Entry::encode).collect();
    let binding = f.enroll();
    assert_eq!(Binding::decode(&binding.encode()).unwrap(), binding);
    assert!(
        matches!(f.roster.get(&binding.epoch).unwrap().body, Body::Update { ref update, .. } if update.is_presence_epoch())
    );
    let after: Vec<_> = f.roster.entries().map(Entry::encode).collect();
    assert_eq!(&after[..before.len()], before);
    let member = f.member.public_key().device_id();
    let entry = Author {
        roster: &f.roster,
        signer: &f.admin,
    }
    .update(
        member,
        DeviceUpdate {
            name: Some("renamed".into()),
            scopes: Some([Scope::LockedControl, Scope::Agents].into()),
            ..Default::default()
        },
    )
    .unwrap();
    f.roster.insert(entry).unwrap();
    assert_eq!(
        f.roster
            .state()
            .active(&member)
            .unwrap()
            .record
            .presence_key,
        Some(f.presence.public_key())
    );
    let mut restored = Roster::from_entries(f.roster.entries().cloned()).unwrap();
    assert!(
        restored
            .state()
            .active(&member)
            .unwrap()
            .record
            .presence_key
            .is_none()
    );
    restored.install_presence_binding(binding).unwrap();
    assert_eq!(restored.state(), f.roster.state());
}
#[test]
fn tampered_wrong_network_member_key_and_nonadmin_bindings_are_rejected() {
    let mut f = Fixture::new();
    let good = f.enroll();
    for field in 0..6 {
        let mut bad = good.clone();
        match field {
            0 => bad.network.0[0] ^= 1,
            1 => bad.device = f.admin.public_key().device_id(),
            2 => bad.epoch.0[0] ^= 1,
            3 => bad.key = SoftSigner::generate().public_key(),
            4 => bad.issuer = f.member.public_key().device_id(),
            _ => bad.signature[0] ^= 1,
        }
        assert!(f.roster.install_presence_binding(bad).is_err());
    }
    let bad = Binding::issue(good.network, good.device, good.epoch, good.key, &f.member).unwrap();
    assert!(f.roster.install_presence_binding(bad).is_err());
    let epoch = Author {
        roster: &f.roster,
        signer: &f.member,
    }
    .update(f.member.public_key().device_id(), DeviceUpdate::default())
    .unwrap();
    assert!(f.roster.insert(epoch).is_err());
}
#[test]
fn revocation_rotation_and_removed_members_never_restore_old_bindings() {
    let mut f = Fixture::new();
    let old = f.enroll();
    let member = old.device;
    f.epoch();
    assert!(
        f.roster
            .state()
            .active(&member)
            .unwrap()
            .record
            .presence_key
            .is_none()
    );
    assert!(f.roster.install_presence_binding(old.clone()).is_err());
    f.presence = SoftSigner::generate();
    let new = f.enroll();
    assert_ne!(old.key, new.key);
    assert!(f.roster.install_presence_binding(old).is_err());
    let remove = Author {
        roster: &f.roster,
        signer: &f.admin,
    }
    .remove(member)
    .unwrap();
    f.roster.insert(remove).unwrap();
    assert!(f.roster.install_presence_binding(new).is_err());
}
#[test]
fn concurrent_epoch_order_converges_without_losing_normal_updates() {
    let mut f = Fixture::new();
    let prior = f.enroll();
    let one = Author {
        roster: &f.roster,
        signer: &f.admin,
    }
    .update(prior.device, DeviceUpdate::default())
    .unwrap();
    // A second admin creates a different entry at the same causal point.
    let signer = SoftSigner::generate();
    let mut record = DeviceRecord::new(signer.public_key(), "second-admin", DeviceKind::Desktop);
    record.role = muster_core::Role::Admin;
    let add = Author {
        roster: &f.roster,
        signer: &f.admin,
    }
    .add(record)
    .unwrap();
    f.roster.insert(add).unwrap();
    let two = Author {
        roster: &f.roster,
        signer: &signer,
    }
    .update(prior.device, DeviceUpdate::default())
    .unwrap();
    let mut a = f.roster.clone();
    let mut b = f.roster.clone();
    a.insert(one.clone()).unwrap();
    a.insert(two.clone()).unwrap();
    b.insert(two).unwrap();
    b.insert(one).unwrap();
    assert_eq!(a.state(), b.state());
    assert!(a.install_presence_binding(prior.clone()).is_err());
    assert!(b.install_presence_binding(prior).is_err());
}

#[test]
fn a_concurrently_removed_admin_cannot_backfill_a_presence_binding() {
    let mut f = Fixture::new();
    let second = SoftSigner::generate();
    let mut record = DeviceRecord::new(second.public_key(), "admin-2", DeviceKind::Desktop);
    record.role = muster_core::Role::Admin;
    let add = Author {
        roster: &f.roster,
        signer: &f.admin,
    }
    .add(record)
    .unwrap();
    f.roster.insert(add).unwrap();
    let removal = Author {
        roster: &f.roster,
        signer: &second,
    }
    .remove(f.admin.public_key().device_id())
    .unwrap();
    let binding = f.enroll();
    f.roster.insert(removal).unwrap();
    assert!(f.roster.install_presence_binding(binding.clone()).is_err());
    assert!(
        f.roster
            .state()
            .active(&binding.device)
            .unwrap()
            .record
            .presence_key
            .is_none()
    );
}

#[test]
fn empty_epoch_keeps_the_existing_v1_update_wire_shape() {
    let mut f = Fixture::new();
    let epoch = f.epoch();
    let signed = epoch.signed_bytes();
    let body_offset = 1 + 32 + 4 + 32 * epoch.parents.len() + 8;
    assert_eq!(signed[0], 1);
    assert_eq!(signed[body_offset], 2); // original Update tag
    assert_eq!(
        &signed[body_offset + 1..body_offset + 33],
        &f.member.public_key().device_id().0
    );
    assert_eq!(&signed[body_offset + 33..body_offset + 37], &[0, 0, 0, 0]); // name, role, scopes, vip all absent
    assert_eq!(signed.len(), body_offset + 37 + 32); // original signer field, no extension
    assert_eq!(Entry::decode(&epoch.encode()).unwrap(), epoch);
}
