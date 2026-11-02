# ADR 0092: Publish Apple 1.1.11 (101) and install it on the three Macs

- Date: 2026-11-02
- Status: Accepted; published; installation via the signed feed and per-Mac verification pending
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

### Source, build and signing — 2026-11-02

- The only built source was the merge commit of PR 95,
  `d2246e4841cc0332a955f759b74044aeed98625a`; the release worktree was a clean
  detached checkout of it. The Cargo release build (daemon, CLI, relay, browser
  host), the Apple FFI archive (macOS, iOS device, iOS simulator) and the Xcode
  Release app all succeeded.
- The built daemon carried exactly one `MUSTER-BUILD-COMMIT:d2246e48…` stamp;
  `verify-release.sh` reported the release ancestry and shipped fingerprints
  verified, and presence signing verified for `89LGY6BD53.dev.linsheng.muster`.
- Notarization `4dae3ad1-562b-4cff-a220-8a2d7ea63f25` returned `Accepted`;
  stapling, strict `codesign --verify --deep`, and `spctl` (`accepted,
  source=Notarized Developer ID`) passed on the packed bundle.
- Desktop portable ZIP SHA-256:
  `b86abd77cb91640e06561bb22a7d434d5be1625f993bb7181097964be4135426`.
  Packaged daemon SHA-256:
  `79a03ef8ab03163f410fd1089992d12dc1e2c8a00dae7b9bf9d281982c5abf8f`.
  The bundle's `sq.version` reports 1.1.11.
- The paired iOS archive built as 1.1.11 (101), team `89LGY6BD53`,
  development-signed for three registered devices (profile expires 2027-09-30,
  `get-task-allow`). No App Store export and no TestFlight upload were made.
  OTA IPA SHA-256:
  `7d55887e25c262ca6473760ac072eeb234d1e2ae84707e7d5f1bd82e9484f31b`.

### Signed feed and publication — 2026-11-02

- Velopack produced the portable and the full package
  (`Muster-1.1.11-osx-arm64-full.nupkg`, SHA-256
  `02c491d671ac9c597939b65abda95f3593ff8b7c21f2154574512a58127a3a1e`).
  `sign-feed.py` signed `releases.osx-arm64.json` (SHA-256
  `3dc72c79ef1717017c20a4976a8a3245e5064481cd88a7668bf08af16cd9a745`)
  with the ADR 0088 key and reported `Verified OK`.
- The publisher accepted 14 files and verified their hashes on the server
  (`{"published_files":14,"mac_version":"1.1.11","ios_version":"1.1.11","build":"101","feed":"releases.osx-arm64.json","hashes_verified":true}`).
  The package was published before the feed and its signature.
- Read back over HTTPS: the root, `ios/` and `desktop/` pages equal the
  generated page; the Mac portable and IPA match their SHA-256 values; the
  manifest names build 101 and the hash-named IPA; the previous 1.1.10 manifest
  resolves to the current one; the published feed signature verifies against
  the public key compiled into the app, and the published full package matches
  the feed's SHA-256 and size; pages, manifest, packages, feed and `SHA256SUMS`
  carry `no-store, no-cache, must-revalidate, max-age=0`.
- The OTA page now offers macOS 1.1.11 and the paired iOS build. No GitHub
  Release, TestFlight upload or server prune was performed.

### Installation — pending

At publication time all three Macs were offline (they had installed 1.1.10
unattended and were asleep), so no manual installation, daemon-hash check or
identity verification was performed. They run 1.1.10, which trusts the ADR 0088
feed, so the signed 1.1.11 update is the delivery path: each Mac installs it
unattended when idle. Per-Mac version/build/identity confirmation after that
remains open, and a reachable Mac is still installed and verified the normal
way. Not verified: installation/identity on the three Macs, ADR 0091's live
send acceptance, physical iPhone installation, and the server prune.
