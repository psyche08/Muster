# ADR 0179: Publish the unified Apple session catalog

- Date: 2026-11-10
- Status: Accepted
- Related: [0178](0178-consistent-session-list-rows.md), [0166](0166-release-apple-1-1-28.md), [desktop and iOS updates](../DESKTOP_UPDATES.md)

## Context

The owner requested a paired release from main and a physical comparison of Claude, Codex and DeepSeek rows on the home screen. An installed desktop already reported 1.1.33 (123) from ea5b3fb, which predates the row change. The public macOS feed and iOS manifest previously offered 1.1.28 (118).

## Decision

Use version 1.1.34 (124) on both platforms. Package the clean main commit containing ADR 0178 and this version change as a signed macOS update and a registered-device iOS OTA release. Publish immutable packages before switching the feed, manifest and download page. Verify the delivered bytes through the page before device acceptance.

## Alternatives

Reusing 1.1.33 would leave existing installations at the same version despite different contents. A simulator and the width regression test can measure synthetic catalog rows. Checking the installed app also exercises the live catalog, nested sessions and real device typography.

## Consequences

The OTA page serves registered iOS devices. Physical iPhone layout acceptance requires an installation on a registered phone.

## Verification

Check the source stamp, macOS notarization and signature, iOS provisioning, public page, package hashes and desktop feed signature. Record physical observations independently for each platform.

## Publication evidence

Both archives came from clean main commit `3408c935975fe58d0aa0507974398fd1af903465`. The macOS daemon stamp and release ancestry check passed. Apple accepted notarization, the stapled ticket validated, and Gatekeeper accepted the portable. The full update package and portable contain identical signed app and daemon executables and sealed resource manifests.

The development-signed iOS archive and OTA IPA both identify 1.1.34 (124). The archive passed strict signature validation. Its provisioning profile covers three registered devices and expires on 2027-09-30.

| Public artifact | SHA-256 |
| --- | --- |
| Apple-silicon Mac portable | `f48e62121965290aacb3b899b62160f0340a4164149284b47e6040b1604bdb47` |
| Mac full update | `e1714469c6eaa5adb109e818dd4d19b4fae945f2d213f84a99543ea624e165a2` |
| iOS OTA IPA | `59ff20342b99f14a2ec2e9d4a21117bea8683c8db8d9b2b3ff5b782abe7da574` |

All 14 public files matched the staged hashes and cache policy. The download page led to the verified desktop portable and the manifest for the hash-named IPA. The desktop feed signature verified with the compiled public key.

After the public switch, the publisher removed 12 superseded files. All 14 current files passed the checks again. The six retired package URLs returned HTTP 410, and the historical manifest resolved to build 124.

The published portable launched on a physical Mac against its existing session catalog. The screen was locked at the time of observation and the system did not expose the catalog rows. No physical iPhone was present in Xcode. Row alignment on an unlocked device remains pending.
