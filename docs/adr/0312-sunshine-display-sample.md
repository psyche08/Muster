# ADR 0312: Check physical display sampling in the macOS Sunshine host

- Date: 2026-11-24
- Status: Proposed
- Related: [Physical lock screen](0306-physical-lock-screen-remote.md), [GameStream roles](0301-desktop-gamestream-roles.md)

## Context

The Remote viewer needs the physical monitor stream over GameStream, including when a signed-in user locks the console. An SSH tool can have different Screen Recording permission than the installed host. Window shareability, monitor sleep, host session eligibility and arrival of a video sample are independent observations.

## Decision

Keep Moonlight as the controller and select the physical display through the installed Sunshine host. Provide the local command `muster debug desktop-display-sample` to check the normal AVFoundation input for that display. Run it under the host's ordinary permissions, take the same local desktop slot as a real capture and bound its lifetime. Report the display state and dimensions of the first sample. The diagnostic exposes no pixels, network listener or input mechanism.

## Alternatives

A second remote-login protocol asks the operator to start another session. Testing a separate recording program measures that program's authorization. Neither validates the installed display source.

## Consequences

A successfully delivered sample establishes capture availability. Establishing that it shows the physical login screen and that authorized pointer input reaches it still requires physical comparison under ADR 0306.

## Verification

Build the signed product. Check the installed host on an awake physical display with the console locked. Record the local sample result, the iPhone's first frame and a harmless visible click separately.

A signed development installation received a fresh 3840 × 2160 AVFoundation sample on an active primary display while the signed-in physical console remained locked. The separate SSH process reported no screen-recording permission. The physical phone's direct Moonlight attempt was refused by the existing host admission check before Sunshine started. No pixels were retained by the sampler. Neither a view of the physical login screen nor a delivered pointer event was verified.
