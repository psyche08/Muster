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

## Two-Mac acceptance on 2026-11-09

Two Apple Silicon Macs ran 1.1.27 (117), commit
`7b489084f782d13320c6de806284803a2ff727b3`. Mac A already ran that build.
Mac B upgraded from 1.1.24 (114) using the notarized ZIP from
[ADR 0157](0157-publish-apple-1-1-27.md). The installer verified the package,
retained the hardware identity, and confirmed Accessibility, relay registration
and Vnet after restart. The peers reported a direct connection.

Each source ran the release commit's shared-core probe against its own daemon.
For each direction, all three entries and three foreground recoveries applied
50 messages on six distinct traces. All twelve traces succeeded on the first
attempt. The following medians are in milliseconds. Entry and foreground rows
use three samples each. Other rows use all six samples in the direction.

| Stage | Mac A to Mac B | Mac B to Mac A |
| --- | ---: | ---: |
| Watch first event since entry | 17.43 | 51.74 |
| History response since entry | 54.02 | 87.51 |
| Reconcile after receipt | 0.042 | 0.066 |
| History applied since entry, all six | 54.12 | 87.58 |
| History applied, entry only | 33.22 | 82.03 |
| History applied, foreground only | 66.91 | 93.14 |
| History RPC pool ready / stream allocated / service ready | 0.003 / 0.006 / 0.596 | 0.002 / 0.007 / 0.630 |
| History request handling on the target | 23.47 | 28.88 |

Each watch and history request recorded allocation of its own stream on a reused
connection. The RPC milestones use the request's source clock. The target
duration uses the target's clock. The history-applied ranges were 32.83–71.98 ms
and 72.44–173.46 ms, respectively.

Both directions also exercised [ADR 0148](0148-per-device-provider-catalog-pages.md).
Claude and Codex returned ten roots initially and twenty after Load more on both
targets. DeepSeek on Mac A returned ten then twenty. Orca returned its available
seven on Mac B and one on Mac A on both reads. Independent cursor reads found
zero overlapping root IDs across the first and second pages. Both live catalogs
reported zero pinned roots. The release commit's mixed Pin/Recent regression
test passed with both kinds sharing the same provider budget.

Mac A opened a remote Codex session on Mac B using the installed native app.
On that trace history applied at 94.35 ms, native decode was acknowledged at
111.86 ms and main-thread model application at 121.46 ms. The recorded decode,
main-queue wait and apply durations were 0.903, 9.535 and 0.022 ms. These
measurements end at model application.

In the reverse direction, the operator opened a session hosted on Mac A in
Mac B's installed Muster app and confirmed that the history was visible. Both
native windows have therefore displayed remote history. On Mac B, the GUI
process and local daemon were running build 117. Its GUI user matched the
console user. The cache directory returned by Foundation was writable, but
its MusterDiagnostics subdirectory and performance log were absent when
checked after the observation. The cause is undetermined. Reverse native
decode and main-thread application timings remain unmeasured. The probe
timings above cover the shared core. Physical iPhone lifecycle and
first-frame latency are outside this run.
