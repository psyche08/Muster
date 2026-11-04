# ADR 0120: `locked.control` authorizes Locked Use without a per-operation signature

- Date: 2026-11-04
- Status: Accepted; source and tests verified
- Supersedes the per-operation Presence requirement of
  [ADR 0007](0007-desktop-manual-locked-use.md) for `locked.control`; that
  decision's transaction, cover, watchdog and relock are unchanged.
- Related: [ADR 0006](0006-desktop-presence-and-console-routing.md) (enrollment
  and console routing), [ADR 0087](0087-idle-compaction-on-a-locked-or-busy-mac.md)
  and [ADR 0097](0097-task-scoped-scheduler-grant-for-locked-use.md) (the two
  leases that already carry no key), [LOCKED_USE.md](../LOCKED_USE.md)

## Context

Until now a remote member could not drive a locked Mac on its granted scope
alone. `muster_core::policy::authorize` answered `NeedsPresence` for
`locked.control`, and the Locked Use gate then demanded a signature, for every
operation, from a key enrolled for that member — a Presence key, or an enrolled
phone's device key. A member whose admin had granted `locked.control` but who
had no enrolled key was refused outright.

A device-verification session on 2026-11-04 walked the whole transaction on the
installed build: challenge issued, signature verified, display guard prepared,
`loginwindow` driven, the plugin consumed once, and then the watchdog stopped it
because a person touched the keyboard. Everything held. What the owner objected
to was not the machinery but the second approval:

- The network admin already decides who may drive a locked machine. Approving a
  joiner and granting `locked.control` **is** that decision, made once, at the
  moment a device enters the network.
- The requesting device has already authenticated its own user — it is unlocked
  and signed in — and its connection is authenticated by its device key. Asking
  a person to approve again, per operation, re-verifies something the join
  already settled.
- The practical effect was that a member with the scope still could not act,
  because enrollment is a separate step that is easy to omit.

This is a deliberate reduction in authorization strength, chosen by the owner
after the alternative was described.

## Decision

**`locked.control` is the authorization.** A member that holds the scope, on a
machine whose local policy allows locked control, may open a Locked Use
transaction on its granted scope with no per-operation signature.

- `Permission::high_risk` now names `NetworkAdmin` alone. `authorize` therefore
  answers `Allow`, not `NeedsPresence`, for `locked.control`, and every site
  that used to read `NeedsPresence` as "granted, now get a signature" reads
  `Allow` as "granted".
- **Administrative RPCs are unchanged.** `NetworkAdmin` stays high risk and
  still requires an operation-bound Presence proof. Joining a network, approving
  a joiner, changing scopes and enrolling or revoking a Presence key are exactly
  the operations that keep asking, which is where the owner wants the prompt.
- The Locked Use lease carries `Authorization::Scope` instead of a key pair.
  `arm` and `renew` no longer verify a signature; they still require the exact
  outstanding challenge and the same requester, so an operation remains bound to
  one request and one principal and cannot be replayed.
- A browser call while the screen is locked (ADR 0007's separate, no-unlock
  path) follows the same rule: `locked.control` admits it, with no presence
  message to sign.
- Enrollment itself is not removed. A Presence key still identifies a desktop
  for console routing (ADR 0006) and still authorizes `NetworkAdmin`.

**Everything else about the transaction stays exactly as it was**: a freshly
observed locked console, the local `allow_locked_control` / `offer_agent` /
`offer_terminal` switches, the app allowlist, the cover over every display and
its watchdog, one-time plugin consumption, the local-input stop, the deadline,
draining, and the relock before the cover is released. Revocation still takes
effect mid-lease: the gate re-runs the authorization on every tick and stops
the transaction when it no longer holds.

## Alternatives

- **Keep the signature and make it per-device, chosen by the admin.** This was
  the first proposal. It keeps a second factor for the devices that want one,
  but it adds a policy axis for a prompt the owner does not want anywhere, and
  it leaves the "scope granted, still refused" trap in place for every device
  the admin has not marked.
- **Drop the requirement for everything high risk.** That would take the
  Presence proof off network administration too, so a member could change its
  own scopes without a person approving. Rejected: the join and admin prompts
  are the ones the owner explicitly wants kept.
- **Keep requiring an enrolled key but accept any of them without a prompt.**
  The key would then prove only that enrollment once happened, which the roster
  already records, while still refusing members that have no key.
- **Gate it on a new local switch.** `allow_locked_control` already is that
  switch, and it is checked at issue and on every tick.

## Consequences

- **The "a person approved this unlock" property is gone.** With
  `allow_locked_control` on and the scope granted, a member can make a locked
  Mac unlock itself briefly without anyone approving at that moment. The
  remaining bounds are the admin's grant, the local switches, the app
  allowlist, the cover, the watchdog, the local-input stop, the deadline and
  the relock. The owner accepted this tradeoff knowing it.
- A member with `locked.control` and no enrolled key now works, where before it
  was refused with "requesting desktop has no enrolled Presence key".
- Nothing changes for this Mac's own leases: the idle-compaction lease and the
  scheduler grant already carried no key and keep their own admission paths.
- The protocol is unchanged. The signature field stays in the challenge
  exchange and is ignored for these leases, so an older peer that still signs
  interoperates with a daemon that no longer checks.
- Revoking `locked.control`, or turning the local switch off, is now the only
  way to take the authority back, and it takes effect on the next gate tick.
  **Revoking a member's Presence key no longer does it**, because no step
  consults that key any more; it clears the binding and nothing else.
- A browser call while locked was bound to its exact body by the presence
  message it signed. Without a signature only the `op_id` idempotency remains,
  which costs nothing against a member that is authorized to send any body.

## Verification

Cover in shared Rust: a member with `locked.control` and no enrolled key can
open and arm a lease; `NetworkAdmin` still answers `NeedsPresence`; the gate
still stops a live transaction when the scope is revoked; the challenge and
requester still have to match. Cover in the daemon: a locked browser call
needs only the scope; an administrative RPC without a presence proof is still
refused. Run the workspace tests, strict clippy, the layer check, and the
repository's documentation gates. Then install on the owner's Mac and walk a
real locked-screen operation from another member, with no signature.

The daemon's locked-use fixture no longer enrolls a Presence key, so its whole
suite now runs on the granted scope alone. That exposed a pre-existing flake:
four tests asserted `unlock_started` immediately after a helper's fixed 120 ms
settle sleep, which a machine running the full workspace suite in parallel can
outrun. They now wait for the flag.

## Implementation verification

Recorded with the evidence once the branch is built and installed.
