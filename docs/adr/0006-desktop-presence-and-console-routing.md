# ADR 0006: Desktop Presence enrollment and console routing

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

- Date: 2026-10-21
- Status: Proposed
- Related: ADR 0005; ARCHITECTURE.md §3.4; LOCKED_USE.md

## Context

Desktop shells own a Presence key but socket daemons never register it. Existing
Mac members therefore lack a key, and Mac-B also lacks locked.control. The user
requests enrollment, release/install and actual sending from Mac-B, with locked,
already-protected and unlocked execution paths kept distinct.

## Decision

Keep all enrollment and routing decisions in Shared Rust. The desktop shell
registers only its public Presence key through the same-user daemon socket.
An explicit administrator operation fetches that candidate from the authenticated
member and approves it; it may explicitly grant locked.control in the same
operation. Registration alone grants no capability.

Use an admin-signed Presence binding tied to network, member, public key and a
roster authorization epoch. An empty, existing-format DeviceUpdate advances the
member's Presence epoch without changing identity, role, grants or address.
This uses the existing admin-signed DAG and removed-admin rules; old clients can
still decode all membership and revocation entries. No new unrecognized body
kind may prevent older phones learning a member removal. The binding is carried
with RPCs, verified in muster-core against the current accepted epoch, and stored
as signed public metadata. Re-enrollment/revocation advances the epoch; stale,
foreign, tampered and removed-member bindings cannot authorize an operation.
Normal scope changes retain the epoch; grants and target policy are checked at
use time. Signed roster bytes and immutable Add/Genesis records are unchanged.

On the target, a live compatible Locked Use owner gets the operation first;
other owners cannot be borrowed. A locked console requires a fresh signed
operation and cleanup/relock after it drains. An unlocked console executes the
ordinary authorized operation without creating an unlock lease. The target's
Locked Use switch controls temporary unlocking, not ordinary unlocked sends.
No uncertain send is retried, and operation receipt and OS-worker lifetime
protections remain in force.

Forwarded cancellation drains a bounded two-second grace period so cooperative
watch acknowledgements and confirmed mutation receipts survive cancellation.
A dispatched stream without a receipt remains an unknown-outcome timeout, never
a retriable pre-dispatch failure. This corrects ADR 0005 cancellation behavior.

Release both Apple platforms as 1.0.75 (66).

## Alternatives

Changing the existing roster encoding breaks old clients' revocation sync.
Re-pairing changes device identity and addresses. Trusting a native boolean or
substituting a device signature for desktop Presence removes user-presence
proof. Unauthenticated key registration must not grant remote control.

## Verification

Test binding tampering, epoch replacement/revocation, concurrent updates,
restart persistence, wrong-network/member rejection and legacy roster decoding.
Test all three console routes, foreign/closing owners and cancellation. Verify
builds, macOS notarization, three-Mac installation, iOS OTA and actual Mac-B send
receipts separately. Native Presence authentication may require the user's
interactive approval; never automate Keychain unlocking or password entry.

## Source verification (2026-10-21)

Core, store, RPC, daemon and shared app tests pass, including six binding/epoch
tests, a real two-daemon registration/approval/proxy/revocation regression,
unlocked sends with temporary unlocking disabled, manual-owner reuse and
confirmed mutation receipt preservation during cancellation. Strict Clippy for
these crates plus CLI and the 16-crate layer check pass. Deployment and native
Mac-B acceptance are tracked separately below when completed.

## Installed finding and signing correction

1.0.75 (66) was notarized and installed on all three Macs. Runtime inspection
found no Presence candidate. Captured Mac-B native startup stderr reports
`keychain error -34018`; inspection shows the app has no application identifier,
keychain access group or embedded Developer ID profile. This is an additional
release defect, separate from enrollment protocol correctness.

Publish 1.0.76 (67) with an Apple-issued Developer ID profile for the existing
team/bundle identity and exact app keychain group. Disable debug entitlements.
Packaging must preserve expanded signing entitlements, never re-sign with an
unexpanded source plist. A release guard reads Security.framework signing
metadata (including DER-only entitlements), checks the app/group/team, profile
expiry/all-device distribution and authorized signing certificate. The 1.0.75
artifact fails this guard as expected. Do not switch to an unprotected key or
relax Keychain/security settings to make enrollment succeed.

Apple references: [keychain access groups](https://developer.apple.com/documentation/security/sharing-access-to-keychain-items-among-a-collection-of-apps)
and [profile creation](https://developer.apple.com/documentation/appstoreconnectapi/post-v1-profiles).

The profile is scoped to the app target through `MUSTER_MAC_PROFILE`, not Swift
package resources, and binds the installed Developer ID G2 certificate.

## Release and installed evidence

1.0.76 (67) macOS/iOS builds and macOS notarization passed. The final package
passes the Presence signing guard; all three installed Macs register and enroll
their real native public keys, retaining identity and other scopes. Six directed
RPC and virtual-IP paths pass. See [release evidence](../DESKTOP_UPDATES.md).
Physical iPhone installation remains untested. On 2026-10-21 at 13:02 CST,
Mac-B's installed desktop app sent once to the dedicated Mac-A Claude session
`3e9c6153-3ed4-57ae-8024-981cd2f7ec47`. Both consoles were unlocked. The target
ledger records `app-1792558929277155000-1` as `agent.send / applied`; native AX
observed the provider receipt, Claude replied `MUSTER76-Mac-B-Mac-A-1021-A 收到。`,
and the Mac-B conversation displayed that reply with an empty composer. No
uncertain send was retried. This accepts the ordinary unlocked desktop route.

On the same day at 13:09 CST, the user explicitly requested another locked
send after the first challenge expired without execution. Before resubmission,
inspection confirmed no receipt or operation for that Claude attempt. Mac-B
submitted the retained dedicated draft once. The new challenge was issued at
05:09:27 UTC, its Presence signature was verified at 05:09:30, and the Mac-A
plugin admitted the temporary unlock. The target entered Active at 05:09:31.
Operation `app-1792559367600704000-3` received a provider receipt and became
`applied` at 05:09:33. The queue and AX work drained; the gate transitioned
`Relocking -> Idle`, `stop=Completed`, and released the display guard. A fresh
OS observation confirmed Mac-A was locked. Claude replied
`MUSTER76-Mac-B-Mac-A-LOCKED-1021-B 收到。`; the Mac-B composer was empty.
This accepts native desktop Presence plus temporary Locked Use, actual Claude
send, cleanup and relock on this device pair. The earlier expired attempt does
not establish a cryptographic authentication defect, and no timeout was relaxed.
Reuse of an existing compatible owner remains source-tested only in this
release; it was not exercised by this temporary-owner acceptance.
