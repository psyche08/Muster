# ADR 0165: A locked permission-card read reserves no provider queue

- Date: 2026-11-09
- Status: Accepted; source and tests verified, locked-device acceptance pending
- Related: [ADR 0100](0100-locked-claude-permission-cards.md) (the
  permission-card read this repairs), docs/LOCKED_USE.md

## Context

ADR 0100 reads a local Claude permission card on a locked Mac with one
read-only `AgentInspect` through the existing signed Locked Use continuation.
On a locked target the iOS app showed, on every refresh, "Could not read
Claude's permission card: invalid: invalid: inspection cannot authorize a
provider queue", and no card appeared.

The app and the target disagree about that continuation:

- When a protected Chat request returns `LockedUseRequired`, the app builds the
  Locked Use operation and always sets `provider_queue` to the request's
  provider (`muster-app-core` `update/operations.rs`). That includes the
  read-only `AgentInspect` that `approval_cards::refresh` sends.
- The target's prepare refuses an `AgentInspect` that names a provider queue
  (`musterd` `locked_use::prepare`). The core lease agrees:
  `Gate::issue_inspection` issues a lease that "can never grant a queue", and
  `bind_provider_queue` rejects an inspection lease.

The target's rule is the intended one. An inspection lease is scoped to the
session's provider through the native operation the target resolves; it does
not reserve that provider's queue against other requesters. Both sides of the
mismatch date to the initial source; it surfaced when ADR 0100 sent
`AgentInspect` through this path. No test covered the app's prepare request for
an inspection, and no daemon test prepared a locked inspection.

## Decision

The app leaves `provider_queue` empty when the protected Chat request is an
`AgentInspect`. Every other protected Chat request keeps naming its provider's
queue, as before. The operation is still bound by its digest, turn and the
signed Presence proof; the target still resolves and authorizes the provider
from the session itself.

The target and the core lease rules are unchanged.

## Alternatives

- Let the target accept and ignore a provider queue on an inspection. That
  would weaken a deliberate lease invariant to accommodate a client bug, and a
  signed operation naming a queue it does not get would misdescribe itself.
- Bind inspections to the provider queue. A read-only inspection would then
  hold the provider against other requesters' writes, which ADR 0100 rules out
  ("No new unattended lease is introduced").
- Move the queue decision into a shared `muster-proto` helper used by both
  sides. Worth doing if more operation kinds diverge; for one kind it adds an
  indirection without removing the target's own checks.

## Consequences

Permission cards can be read on a locked Mac again, subject to the existing
policy, Presence, cover, watchdog and relock requirements. The fix is in the
requesting app; the target daemon needs no update. Until the fixed app is
installed on the requesting device, the error continues.

## Verification

Source and tests:

- `muster-app-core` test
  `a_locked_permission_card_read_reserves_no_provider_queue` drives
  `Msg::ChatControl` against a real RPC fixture that answers
  `LockedUseRequired` and applies the target's prepare rule. It asserts that the
  inspection's prepared operation names no queue and that a completion's still
  names `claude`. With the fix reverted, the test fails with "invalid:
  inspection cannot authorize a provider queue", the error seen on the device.
- `musterd` test
  `a_locked_inspection_is_prepared_only_without_a_provider_queue` prepares a
  locked inspection through the real owner: one naming a queue is refused before
  any OS work, and one without a queue is issued a challenge for its turn.
- Affected crate suites, strict Clippy and repository gates: see the pull
  request.

Not verified here: building and installing the apps, and reading a permission
card from a physically locked Mac. No provider approval was executed.
