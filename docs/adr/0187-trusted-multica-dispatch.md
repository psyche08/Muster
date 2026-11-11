# ADR 0187 – Let the host authorize the owner's direct Multica assignment

- Date: 2026-11-11
- Status: Accepted, unattended issue verified on an installed Mac
- Related: [0184](0184-multica-task-authorized-codex-creation.md), [0097](0097-task-scoped-scheduler-grant-for-locked-use.md)

## Context

A locked executor currently needs a local grant for each first turn. In the
previous device test a separate test program reviewed and granted the run.
The owner wants to operate solely by assigning an issue in Multica.

## Decision

The local owner opts in once for a specific HTTPS scheduler, workspace,
member, agent, runtime, runtime daemon and host workspace root. A private host
credential lets the Rust daemon read the service independently. The bridge
provides routing hints. The host obtains the actual issue and dispatch from
the configured service and constructs the native prompt from that snapshot.

Only a new owner-created, owner-initiated issue assigned directly to the
configured agent can qualify. Require the first direct attempt, the expected
execution directory, an online owner-controlled Codex runtime, matching
workspace and unchanged issue creation timestamp. Verify the authenticated
reader identity. Reject edited, ambiguous, stale or mismatched dispatches.
Authorization stays subject to the ordinary target-local policy, console
fence, deadline, delivery ledger and relock.

## Alternatives

Approving the waiting guest-provided digest automatically would grant the
container authority over the first message. Reading the service independently
and submitting the host's snapshot keeps that authority with the local owner.

## Consequences

Enabled agents can execute the owner's direct issue title and body unattended.
The owner is responsible for the tasks submitted under that identity.
A missing host enrollment retains explicit review. Failed verification on an
enrolled host fails closed. The native runtime streams its answer against the
approved first-message digest in the local scheduler receipt.

## Verification

Exercise wrong principals, foreign assignments, altered requests, extra
attempts and directory substitution. Validate the normal entry point on a
locked Mac using a fresh real issue, with only read-only observation after
assignment. Verify a single session, the service result and automatic relock.

The service returns issue and queue timestamps to whole seconds. A real initial
direct assignment crossed the boundary between two adjacent seconds. Accept a
queue creation in that same or immediately following second. Check the run's
initiator name against the authenticated owner's name as an additional
consistency check alongside the stable issue, account and runtime UUIDs.
The contract covers the exact service snapshot read by the host.

### Installed acceptance

A notarized executor built from `3d694a55` ran the normal managed Multica
0.3.62 Codex bridge. A separate test client created one fresh issue assigned
to the enrolled agent and then only read state. It made no grant, operate,
revoke or manual lease request. The daemon verified the dispatch and minted
its own grant. Native submission took 10.45 seconds from UI admission.
The operating system was observed locked, then temporarily unlocked, then
locked again. Multica completed 14 seconds after the observed relock, with
the correct arithmetic answer in both its result and its text event.
There was one service run, one applied creation, and exactly one native
session in the run directory. Its first message matched the service-sourced
snapshot and its digest. Its final assistant answer matched the Multica
answer. The host had revoked the grant.

Earlier test issues also exercised the existing limits. Physical input
stopped a creation before submission. An unready existing window and a
separate slow new renderer exhausted a lease before submission. Those grants
closed and their consoles relocked. The successful test began with the Mac
idle and the Codex composer ready. These results establish unattended
authorization and successful delivery under those conditions.

The final daemon and ACP suites passed (147 and 24 tests, three daemon tests
ignored), along with Clippy. Source stamp, Developer ID signature, Apple
notarization, stapling and Gatekeeper were checked. Only the executor was
updated. No public app release was made.
