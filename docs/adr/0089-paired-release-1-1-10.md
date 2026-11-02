# ADR 0089: Publish Apple 1.1.10 (100) and install it on the three Macs

- Date: 2026-11-02
- Status: Accepted; published, on GitHub and installed on three Macs
- Related: [ADR 0088](0088-unattended-desktop-updates.md),
  [ADR 0085](0085-paired-release-1-1-9.md),
  [ADR 0081](0081-paired-release-1-1-8.md),
  [DESKTOP_UPDATES](../DESKTOP_UPDATES.md)

## Context

The owner asked to merge unattended desktop updates (ADR 0088), publish them
and install them on the Macs. The published line is 1.1.9 (99), built from
`c8e6e117`. Since then `main` gained ADR 0088: the app checks the signed
Velopack feed on a schedule and installs a verified update when the Mac has
been idle, and it trusts a new feed signing key.

Builds before this one trust the old feed key, so they can never accept a feed
signed with the new key. This release is therefore the last one that has to be
installed by hand.

## Decision

Publish **Apple 1.1.10 (100)** from the merge commit that carries this version
bump, following the ADR 0081 and ADR 0085 process, with one addition:

- **Source.** This PR sets `MARKETING_VERSION: 1.1.10` and
  `CURRENT_PROJECT_VERSION: 100` and regenerates the committed Xcode project
  with xcodegen 2.46.0. The release worktree is a clean detached checkout of
  the merge commit `C`; `verify-release.sh` must prove the daemon's
  `MUSTER-BUILD-COMMIT` equals `C`.
- **macOS.** Release daemon, CLI, relay and browser host, Apple FFI archive and
  Developer ID app; Velopack 1.2.158 portable and full package; notarize,
  staple, strict signature, Gatekeeper and presence-signing checks.
- **Signed feed (new).** `releases.osx-arm64.json` is signed with the ADR 0088
  key by `sign-feed.py`, which verifies the signature against the public key
  committed in the app before it writes the `.sig`. The full package must hold
  an app with the same code directory hash as the notarized portable. The
  publisher receives the package first, then the feed and its signature, so a
  client never sees a feed that names a missing package. The private key never
  leaves the release Mac.
- **iOS.** The paired development-signed OTA build 100 for the registered
  devices; no TestFlight upload or App Store export.
- **OTA pages.** As in ADR 0085, read back over HTTPS, including the feed
  signature against the committed public key.
- **Installation.** The same notarized portable on M4Mini, M1Max and M4Pro,
  one at a time, preserving device identity and Secure Enclave key, keeping a
  previous-bundle copy, never over the virtual-network address.
- **GitHub Release.** After publication the owner also asked for a GitHub
  Release: `v1.1.10` at `C`, carrying the same notarized portable ZIP and its
  `SHA256SUMS`, marked Latest.
- **Not in this release.** TestFlight, App Store, physical iPhone installation
  and the server prune (needs explicit owner authorization).

## Alternatives

- **Let 1.1.9 update itself.** It trusts only the old key, which is not
  available, so it would reject the new feed.
- **Publish the feed without installing 1.1.10 by hand.** The Macs would stay
  on 1.1.9 and never see it.

## Consequences

- From 1.1.10 on, the Macs pick up later releases by themselves when idle,
  unless the owner turns Automatic Updates off.
- An unattended update is first exercised by the release after this one.
- Superseded releases stay on the server until the owner authorizes a prune.

## Verification

No real conversation was used as a release test.

### Source, build and signing — 2026-11-02

- The only built source was the merge commit of PR 91,
  `18d542f46780e6fcfd1021aec2d21853cb455152`, in a clean detached release
  worktree. `cargo build --release` for the daemon, CLI, relay and browser
  host, the Apple FFI archive and the Xcode Release app all succeeded.
- `verify-release.sh` reported release ancestry and shipped fingerprints
  verified with the daemon built from `18d542f4…`, before and after packaging;
  presence signing verified for `89LGY6BD53.dev.linsheng.muster`.
