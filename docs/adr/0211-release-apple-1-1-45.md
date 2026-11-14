# ADR 0211: Release Apple 1.1.45 (135) with the refreshed pricing catalog

- Date: 2026-11-14
- Status: Accepted
- Related: [pricing catalog](0210-refresh-turn-pricing.md), [Apple distribution](0209-release-apple-1-1-44.md), [beta opt-in](0024-cloud-external-testing-opt-in.md)

## Context

The operator requested publication of the model pricing fix. The previous
Apple release was 1.1.44 (134).

## Decision

Version both Apple targets as 1.1.45 (135) and build from the release merge
on main. Publish the notarized Mac portable and signed update, the GitHub
release, the registered-device iOS OTA package and the internal TestFlight
build.

## Alternatives

Reusing the preceding version would prevent installed copies from recognizing
the pricing fix as an update.

## Consequences

Mac executors calculate estimates using the bundled November catalog after
updating. Registered iPhones can use the paired OTA release.

## Verification

Check the project versions and daemon source revision. Verify signing,
notarization, public download hashes, the signed feed, OTA manifest, and
App Store Connect processing and internal group access. Device installation
is a separate operation.
