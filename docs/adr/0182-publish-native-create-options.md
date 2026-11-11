# ADR 0182: Publish native create options as Apple 1.1.36 (126)

- Date: 2026-11-11
- Status: Accepted
- Related: [0181](0181-native-create-model-and-effort.md), [0180](0180-compact-ios-catalog-status.md), [0179](0179-publish-unified-apple-catalog.md), [desktop and iOS updates](../DESKTOP_UPDATES.md)

## Context

The owner asked for a release from main after ADR 0181 landed. A new desktop
session can now take its model and effort before the first message, and the
daemon advertises `agent.native_create.options.v1`. The public macOS feed and
iOS manifest offer 1.1.35 (125), built from `89e18f07`, which predates that
change.

## Decision

Use version 1.1.36 (126) on both platforms. Package the clean main commit
that contains ADR 0181 and this version change. Ship it as a signed and
notarized macOS update and a registered-device iOS OTA release. Publish the
immutable packages before switching the feed, manifest and download page,
then verify the delivered bytes through the page.

## Alternatives

- **Publish macOS only.** The repository pairs every macOS update with an iOS
  build unless the owner says otherwise, and the iOS app carries the same
  protocol bindings.
- **Reuse 1.1.35.** Existing installations would see the same version with
  different contents, and the updater would not offer it.

## Consequences

Installed Macs are offered 1.1.36 through the signed feed. Callers can check
`agent.native_create.options.v1` on updated Macs before they send a model or
an effort. The OTA page serves registered iOS devices only.

## Verification

Check the daemon's source stamp, release ancestry, notarization, stapled
ticket, Gatekeeper, iOS provisioning, the public page, package hashes and the
feed signature. Record each platform's results separately.
