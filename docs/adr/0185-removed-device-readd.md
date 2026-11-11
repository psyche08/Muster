# ADR 0185: A removed device may rejoin through a fresh admin-approved Add

- Date: 2026-11-11
- Status: Accepted; source and tests verified, paired-device acceptance
  pending
- Related: architecture §3.4 ([identity-and-trust.md](architecture/identity-and-trust.md)),
  [PROTOCOL.md](PROTOCOL.md), ADR 0183 (locked Codex creation stages, adjacent
  pairing work)

## Context

A phone removed from a network can never rejoin: the roster folds Remove as a
permanent tombstone, and `Roster::insert` rejected every `Add` naming a device
id that had ever appeared, with "device already in roster (removed ids are
never re-added)". The failure surfaced to the user as an opaque
`protocol: admin closed the pairing stream` on the phone, because the admin's
`author_add` failed after the user had already approved the pairing dialog.
With no in-app identity reset, the only workarounds were deleting the iOS app
(and hoping the hardware key is not resurrected) or wiping the admin's whole
network state — both observed in practice on 2026-11-11.

Permanence served one purpose: a removed device's authorization could not be
resurrected. But the anti-backfill guarantee the roster actually needs is the
separate removed-admin rule — an accepted `Remove(X)` invalidates entries X
signed that the Remove does not causally include, so neither a stolen admin
key nor the removed device itself can backfill history. A *new* `Add` of X,
signed by an active admin causally after the Remove, is a fresh explicit
authorization and does not weaken that guarantee.

## Decision

1. `Roster::validate` accepts an `Add` whose device id is absent or removed in
   the causal past; only a currently active member may not be added twice.
2. `State::apply` treats an `Add` for an existing (removed) member as a
   re-add: the new record replaces the tombstone wholesale — name, kind,
   scopes, role, key — `removed` clears, and `presence_epoch` resets to
   `None`, so no Presence binding anchored before the Remove can revive
   (`Binding::verify` requires the member's current epoch to match).
3. The removed-admin rule gains one exception: an entry signed by X survives
   a `Remove(X)` when an accepted re-add of X folds after the Remove and lies
   causally before the entry. Entries X signed before the Remove, including
   concurrent forks the Remove does not include, stay invalid — the stolen-key
   backfill defense is unchanged.
4. `Author::add` gives a re-added desktop or server its previous vip (no
   active member holds it; the device's stale routes still point there); a
   first-time device keeps getting the lowest free address.

The pairing flow needs no change: the admin's approval dialog already
requires an explicit human verdict, so a rejoin is always a deliberate act.

## Alternatives

- Keeping the tombstone permanent and adding an in-app identity reset on
  Apple platforms: preserves the invariant but punishes every ordinary
  "remove then re-pair" cycle, and a reset key would need its own secure
  deletion story in the Secure Enclave.
- A dedicated `Rejoin` body: a distinct wire type only to express "Add with
  relaxed history" — the codec, the fold, and every binding regen would grow
  for no behavioral difference over a plain `Add` after `Remove`.
- Deleting or editing roster rows in the admin's database: impossible without
  breaking signature and parent verification, and it would fork the network.

## Consequences

- Removing a device is reversible by an explicit admin act; the admin dialog
  must be understood as re-granting access to whoever holds the device's
  hardware key. A device removed because it was lost or compromised should
  not be casually re-added — the same approval path that adds a stranger now
  also revives a removed one, and the UI shows the returning device's name
  and fingerprint either way.
- A re-added device gets a clean presence epoch and keeps its old vip.
- Re-adds participate in the same gossip, persistence, and relay paths as any
  entry; no wire format changed and no binding regeneration is needed.

## Verification

- Source: `muster-core` unit tests updated (`genesis_add_update_remove`) and
  extended (`readd_does_not_revive_pre_remove_entries`); the property test
  `removal_is_permanent_and_non_admins_cannot_author` now asserts a non-admin
  cannot re-add, an active admin can, and the re-added member counts as
  active.
- Tests: `cargo test -p muster-core`, `-p muster-net`, `-p musterd` all pass.
- Pending: an end-to-end re-pair of a previously removed iPhone against a Mac
  built from this change; the roster semantics behind the
  `admin closed the pairing stream` report of 2026-11-11 are covered by the
  unit tests above.
