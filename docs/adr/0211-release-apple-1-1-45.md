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

### Publication on 2026-11-14

Both packages were built from main commit
`048336c26d22f26fb3df9a9caf8cb0a6548312e0`. The Mac daemon stamp matched
the commit and the binary contained the November pricing catalog revision.
The macOS package passed Developer ID and Presence signature checks,
notarization, stapling and Gatekeeper. The full update matched the signed
portable. The portable SHA-256 is
`600a509018f269ea1f10b0c15d4d1be431571d431d01cdc798e1f1a0b2c6c739`.

The registered-device iOS package matched the signed archive. Its SHA-256 is
`2fd59853f661a7e75c4f71a2ddee2d06b92dbdd01657f401feca9da45f741e69`.
The separate App Store export passed strict signature and production entitlement
checks. App Store Connect reported build 135 as VALID and IN_BETA_TESTING
in the Internal group. External beta submission remained available as a
separate operator action.

All 14 public files matched staged hashes and cache headers. The live feed
signature, page links, manifest redirection and retired package responses
passed verification. The GitHub release tag matches the build commit and both
asset hashes matched. Physical installation was not tested in this release.
[Download page](https://linsheng.dev:8443/muster/) ·
[GitHub release](https://github.com/psyche08/Muster/releases/tag/v1.1.45)
