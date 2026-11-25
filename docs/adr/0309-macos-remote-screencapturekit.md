# ADR 0309. Try ScreenCaptureKit on the signed-in physical console

- Date: 2026-11-25
- Status: Proposed
- Related: [Physical lock screen](0306-physical-lock-screen-remote.md), [Display sample](0308-sunshine-display-sample.md)

## Context

The macOS host refuses a paired viewer before Sunshine can ask the OS for frames when a signed-in user locks the display.

## Decision

Admit the existing grant on a signed-in active console. Bind each connection to its initial lock state. Keep the older JPEG profile limited to the unlocked desktop. The Moonlight host uses ScreenCaptureKit on the selected display. For locked streaming, restrict the display filter to the system login surface if macOS exposes it. End capture on a blank, suspended or stopped stream. A local metadata-only sample can measure full-display capture even when the lock surface cannot be identified.

## Alternatives

Continuing with AVFoundation would leave the behavior of ScreenCaptureKit unknown. Removing the shared predicate alone would also broaden the legacy profile.

## Consequences

An enumerated display or an image buffer alone cannot identify the login UI. A locked connection can still report capture unavailable. The viewer may reconnect after the OS changes console state.

## Verification

Build the macOS host, exercise admission, grants and transitions, and compare the installed display sample against the physical iPhone result on a locked host. Record capture, lock UI and click separately.

A signed development host linked ScreenCaptureKit and received a fresh 3456 × 2234 sample on an unlocked Mac. The private host handshake, controller replacement and grant-revocation regressions passed. A second Mac's SSH process had no Screen Recording grant. Locked iPhone video and a visible click on the login UI remain unverified.
