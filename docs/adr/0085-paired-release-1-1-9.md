# ADR 0085: Publish Apple 1.1.9 (99) and install it on the three Macs

- Date: 2026-11-02
- Status: Accepted; published and installed on three Macs
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

Implementation and device acceptance are recorded separately. No real
conversation was used as a release test.

### Source, build and signing — 2026-11-02

- The only built source was the merge commit of PR 86,
  `c8e6e1177c0c8abf869ffa4162ca2fac3f6fe109`, in a clean detached release
  worktree. `cargo build --release` for the daemon, CLI, relay and browser
  host, the Apple FFI archive and the Xcode Release app all succeeded.
- `verify-release.sh` reported release ancestry and shipped fingerprints
  verified with the daemon built from `c8e6e117…`, before and after packaging;
  presence signing verified for `89LGY6BD53.dev.linsheng.muster`.
- Notarization `3e602091-068a-4973-b2e7-05f817f66984` returned `Accepted`; the
  packed bundle was stapled and passed strict `codesign --verify --deep`,
  `xcrun stapler validate`, and `spctl` (`accepted, source=Notarized
  Developer ID`).
- Desktop portable ZIP SHA-256:
  `5db980eab2f9497a4396c04812edb7fa8b173eea8736e09a809e6532358610df`.
  Packaged daemon SHA-256:
  `b884216d9d51475e72d0402a14f064a5109b96bd5d441c8fa5b96eeee0f48d84`.
- The paired iOS archive built as 1.1.9 (99), team `89LGY6BD53`,
  development-signed for three registered devices with a profile that expires
  2027-09-30. No App Store export and no TestFlight upload. OTA IPA SHA-256:
  `68a220f69e0937ffcd58840edeba36b4d4e82a1d520b669aa2901390f0c44727`.

### Publication and public read-back — 2026-11-02

- The publisher accepted all 11 staged files and verified their hashes
  (`{"published_files":11,"mac_version":"1.1.9","ios_version":"1.1.9","build":"99","hashes_verified":true}`).
- Read back over HTTPS: the root, `ios/` and `desktop/` pages equal the
  generated page; the Mac portable and the IPA match the SHA-256 values above;
  the manifest names build 99 and the hash-named IPA, whose download matches;
  the 1.1.8 hash-named manifest now resolves build 99; pages, manifest,
  packages and `SHA256SUMS` carry
  `no-store, no-cache, must-revalidate, max-age=0`.
- No signed update feed was published and superseded releases were **not**
  pruned.

### Installation and runtime — three Macs, 2026-11-02

- The same notarized portable was installed one Mac at a time with the
  release installer (Rust update lease, previous-bundle copy kept, identity
  checked before and after). Routes:
  - **M4Mini:** local.
  - **M4Pro:** the physical Thunderbolt bridge address.
  - **M1Max:** its sshd listens only on loopback, so the only route from the
    other Macs is its own reverse tunnel, which rides on Vnet. The installer
    was copied there and started detached (`nohup`) on M1Max itself, so the
    Vnet restart could not interrupt it; the session was used only to start it
    and read its log.
- All three report desktop **1.1.9 (99)**, commit prefix `c8e6e117`, the
  packaged daemon SHA-256 above, and unchanged device identity, fingerprint and
  Secure Enclave backend. Accessibility is granted, the relay is registered and
  Vnet is up on each. Strict signature, Gatekeeper and stapling pass on each
  installed bundle.
- M4Pro's installer stopped at its scheduler check while the managed Multica
  daemon still reported `starting`; it had already restored `keep_alive` and
  the new app was running. Four direct checks over 60 seconds then reported
  `running`, with the same pid and no active tasks. As in 1.1.8 this is
  delayed recovery, not a failed install, and nothing was reinstalled.
- `muster status` on each Mac shows both peers on a `direct` link, and 24 of
  24 virtual-IP ping packets (six directions) were received.
- Keep-awake (ADR 0084) runs on each Mac: `/usr/bin/caffeinate -imsu -w
  <Muster pid>` with the default preference.
- Not verified: a provider send, the menu switches on the installed release,
  physical iPhone installation, GitHub Release, TestFlight, App Store
  submission and the server prune.
