# 0314. Type into an authorized Remote desktop from a mobile keyboard

- Date: 2026-11-25
- Status: Proposed
- Related: [0287](0287-desktop-service-authorization.md), [0311](0311-remote-continues-after-mac-unlock.md)

## Context

A mobile viewer can navigate a desktop with touch, but entering text requires a keyboard. Older viewers and hosts negotiate only view and mouse access.

## Decision

Use the system text editor for Unicode composition and the active authenticated Moonlight session for delivery. Shared Rust binds input to the selected peer and live presentation. The receiver owner can add keyboard permission to a mouse rule for specific paired devices. Existing mouse, view and blanket grants retain their original input capabilities. The host negotiates keyboard only for a session opened on the unlocked physical console, rechecks the console before key dispatch and stops on a lock. Keep typed content out of diagnostics. Preserve interoperability with peers that understand only mouse input.

## Alternatives

Clipboard transfer would change the host clipboard and an independent input connection would need its own lifetime and authorization checks.

## Consequences

The native keyboard can compose text using the device's installed input methods. Return and Backspace are explicit remote key actions. Closing or replacing the viewing session discards pending input.

## Verification

Check old clients, missing permission, malformed input, stale sessions and the host's lock boundary. Verify Unicode, Return and Backspace against a non-sensitive text field on a physical host.

The shared Rust tests exercise a keyboard grant for a named peer, revocation, stale session IDs, ordered Unicode input and special keys. A legacy video hello omits keyboard capability and a legacy settings decoder can parse the new hello. Physical keyboard delivery to a remote text field still requires device acceptance.
