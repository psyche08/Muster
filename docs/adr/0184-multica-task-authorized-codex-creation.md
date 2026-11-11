# ADR 0184 – Authorize a locked Codex creation for one Multica run

- Date: 2026-11-11
- Status: Proposed, device verification pending
- Related: [0097](0097-task-scoped-scheduler-grant-for-locked-use.md), [0183](0183-locked-codex-creation-stages.md)

## Context

A real Multica issue reached the managed Codex runtime on a locked Mac. The
desktop adapter sent an ordinary create request and the daemon refused before
unlock. The container's shared bridge credential authenticates the bridge.
A run ID supplied by that container cannot grant desktop authority.

## Decision

The managed desktop Codex bridge passes its run ID into the host adapter.
The host submits the first turn to the local daemon, using one operation ID
per run. A locked request proposes the complete first turn to the local
owner. An existing matching task grant admits it immediately. Otherwise the
daemon holds a bounded proposal in memory for up to one minute while locked.
The local scheduler status RPC for the task exposes its canonical directory,
the SHA-256 of the exact first prompt and the text for review. A local owner
or trusted local scheduler can grant the reviewed run using the existing
local-only grant RPC. The guest cannot mint, extend or revoke this grant.

The reserved task namespace permits one producer creation, one provider and
one operation ID. The daemon checks the reviewed text, identity and directory
at admission and uses the existing reservation, lease, receipt, and
post-relock observation. Multica receives the final assistant turn through
the existing native runtime stream.

## Alternatives

Granting on the strength of guest environment variables would delegate
unlock authority to the container. Allowing ordinary local creates to unlock
would extend the capability to unrelated local callers.

## Consequences

A changed prompt or directory, absent approval, cancellation, revoked grant,
or ambiguous submission cannot trigger a second creation. The proposal is
readable only on the host socket and disappears when the waiter exits.
Unlocked creation remains available through its existing native guards.

## Verification

Exercise refusal and digest binding in the daemon. On the installed build
dispatch a real issue to the Mac's Multica runtime while locked, authorize
only that run, check its ledger and unique first message, then verify the
answer at Multica and the OS lock observation. Clean up the test grant.

On the real service the first turn contains the issue UUID and instructions
for reading the issue. The issue body is fetched by the native agent. Review
the issue itself as well as the proposed first turn. The Multica runner also
has a 30-second initial progress deadline. The adapter emits the received
user item at turn start and completes that item when the native prompt
returns. Only the provider's completed assistant turn emits a final answer.
