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

### Remote installation and lock boundary follow-up (2026-10-23)

The second remote Mac's deployment connection was recovered through its physical
bridge management address. Its old SSH alias used the virtual network instead.
Local per-application proxy interception also affected the physical address:
the extension's private-network forwarding selected the default Ethernet
interface instead of the bridge. With the original configuration backed up,
private-address DIRECT rules and a named direct adapter bound to the management
bridge restored SSH while application proxying remained enabled in Rule mode.
The extension-level CIDR forwarding was disabled in favor of these core rules;
this configuration workaround does not claim to fix the extension implementation.

The same signed source build `93529e00` was then installed on that Mac. Its
daemon SHA-256 matched the local acceptance build, its identity and hardware key
backend were preserved, and Accessibility, relay, virtual network and GUI
readiness passed. The host advertised the new receipt capability. While locked,
a remote read-only query for an absent operation returned Unknown and the host
remained locked.

A send from the actual Muster app to the dedicated remote acceptance conversation
entered the normal Presence authentication flow. No user authentication completed
before the challenge expired. The challenge returned to Idle, the composer kept
the marker, and the remote transcript contained no new marker. This verifies the
no-send boundary, not successful locked delivery. The other remote Mac remained
unreachable and Device Hub still reported the physical phone unavailable. The
successful locked-send and phone end-to-end acceptance items therefore remain open.

### App Proxy route repair follow-up (2026-10-23)

The proxy's interface-preservation repair was built and notarized by Xcode Cloud
(MihomoBox build 49, source `f258bdf`). The original signed app was installed and
macOS activated extension 1.0.0/49. Default private-network bypasses were restored,
and the temporary bridge adapter and seven workaround rules were removed through
the proxy's authenticated profile-import path.

With App Proxy enabled, both physical-bridge SSH and Muster virtual-network SSH
passed five independent 256 KiB integrity checks. Connected and unconnected UDP
echo also passed. Native connection logs confirmed the bridge and virtual
interfaces were retained instead of selecting the default Ethernet interface.
Public HTTPS remained reachable. The remote Muster status RPC and absent-receipt
query passed again, and the target stayed locked. This supersedes the configuration
workaround above with a tested extension implementation fix.

Device Hub still reported the physical iPhone unavailable, the third Mac remained
offline, and successful locked delivery still needed user Presence authentication.
Those outstanding acceptance items are not claimed as passed.
