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

The 44 Locked Use integration tests also passed, including challenge-field
validation, wrong signer/principal rejection, revocation and terminal authority.
Strict Clippy, formatting, public-content and ADR checks passed.

Commit `b0f91b4f` was built for macOS and physical iOS. Developer ID signing,
Presence entitlements, release ancestry and bundle verification passed for the
Mac. The iPhone build passed Xcode signing and bundle verification. Both were
installed in place; the target daemon reports the exact commit, preserves its
hardware identity, advertises the new capability and has healthy permissions,
virtual network and relay. Its original six iTerm2 sessions and one Ghostty
session remain present.

### Physical acceptance follow-up

The protected Ghostty creation path passed on the physical phone: shell startup,
exact burst input/output, background relock, foreground recovery and leaving-view
relock. The iTerm2 regression exposed a routing issue: making TerminalCreate a
signable operation caused the generic remote native-operation guard to demand
Locked Use for iTerm2 too. Exclude terminal creation from the agent queue and
that generic guard; the explicit source-specific Ghostty check remains in force.
Keep the ordinary console-write reservation for both sources. A remote
principal RPC regression now covers this route; a local-principal fixture did
not exercise that guard. All 58 target tests and strict Clippy passed again.

The target was rebuilt, Developer ID signed, verified and installed from
`0636dce5`; the physical phone retained its signed `b0f91b4f` client, whose code
was unchanged by the target-only routing correction. Final physical acceptance
passed for both native sources through Device Hub:

- iTerm2: new native window, exact rapid alphanumeric input and Return/output,
  with the Mac verified locked before creation and after output.
- Ghostty: new native window created after verified unlock under cover; the
  owner confirmed relock before the separate terminal lease opened. Shell
  startup and exact rapid input/output passed on the final target build.
- Existing Ghostty recovery: phone background caused relock; foreground
  reattached and accepted a new command; leaving the view relocked again.
- All owned test windows were closed. The original six iTerm2 and one Ghostty
  session identifiers matched the initial inventory exactly. Final target
  state was locked with no manual lease, and phone keyboard capture was off.

Device Hub initially diverted clicks to other phone apps; reconnecting its
screen sharing and dismissing notification banners within the phone viewport
restored reliable control. This does not establish a Muster defect or require
any phone permission/settings change. Physical punctuation/Unicode coverage
remains limited as recorded in ADR 0043; the ordered RPC fixture covers those
bytes. No update-feed publication or notarization was performed.
