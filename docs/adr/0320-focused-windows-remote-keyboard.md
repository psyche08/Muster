# ADR 0320. Send keyboard input from the focused Windows Remote viewer

- Date: 2026-11-26
- Status: Proposed. Device acceptance pending.
- Related: [Mobile Remote keyboard](0314-mobile-remote-keyboard.md), [GameStream roles](0301-desktop-gamestream-roles.md)

## Context

The embedded Windows viewer sends pointer input. Its fixed video hello cannot request a keyboard lease. A fixed RTSP listener can also collide with an operating-system port exclusion before the first frame.

## Decision

Shared Rust negotiates keyboard support for the existing fixed-size Windows presentation. Use the original hello with a host that does not advertise this extension. The authenticated launch reply carries the effective input mode. Reuse the receiver's per-device grant, live lease, lock checks and existing Sunshine input filter. The Windows child accepts committed Unicode text, Backspace and Return only while its painted video surface owns keyboard focus. Focus loss clears partial UTF-16 input. Leave system shortcuts local.

Bind the Windows viewer's RTSP listener to an OS-assigned loopback TCP port and give the resulting port directly to its Moonlight child. Keep the RTSP-negotiated UDP offsets and engine ownership checks. No system port exclusion needs to change.

## Alternatives

A global keyboard hook or shared clipboard would capture input outside the viewer. Changing the fixed video resolution negotiation would expand the change into rendering and pointer scaling. The per-session handshake keeps existing mouse-only viewers usable.

## Consequences

An existing installation can continue viewing and pointing at an older host. A keyboard grant takes effect only in a session where the receiver and viewer both negotiate support. Mouse-only and view-only sessions keep their prior behavior.

## Verification

Test ordinary text, surrogate pairs, Backspace and Return, loss of focus, absent grants, legacy peers and loopback binding. Build x64, then verify a painted frame, pointer click and exact text at an unlocked non-sensitive target. Record deployment and device evidence separately.

On Windows x64 the release daemon desktop-session tests passed, including pairing, revocation, and legacy negotiation. Native CTest passed all three cases. The viewer test read back Unicode, Backspace and Return in a private Win32 edit control. A separate process test received the expected RTSP rejection on both the legacy listener and an OS-assigned port. Cross-device first-frame and input acceptance is pending.
