# ADR 0206: Publish Apple 1.1.42 (132) with background session selection

- Date: 2026-11-13
- Status: Accepted
- Related: [0205](0205-background-native-tasks.md), [0204](0204-release-apple-1-1-41.md)

## Context

The operator requested a locked-console retest and paired Apple release after merging PR #227. The preceding installed release located the target but lacked a composer during a test send, and its restoration timed out.

## Decision

Package both platforms from the merged main commit with version 1.1.42 and build 132. Exercise the signed macOS build on the test executor before publishing the desktop portable, OTA feed, registered-device iOS build and selected public TestFlight build.

## Alternatives

Testing the previous installed daemon cannot establish the behaviour of the new background selector.

## Consequences

Device installation, provider availability, session restoration and external beta review each require separate observations.

## Verification

Check commit ancestry, build versions, signatures, notarization and package digests. Record physical Locked Use selection, message receipts and relock separately from the release checks. Verify the published downloads and Apple build state.
