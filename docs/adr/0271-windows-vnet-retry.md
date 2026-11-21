# ADR 0271. Retry failed Windows vnet startup on the running local node

- Date: 2026-11-21
- Status: Accepted
- Related: [stable vnet owner](0004-transport-recovery-and-stable-vnet-owner.md), [bounded helper observation](0008-complete-delayed-vnet-startup.md), [Windows development](0236-unprivileged-windows-development.md), architecture §3.5

## Context

Cancelling Windows elevation leaves the signed-in node serving peer RPC with an assigned virtual address and no adapter. In an existing network the daemon has no user entry to repeat the failed open. Restarting the node interrupts unrelated connections.

## Decision

Have the daemon remember a failed Windows interface open for its signed local address and subnet. Expose retry availability in device state. Admit a dedicated local-only request only while that failed plan still matches the active member and vnet is configured. Consume the opportunity when starting the next attempt and reuse the existing single startup worker and its bounded observation. Its completion publishes a new state. The Windows Settings action uses the local Shared Rust client. An older daemon does not advertise the action. Keep regular refresh, service traffic, membership, and the protected helper's OS authorization separate.

## Alternatives

A timer or generic refresh could prompt while the operator was away. Rejoining the network would touch membership to repair a local adapter. Retrying a retired interface after signed renumbering would violate the existing restart requirement and the single datagram-reader lifecycle.

## Consequences

An operator at the console can initiate Windows authorization again after a failed open, including cancellation. A newer GUI attached to an older production node still requires the existing handoff before this capability becomes available.

## Verification

Test refusal for remote principals, disabled vnet and missing or changed plans. Exercise failure, retry, concurrent admission and state observation with a synthetic opener. Check the Settings intent against an isolated node and old daemon behavior. Report real OS approval, adapter creation, peer traffic and enrollment separately.

Three Windows daemon regressions passed with a software signer. A simulated open failed, the explicit local RPC admitted one pending retry, and the worker survived the observer timeout. An active administrator using a remote RPC principal received Unauthorized. Refresh, a concurrent retry, a disabled adapter and a changed signed plan did not open a new adapter.

The Release WinForms build and the Shared Rust FFI build passed on Windows x64. A separate form process connected the real Settings page and FFI to a temporary named-pipe fixture. An absent capability hid the action. Refresh fetched the failed-open projection, one click sent one untargeted local retry, and the updated up projection removed the action. Swift and HarmonyOS bindings were regenerated locally. The HarmonyOS generator also picked up the existing side-chat fields from the base schema.

The simulated open and up projection do not exercise Windows authorization or packet flow. Installed-node handoff, real Wintun consent and cross-device traffic require device acceptance.
