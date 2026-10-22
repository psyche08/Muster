# ADR 0005: Isolate desktop RPC requests and retain desktop authorization

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

- Date: 2026-10-21
- Status: Accepted
- Related: ARCHITECTURE.md §§3.6, 4; LOCKED_USE.md; ADR 0004

## Context

The user reports session-list response timeouts and failed remote sends from
Mac-B's desktop app while iPhone works. Fresh CLI connections from Mac-B to
Mac-A/Mac-C return in 32–114 ms. The desktop app multiplexes local and remote
calls through one daemon socket. Its RPC server awaits target connection setup
inside the common read loop, so a slow peer prevents it reading unrelated
requests and cancellation. This is a reproducible implementation defect;
existing logs do not establish that every reported timeout has this cause.

Mac-B has no locked.control scope. Desktop operation authorization also needs an
enrolled Presence key, unlike foreground phones' enrolled device-key flow.
The prior Mac-A CoverLost event was a phone operation and must not be used to
explain Mac-B's authorization failure. Grant/key enrollment must remain explicit
and use existing trust rules; do not substitute phone signing for desktop auth.

## Decision

Keep routing and request lifecycle in Shared Rust. Dispatch addressed calls
without blocking the connection reader; share one serialized connection setup
per target, not across targets. Cancellation and socket EOF must cancel pending
setup and must not later forward an abandoned mutation. Preserve request IDs,
presence proofs, single dispatch and local-only proxy authorization. Bound active
forwarded calls to 256 and release cached transports on connection exit. A dropped
call/deadline removes its pending entry and queues cancellation; if the bounded
writer cannot accept cancellation, close that connection instead of accumulating
background cancellation tasks. Cancellation after dispatch is an unknown outcome,
never evidence that a mutation did not execute and never a retry trigger.

Unary completion must actually drain events while waiting for its response;
retaining an unread bounded event receiver can deadlock a response after 256
events. Do not enlarge the session-list or Locked Use safety deadlines.

## Alternatives

Increasing UI timeouts hides shared-socket blocking. One connection per UI call
avoids some interference but leaves the server cancellation race. Automatically
granting locked control or treating a desktop as a phone would change trust.
Replaying failed writes can duplicate a submitted message.

## Consequences

An unavailable peer no longer stalls unrelated device lists or cancellation.
Target connection attempts remain serialized and canceled calls do not execute
later. Desktop remote sends still require their actual grants and Presence.

## Verification

First reproduce slow-target head-of-line blocking, cancellation during connect,
and a unary response following more than 256 events. Test isolation across
peers, connection reuse, EOF cleanup, authorization and preserved signatures.
Separate regression/build evidence from installed desktop and real-send checks.


## Evidence and remaining work (2026-10-21)

- Before the fix, injected stalled target setup blocked a local request and the
  300-event unary completion test timed out. Both pass after the fix.
- Rust tests: RPC 12 passed; Locked Use authorization/state-machine 44 passed;
  app-core library 80 passed with one existing public-download test ignored;
  daemon library 49 passed. Strict Clippy passed for these four crates' tests.
- Mac-B 1.0.74 (65): an isolated, read-only socket probe returned local DeviceInfo
  in 48 ms and AgentList in 8 ms while also requesting an unavailable phone.
  This probe did **not** reproduce the reported desktop timeout. Existing CLI
  and source tests do not prove the installed GUI incident is fully resolved.
- The signed roster records Mac-B without locked.control and without a Presence
  key. Mac-A/Mac-C also lack enrolled Presence keys; iPhone has the grant/key.
  Current DesktopBackend does not enroll the shell's Presence public key in the
  daemon identity; DeviceUpdate cannot change an existing member's Presence key.
  This change reports the missing key explicitly and logs failed preparation
  with requester/RPC ID; it does not implement enrollment or modify grants.
- Restoring desktop Locked Use still requires a separately designed, admin-
  authorized Presence enrollment/migration with protocol compatibility and
  native-authentication acceptance. Do not remove/rejoin devices or mutate
  stored signed roster bytes to simulate enrollment.
- No real chat was sent or retried for this investigation. No new app bundle,
  signing/notarization, installation, iOS OTA or desktop send acceptance is
  claimed for this source change. The installed version remains 1.0.74 (65).
