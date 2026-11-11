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

## Release evidence

The public download page offers macOS and registered-device iOS 1.1.36 (126)
from main commit `1319501a5f312f25a8ce5565fb65e64a6f34e213`. The macOS
daemon stamp, release ancestry, Presence signing, Apple notarization, stapled
ticket, Gatekeeper and update-feed signature passed verification. The full
update package carries the same signed app and daemon as the notarized
portable. The iOS archive passed strict signature verification for three
registered devices (profile valid until 2027-09-30), and its IPA matched the
package named in the public manifest.

| Artifact | SHA-256 |
| --- | --- |
| macOS portable | `3e92429ce87230843e45c79859d68bba382911ef5d56eb154d25dc8e26441f2e` |
| macOS full update | `fe535c7a5a9227df8e5553de7a6d5af59f38608910ebce00e40225d6e39d28b1` |
| iOS OTA IPA | `65f301e7488cd3c3a9ce31c7caf6dec160d96e0b16c12028527eba59dcd03057` |

All 14 files matched the staged hashes over public HTTPS. The download and
install links, the cache policy, and an older manifest URL resolving to
build 126 were also verified. Superseded 1.1.35 files are still on the
server; pruning them waits for the owner's approval. No installed Mac was
updated and no physical iPhone was connected for this release.
