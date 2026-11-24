# 0301. Use Sunshine hosts and Moonlight controllers for Remote

- Date: 2026-11-24
- Status: Proposed
- Related: [0242](0242-muster-remote-desktop-engines.md), [0282](0282-source-built-desktop-engines.md), [0295](0295-desktop-takeover-confirmation.md), [0299](0299-apple-remote-workspace.md), [0300](0300-apple-remote-stream-settings.md)

## Context

The Apple JPEG pull profile delays pointer feedback until the next requested frame. A desktop can have incoming sharing grants independently of its choice to control another desktop.

## Decision

Use the vendored Moonlight core on the viewing device and a session-managed Sunshine process on a Mac or Windows host. Carry GameStream over the fixed-channel PeerLink broker. Let Rust validate the configured stream, check persistent grants, and arbitrate occupancy. Retain the existing Windows handshake and the previous Apple host profile for older clients. Use an independent video callback for ordered H.264 access units. The Apple display adapter owns decoding and presentation.

The macOS viewer is a separate executable. The iOS development target embeds the core, as requested by the owner. Review the combined binary and source distribution before an iOS release. This decision supersedes the JPEG transport choice in 0299 for new viewers and keeps the persisted preferences from 0300.

## Alternatives

Increasing JPEG cadence still sends independently encoded frames. Running Moonlight on the host would require an additional media transport and would change the requested controller architecture.

## Consequences

The configured bitrate is an encoder target subject to the chosen ceiling. A zero ceiling allows automatic selection. Map the persisted quality preference to the automatic bitrate. Each incoming session obtains its own launch credentials. Retain OS-specific lock checks and the input gate at the host.

## Verification

Build both Apple targets, verify isolated engine packaging and Windows host admission. On paired Macs measure displayed frames over elapsed time and click-to-present intervals on the viewer's monotonic clock. Record settings, sample size and missing physical coverage separately.
