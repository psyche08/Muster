# ADR 0315. Group settings and expose permission controls

- Date: 2026-11-26
- Status: Accepted
- Related: [Service access](0284-basic-network-and-device-service-access.md), [desktop grants](0287-desktop-service-authorization.md), [Windows navigation](0263-windows-back-and-settings-shortcuts.md)

## Context

A single settings list mixes personal preferences, member administration and incoming service policy. Some CLI permission controls have no visible editor. Phones consume services and can administer member access, while desktop owners also configure incoming offers.

## Decision

Organize each native settings surface into selectable groups. Put signed member permissions under a named device and receiving policy under this device. Offer native pickers, switches and editable lists for the applicable CLI permission controls. Preserve unedited policy fields, provider selections and role when submitting changes. Continue to use the existing Shared Rust intents and daemon authorization for each operation.

The Basic editor offers explicit removal of retained Advanced port grants and receiver port rules. The member editor includes all provider-independent view/control scopes, custom provider IDs and the signed member role. An optional enrollment field distinguishes a manageable desktop/server Presence binding from an original pairing key or an older daemon. Enrollment and revocation use the existing admin RPC.

## Alternatives

A raw JSON or command-line editor requires users to know the policy schema. A second permission model in each shell would diverge from the daemon.

## Consequences

Local display preferences and navigation are platform-specific. Access changes retain the CLI's membership, compatibility, local-owner and operating-system requirements. Installed devices need an updated client to display new controls.

## Verification

Check member role, Basic and Advanced scopes, service offers, application allowlists and web origins. Verify that changing one section preserves other fields. Compile the native clients where toolchains are available. Track installation and physical-device interactions separately.

The app-core suite passed 209 tests. The access, Presence and two-device flow suites passed 10 tests, including local policy ownership and explicit Basic cleanup. Two tests were ignored across these suites. macOS and iOS Simulator compiled the shared SwiftUI target. The Windows x64 cross-build, Android Kotlin build with 32 unit tests, and unsigned HarmonyOS UI-only build passed. HarmonyOS binding checks covered optional field presence and round trips. Installation, signing and physical interaction with the new screens remain unverified.
