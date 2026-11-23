# 0289. Keep the capture engine active for the paired desktop session

## Context

The private host exposes a single desktop with application ID zero. It launches no game process. After a paired viewer connects to the control channel, the upstream game-exit check sees no running game and sends a termination message. Moonlight reports early termination before the first frame.

## Decision

Use the existing session and broadcast shutdown signals for the private desktop control loop. The daemon owns the engine lifetime. Keep membership, desktop permissions, console state, input checks, ping timeouts and disconnect handling in force.

## Alternatives

Creating a placeholder game would couple desktop access to an unrelated process. Increasing timeouts cannot prevent a termination sent by the host.

## Consequences

A desktop-only session can remain connected while it produces frames. The host still shuts down when its owning session ends.

## Verification

Build and verify the signed macOS bundle. Connect from the packaged Windows viewer, verify a real frame, click a recognizable test target, and check the host-side receipt. Repeat after disconnect using the saved service grant.

The signed Mac acceptance bundle passed strict signature verification and contained the capture host under the Muster application identity. After installation and restart, a Windows viewer painted a real Mac desktop and the host recorded two distinct clicks sent from Windows. The initial viewer could retain a stale picture. See [ADR 0290](0290-desktop-media-bursts.md) for the subsequent receive-path change and live refresh result.
