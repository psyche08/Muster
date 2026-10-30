# ADR 0068: Retry and ignore unconfirmed-send notices

- Date: 2026-10-30
- Status: Accepted
- Related: supersedes ADR 0061's visible interaction and ADR 0046/0048's inspect-before-explicit-release UI policy; ADR 0063

## Context

The owner requests two actions, Retry and Ignore, and dismissal of the orange
notice after either action. Rechecking a receipt does not retry a send, while
ignoring a notice does not assert that delivery occurred.

## Decision

Swift renders two buttons. Retry uses the existing composer send intent with
its current text, attachments and draft revision. Shared Rust closes the matching
notice before normal send validation, then uses the existing authorization,
Locked Use, durable request and dispatch path with a fresh operation ID.

Ignore is a distinct intent flag, not the existing delivered assertion. Rust
persists the dismissed operation token in the pending-send journal, hides that
notice and its local pending presentation, and preserves the composer and actual
provider transcript. It makes no write RPC and manufactures no delivery receipt.
Read-only reconciliation may still accept a real late receipt, but cannot reopen
a dismissed notice. Navigation and restart retain dismissal. The next explicit
normal send carries the ignored latch token through the existing host release
path; replacing the journal consumes that token atomically. New failed attempts
can show their own notice. Stale tokens cannot dismiss or retry another operation.
Legacy recheck and manual-confirmation intents retain their existing meaning.

## Alternatives

Renaming recheck to Retry would not resend. Renaming manual delivery confirmation
to Ignore would falsely assert delivery. Swift-local hiding would lose dismissal
on navigation and leave the durable host latch blocking subsequent sends.

## Consequences

An explicit retry can duplicate a message whose original delivery is unknown;
there is no automatic replay. The host still reconciles evidence before releasing
a latch and rejects a release whose receipt already proves delivery. Ignore abandons the local pending presentation,
not the provider message. The original request remains available for read-only
reconciliation until settled or replaced by the next explicit send.

## Verification

Test immediate and durable dismissal, unknown/late callbacks, real late receipts,
new drafts, stale tokens, host-only refusals, normal retry dispatch and subsequent
normal sends. Check generated bindings and compile the shared Apple UI. Record
source tests separately from signing, installation and live provider acceptance.

### Source and build evidence

The final agent, app-core and protocol library suites pass 388 tests with five
pre-existing ignored cases. Five new recovery tests cover durable dismissal,
late terminal callbacks, stale/session-bound tokens, foreign host refusals,
current composer text/attachments, fresh IDs and one ordinary RPC dispatch after
Retry or Ignore followed by Send. Strict Clippy passes for all targets of those
three crates. Swift and HarmonyOS bindings were regenerated. The shared
MusterUI target compiles for macOS and arm64 iOS Simulator.

These are source/library and UI compilation results. No new signed bundle,
notarization, publication, installation or live provider execution was performed.
