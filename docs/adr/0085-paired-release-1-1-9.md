# ADR 0085: Publish Apple 1.1.9 (99) and install it on the three Macs

- Date: 2026-11-02
- Status: Accepted; release pending
- Related: [ADR 0084](0084-keep-awake-menu.md),
  [ADR 0083](0083-idle-compaction.md),
  [ADR 0081](0081-paired-release-1-1-8.md),
  [DESKTOP_UPDATES](../DESKTOP_UPDATES.md)

## Context

The owner asked to merge the keep-awake menu (ADR 0084) and publish it. The
published line is 1.1.8 (98), built from `8c79329d`. Since then `main` gained:

- ADR 0084: Keep Awake and Keep Screen On in the Mac app menu, which run
  `caffeinate -imsu` or `-dimsu` tied to Muster's process;
- ADR 0083: opt-in `/compact` for idle Claude Code conversations (off by
  default);
- documentation-only changes (progressive architecture documents, release and
  phone acceptance records).

Keep-awake was verified only on a local development build; it needs a
published, notarized build before the Macs run it.

## Decision

Publish **Apple 1.1.9 (99)** from the merge commit that carries this version
bump, following the ADR 0081 process unchanged:

- **Source.** This PR sets `MARKETING_VERSION: 1.1.9` and
  `CURRENT_PROJECT_VERSION: 99` and regenerates the committed Xcode project with
  xcodegen 2.46.0. The release worktree is a clean detached checkout of the
  merge commit `C`; `verify-release.sh` must prove the daemon's
  `MUSTER-BUILD-COMMIT` equals `C`.
- **macOS.** Release daemon, CLI, relay and browser host, Apple FFI archive and
  Developer ID app; Velopack 1.2.158 portable; notarize, staple, strict
  signature, Gatekeeper and presence-signing checks. No signed update feed (no
  feed key on the publisher Mac).
- **iOS.** The paired development-signed OTA build 99 for the registered
  devices; no TestFlight upload or App Store export.
- **OTA pages.** The portable under `desktop/`, the hash-named and fixed-alias
  iOS files under `ios/`, and the unified `package-ota.py` page at the root,
  with `no-store` cache headers; read back over HTTPS.
- **Installation.** The same notarized portable on M4Mini, M1Max and M4Pro,
  one at a time, preserving device identity and Secure Enclave key, keeping a
  previous-bundle copy, never over the virtual-network address.
- **Not in this release.** GitHub Release, TestFlight, App Store, physical
  iPhone installation, a signed feed and the server prune (needs explicit owner
  authorization).

## Alternatives

- **Install the local keep-awake development build.** It is not notarized and
  its daemon stamp would not match a published commit.
- **Reuse 1.1.8 (98).** A published version cannot carry different source.

## Consequences

- The Macs keep themselves awake while Muster runs, by default, until the
  owner turns Keep Awake off; Keep Screen On is opt-in.
- Idle compaction ships but stays off until enabled per Mac.
- In-app update feeds stay unavailable; superseded releases stay on the server
  until the owner authorizes a prune.

## Verification

Recorded after publication and installation.
