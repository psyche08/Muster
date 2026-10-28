# ADR 0059: First public Apple release and pull-request-only main

- Date: 2026-10-28
- Status: Accepted; public macOS publication, iOS beta review pending
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


### Packaging evidence, 2026-10-28

- **Source:** `02718a540a5d3bd0fc1e979e5b81bf0c90a6d14a`, merged by
  PR #56 (`836ad6a0`). Version 1.1.2 (92) is identical across both Apple
  targets. The public-content scan passed for 723 tracked text files, all
  seven release-ancestry regression tests passed, and the ADR/diff gates passed.
  Hosted GitHub Actions remain disabled under ADR 0016; no CI pass is claimed.
- **Builds:** optimized daemon/CLI/browser host and all three Apple FFI targets
  compiled; the macOS Release build and iOS Release archive/export succeeded.
- **macOS signing:** Developer ID and nested signatures, Presence provisioning,
  clean daemon stamp, recorded-release ancestry and required fingerprints passed.
  A transient Apple timestamp-service failure during the first packaging attempt
  was resolved by retrying with timestamping still enabled. Notarization
  `0ee50f89-4895-438c-81fa-6d16e7f23120` was Accepted; ticket stapling, stapler
  validation, strict nested verification and Gatekeeper assessment passed.
  Final portable ZIP SHA-256 is
  `7c39a006294ac5af18af100c8f411c75ee4dc05b39cd041e653ec9b21b08c500`.
- **iOS signing/upload:** App Store Connect export passed strict signature and
  version checks with production APNs and debugging disabled. IPA SHA-256 is
  `e3e200d47f06289d9777c2684d9646e4636acfe2edd2b77f71e795d17197a3fc`.
  Apple acknowledged the upload without errors; build processing and selected
  public-beta acceptance remain pending at this evidence checkpoint.
- **GitHub publication:** release `v1.1.2` is a draft targeting the exact build
  commit above. Its uploaded ZIP digest matches the local hash. No public
  release or source-visibility change is claimed. The API still reports private
  visibility and rejects protection with HTTP 403 under the current plan;
  confirmation of public visibility versus a private-plan upgrade is pending.
- **Download site:** the operator's private release destination, SSH port and
  root have not been supplied in this session. No download-site mutation or
  signed update-feed publication is claimed; no feed private key was available
  to this packaging run.
- **Installation/runtime/end-to-end:** not performed for this version. Prior
  device acceptance in ADRs 0057 and 0058 is not a new acceptance run.


### Public distribution decision and hosted evidence

The owner confirmed public visibility and supplied the private publisher
configuration on 2026-10-28. GitHub now reports `public` for Muster; the
historical archive remains private. Active ruleset `26021219` applies only to
`refs/heads/main`, requires pull requests, blocks deletion and force pushes,
and has no bypass actors. Zero approving reviews are required; this does not
waive the PR requirement. Hosted Actions remain disabled.

The App Store Connect build for 1.1.2 (92) is VALID and IN_BETA_TESTING for
internal testing. Membership in the existing Public Beta group is verified;
its external state is WAITING_FOR_BETA_REVIEW. No new public-build availability
or physical iPhone installation is claimed. The group invitation is enabled;
it may still provide an older approved build until Apple's review completes.

The public download page is now sourced from `deploy/public/download.html`.
It makes TestFlight the primary iOS route and explicitly states the selected
build's review status. Registered-device OTA remains a separate secondary
route with the existing signed app, hash-bound manifest and fixed aliases.
Publish the same page at the existing root, desktop and iOS entry points.
This continues the first-public-release decision; it does not alter future
Cloud distribution. Without an available feed signing key, publish the signed,
notarized portable Mac package and remove obsolete update metadata only after
validating both new platform aliases, as the release runbook requires.
