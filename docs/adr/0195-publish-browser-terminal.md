# ADR 0195: Publish the Chrome terminal identity probe in Apple 1.1.38

- Date: 2026-11-12
- Status: Accepted for release; publication and installed acceptance pending
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
