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

### Local physical acceptance (2026-10-23)

Both Apple Release builds succeeded from source commit `93529e00`. The macOS
Developer ID signature and iOS development signature passed strict verification.
The macOS app was installed on the local acceptance Mac with the daemon update
lease; device identity and hardware key backend were preserved, and Accessibility,
relay registration and the virtual network were ready after restart. These are
local verification builds, not a notarized release or a public desktop/OTA update.
The iOS build was signed but has not been installed in this acceptance run.

Three physical sends in a dedicated native acceptance conversation each produced
exactly one user message and one assistant reply: normal CLI submission, deliberate
caller disconnection after the native send-button press, and submission from the
Muster app. In the disconnected case the caller did not read the original response;
a new connection first queried Unknown, then Applied, with one submission total.
The normal CLI operation took approximately 0.54 seconds, including local RPC
overhead, not assistant response latency. A changed request body returned Conflict;
an absent operation remained Unknown. The app's normal send cleared its composer.

Physical acceptance remains partial. The second remote Mac answered Muster RPC
but its SSH deployment channel timed out and it still ran the earlier build without
the receipt capability. The other remote Mac was unreachable. Device Hub reported
the physical iPhone unavailable. Remote installation, phone-driven sends, locked
native sends, client restart recovery and quiet-input contention on real hardware
remain unverified for this change; automated coverage does not replace those cases.
