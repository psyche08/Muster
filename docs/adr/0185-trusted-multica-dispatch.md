# ADR 0185 – Let the host authorize the owner's direct Multica assignment

- Date: 2026-11-11
- Status: Proposed, installed acceptance pending
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
