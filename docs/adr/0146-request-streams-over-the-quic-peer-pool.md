# ADR 0146: Requests own service streams over the QUIC peer pool

- Date: 2026-11-06
- Status: Accepted; source verified, release pending
- Related: ADR 0145; architecture/network.md; architecture/services.md

## Context

The operator specifies requests -> connection stream -> QUIC pool. Two shared
traffic-class streams, as initially selected in ADR 0145, still multiplex
unrelated operations inside each class. Node already caches authenticated QUIC
connections by peer, serializes only connection establishment and opens independent
service streams through `Node::open`. The RPC layer should use that boundary.

## Decision

Supersede only ADR 0145's two reusable traffic-class RPC clients. Each forwarded
RPC call obtains its own service stream. Each embedded remote operation also
obtains its own stream; a long watch owns its stream until it ends, and its
history reconciliation uses a separate operation-owned stream. There is no
cross-operation RPC client cache or fixed request/subscription channel pair.

Reuse authenticated connections in Node's existing peer pool. A new service
stream does not mean a new QUIC connection or TLS handshake. Retain the existing
forwarded-call admission bound, QUIC stream flow control and connect/service
opening deadline. Exhausted credits wait within that deadline; do not open extra
connections to bypass the peer's limits. End or cancel owned RPC streams without
closing the shared QUIC connection or replaying uncertain mutations.

Keep ADR 0145's bounded event-queue isolation, smaller initial history, ordering
validation and same-subscription reconciliation. Apply the final architecture to
both desktop proxy and embedded mobile paths before publishing 1.1.24 (114).

## Alternatives

A fixed pair of RPC streams limits the damage but retains cross-operation
head-of-line blocking. A new QUIC connection per request repeats authentication
and congestion setup. An additional custom connection pool duplicates Node's
existing identity, lifetime and recovery decisions.

## Consequences

Each operation pays the existing service-open acknowledgement on a warm QUIC
connection. Subscriptions and large replies no longer share a remote byte stream.
Stream limits and cancellation remain explicit; relay outages can still interrupt
the shared underlying connection and require separate physical acceptance.

## Verification

Verify independent streams for concurrent and repeated forwarded requests, stream
cleanup on cancellation/EOF, late mutation outcomes and existing access proofs.
Use real loopback QUIC to prove concurrent distinct stream IDs share one stable
peer connection while another stream remains open. Run RPC, affected network and
app-core tests, strict Clippy, formatting, public-content and ADR checks. Rebuild
from clean main and record signing, publication, installation and before/after
cross-device latency separately from physical iPhone acceptance.

## Source evidence

The RPC/proto suites passed 35 tests, app-core passed 158 unit tests (one existing
ignored) and 8 chat-watch integration tests, and the real QUIC regression passed.
Nine distinct service stream IDs shared one unchanged peer connection; eight
round trips completed while the ninth stream waited for input, and that held
stream remained usable. Signed forwarding, bounded cancellation, confirmed late
mutation receipts and EOF behavior passed. Strict Clippy and changed-file
formatting passed. Installation and physical mobile acceptance remain pending.
