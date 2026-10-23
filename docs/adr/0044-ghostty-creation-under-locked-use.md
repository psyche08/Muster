# ADR 0044: Create Ghostty windows after an authorized unlock

- Date: 2026-10-23
- Status: Accepted
- Related: [ADR 0035](0035-native-terminal-dev.md), [ADR 0043](0043-ordered-terminal-input.md), LOCKED_USE.md

## Context

Physical-phone acceptance found that Ghostty can return a new window identifier
while the Mac is locked even though its renderer failed to initialize and no
shell started. The resulting window remains blank after unlocking. Existing
Ghostty sessions work through their existing scoped terminal leases. This is
consistent with the upstream report:
https://github.com/ghostty-org/ghostty/discussions/13248.

## Decision

Bind the complete native creation request into a one-shot Locked Use operation
in Shared Rust. Refuse ordinary Ghostty creation while locked before any window
or ledger mutation. A foreground phone signs the exact network, target, caller,
operation ID and request digest. The target requires Terminal control and
Locked Control, creates only the requested Ghostty window after confirmed
unlock under cover, drains in-flight OS work, and confirms relock before
returning the session identifier. Creation cannot authorize other sessions,
provider queues, renewals, or arbitrary device access. Opening that resulting
session uses a separate existing terminal lease.

Cancellation/background/navigation withdraws the one-shot operation. Ambiguous
creation is never automatically retried. The existing creation ledger keeps
the operation ID bound to its complete request. iTerm2 creation remains usable
without unlocking. Platform applications render and forward intents only.

## Alternatives

Returning a window ID without a shell falsely claims success. Recreating the
window silently after attachment would change the session identity already
bound to a terminal lease. An automatic device-wide lease would grant broader
access than the selected creation action. Requiring a local unlock would leave
the requested remote workflow incomplete.

## Consequences

Locked Ghostty creation requires updated target and phone code. It includes
one authorized unlock/relock cycle before the ordinary terminal attachment
cycle. Existing OS permissions and user-enrolled keys remain prerequisites.
No password handling, permission changes, or authorization bypass is added.

## Verification

Automated tests pass for binding every creation field, Terminal control without
Agents authority, refusal before mutation, queue/renewal rejection, waiting
for verified unlock, cancellation while OS work drains under cover, confirmed
relock before reply, and ledger deduplication. Client tests cover old-target
refusal, background/navigation cancellation and ignoring late replies. The
five affected crate suites passed 223 tests with five environment-dependent
ignored tests.

Physical iPhone creation/input/output from a locked target, updated signed
Mac/phone builds and installation remain pending. No update-feed publication
is implied.
