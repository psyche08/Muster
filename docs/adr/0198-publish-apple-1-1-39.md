# ADR 0198: Publish Apple 1.1.39 and select build 129 for external testing

- Date: 2026-11-13
- Status: Accepted
- Related: [0196](0196-foreground-waits-and-provider-queue.md), [0197](0197-inline-recorded-work.md), [0024](0024-cloud-external-testing-opt-in.md)

## Context

The published Apple packages predate the send queue fixes and inline session
descriptions. The operator requested both platforms, the registered devices,
GitHub, the OTA page and the public TestFlight invitation.

## Decision

Build macOS and iOS 1.1.39 (129) from the same clean main commit. Publish the
notarized macOS package and checksum to GitHub and the signed desktop feed and
registered-device iOS package to OTA. Submit this iOS build to the existing
public external testing group. Default workflow distribution remains opt-in.

## Alternatives

A registered-device install alone cannot serve public testers. A repository
release alone does not update the installed Mac clients' signed feed.

## Consequences

Public TestFlight availability depends on Apple's processing and beta review.
An iPhone install also needs an available device connection or user action.

## Verification

Check archive versions and source stamp, signatures, notarization, feed and OTA
readback, GitHub assets, installed device versions and App Store Connect build,
group and review status independently.
