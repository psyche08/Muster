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

### Publication and device verification

Both Apple archives were built from clean main commit `3e1deedf`. The
desktop package passed Developer ID and Presence signing checks, release
ancestry, notarization, stapling and Gatekeeper. The
[GitHub release](https://github.com/psyche08/Muster/releases/tag/v1.1.39)
tag points to that commit. Both public asset digests matched the local files.

The signed desktop feed and iOS registered-device IPA passed package checks.
All 14 files, download links and cache headers passed HTTPS verification.
Four superseded publisher files were removed. The portable SHA-256 is
`0b440835802da5839c4b8e92dfd7d3df090f483a35d64450ec6c4479c914dbc4`.
The registered-device IPA SHA-256 is
`5fe64c383f487b85d32f9e855b0e2762d29c198066b54b8d850d1b63ae355429`.

All three roster Macs reported version 1.1.39, source commit `3e1deedf`,
hardware-backed identity, registered relay and a working virtual network
after installation. Xcode listed the paired iPhone as unavailable during
the install checks. Its installed version is unverified.

App Store Connect processed the separate iOS distribution upload as VALID.
Build 129 was attached to the existing Public Beta group and submitted for
external beta review. At 08:47 CST on November 13 the review was waiting.
External availability of this build awaits Apple's decision.
