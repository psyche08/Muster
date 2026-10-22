# ADR 0012: Promote the verified pairing link to online

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

- Date: 2026-10-22
- Status: Accepted
- Related: ADR 0004; ARCHITECTURE.md networking presence

## Context

Linux namespace acceptance in run 39699645359 established direct paths in both
directions and completed pairing, but the joiner displayed the admin link as
`-` with a live RTT and timed out waiting for `direct`. The CLI displays a path only
when presence is online. `Node::adopt` runs before the joiner has a roster and
therefore does not mark the pinned admin online. Installing the returned roster
in `Node::join` did not promote that already authenticated, live connection.

## Decision

After successful pairing installs its verified roster, mark the admin online
only if the admin is now an active member and its current connection is alive.
Do not infer online from relay registration or roster membership alone, open a
new link, duplicate heartbeat tasks, or extend timeouts. Keep revocation and
transport retirement responsible for closing/invalidation as before.

## Alternatives

Weakening the namespace test to ignore the CLI status conceals a user-visible
state error. Reconnecting a healthy pairing link is unnecessary. Marking every
roster peer online has no liveness evidence.

## Consequences

A successfully paired node immediately reports its live admin link. A missing
or closed connection is not promoted. This is separate from the native send
matching defect and does not establish device network acceptance.

## Verification

Assert joiner's online presence immediately after a real loopback pairing in
the existing pair/sync/forward/revoke regression. Demonstrate failure before
repair and success afterward, then pass unchanged Linux namespace acceptance.
Source/CI evidence is distinct from Mac-A/Mac-C/Mac-B device acceptance.

The new assertion failed on the original join path on Mac-A. After repair, all
three `muster-net --test link` integration tests pass, including pairing,
forwarding/revocation, unavailable-relay recovery and service-direction bounds.
Strict `cargo clippy -p muster-net --tests -- -D warnings` passes. The unchanged
Linux namespace test remains a required CI gate; no device services were replaced.
