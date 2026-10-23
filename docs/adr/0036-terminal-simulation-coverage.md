# ADR 0036: Preserve native terminal events and bound cancelled requests

- Date: 2026-10-23
- Status: Accepted
- Related: [ADR 0035](0035-native-terminal-dev.md), [ADR 0033](0033-managed-terminal-compatibility.md)

## Context

Native host routing has simulation coverage, but iTerm2's notification coalescer
can discard termination and cursor-only changes. Cancelled requests can retain
pending response entries, and close responses are not checked for refusal.
These failures can be verified without native GUI permissions.

## Decision

Keep terminal behavior in Shared Rust. Preserve termination during coalescing,
publish cursor-only frames, resynchronize after notification loss and propagate
buffer/close failures. Bound request queueing and response wait together, remove
pending entries on cancellation, and do not replay submitted input.
Unsupported Ghostty resizing returns an explicit error instead of a successful
no-op receipt; its existing native window sizing policy is unchanged.

Extend simulated protocol, native RPC and app-core regression coverage for the
existing terminal capabilities. Record a reproducible acceptance matrix, keeping
real GUI, permission, physical-device and latency acceptance explicitly pending.
Build only the isolated local dev bundle.

## Alternatives

Retrying mutations after connection failure may duplicate input or windows.
Relying only on happy-path GUI tests would miss deterministic event races and
would require permissions the operator has deferred.

## Consequences

No new host capability, native permission grant, installation or release is
introduced. An unreadable or refused operation becomes an explicit error;
simulation cannot prove native accessibility rendering or real device latency.

## Verification

Source verification on the final local revision:

- The full terminal/daemon/app-core suite passed 233 tests, with five opt-in
  tests excluded from its default run. Four real-tmux tests then passed on
  dedicated private sockets; the remaining ignored test is an existing manual
  app-core diagnostic. The reproducible script completed successfully.
- Protocol fixtures cover cursor-only changes, coalesced output/termination,
  lost notifications, out-of-order/late responses, queue and response timeouts,
  cancellation during connection setup/fetch, close refusal and a receipt racing
  disconnect. A child-process Unix WebSocket fixture reconnects for the next
  input without replaying the previous input or scanning inventory first.
- Seven native RPC fixture tests cover independent sources, slow/failed reads,
  simulated permission denial/recovery, exact IDs, duplicate/conflicting input,
  concurrent/ambiguous creation, unsupported resize and view-only cancellation.
  Remote scope tests reject native operations before adapter lookup. Shared
  app-core tests check both host choices and managed handles on the terminal's
  device despite an unrelated global selection, plus frame projection/guards.
- Strict Clippy, formatting, shell syntax, architecture-layer and public-content
  checks passed. The final dev bundle built and passed local signature checking.
  The ADR gate is checked on the committed branch before landing.

Runtime: the rebuilt dev app started a separate software-identity daemon, and
read-only local RPC confirmed readiness, no virtual interface and no configured
relay. It neither drove native windows nor changed system permission settings.

Signing: local ad-hoc signature only; certificate signing/notarization not run.
Installation/publication: neither macOS nor iOS was installed or published.
Real iTerm2/Ghostty GUI, macOS grants, physical iOS and end-to-end latency remain
pending. The local toolchain still warns about Rust runtime objects targeting a
newer macOS than the Swift package minimum; older-macOS compatibility is not
established by this dev build. Hosted Actions stays disabled under ADR 0016.

The full coverage matrix and commands are in [terminal development](../TERMINAL_DEV.md).
