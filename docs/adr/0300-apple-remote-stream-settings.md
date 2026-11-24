# 0300. Configure the Apple Remote stream before connecting

- Date: 2026-11-24
- Status: Accepted
- Related: [0299](0299-apple-remote-workspace.md)

## Context

The Apple viewer uses a pull-based JPEG stream. Its fixed 1280-pixel capture,
10 fps ceiling and JPEG quality prevent users from trading detail and traffic
against responsiveness.

## Decision

Store the viewer's resolution limit, frame rate ceiling, video payload bitrate
limit, JPEG quality and cursor visibility as one local preference. Shared Rust
validates it before saving, before connecting and on host admission. Capture
preserves the display aspect ratio. The host limits the frame cadence using
both elapsed time and actual compressed frame size. Existing sessions retain
the profile with which they opened.

## Alternatives

Scaling decoded frames changes their display size without reducing capture
traffic. Encoder quality alone cannot bound bitrate on a changing screen.

## Consequences

The original settings use the original handshake. Custom settings require an
updated Mac host. The bitrate ceiling applies to JPEG payload over time and
allows a single frame burst. Screen content, display size, capture time and
network latency affect the achieved resolution and frame rate. For images that
exceed the 768 KiB frame limit, the encoder reduces quality and then dimensions
until the image fits.

## Verification

Exercise preference round trips, validation, the old handshake, custom profile
decoding and frame pacing. Build the macOS and iOS applications independently.

A synthetic 2560-pixel square filled with noise exercised the frame-size fallback
at all three quality settings. Each result decoded within the frame limit.
