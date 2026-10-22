# ADR 0002: Respect provider limits and tolerate lost relay probes within the deadline

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

- Date: 2026-10-20
- Status: Accepted
- Related: ARCHITECTURE.md §§3, 5; DESKTOP_UPDATES.md; ADR 0001

## Context

Mac-C 1.0.72 logged Codex Cloud HTTP 400 at 22:31 CST. The service requires a
list limit of at most 20; Muster requested 50. A read-only comparison on the same
host returned 400 for 50 and 200 for 20 (an empty task list is valid). The
[official Codex client](https://github.com/openai/codex/blob/main/codex-rs/backend-client/src/client.rs)
also passes an optional cursor to this endpoint.

Evening logs also showed lost QUIC keepalives and successful TLS fallback. The
client sent one unreliable datagram probe and destroyed the connection if its
ACK did not arrive within three seconds. A lost request or ACK can therefore
cause avoidable reconnects. Logs alone do not prove why every network packet was
lost. Quinn's NotConnected lines also occur when a retired socket is deliberately
closed; they are not separate message-send failures.

## Decision

Shared Rust requests at most 20 cloud tasks per page, follows encoded cursors,
deduplicates task IDs, preserves the existing 50-task listing bound, and caps
pagination at five requests. A repeated cursor fails without replacing the
previous successful catalog. Authentication errors remain errors and tokens are
never logged, refreshed, or copied by this repair.

A pending relay liveness probe retries the same nonce at one-second intervals
inside its original three-second window. ACK matching, authentication, MTU
validation, TLS fallback, and selection hysteresis remain unchanged. Retries do
not extend the deadline or replay application operations. Healthy probes still
send only once; phone connections still have no periodic application probes.

Native renderer-not-ready and conflicting-draft refusals retain their guards,
but explain the safe recovery action. A different existing draft is never
overwritten. The historical four renderer failures do not establish their root
cause or justify an automatic press, send retry, or larger deadline. Claude's
missing login requires user sign-in; it is not repaired by bypassing credentials.

## Alternatives

Using 20 without pagination would reduce the previous task coverage. Increasing
the relay timeout would violate the existing three-second failure-detection
contract. Silencing Quinn errors would conceal genuine socket failures without
repairing packet loss. Retrying sends after uncertain outcomes could duplicate
messages. Removing draft checks would overwrite user input.

## Consequences

Multi-page listings may use more HTTP requests, bounded at five. Loss recovery
can use at most three probe sends in one deadline; measured RTT includes time
lost before acknowledgement. Persistent outages still fail over. Native readiness
root cause and manual login remain separate acceptance gates, not claimed fixed.

## Verification

Cloud regression tests cover a limit-20 request, cursor encoding, deduplication,
the 50-task bound, and repeated-cursor termination. A real TLS/QUIC fault proxy
blocks UDP for 1.2 seconds during verification: the probe recovers on the same
connection without TLS fallback. Existing sustained-blackhole, MTU, trust, and
carrier-change tests remain required. Mac-C release/runtime evidence is appended
to this record and DESKTOP_UPDATES.md after deployment; compilation alone is not
a device or native AX acceptance result.

### Deployment evidence, 2026-10-20 23:42 CST

- Source `e4f48b8ec27eb75b3081323ea0c2866635076ccf`: 12 cloud tests and 11
  relay tests passed, plus Clippy and the 16-crate layer check. The ADR gate passed
  seven Git-history cases (missing/index/template/deleted/rename-only rejected;
  added or modified numbered record accepted).
- macOS 1.0.73 and iOS 1.0.73 (64) built and were published. macOS notarization
  `ec396a5b-ff92-4e7b-b37b-6cc56c3f6651` was Accepted; signatures, staple,
  Gatekeeper, source stamp, public hashes and OTA manifest build 64 passed.
- Mac-C installed through the normal update lease and preserved its identity.
  Over 97 seconds after startup, no Codex Cloud errors, relay warnings/errors or
  new Muster crash reports appeared. SSH over virtual IP 192.168.13.13 succeeded.
  This bounded observation does not prove an external network outage cannot recur.
- The only warning was Claude Cloud lacking CLI credentials. User sign-in remains
  required; desktop-app login is separate from CLI cloud credentials. No native
  AX send, historical renderer-failure reproduction, or physical iPhone install
  is claimed in this release pass. Native changes improve recovery guidance while
  retaining refusal, and do not establish a fix for that unproven historical cause.
- Evidence is retained under `build/releases/Apple-1.0.73-64/`, including
  `public-check/result.json`, `muster73-runtime-review.log`, and the loss-injection
  test log. See DESKTOP_UPDATES.md for artifact hashes and installation details.
