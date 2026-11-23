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
