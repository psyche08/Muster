# ADR 0100: Observe and retain local Claude permission cards across lock

- Date: 2026-11-03
- Status: Accepted; source verified, device verification pending
- Related: ADR 0099, ADR 0018, docs/LOCKED_USE.md
- Supersedes: ADR 0099 permission-card removal merely because the console locks

## Context

Device observation confirms that a local Claude permission card is delivered
while unlocked but disappears when the AX window is unavailable after locking.
A newly attached controller also polls only after a short inspection has ended.
The owner requires permission cards to remain visible with the source Mac locked.
Registry metadata contains no pending permission card; transcript activity alone
cannot supply a command or authorize a decision.

## Decision

Keep the last complete, observed local Claude permission card in Rust memory
while the same provider process is confirmed running and the console is locked.
Locking alone does not resolve a provider request. Process uncertainty/exit,
identity loss while unlocked and incomplete unlocked observations still withdraw it.
A lock arriving during an AX read can preserve display, but that failed read
cannot admit a response.
Retained native handles never authorize input: every response refreshes the
exact session and card and rejects changed content, options or native elements.

For a foreground local Claude Code conversation that reports waiting for input
without a required interaction, request one read-only AgentInspect per waiting
episode through the existing signed Locked Use continuation. Capability-check
the target first. Inspection synchronously refreshes interactions before it
returns, so a short covered operation cannot miss the polling interval. Streamed
Rust interactions supply the card; clients do not infer it from diagnostic JSON.
Navigation, backgrounding and cancellation use the existing continuation epoch
and watch token. Never loop temporary unlocks or synthesize grant options. The
existing policy, Presence, provider queue, cover, watchdog and confirmed relock
requirements remain intact. No new unattended lease is introduced.

## Alternatives

Reading Claude Remote Control or its credentials violates the local-only scope.
Inferring a card from transcript tool calls cannot prove provider authorization.
Caching a grant or pressing a stale AX handle would permit a later request to
consume an earlier decision. Permanent covered unlock and periodic unattended
polling exceed the necessary read-only operation.

## Consequences

An observed card remains a last observation until refreshed or resolved; it is
not a claim of continuous observation while locked. Missing cards can require
the existing desktop Presence prompt. Disabled Locked Use remains an explicit
failure. Restart loses the in-memory card and a foreground view reads it again.
Provider approval execution remains a separate manual acceptance step.

## Verification

Test lock retention separately from process/identity loss, changed and incomplete
cards, inspection refresh before return, one refresh per waiting episode,
required-question priority, cancellation and capability refusal. Run affected
Rust tests, strict Clippy and repository gates. Keep source, build, signing,
notarization, deployment and physical locked-screen display evidence separate.
Do not answer the user's pending provider command during acceptance.

### Source verification

The affected library suites passed: agent 310, app-core 128, platform 38 and
daemon 97 tests, with existing ignored tests unchanged. Two real-RPC provider
queue tests also pass and assert interaction refresh precedes diagnostic return.
Strict Clippy passed for all targets of the four affected crates. The final
bounded-refresh test was rerun after preventing in-flight state changes from
rearming inspection. Layer and public-content checks pass. The current stable
rustfmt also normalizes existing formatting in a small set of web-related files
so the repository-wide format gate passes; those changes are formatting only.
Physical locked display and cold-controller inspection remain to be verified;
no provider approval has been executed.
