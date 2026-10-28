# ADR 0059: First public Apple release and pull-request-only main

- Date: 2026-10-28
- Status: Accepted; hosted configuration and release acceptance pending
- Related: [ADR 0023](0023-bootstrap-sanitized-repository.md), [ADR 0024](0024-cloud-external-testing-opt-in.md), [ADR 0057](0057-targeted-deployment-1-1-0.md), [ADR 0058](0058-duplicate-views-use-the-main-window.md), [release runbook](../DESKTOP_UPDATES.md)

## Context

The owner requests protection of main so changes land only through pull
requests, followed by the first public release with public iOS testing. The
current main contains the targeted 1.1.1 (91) fixes, but those installations
were not a paired public publication. There is no GitHub Release yet.

The initial GitHub API check still reports a private repository and rejects
branch-protection reads under the current plan. Public visibility must be
confirmed before applying protection; the historical private archive is not
part of this publication.

## Decision

Prepare paired Apple version 1.1.2 (92) from committed main-based source. Keep
existing product architecture and platform capabilities unchanged. Publish a
signed, notarized Apple-silicon macOS package and an App Store Connect iOS
build. Explicitly opt this selected iOS build into the existing public beta
group and Beta App Review. Only advertise the public invitation as available
when the selected build is approved for external testing.

Protect main by requiring a pull request for every update, including owner
updates, with no bypass actors and no force push or deletion. Do not invent a
mandatory reviewer or require disabled CI checks. Keep merge commits available
for the repository's finish-session workflow. Apply and read back the hosted
rule; this ADR alone does not establish protection.

Keep default Xcode Cloud external distribution off as required by ADR 0024.
The explicit public beta selection does not authorize automatic distribution
of future builds. Publish through the existing operator-configured release
root and a GitHub Release with versioned artifacts and SHA-256 checksums.
Do not place credentials, private deployment details, raw operational logs,
or private historical refs in the public repository or release assets.

## Alternatives

- Reuse a targeted installation's version: makes the first paired public build
  harder to distinguish from earlier device-only acceptance.
- Allow owners to push directly: contradicts the PR-only requirement.
- Require an independent reviewer without an available reviewer: would block
  the owner's existing PR workflow without a requested review policy.
- Use registered-device OTA as the public iOS channel: does not let arbitrary
  testers install; public testing requires TestFlight and Apple's acceptance.
- Enable default external Cloud distribution: changes future release policy
  beyond the selected build and conflicts with ADR 0024.

## Consequences

Main changes must pass through a PR. macOS publication, iOS processing,
external review, and public invitation availability are independent results.
Apple may delay public testing after upload. Signing identities, provisioning,
notarization access, and operator-supplied deployment configuration remain
prerequisites; no Keychain unlock is automated. An upload or download check
is not physical-device acceptance.

## Verification

Planned: generated Apple project consistency, public-content check, ADR gate,
release-ancestry tests, paired release compilation, strict nested signature
and Presence entitlement checks, clean daemon source stamp, release ancestry
and fingerprints, Apple notarization and Gatekeeper validation, and artifact
checksums. Read back the main rule and the iOS build's processing, beta-group,
review and external-testing state. Verify published downloads match built
artifacts. Keep unresolved acceptance explicit; no new device installation or
end-to-end provider acceptance is claimed by these packaging checks.
