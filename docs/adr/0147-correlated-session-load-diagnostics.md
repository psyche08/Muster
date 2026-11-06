# ADR 0147: Correlated session loading diagnostics

- Date: 2026-11-06
- Status: Accepted
- Related: ADR 0038; ADR 0146; architecture/services.md; architecture/runtime.md

## Context

The installed mobile performance export contains periodic view-model statistics
and send phases but cannot attribute session-entry latency. Background suspension
also leaves gaps that cannot be interpreted as blocked requests. The operator
requests complete timing and initial validation using two Macs.

## Decision

Shared Rust owns a content-free session-load trace with an opaque correlation ID.
Record entry/retry, service acquisition, first watch event, each history request
and response, reconciliation, model publication, failure/cancellation and lifecycle.
Instrument pooled connection acquisition and service-stream admission independently.
Propagate read-only trace context and bounded typed timing events through RPC so
desktop proxy timing is visible to the same observer used by embedded clients.
Timing events never count as a watch's first business event or alter its outcome.
Record connection closure using fixed reason categories, never raw error text.

Apple adapters persist opaque records using bounded storage on both platforms.
They report native decode and main-thread model application timing using Rust's
trace token; this establishes model application, not proof of a rendered frame.
No request bodies, transcript text, raw identities, paths, tokens or credentials
enter diagnostic records. Lifecycle records remain event-driven in the background.
Old peers can ignore new optional fields; absent timing stays absent.

## Alternatives

Periodic CPU samples cannot identify RPC stages. Increasing timeouts hides the
unobserved delay. Raw request or network dumps expose content. Platform-owned
request state machines would duplicate shared behavior.

## Consequences

Small bounded records add work at phase boundaries. Wall clocks are used only
for correlation; each duration uses a monotonic clock within its own process.
Concurrent operations and cancelled/replaced sessions must remain distinguishable.
Two-Mac validation precedes any further mobile publication; physical iPhone
acceptance remains separate.

## Verification

Test phase order/correlation, success/failure/cancellation, ignored stale native
callbacks, diagnostic-event isolation, bounds/privacy and mixed export. Exercise
real cross-Mac session loads and capture phase records, checking the native
application boundary separately from headless core callbacks. Run relevant Rust
and Apple checks, generated-code/public-content/ADR gates. Record build/signing,
installation, runtime and any unverified device scope independently.

## Implementation evidence

Focused Rust checks passed: 160 application-core tests, eight RPC chat-watch
integration tests, 17 RPC tests, and 11 catalog tests (two existing ignored tests).
Three QUIC link tests and strict Clippy also passed. The new live probe exercises shared-core paging and cross-device entry without
printing conversation content. Native build and two-device acceptance are pending.
The two-Mac candidate uses 1.1.25 (115); no public OTA is claimed here.

## Acceptance refinement

A live pre-installation probe showed that foreground re-entry preserves the old
chat contents while reconnecting. Therefore diagnostic readiness is owned by the
trace and becomes true only when that trace applies its fresh history response;
a visible cached model must not be reported as a completed foreground refresh.
The live probe waits for a new trace on the Chat screen before accepting a round.
