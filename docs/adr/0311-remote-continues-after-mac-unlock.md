# ADR 0311. Continue an active Remote connection after macOS unlock

- Date: 2026-11-25
- Status: Proposed
- Related: [0309](0309-macos-remote-screencapturekit.md), [0306](0306-physical-lock-screen-remote.md)

## Context

ADR 0309 binds capture to the initial console state. Unlocking on the same signed-in physical console makes the viewer reconnect manually.

## Decision

Supersede the disconnect-on-unlock part of ADR 0309 for Moonlight. Keep the authenticated transport, controller reservation, encoder and chosen display. Shared Rust observes the unlock and rechecks the original grant. Signal the private Sunshine child through its existing owner pipe once. Replace the lock-window capture with a ScreenCaptureKit display stream and resume mouse dispatch on a fresh desktop frame. Preserve termination on relock, loss of the console, revocation, or controller replacement.

## Alternatives

A viewer retry would acquire a new reservation and could race another controller. Reusing the lock-window filter after unlock would keep capturing the old surface.

## Consequences

The iPhone needs no second Connect action or protocol negotiation. The Mac must first provide a usable lock surface. Loss of an input or capture permission still ends or limits the session.

## Verification

Exercise the one-way console transition, view-only and revoked access, stale pointer handling, and builds. In physical acceptance, connect the iPhone while locked, unlock the same console locally, observe the desktop in the original connection, then verify a fresh click. Record device results separately.

The host and transport regression suites passed, including grant revocation, reservation replacement and the one-way console transition. The private host handshake passed with the rebuilt macOS binaries. On an unlocked Mac, replacing the native capture stream on the same display returned a new 1280 × 720 frame with a later presentation timestamp. The physical transition from lock screen to desktop on a connected phone remains to be verified.
