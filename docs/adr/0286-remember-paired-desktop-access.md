# ADR 0286: Remember incoming desktop access for a paired device

- Date: 2026-11-23
- Status: Accepted
- Related: Supersedes the per-connection host approval in [ADR 0277](0277-paired-desktop-host-offer.md). See [ADR 0280](0280-native-paired-desktop.md) and [ADR 0284](0284-basic-network-and-device-service-access.md).

## Context

A viewer timeout consumed the host offer during device acceptance. Reconnecting required the owner to return to the Mac and approve the same Windows device again.

## Decision

Let the local desktop owner remember view or view-and-mouse access for one current pairing. Store that choice on the receiving device in Rust. Bind it to both endpoints' roster admissions and the network. A reconnect creates a fresh, bounded engine session with the saved access. Disconnect, navigation and application restart end or leave the current session according to its existing lifetime without consuming the saved permission. Replacing or revoking the permission ends its current session. Screen lock, cover, removal and disconnect continue to gate capture and input. A legacy one-time offer retains its original lifetime.

## Alternatives

Granting mouse control to every roster member would discard the host's selected audience. Extending the lifetime of the existing UI handle would still lose the permission on restart.

## Consequences

The owner can see and revoke the remembered permission from the member's device page. First-time access still uses the native OS permission prompts. Older viewer clients use the existing peer protocol.

## Verification

Exercise persistence, separate view and mouse access, revocation during use, unrelated devices, and removal followed by pairing again. Verify repeated Windows opens after one host grant and after closing the host page. Record rendered pixels, Mac-side mouse events, and build and installation results separately.

On Windows, the daemon fixture used a temporary software identity and a private
store. It verified view, mouse, change, revoke, fresh authority state, and removal
followed by a new admission of the same public key. Four signed-peer transport
tests passed, including two new connections under one permission. The manual
two-host transport test was skipped. These checks do not establish rendered
pixels or mouse delivery on a physical device.