- Notarization `612ca3fd-a2a0-415a-9ee0-0d57e291c639` returned `Accepted`; the
  packed bundle was stapled and passed strict `codesign --verify --deep`,
  `xcrun stapler validate`, and `spctl` (`accepted, source=Notarized
  Developer ID`).
- Desktop portable ZIP SHA-256:
  `d40a50f07846ff3da188825243cfc93c3bf2fd0ea073cb9a98711d86d9627942`.
  Packaged daemon SHA-256:
  `b7577cbfe188ff351158504bfb1b6690c6603af1b17752105ccf3aabee615fec`.
- The paired iOS archive built as 1.1.10 (100), team `89LGY6BD53`,
  development-signed for three registered devices with a profile that expires
  2027-09-30. OTA IPA SHA-256:
  `f43cc52f1cc47e08f47eb29feb37668d84817b350c9609a380ce29f7e4f244de`.

### Signed feed — 2026-11-02

- `Muster-1.1.10-osx-arm64-full.nupkg` comes from the same `vpk pack` run as
  the portable: SHA-256
  `f904e0447efe0e6944deb2f573942befd6dac24d431fd598a9755149927ca5d7`,
  40,571,870 bytes, matching the feed entry.
- The app inside the package, with its symlinks restored, has the notarized
  portable's code directory hash (`b62db6d1ae4d4924b399a08d0f2c7e2ee77250bf`),
  passes strict `codesign --verify --deep`, and `spctl` accepts it as
  `Notarized Developer ID` without a stapled ticket.
- `sign-feed.py` signed `releases.osx-arm64.json` (one full 1.1.10 package)
  with the ADR 0088 key and reported `Verified OK` against the committed public
  key. The private key stayed on the release Mac.

### Publication and public read-back — 2026-11-02

- The publisher accepted all 14 staged files and verified their hashes and the
  feed's package entry
  (`{"published_files":14,"mac_version":"1.1.10","ios_version":"1.1.10","build":"100","feed":"releases.osx-arm64.json","hashes_verified":true}`).
  The package was published before the feed and its signature.
- Read back over HTTPS: the root, `ios/` and `desktop/` pages equal the
  generated page; the Mac portable and the IPA match the values above; the
  manifest names build 100 and the hash-named IPA, whose download matches; the
  1.1.9 hash-named manifest now resolves build 100. The public feed equals the
  staged feed, its public signature verifies with `openssl dgst -sha256
  -verify` against the committed public key over the file name, a newline and
  the feed bytes, and the public package matches the feed's SHA-256 and size.
  Every response carried `no-store, no-cache, must-revalidate, max-age=0`.
- Superseded releases were **not** pruned.
- GitHub Release `v1.1.10` (Latest) points at `18d542f4…`; its downloaded ZIP
  passes the attached `SHA256SUMS`.

### Installation and runtime — three Macs, 2026-11-02

- The same notarized portable was installed one Mac at a time with the
  release installer (Rust update lease, previous-bundle copy kept, identity
  checked before and after): M4Mini locally, M4Pro over the physical
  Thunderbolt bridge, and M1Max with a detached installer started over its
  Vnet-carried reverse tunnel. The M1Max copy, start and log polling were done
  by a reusable ssh/scp script, which kept polling through the Vnet restart.
- All three report desktop **1.1.10 (100)**, commit prefix `18d542f4`, the
  packaged daemon SHA-256 above, and unchanged device identity, fingerprint and
  Secure Enclave backend. Accessibility is granted, the relay is registered and
  Vnet is up on each. Strict signature, Gatekeeper and stapling pass on each
  installed bundle.
- M4Pro's managed Multica scheduler reported `running` within the installer's
  check. The installer now waits between scheduler polls; before, a `starting`
  reply used up every retry at once, which is why 1.1.8 and 1.1.9 recorded a
  delayed recovery.
- `muster status` on each Mac shows both peers on a `direct` link, and 24 of
  24 virtual-IP ping packets (six directions) were received.
- `/usr/bin/caffeinate -imsu -w <Muster pid>` runs on each Mac.
- Not verified: the result of the app's own first scheduled feed check (it is
  shown only in Settings and not logged when current), an unattended update to
  a later release, a provider send, physical iPhone installation, TestFlight,
  App Store submission and the server prune.
