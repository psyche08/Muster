# ADR 0138: Publish Apple 1.1.22 (112) and install it on the three Macs

- Date: 2026-11-05
- Status: Accepted; published and installed on three Macs
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

### Release gate — 2026-11-05

The first build, from the bump's merge commit `d39ccabb`, built and signed. But
`verify-release.sh` refused it: the daemon lacked the shipped-fix fingerprint
`Question Submit changed; no key sent`, which stands for the 1.0.80 question
submit guard.

- **The guard is still there.** ADR 0134 left the guard's logic unchanged:
  the command button is re-read and must be the same element before a key is
  sent. What changed is the message. It now names the command,
  `format!("Question {name} changed; no key sent")`, so the old literal is no
  longer in the binary.
- **The fingerprint had to follow.** The fingerprint file's rule is to keep a
  line when its message is reworded, and ADR 0134 did not update the line.
- **Fix.** The line now holds the fixed tail, `changed; no key sent`, which the
  daemon carries and nothing else in the source produces.
- **Check.** Every line of `release-fingerprints.txt` was checked against
  `strings -a` of that first daemon, and all of them passed.

The release is built again from the merge commit that carries this fix.

### Source, build and signing — 2026-11-05

- **Source.** The only built source was the merge commit of PR 143,
  `b272743ae4e6ec5d9284b341624a46017ef36bb6`, from a clean, detached checkout.
- **Build.** All of these succeeded:
  - the Cargo release build: daemon, CLI, relay, browser host and panel
    helper;
  - `l4forwarder`;
  - the Apple FFI archive;
  - the Xcode Release app.
- **Gate.** The daemon carried exactly one `MUSTER-BUILD-COMMIT:b272743a…`
  stamp. `verify-release.sh` verified the release ancestry and every shipped
  fingerprint, and presence signing verified for
  `89LGY6BD53.dev.linsheng.muster`.
- **Notarization and signing.**
  - Notarization `b3ea0b0a-8a8d-4de4-a7b9-68747ce74448` returned `Accepted`.
  - Stapling, strict `codesign --verify --deep` and `spctl` (`accepted,
    source=Notarized Developer ID`) passed on the packed bundle, and
    `verify-release.sh` passed on it again.
  - The bundle's `sq.version` reports 1.1.22.
- **Hashes (SHA-256).**
  - Desktop portable ZIP:
    `fabdc626e7b37c2803b87e1e66a5c3a170e48787568c10e18955ba2ae8ba8cfe`.
  - Packaged daemon:
    `90b1cc2e6e6cc55358e9add87c8b30283f196cdc79acbf8f1b3fa0c254bc002b`.
- **iOS.**
  - The paired iOS archive built as 1.1.22 (112), team `89LGY6BD53`.
  - It is development-signed for three registered devices: the profile
    expires 2027-09-30 and carries `get-task-allow`.
  - OTA IPA SHA-256:
    `b2f21cf982595a8960b31349bdf5af9aa34b3f290b3100ba75fe377513008a30`.

### Signed feed and publication — 2026-11-05

- **Full package.** Velopack produced the portable and the full package
  `Muster-1.1.22-osx-arm64-full.nupkg` (SHA-256
  `4fec837d76f3e27c352b5a22d5bfc47121de4f635d89dfb28de322ea6d9e914d`, 44,578,040
  bytes). Its app has the notarized portable's CDHash.
- **Feed.** `sign-feed.py` signed `releases.osx-arm64.json` (SHA-256
  `b1af5cfd114fe5b0b67ec139dc3f4cdd9c4f7446604bf49134f1d1354509669e`) with the
  ADR 0088 key and reported `Verified OK`. The feed lists that one full
  package.
- **Publisher.** The publisher accepted 14 files and verified their hashes:
  `{"published_files":14,"mac_version":"1.1.22","ios_version":"1.1.22","build":"112","feed":"releases.osx-arm64.json","hashes_verified":true}`.
  The package went first, then the aliases, the feed and signature, and the
  pages.
- **Read back over HTTPS.**
  - The root, `ios/` and `desktop/` pages equal the generated page.
  - The Mac portable and the IPA match their SHA-256 values.
  - The manifest names build 112 and the hash-named IPA, and the 1.1.11
    manifest resolves to the current one.
  - The published feed signature verifies against the public key compiled
    into the app, and the full package matches the feed's SHA-256 and size.
  - Every response carried `no-store, no-cache, must-revalidate, max-age=0`.
- **Result.** The OTA page offers macOS 1.1.22 and the paired iOS build 112.

### Installation — 2026-11-05 21:49 to 21:58 CST

- **Order and method.** The three Macs were installed one at a time: Mac-C,
  then Mac-B, then Mac-A. Each used the same verified portable and the
  guarded installer, run detached on the Mac itself. That installer:
  - checks the package hash and the identity;
  - takes the Rust update lease;
  - keeps the previous bundle;
  - replaces the bundle, opens the app once, and waits for the daemon.
- **Two changes to the local release scripts since 1.1.11.** Both are outside
  the repository:
  - The build script also builds the panel helper and `l4forwarder`.
  - The installer leaves the Multica scheduler alone when the container is
    stopped. Mac-B's container was stopped with keep-alive off, and the
    1.1.11 installer would have refused to run.
- **Copying the files.** `scp` to Mac-C failed with "Connection closed", so
  the files were copied over a plain `ssh` stream and their hashes checked on
  arrival.
- **Each Mac now reports:**
  - desktop **1.1.22**, commit `b272743a`, `sq.version` 1.1.22;
  - the packaged daemon SHA-256 above;
  - an unchanged device ID, fingerprint and Secure Enclave backend;
  - Accessibility granted, relay registered and Vnet up.

  Mac-B was locked during its installation.
- **Multica.** Mac-A's Multica scheduler was idle with no active tasks. The
  installer stopped it for the install, and it recovered. Mac-B's container
  stayed stopped as it was. Mac-C has no Multica container.
- **Screen Recording.** It is granted on Mac-B and Mac-C. On Mac-A it is
  `missing`, as it already was before the installation.
- **Network.**
  - Each Mac lists the other two over the relay, not direct, at this check.
  - Virtual-IP pings in all six directions received 53 of 56 packets.
- **Not verified:**
  - an unattended feed update (the next release is the first chance);
  - physical iPhone installation;
  - the server prune;
  - a GitHub Release, TestFlight upload or App Store submission.

### ADRs 0134 to 0137 on the installed build

- **ADR 0137.** On all three Macs, `~/Library/Logs/Muster/app.log` begins with
  `app log opened, pid N` for the app that the installer opened through
  Launch Services.
- **ADR 0136.**
  - The new daemons' switch lines carry both RTTs, for example
    `relay carrier None -> Some(Quic) (rtt quic=5ms tls=-)`.
  - One line on Mac-C, `Some(Quic) -> Some(Tls) (rtt quic=116ms tls=211ms)`,
    leaves the faster carrier, so it reads as a failover rather than an RTT
    decision.
  - Whether quiet switches drop needs days of these logs.
- **ADRs 0134 and 0135.** Installed. Their live checks are still pending: a
  two-question Claude card, and the banner across a daemon restart.
