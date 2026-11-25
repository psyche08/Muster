# 0313. Display one video pixel per mobile layout point

- Date: 2026-11-25
- Status: Accepted
- Supersedes [0304](0304-mobile-remote-native-pixels.md)

## Context

On high-density phones, small desktop controls were hard to identify and their response to a tap was difficult to see.

## Decision

Give every decoded pixel one logical display point on mobile. A 1920-pixel-wide frame occupies 1920 points. Clip overflow, center smaller axes, and use a three-finger pan to move the viewport with either view or control access. Convert input in the video canvas's coordinate system so a pan moves the visible region without changing what a tap targets.

## Alternatives

Using physical device pixels made the controls smaller as screen density increased. Fitting the video to the screen or changing the encoder resolution would make the scale depend on the phone or the network settings.

## Consequences

Fewer source pixels fit at once. The existing iPhone and iPad viewers implement the rule. Future mobile viewers can apply the same logical-unit mapping. Streaming settings and host pointer coordinates retain their meanings.

## Verification

Check canvas dimensions, panned input, centering and view-only scrolling on a Retina device. Report real-device observations separately from host capture and authorization.
