# ADR 0145: Isolate slow subscriptions and bound chat entry history

- Date: 2026-11-06
- Status: Accepted; source verified, release pending
- Related: ADR 0135; ADR 0142; architecture/services.md; architecture/agent-halo.md

## Context

The operator reports stale conversations and session-watch timeouts on iOS
1.1.23, confirmed installed through OTA. The host also runs 1.1.23. Local watch
initialization took 26–53 ms and 50-message history 7–14 ms. Across another Mac,
500-message history took 915–1924 ms versus 151–653 ms for 50 messages. Diagnostic
JSON sizes were approximately 0.83–1.00 MB versus 0.04–0.17 MB; these are not
protobuf wire sizes or physical iPhone measurements.

The RPC client awaits each call's bounded event queue inside its shared reader.
A stalled subscription can block unrelated watch initialization and responses.
This requires a regression reproduction; it does not prove that this exact queue
overflow occurred on the phone. Host relay keepalive failures near the screenshot
times remain a separate observed network problem.

## Decision

Retain bounded event queues but never await a full per-call queue in the shared
reader. End that call with an explicit unavailable/resynchronize error and send
its correctly addressed cancellation. Do not silently drop events, report a
successful mutation or replay mutations. If cancellation cannot enter the bounded
outgoing queue, close the connection rather than spawning unbounded tasks.

Keep separate reusable request and subscription clients per remote peer, each on
its own service stream over the shared QUIC connection. The desktop proxy selects
the class from the request; the embedded client explicitly uses the subscription
class for agent watches and the request class for history. Serialize connection
setup only within one peer/class slot, allowing the two classes to open
concurrently. This avoids a large history frame blocking subscription frames on
one reliable byte stream without making a new transport handshake per request.

Request the most recent 50 messages on entry and bounded delta catch-up instead
of 500. Preserve content validation, gap/reset semantics, subscribe-before-history
ordering and older-history navigation in Shared Rust. Keep existing network and
heartbeat budgets; this change does not claim to fix relay instability.

Publish paired Apple 1.1.24 (114) from the clean main merge, using the local
compile/sign/notarize workflow and guarded three-Mac installation of ADR 0144.
iPhone direct installation remains deferred by the operator; provide the iOS OTA.

## Alternatives

Longer timeouts hide blocking. Unbounded queues or a task per excess event grow
memory without limit. Silent event loss can leave plausible incomplete history.
Fetching 500 messages makes old tool output part of time-to-current-content.

## Consequences

An overloaded subscription must resynchronize while unrelated calls continue.
Less history arrives initially; older messages remain pageable. Cellular-network
and actual device latency acceptance remain separate deployment requirements.

## Verification

First reproduce a full queue blocking another call. Verify call-local failure,
targeted cancellation, no stale success, ordered normal delivery and existing
proxy cancellation/security tests. Check two reusable peer streams under repeated
concurrent calls, history catch-up and older pagination with a smaller tail. Run
affected RPC/protocol/app-core tests, formatting, Clippy,
public-content and ADR gates. Record build, release, installation and physical
acceptance independently after execution.

### Source evidence

- Before the fix, the deliberately undrained 256-event queue blocked an unrelated
  response until the one-second regression deadline. After the fix, a second
  watch's initial event and an unrelated response complete while that queue stays
  full. The failed call reports unavailable, ignores a late success and sends
  cancellation to its original target without closing the healthy connection.
- Repeated concurrent request/watch pairs open exactly two peer service streams
  and reuse them. AgentWatch is classified with subscriptions and AgentMessages
  with requests. Both desktop forwarding and embedded client pooling use these
  traffic classes; setup locks are scoped to peer and class.
- RPC/protocol suites: 35 passed. App-core unit tests: 158 passed, one existing
  ignore. Six chat-watch integration tests passed, including a burst during
  history, cache delta recovery, reconnect and full earlier-history traversal.
- Strict Clippy passed for all targets of the affected crates. Production
  signing, publication and physical-device acceptance remain separate.

### Snapshot race under concurrent streams

Separate streams can deliver a message appended after a history snapshot before
that snapshot's response arrives. If the queued message has no snapshot overlap,
replacing the subscription discards the ordering evidence and repeats the race
under sustained output. Keep the same subscription and bounded queue for one
additional history reconciliation. A second unresolved overlap still reconnects;
never infer order from text or timestamps. Reproduce the stale-snapshot/new-event
ordering and verify exactly one watch, a bounded second snapshot, ordered unique
messages and cancellation on foreground replacement before publishing.

The deterministic race test first failed with two subscriptions instead of one.
A metadata marker after the live event proves the event was queued before the
stale snapshot is released. With bounded same-subscription reconciliation, the
newer event is recovered in order using one watch and two snapshots. A second
case with a permanently unorderable event reconnects after exactly one additional
snapshot and never displays that stale event. All eight chat-watch integration
tests, 158 app-core unit tests (one existing ignore), and strict app-core Clippy
pass. Both cases were added before publishing 1.1.24.
