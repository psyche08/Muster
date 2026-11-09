# ADR 0157: Publish Apple 1.1.27 (117) on the OTA page

- Date: 2026-11-07
- Status: Accepted
- Related: [ADR 0154](0154-wait-for-slow-codex-windows.md), [desktop update distribution](../DESKTOP_UPDATES.md)

## Context

The public page offered 1.1.24 (114). The operator requested OTA publication
after local deployment of 1.1.27 (117). ADR 0154 records the local installation
and runtime evidence.

## Decision

Publish the existing notarized Apple Silicon ZIP, its signed full-update feed,
and a registered-device iOS archive from the same release commit,
`7b489084f782d13320c6de806284803a2ff727b3`. Publish the named packages
before replacing the fixed download aliases, feed and installation pages.

## Alternatives

Changing only the HTML would advertise build 117 while the fixed download
links still served build 114. A new build from main would require its own
source identity and package verification.

## Consequences

Mac users can download the portable package or receive the signed update.
The iOS install link requires a device included in the provisioning profile.
Physical iPhone installation and bidirectional Mac session-load acceptance
remain separate device checks.

## Verification

- The macOS portable and full-update package have matching executable CDHashes.
  Apple accepted notarization submission `4416b3c1-8613-49b6-81f2-d61df3f45585`.
- The iOS archive and packaged application identify version 1.1.27 (117).
  Both installation manifests reference the IPA by its SHA-256.
- Downloaded all 14 published files over HTTPS and matched each SHA-256
  against staging. All 20 URLs checked, including the three page routes and
  the links used by the install buttons, returned the required cache directives.
  The downloaded full package matched the signed feed's hash and size.
  OpenSSL verified the public feed signature against the release public key.
- Portable ZIP SHA-256: `f704746cbcf2d370f034e883c76d4e6ea699062cf1236fa4b5f5e8a9beddfaea`.
- IPA SHA-256: `d3bac1b0dcf1367c79a31c28dacc0b6499ebdddd31c1e62bc63e64f6fec23a34`.
