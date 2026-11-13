# ADR 0209: Release Apple 1.1.44 (134) with single-window session selection

- Date: 2026-11-13
- Status: Accepted
- Related: [0208](0208-one-window-per-native-session-operation.md), [0207](0207-apple-143-retest.md)

## Context

The operator requested a paired macOS and iOS release of the merged single-window fix. The preceding macOS retest used build 133.

## Decision

Build both applications from the release merge on main. Notarize the desktop package, exercise the known two-window case on the executor, and distribute through GitHub and the signed desktop feed. Publish the registered-device iOS package and submit this specific build to the existing public TestFlight group.

## Alternatives

Reusing the preceding build would make it difficult to distinguish installed versions during acceptance.

## Consequences

Desktop executors need the updated daemon. Apple controls external beta availability after submission.

## Verification

Check versions, ancestry, signatures, notarization, downloaded hashes and installed daemon identities. Record the result of physical session selection independently of package verification. Check App Store Connect processing, group membership and external beta state.
