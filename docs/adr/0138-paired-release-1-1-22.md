# ADR 0138: Publish Apple 1.1.22 (112) and install it on the three Macs

- Date: 2026-11-05
- Status: Accepted; release pending
- Related: [ADR 0092](0092-paired-release-1-1-11.md) (the last publication),
  [ADR 0088](0088-unattended-desktop-updates.md) (signed feed),
  [ADR 0121](0121-generated-xcode-project-follows-project-yml.md) (version
  source), [ADR 0134](0134-activate-claude-next-and-back-by-keyboard-focus.md),
  [ADR 0135](0135-give-session-watches-the-forwarding-budget.md),
  [ADR 0136](0136-settle-relay-carrier-switches.md),
  [ADR 0137](0137-keep-the-desktop-apps-log-in-a-file.md),
  [DESKTOP_UPDATES](../DESKTOP_UPDATES.md)

## Context

The OTA page and the signed feed still offer 1.1.11 (101), published on
2026-11-02. Since then 58 pull requests have merged, ADRs 0093 to 0137. The
latest are the watch budget and the question-card focus fix (ADRs 0134 and
0135), relay carrier settling (ADR 0136) and the app's log file (ADR 0137).

The three Macs do not run a published build. Mac-A, Mac-B and Mac-C were each
last installed with `install-mac.sh`, a plain Xcode build: bundle 1.1.21 (111),
from `a8c69ca` on Mac-B and Mac-C and from a later main commit on Mac-A. A plain
bundle has no Velopack metadata, so:

- its daemon reports the crate version `0.1.0`, not the bundle's;
- the signed feed cannot update it, so ADR 0088's unattended path does not
  reach these Macs.

## Decision

Publish **Apple 1.1.22 (112)** from the merge commit that carries this version
bump, following the ADR 0092 process.

- **Version.** `MARKETING_VERSION: 1.1.22` and `CURRENT_PROJECT_VERSION: 112`
  in `project.yml`. The committed Xcode project is regenerated with xcodegen
  2.46.0. 1.1.22 is above both the published 1.1.11 and the installed 1.1.21,
  so the feed offers it to every Velopack installation and nothing is a
  downgrade.
- **Source.** The release worktree is a clean, detached checkout of the merge
  commit `C`. The daemon's build stamp must equal `C`.
- **macOS build.** Build these in release mode:
  - the daemon, CLI, relay and browser host;
  - the panel helper (ADR 0128) and `l4forwarder` (ADR 0107), which the bundle
    now also carries;
  - the Apple FFI archive and the Developer ID app.

  Then run `verify-release.sh` and the presence-signing check.
- **Packaging.** Velopack 1.2.158 builds the portable and the full package.
  The portable is notarized and stapled, then must pass a strict signature
  check and Gatekeeper.
- **Feed.** `sign-feed.py` signs the feed with the ADR 0088 key.
- **iOS.** A paired iOS OTA build is made for the registered devices.
- **Publication.** Publish to the private publisher in this order: the package
  first, then the fixed aliases, the feed and its signature, then the unified
  page. Then read everything back over HTTPS.
- **Installation.** Install on the three Macs, one at a time, with the
  verified portable. Use the guarded installer:
  - it holds the Rust update lease;
  - it keeps a copy of the previous bundle;
  - it checks that the device identity is unchanged.

  On a remote Mac the installer runs detached on the Mac itself, so stopping
  its daemon cannot cut the session that started it. Once installed this way,
  each Mac is a Velopack installation and later releases can reach it through
  the signed feed.
- **Not in this release.** GitHub Release, TestFlight, App Store export,
  server prune, and any installation beyond the three Macs.

## Alternatives

- **Keep installing plain bundles with `install-mac.sh`.** This is fast for
  one fix on one Mac. But the Macs stay off the feed, report no version, and
  run builds nobody else can download.
- **Publish as 1.1.21.** That number is already on the Macs, with different
  bytes, from several commits. Velopack would not offer it as an update, and
  provenance would be unclear.

## Consequences

- The OTA page offers macOS 1.1.22 and the paired iOS build.
- The Macs report `1.1.22` and the release commit. ADR 0088's unattended
  update reaches them for later releases.
- The installed acceptance still open in ADRs 0134 to 0137 can then be checked
  on a published build.
- Superseded server artifacts stay until the owner authorizes a prune.

## Verification

Release evidence is added below after publication and installation, with code,
signing, publication and installation reported as separate states.
