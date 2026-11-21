# ADR 0280: Open a paired desktop from the native device view

- Date: 2026-11-21
- Status: Implemented; two-device capture and mouse acceptance pending
- Related: [ADR 0242](0242-muster-remote-desktop-engines.md), ADR 0277–0279

## Context

The host and transport have independent tests but neither the installed daemon nor the desktop app starts an authorized stream.

## Decision

The local Mac owner chooses the paired viewer and either view or mouse access. The local Windows viewer selects the paired host. Shared Rust owns the host offer, PeerLink session, private child lifetimes, temporary certificates, launch negotiation and failures. The native viewer decodes H.264 with the bundled Moonlight and FFmpeg helper, draws pixels and maps pointer events in its window. Keep GPL media code in the separate helper and include license and corresponding source information with the distribution. Restrict the first stream profile to 720p, 30 fps, stereo negotiation without playback, and bounded packets for the relay.

## Alternatives

An external Moonlight application would require its own pairing and would lose the host owner's ability to stop the stream in Muster. The upstream Qt shell includes management and discovery unrelated to viewing the selected peer.

## Consequences

The host requires Screen Recording for viewing and Accessibility for mouse control. Terminating the owner must close the child and its bridge. The device view must surface missing components, absent authorization and startup failures.

## Verification

Build both native binaries, test the viewer's H.264 decode and pointer mapping, exercise host/viewer selection and denial through the normal UI, and confirm a real frame and harmless mouse movement on two devices. Record deployment and test fixtures separately.

On Windows the native x64 viewer built with MSVC and the pinned static
dependencies. CTest passed the loopback, host bootstrap, and H.264 decode
checks. The decode test drew a synthetic blue 720p frame to a 640×480 DIB,
checked its center and letterbox pixels, and mapped its last displayed pixel
to the source corner. The Windows native shell and the macOS SwiftUI target
compiled. Rust device action projection, launch parsing and remote caller
rejection were exercised. On the test Mac, the selected host authenticated
the Rust client for both the application list and encrypted launch. Actual
capture returned status 503 and the host reported missing screen recording
permission. Rendering this Mac and delivering mouse input remain unverified.

Across a Windows PC and a Mac, two temporary software-identity nodes completed
the normal invite and owner approval process, each reporting its installed local
helper. The Windows open request without an offer was denied. The Mac's local
share request was also denied while its console was locked. No real screen or
mouse movement was observed. The Windows app-core suite had the same seven
failing tests as a separate build of the unchanged main branch.
