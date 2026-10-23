# ADR 0046: Recover send outcomes without replay and wait for quiet before navigation

- Date: 2026-10-23
- Status: Accepted
- Related: ADR 0045; Architecture §3.6 and §3.7

## Context

A lost response does not establish whether a native send was delivered. Clients
currently retain drafts but lack a durable reference for read-only recovery.
Local input can also invalidate navigation before any draft is written.

## Decision

Shared Rust records a pending send before dispatch, scoped to the network, host,
and session. The record is a request to investigate, never delivery evidence or
authority to execute. Reopening the conversation or resuming the client queries
the original request through a new read-only RPC. The daemon authorizes the
current provider control grant, then reads only an exactly bound ledger row.
Missing rows remain unknown: they are not permission to replay a delayed request.
No query obtains console ownership, triggers AX, or renews an unlock lease.
Only authenticated applied/rejected results settle the pending record. Newer
drafts and newer operations cannot be cleared by old results. An explicit
existing user release remains available for unresolved sends after inspection.
Recovery is cancelled in the background and paced while foregrounded.

Before native navigation, wait for a bounded quiet input interval with continuous
authority checks. The final target and input-generation checks remain in place.
Never retry navigation or submission after a side effect just because input
changed. Do not extend Locked Use deadlines.

## Verification

Test exact request/kind binding, unknown and rejected outcomes, authorization,
read-only status across reconnects, durable client recovery and newer-draft
preservation, cancellation and bounded quiet admission. Build/sign macOS and iOS
and record physical normal, lock, interruption and recovery acceptance separately.

### Source verification

The affected Rust library suites passed 440 tests (six existing environment
cases ignored). Three daemon RPC integration tests and one additional client
RPC recovery test passed, for 444 distinct tests. Reconnected clients read a
late receipt without a second submission; locked receipt queries reject changed
bodies and revoked provider policy. Restarted local records preserve later drafts
and newer leases. Strict Clippy, formatting, layer and public-content checks
passed. Signing, installation and physical acceptance are recorded separately.
