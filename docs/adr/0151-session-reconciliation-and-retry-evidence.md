# ADR 0151: Content-aware session reconciliation and retry evidence

- Date: 2026-11-06
- Status: Accepted; source verified, device acceptance pending
- Related: ADR 0145, ADR 0147; architecture/services.md; architecture/runtime.md

## Context

Independent history and watch streams can deliver a newer revision of a message
before an older snapshot carrying the same ID. ID-only overlap discards that
update. Automatic reconnect also reuses a completed diagnostic marker, hiding
recovery model application. The live probe leaves a foreground refresh after two
seconds without requiring that refresh to finish.

## Decision

Shared Rust validates the last queued revision of each overlapping message
against the snapshot before discarding replayed events. A content mismatch keeps
the queue and uses the existing single same-subscription reconciliation retry;
unresolved evidence still reconnects. Do not infer revision order from text size,
timestamps, or arrival order across streams.

Every automatic retry gets a fresh Rust-owned trace and readiness state, linked
to the entry and preceding trace. Publish the new marker during retry backoff;
ignore stale native acknowledgements and cancelled watch generations. Native
shells continue to report observations using the existing opaque marker ABI.
Record history application only after the Rust model merge completes.

The live probe waits for a new ready Chat trace after every foreground event,
reports each completed refresh explicitly, and fails on a bounded timeout rather
than cancelling an unfinished refresh as if it passed.

## Alternatives

ID-only overlap loses in-place updates. Unconditionally replaying queued content
can overwrite a newer snapshot with an older event. Platform-side conflict
resolution duplicates Shared Rust state. Longer fixed sleeps cannot prove that
foreground recovery completed. Resetting a boolean on one trace would reuse the
native acknowledgement and publication deduplication keys.

## Consequences

Conflicting revisions can require one additional history request or a reconnect.
Each attempt retains bounded diagnostics with an independent record budget.
No transport, authorization, provider paging, or public ABI contract changes.
Two-Mac and physical iPhone acceptance remain separate from synthetic tests.

## Verification

Add deterministic RPC regressions for same-ID updates racing snapshots and for
successful entry followed by a disconnected watch with a delayed recovery
snapshot. Verify new false/true markers, rejected stale native callbacks, and new
native application evidence. Test foreground probe waiting and timeout behavior
against stale and delayed markers. Run focused application-core tests, example
checks, strict Clippy, public-content and ADR checks. Record results below before
landing. Production build, signing/notarization, installation, runtime, dual-Mac
and physical iPhone acceptance are not performed by this source fix; no OTA.

### Source evidence

The same-ID RPC regression failed before the fix by displaying the old message
body. The reconnect regression then failed with equal old/new trace IDs. Both
pass after the fix; recovery rejects the stale native acknowledgement and records
fresh pending/ready native observations under the new trace.

Application-core unit tests: 163 passed, one existing public-download test ignored.
All 10 chat-watch RPC integration tests passed, including existing queued bursts,
cache deltas, unresolved overlap, foreground replacement and history timeout
recovery. Both probe tests passed: stale ready and fresh unready states cannot
complete the wait, delayed fresh history can, and unfinished/left refreshes time
out. Strict all-targets application-core Clippy passed. Changed Rust files passed
format checks; public-content and ADR gates are required before landing.

Only Rust development/test artifacts were compiled. No production Apple build,
signing/notarization, installation, live native model application, dual-Mac or
physical iPhone acceptance is claimed. The prior build-115 candidate does not
contain this fix. No OTA was published.
