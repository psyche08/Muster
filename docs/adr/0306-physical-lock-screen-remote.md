# ADR 0306: Control the physical lock screen from Remote

- Date: 2026-11-24
- Status: Accepted product requirement. Platform integration and device acceptance pending.
- Related: [Paired desktop host offer](0277-paired-desktop-host-offer.md), [Desktop service authorization](0287-desktop-service-authorization.md), [GameStream roles](0301-desktop-gamestream-roles.md)

## Context

A paired phone with a persistent mouse grant can select a locked Windows host in Remote. The current host rejects the connection before the first frame. Windows evaluates the session containing the daemon. An RDP disconnect can leave the daemon in a different session from the physical console. The Mac host also requires a logged-in, unlocked console. These checks implement the earlier desktop-sharing decisions.

## Decision

On Windows and macOS, a viewer with the appropriate existing grant should be able to connect in Muster Remote while the host is locked. Before anyone authenticates to unlock the host, the viewer must see the live lock screen shown on the physical display. A mouse grant permits moving the pointer and clicking the lock screen. A view grant permits observation only. The local operator does not need to reapprove each connection. The OS owns the transition to the unlocked desktop and its authentication UI.

This decision revises the unlocked-console requirement of ADRs 0277 and 0287 for this specific surface. Shared Rust continues to enforce pairing, grants, revocation and controller replacement. Each platform needs a supported way to identify the physical lock surface, observe it and deliver only the permitted pointer events. A platform that cannot establish which surface it is handling must report that limitation and stop forwarding frames and input. This capability gives no authority over unrelated secure prompts, credentials or desktops.

## Alternatives

An authenticated remote login can establish a separate session. Waiting for a local unlock delays the first frame until after authentication. A cached frame or a generated lock screen cannot demonstrate interaction with the physical screen. None satisfies the required observation.

## Consequences

The current engines run in the user session. Changing a Rust admission predicate alone cannot establish access to the required OS surface. Windows documents access restrictions for desktop duplication and input integrity levels. Apple's documented Screen Sharing workflow can require its own sign-in. We have not verified a supported adapter on either platform that meets all of the conditions above. The existing checks remain in force until such an adapter is available. [Windows desktop duplication](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_2/nf-dxgi1_2-idxgioutput1-duplicateoutput) · [Windows input](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput) · [Apple Screen Sharing](https://support.apple.com/guide/mac-help/mh14066/mac)

## Verification

For each OS, use a physical host and a paired viewer in Remote. Lock the physical console, connect, and compare both displays at the same time. Move the pointer and make a harmless click with a visible OS response on both displays. Independently confirm the console stayed locked throughout. On Windows, repeat after disconnecting RDP and record both console and daemon session IDs. A session ID or a rendered frame alone is insufficient evidence.

Repeat with view-only and revoked grants. Verify that mouse events from a view-only peer have no effect and that revocation ends access. Check that a switch to an unrelated secure surface stops capture and input. Record host build, installed version, actual frames, pointer response and OS lock state separately. Neither platform has passed this physical acceptance.
