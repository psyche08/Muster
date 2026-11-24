# 0304. Display mobile Remote video at native pixel size

- Date: 2026-11-24
- Status: Accepted
- Related: [0299](0299-apple-remote-workspace.md), [0300](0300-apple-remote-stream-settings.md), [0301](0301-desktop-gamestream-roles.md)

## Context

Fitting a desktop stream into a phone screen can shrink text and pointer targets. The negotiated stream dimensions already describe the source pixels.

## Decision

All native mobile Remote viewers display one source pixel per physical display pixel. Clip oversized frames to the viewport. Center an image along any axis where it fits. A three-finger pan moves the local viewport in both axes and works with view-only access. Remote pointer coordinates refer to the full source frame, including after panning. Keep the viewport within the image when the viewport or video dimensions change.

## Alternatives

Fit-to-screen rendering prioritizes an overview at the expense of legibility. Changing the encoder resolution to fit the viewport would also change network traffic. Pinch zoom makes the pixel scale variable.

## Consequences

Phones, tablets and foldable mobile viewers share this interaction contract. Each shell adapts display density, touch recognition and clipping to its native view system. The receiving host continues to enforce input permissions. iOS and iPadOS currently have a video surface. Android and HarmonyOS require a Remote controller and decoder before they can present a stream.

## Verification

Verify a high-density display against a known source size, pan to both ends, check pointer mapping after moving, and exercise view-only panning. Report signed device results separately from geometry checks and simulator builds.
