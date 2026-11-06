# ADR 0146: Requests own service streams over the QUIC peer pool

- Date: 2026-11-06
- Status: Accepted; published and installed on desktops; physical iPhone acceptance pending
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

## Release and installed-runtime evidence

Apple 1.1.24 (114) was built locally from clean main
`c4f379b7770d0728a11ad927deaaf6d6046ae817`. Developer ID signing,
Apple notarization (Accepted, `8c3e3e67-a152-4c50-855f-c07c7871cba8`),
stapling, Gatekeeper assessment and embedded daemon source verification passed.
The portable app and full update package have matching code-directory hashes.
The paired iOS archive was signed and packaged for OTA; no TestFlight external
submission was made.

All 14 public release files passed SHA-256 readback. The compiled public key
verified the feed signature, and the public page's install link resolved through
the build-114 manifest to the matching immutable IPA. Cache policy passed.
After this verification, exactly the four explicitly approved superseded 1.1.23
files were deleted. Current-file hashes were unchanged; the two old package URLs
returned 410 and the two old manifest URLs resolved to build 114.

Three installed Macs reported 1.1.24, build 114 and daemon commit `c4f379b` with
matching daemon hashes. Device IDs, fingerprints and Secure Enclave identities
were preserved; Accessibility, Relay registration and Vnet recovered. The active
managed scheduler recovered; stopped containers were left stopped. Old installed
program copies were removed immediately after atomic replacement; app data was
preserved.

The same read-only cross-Mac probe started a 500-message history request, then
started a watch 20 ms later. In six pre-update rounds, first-watch-event latency
was 1066–1840 ms (median 1183.5); after installation it was 260–667 ms (median
572). Every updated watch returned before its concurrent history completed
(1493–2945 ms). History content changed during the work: 401 messages/1,074,347
protobuf bytes before and 500 messages/1,097,815 bytes after. This is observed
stream isolation and a latency improvement on this link, not a controlled claim
about every network or physical iPhone UI.

Separate entry probes for the two reported provider sessions returned all six
initial states in 134–364 ms and their 50-message histories in 256/268 ms. A
60-second subscription received 48 state and three message events, ending only
at the probe's intentional cancellation deadline. That probe's initial event took
1651 ms including CLI startup/target resolution, so variable connection-entry
latency remains visible. No message-generation-to-render latency was measured.
The operator's iPhone must still install build 114 through OTA and validate
loading and live updates; the previously observed relay outages remain a
separate unresolved condition.
