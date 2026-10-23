# ADR 0045: Bound native send scans and settle receipts after relocking

- Date: 2026-10-23
- Status: Accepted
- Related: Architecture §3.6; ADR 0027, ADR 0038; LOCKED_USE.md

## Context

Device traces show millisecond draft writes and presses surrounded by repeated
Accessibility ancestry scans. Already-visible sessions still traverse navigation
and discovery. Some provider receipts arrive after the ten-second synchronous
wait; a one-shot Locked Use lease can expire while only file evidence is pending.
Local input changes also invalidate otherwise expensive preparation snapshots.

## Decision

Keep validation and orchestration in Shared Rust. Validate ancestry by a bounded,
cycle-checked parent path with reciprocal child edges; fall back to the existing
bounded scan when the OS cannot supply that path. Never treat a cached element,
coordinate, incomplete scan or enabled flag as submission authority.

A retained native controller can freshly prove that the exact unique target
surface and composer are visible, avoiding redundant navigation/discovery.
Preparation still checks live authority and user-input generation; the send
still resolves current controls after focus/write and validates before one press.
Start existing background discovery immediately, retaining its provider-queue
priority and cadence. Do not introduce a second high-frequency scanner.

For native agent sends under Locked Use, give immediate provider receipts a short
bounded wait, then end native work with an unknown result. The existing owner
still drains OS work and confirms relock before releasing cover. Outside that
owner, wait read-only for the exact original ledger binding, with cancellation,
permission checks and a bounded deadline. A late durable receipt may return an
applied result; absence of evidence stays unknown. No operation is replayed and
no unlock lease is renewed for receipt collection. Preserve the existing
controller's late-receipt latch and ledger confirmation. Terminal and question
flows keep their existing behavior.

Record fast-path selection and receipt handoff timings without request contents.
Use existing exported client send diagnostics for end-to-end correlation.

## Alternatives

Startup-only coordinate caching becomes stale after navigation and editor
replacement. Removing final verification, ignoring local input or automatically
retrying unknown submissions is rejected. Extending the unlock lease to cover
provider persistence latency unnecessarily holds the desktop unlocked. Full AX
notification-driven subtree caching is deferred until this bounded fast path
has physical timing evidence; discovery already warms controllers.

## Consequences

Normal sends should spend less time in redundant traversal; slow provider writes
no longer need to consume the console lease. Actual speedup requires comparable
physical samples. Local input protection and final exact-draft validation remain.
Read-only receipt waiting must not retain console ownership or admit another write.

## Verification

Test ancestry cycles, missing/replaced edges, budgets, ambiguous controls and
stale controllers; test exact ledger bindings, cancellation/revocation, late
receipts after relock and no replay. Run affected Rust suites, strict Clippy,
formatting, architecture/public-content/ADR checks. Record signed local builds,
installation, dedicated-session device acceptance and publication separately.

### Source verification

Affected agent, app-core, platform, store and daemon suites passed 434 tests
with six existing environment-dependent tests ignored. An additional owner
integration test passed: its single native submission returns unknown, the
owner confirms relock/releases ownership, then a late exact ledger receipt
resolves the still-pending caller without resubmission (435 distinct tests).
Strict Clippy passed; architecture and public-content checks passed. The client
also accepts an exact pending draft's late result after its lease ends without
closing a newer lease or changing a newer draft. Physical acceptance is recorded below.


### Signed builds and physical acceptance

The macOS Developer ID build and matching physical iOS build succeeded from
source commit `66a39fbf`. Presence entitlements, signatures and release ancestry
checks passed. All three acceptance Macs installed the same daemon digest,
retaining their identities, SEP backends, Accessibility grants and network
readiness. The matching iOS app installed and ran on a physical phone. This
acceptance run did not notarize or publish a desktop feed or iOS OTA release.

Small before/after samples from dedicated native conversations show:

| Case | Before: prepare + send including receipt | After | Visible-controller path |
| --- | --- | --- | --- |
| Desktop A, Codex, already-visible baseline | 339 + 1029 = 1368 ms | 145 + 776 = 921 ms; 105 + 577 = 682 ms | Confirmed in both new samples |
| Desktop B, Claude, already-visible baseline | 357 + 1432 = 1789 ms | 151 + 899 = 1050 ms | Confirmed |

These are daemon stage timings, not model response latency or a population
benchmark. Provider receipt waits varied; the local build machine also had
build activity during baseline sampling. A separate baseline requiring navigation
is retained privately but excluded from the already-visible comparison.

Physical phone acceptance passed for Desktop A's Codex conversation, including
navigation back to a target that was no longer visible. Desktop B was confirmed
locked before a phone-originated Claude send. Logs showed a verified operation,
unlock under cover, the exact submission and provider receipt, queue drain,
confirmed relock and cover release; the phone displayed the unique reply. The
native prepare stage took 281 ms and send including receipt took 840 ms. The
receipt arrived in 101 ms, so the slow-receipt handoff is covered by the owner
integration test, not claimed as a naturally delayed physical sample.

Desktop C installation, runtime and dedicated-conversation acceptance passed.
After the operator paused competing UI activity, a new native Claude conversation
was created without a project folder and acknowledged its isolated test rules.
A Muster-originated marker then appeared exactly once as a user message and once
as a provider reply. The visible-controller path took 108 ms to prepare and
475 ms to send including a 101 ms receipt wait (583 ms combined). There is no
same-session old-build baseline for this new conversation. Earlier attempts
while unrelated foreground navigation continued entered no test text into
business conversations. No unknown submission was retried.
