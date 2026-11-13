# ADR 0204: Publish Apple 1.1.41 (131) with native session restoration

- Date: 2026-11-13
- Status: Accepted
- Related: [ADR 0202](0202-publish-apple-1-1-40.md), [ADR 0203](0203-restore-native-navigation.md)

## Context

Build 130 was already uploaded to Apple when distribution was paused. The operator selected PR #225 for the next paired release.

## Decision

Build macOS and iOS 1.1.41 (131) from the clean main commit containing the native restoration change. Publish the notarized portable to GitHub, the signed desktop feed and registered-device iOS package to OTA, and build 131 to the existing public TestFlight group. Install on reachable devices.

## Alternatives

Build 130 predates the restoration change and its upload is immutable.

## Consequences

Executors need the new daemon for restoration. Physical two-device send and inspection acceptance remains open. Apple's external review controls when public testers receive the selected build.

## Verification

Check source ancestry, signatures, notarization, package versions, public download digests, installed device identities and App Store Connect group and external build state.
