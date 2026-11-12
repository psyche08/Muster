# ADR 0195: Publish the Chrome terminal identity probe in Apple 1.1.38

- Date: 2026-11-12
- Status: Accepted; both Apple artifacts published and the installed macOS probe verified
- Related: [0182](0182-publish-native-create-options.md), [0188](0188-publish-macos-trusted-dispatch.md), [0193](0193-jumper-browser-identity-probe.md), [desktop updates](../DESKTOP_UPDATES.md)

## Context

The signed macOS 1.1.37 package predates the Jumper identity probe. The earlier end-to-end check used a development CLI, native host and unpacked extension.

## Decision

Publish macOS and registered-device iOS as 1.1.38 (128) from the same clean main commit. Install the verified macOS release on the test Mac and bind Chrome to the host and extension in the installed bundle. Invoke the existing skill with its CLI override unset.

## Alternatives

Continuing to run the development binaries leaves the installed package and the skill's default executable selection untested.

## Consequences

macOS operators can use the signed CLI and native host shipped together with the extension. The configured terminal site and target remain operator-local. The iOS OTA follows the repository's paired publication policy.

## Verification

Verify source stamp, signatures, notarization and stapling, macOS full-package contents, iOS version and provisioning, and the published files and feed. Separately verify the installed CLI and native host paths, extension resources, fresh output boundaries and exit receipt in the signed-in browser.

## Published and installed evidence

On 2026-11-12, clean main commit `bc5616789e5687a523cbf602f961d40e3467247f` produced both 1.1.38 (128) builds. Source ancestry, the daemon stamp and Presence signing passed. Apple accepted macOS notarization. Stapling, Gatekeeper, the signed feed and its public key verification passed. The full update matched the install archive for the CLI, native host and extension. The registered-device iOS IPA matched its signed Xcode archive. All 14 files, the install links and cache policy passed public HTTPS readback after removal of four superseded files. The macOS archive and checksum were also published in the [GitHub release](https://github.com/psyche08/Muster/releases/tag/v1.1.38).

| Artifact | SHA-256 |
| --- | --- |
| macOS install archive | `42a447db755c141480de4d159f150618a78c4961183d4f73e595f519f099e4b1` |
| macOS full update | `2736ee773a1c53f0c7326f1c1aba6429c99b2104a8abee1bc1b69a11641957a6` |
| iOS OTA IPA | `b8580b1e48137d0a617847cc83816ea28833d6a2c2b082ce9a9f14391659a024` |

The test Mac acquired its update lease, installed the verified archive and retained its hardware identity. Chrome loaded the extension from the installed application, and the signed native host in the same bundle answered a version 0.4.0 ping. The skill then ran with both CLI and state-directory overrides unset and only system directories in PATH. The fresh probe read a single account line between the command echo and target prompt, saw the gateway prompt return after the exit echo, and reported closed with the page released. The iOS publication was verified through its manifest and bytes; physical iPhone installation was not exercised.
