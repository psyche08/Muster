# ADR 0092: Publish Apple 1.1.11 (101) and install it on the three Macs

- Date: 2026-11-02
- Status: Accepted; publication and installation pending
- Related: [ADR 0091](0091-prune-conversation-views-only-where-separate.md),
  [ADR 0089](0089-paired-release-1-1-10.md),
  [ADR 0088](0088-unattended-desktop-updates.md),
  [ADR 0081](0081-paired-release-1-1-8.md),
  [DESKTOP_UPDATES](../DESKTOP_UPDATES.md)

## Context

The hourly send check found a regression on desktop 1.1.10 (100): ADR 0080's
sidebar walk pruned ChatGPT.app's single web area, so remote sends to a Codex
session that was not the open one were refused as "desktop content is not
ready". ADR 0091 makes that prune profile data (Claude yes, ChatGPT no) and
merged as PR 94. The published line is 1.1.10 (100), installed on the three
Macs; the fixed source is on `main` and needs a release to reach them.

1.1.10 is also the first build that trusts the ADR 0088 signed update feed and
installs a verified update unattended when the Mac is idle. At publication time
the three Macs were not reachable for a manual rollout (all members were
offline/asleep after their unattended 1.1.10 update).

## Decision

Publish **Apple 1.1.11 (101)** from the merge commit that carries this version
bump, following the ADR 0081/0089 process:

- `MARKETING_VERSION: 1.1.11`, `CURRENT_PROJECT_VERSION: 101`, committed Xcode
  project regenerated with xcodegen 2.46.0; the release worktree is a clean
  detached checkout of the bump merge commit `C`, whose daemon stamp must equal
  `C`.
- macOS release daemon, CLI, relay and browser host, Apple FFI archive and the
  Developer ID app; Velopack 1.2.158 portable and full package; notarize,
  staple, strict signature, Gatekeeper and presence-signing checks.
- The Velopack feed `releases.osx-arm64.json` is signed with the ADR 0088
  private key by `sign-feed.py`, which verifies the signature against the
  public key compiled into the app. The publisher receives the package first,
  then the feed and its signature, so no client can see a feed naming a missing
  package.
- Paired iOS OTA build for the three registered devices and the unified page at
  the private publisher, with `no-store` headers, both platforms published.
- **Deployment:** exactly the three Macs. Where a Mac is reachable it is
  installed and verified as before; the signed feed is the delivery path for a
  Mac that is idle before it is reachable, per ADR 0088. No other channel is
  published.
- Not in this release: GitHub Release, TestFlight upload/review, App Store
  export, server prune, and any installation beyond the three Macs.

## Alternatives

- **Publish nothing and wait for a manual wake of the Macs.** Leaves the
  regression on the installed fleet and wastes the unattended-update path that
  1.1.10 exists to provide.
- **Reuse 1.1.10.** Its bytes predate ADR 0091; rebuilding different source
  under the same number would obscure provenance.
- **Ship with the old feed key.** Pre-1.1.10 builds trust a retired key, and
  1.1.10+ would reject the feed.

## Consequences

- The three Macs receive the Codex sidebar fix through the signed feed once
  each is idle, or by the normal guarded installation when reachable.
- The OTA page offers macOS 1.1.11 and the paired iOS build; other channels are
  unchanged; superseded server artifacts remain until the owner authorizes a
  prune.
- A Mac that is offline during publication keeps its current build until its
  next idle check; publication is not proof of installation.

## Verification

Recorded separately below: source ancestry and fingerprint gates, build and
Swift compilation, signing/notarization/stapling/Gatekeeper, package and feed
hashes and signature, iOS archive and profile, published pages/manifest/feed
read back over HTTPS with cache headers, and per-Mac installation state (manual
or unattended) with unchanged identity. Unverified items are stated as such; no
real conversation is used as a release test.
