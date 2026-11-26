# 0319. Type on the focused macOS Remote video surface

- Date: 2026-11-26
- Status: Accepted; physical acceptance pending
- Related: [0314](0314-mobile-remote-keyboard.md)

## Context

The macOS viewer negotiates keyboard capability with the host, but its video surface handles only pointer events.

## Decision

Clicking the visible video surface can give it keyboard focus when the active session has a keyboard grant. Translate ordinary hardware keyboard text, Backspace and Return into the shared desktop input actions. Bind every action to the displayed session. Keep application shortcuts local. Shared Rust remains responsible for capability negotiation, revocation, lock transitions and delivery.

## Alternatives

A window-wide event monitor would intercept input intended for other controls. A separate keyboard connection could outlive the authorized video session.

## Consequences

Mouse-only and view-only sessions retain their existing behavior. The viewer sends no keystroke diagnostics. This change covers ordinary hardware keyboard typing; it does not add shortcut or IME composition forwarding.

## Verification

Exercise text and special-key translation, shortcut handling and grant removal in native tests. Build the macOS application. For physical acceptance, focus an unlocked host's disposable test field from Remote and verify the resulting text on the receiving machine. Record installation and device results separately.
