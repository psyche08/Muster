# ADR 0299: View paired desktops in the Apple Remote workspace

- Date: 2026-11-24
- Status: Accepted
- Related: ADRs [0242](0242-muster-remote-desktop-engines.md), [0287](0287-desktop-service-authorization.md), [0295](0295-desktop-takeover-confirmation.md), and [0296](0296-embedded-remote-workspace.md)

## Context

Windows can view and operate a paired Mac in the embedded Remote workspace.
macOS offers a host and iOS embeds its own daemon. Neither Apple app can
consume a Windows window handle or embed its process-based viewer.

## Decision

The Apple apps use the shared Remote workspace and request a native frame
session. The Mac host negotiates this profile on the existing authenticated
desktop service. Both profiles share the same host occupancy, console lease,
persistent authorization and confirmed takeover. The new bounded frame profile
uses ScreenCaptureKit and ImageIO for aspect-preserving JPEG updates at up to
10 frames per second, with a latest-frame slot at each local endpoint.
Shared Rust owns framing, session lifetime and input checks. Swift renders the
current frame and forwards mouse actions. Switch tabs without closing the
session. On iOS, backgrounding ends it.

Phone devices can receive an explicit signed desktop grant or a receiver's
assigned-device desktop rule. Existing blanket grants for paired desktops do
not extend to phones. Capture requires Screen Recording and mouse operations
require Accessibility. A phone is a viewer only.

## Alternatives

Linking the vendored GPL Moonlight engine into the Apple app would change the
distribution boundary and would still require a mobile media adapter. A
separate screen-sharing protocol outside PeerLink would duplicate authorization
and takeover. A datagram video profile would require frame fragmentation, loss
recovery and codec negotiation. The reliable frame profile bounds work and
buffers at the cost of lower motion quality and increased delivery delay on
congested links.

## Consequences

Apple viewing requires an updated Mac host. The Windows GameStream profile
continues to use its source-built viewer and on-demand Sunshine engine.
The native profile starts OS capture on demand and stops it on disconnect.

## Verification

Check bounds and malformed messages, phone grant precedence and old blanket
grants, view-only input denial, workspace navigation and disconnect, and the
macOS and iOS builds. Independently verify a real frame, mouse receipt, screen
lock, revocation and takeover on paired physical devices before claiming
device acceptance.

On 2026-11-24, the workspace RPC tests retained the native frame across a tab
switch and released the lease on disconnect. Two isolated nodes exchanged a
takeover confirmation, a multi-chunk synthetic frame and an absolute pointer
receipt. Policy tests exercised a phone with explicit grants, receiver
precedence, old all-desktops rules, revocation and re-pairing. A view-only
fixture on a physical Mac received one real display JPEG through the paired
service. The viewer and host of this last test ran on the same machine.
