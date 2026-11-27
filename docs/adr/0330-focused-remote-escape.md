# ADR 0330. Forward Escape from the focused Mac Remote viewer

- Date: 2026-11-27
- Status: Accepted for implementation
- Related: [0314](0314-mobile-remote-keyboard.md), [0319](0319-macos-remote-hardware-keyboard.md), [0325](0325-ordinary-console-stream-worker.md)

## Context

A viewer needs to dismiss an open menu with the physical Escape key. The focused Mac video surface rejects control characters and the explicit key protocol supports Backspace and Return.

## Decision

Add Escape as an explicit key in the local desktop keyboard request. The focused Mac viewer submits a bare Escape under the existing keyboard grant. Shared Rust validates the peer and active presentation before the Apple streaming client sends the GameStream key. The receiving engine permits one complete Escape press under its existing keyboard grant and desktop checks.

## Alternatives

Encoding Escape as text would conflate a key event with a control character and would weaken text validation. A window-wide shortcut would also capture Escape when a local control owns focus.

## Consequences

An older host ignores the additional key. Existing grants and wire values remain valid. Lock-screen capture and input require a separately verified console backend.

## Verification

Test focused Escape, modified Escape, loss of focus and a missing keyboard grant. Check the encoded virtual key and preserve rejection of control characters. Physical acceptance requires observing a remote menu open after a click and close after Escape. The Windows lock-screen session currently ends before its first frame.

The focused AppKit test and the Rust session-routing test passed on macOS. The ARM64 Mac streaming helper compiled and linked. These checks cover key translation and grant enforcement. Delivery to the Windows lock screen remains pending.
