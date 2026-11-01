# Desktop updates and permission restart

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Muster's active macOS client uses Velopack 1.2.158. Shared Rust owns update state,
version/downgrade decisions, transport and validation. The same app ABI is
available to the new Windows shell; the retired ABI 1.86 Windows preview is not
extended. iOS uses its registered-device OTA page and separately published
TestFlight builds; publishing one does not update the other.

## Permissions

The core remembers observed missing permissions during this app lifetime. A
missing-to-granted Accessibility, Locked Use installation or iTerm2 Automation
transition keeps the setup sheet open with **Restart Muster**. Unknown/checking
observations do not create transitions. Existing grants at launch do not cause a
restart loop. Restart exits Muster, sends SIGTERM to its own launchd daemon and
reopens the app. It never restarts Codex, Claude or iTerm2. Pairing/store remain
in Application Support. Busy sends, terminal attachments, authentication and
Locked Use prevent a restart from the UI.

Uninstalled iTerm2 is detected through Launch Services and existing bundle paths.
It does not show an authorization row or trigger the setup sheet. An installed
but stopped iTerm2 remains a distinct observation.

## Upgrade protocol

- Settings → Muster Updates: check, download, install and restart explicitly.
- Fixed source: `https://linsheng.dev:8443/muster/desktop/`.
- Separate `osx-arm64`, `osx-x64`, `win-arm64`, `win-x64` channels.
- `releases.CHANNEL.json` and `.json.sig`: P-256/SHA-256 signature over
  `UTF8(filename) + LF + exact JSON bytes`. Public key is compiled in
  `muster-app-core/src/desktop_update/release-public.pem`. The private release
  key stays on the publisher; it must never be uploaded with artifacts.
- DoH only, verified HTTPS, no system DNS fallback/proxy/redirect, 3-second
  connect deadline, bounded feed/signature/package size. Full packages only.
- Require package ID Muster, a plain filename, SHA256 and exact byte size.
  Recheck cached downloads and check again immediately before application.
  Downgrades and automatic application of cached packages on startup are off.
- Rust retains opaque candidate/prepared objects. UI receives display fields;
  it cannot supply a package URL, hash or path to the apply operation.
- Before stopping the service, Shared Rust acquires a local-only streaming
  update lease from musterd. Its exclusive console fence waits up to 18 seconds
  for in-flight operations and protection cleanup, blocks new console writes,
  and checks persistent orphan protection. Failure leaves the daemon running.
  The opaque lease remains owned by Rust through the platform stop callback;
  cancelling/abandoning the update releases it.
- macOS stops its user launchd service before bundle replacement; on failure
  it restores the service. Velopack waits for the UI to exit, replaces the
  bundle and relaunches. Startup re-registers musterd when necessary.

## Multica container: install, version and manual upgrade

Multica is a separate product from Muster and has its own update cycle. Muster
manages a local Apple container that runs the Multica daemon; it does **not**
upgrade or stop Multica through an in-app button. This section is the operator
runbook for the desktop side. It applies to the current managed bridge (ADR
0051/0054/0064/0077), not to the historical ACP shim of ADR 0009.

### Prerequisites

- Apple `container` installed on the Mac (from Apple's public releases; Muster
  does not bundle it), and enough disk for the guest image and the data root.
- A trusted HTTPS installer URL for the Multica CLI, and the server and
  application HTTPS URLs, obtained from the deployment operator. Muster writes
  none of these into the repository or its own binary; treat the installer as
  executable code and verify its source before use.
- A terminal for `multica login`. Login is an explicit terminal operation and
  is never performed or proxied by Muster.

### Install and status

Settings → Multica supplies the data root (default `~/Documents/Multica`), the
container name and the URLs, then starts the guided install. The daemon reports
stages and bounded, redacted installer output (stdout and stderr, last 64 KiB).
An existing container is adopted without changing it; its tools, mounts and
login are preserved. The Multica tab appears after Agents and Terminals only
when Rust observes an installed container.

The status shown by Muster is read-only and comes from the daemon:

- `installed` — the guest Multica CLI is present;
- `running` — the Multica daemon is running inside the container;
- `version` — the guest `multica --version` report;
- `migration_required` — the installed container's bind mounts no longer match
  the saved data root; saving a changed root is refused until the attended
  offline migration below is done;
- a message and the terminal login command.

Stopped container, installed-but-stopped daemon and missing login are distinct
states; do not infer success from a running container alone.

### keep-alive

Keep-alive opts the container and the Multica daemon into `musterd`'s
supervisor. When it is on, `musterd` starts the Apple container runtime and the
container if they are stopped, reconciles the bridge listener, token and
provider wrappers, and starts the daemon. Closing the app does not stop them.
When keep-alive is off, Muster does not start stopped services; it still
reconciles a running container's bridge and wrappers so the next task does not
use a stale bridge. Turning keep-alive off is not a stop operation.

### Manual Multica upgrade

There is no in-app Multica upgrade, image pull or rollback in this release.

1. Record the current version shown in Settings and let running tasks finish;
   do not upgrade while a task is executing.
2. Turn keep-alive off and stop the container with Apple
   `container stop <name>`. Do not delete or move the data root.
3. Install the desired Multica CLI version inside the container and on the host
   from the trusted installer (host copy under the data root's `bin/`, with
   `MULTICA_BIN_DIR` set there). Keep the existing mounts, login configuration
   and workspace paths unchanged.
4. Start the container and daemon again; re-enable keep-alive to have Muster
   supervise them, then save the Multica settings once so bridge wrappers are
   reconciled.
5. Verify the version, that workspaces and tasks load, and that a task runs on
   the intended runtime. If login was invalidated, repeat `multica login` in
   the terminal as shown by Settings.

Muster does not verify an upgrade or provide a downgrade path; keep a backup of
the data root before changing versions. Device and end-to-end acceptance of
this runbook is pending; the recorded checks in ADR 0052/0054/0060/0064/0072
are historical, not a repeatable test.

### Data-root migration

Changing the data root is attended and offline, never automatic. Disable
keep-alive and stop the container, move the data on the same filesystem, update
every bind mount (including the guest workspace path and the daemon workspace
configuration), remove the old path, then save the new root. Muster accepts it
only when the old path is absent, the new root is a real directory (not a
symlink) and the stopped container inventory confirms the required mounts. It
never creates a compatibility symlink, moves data itself or rewrites Apple's
private container metadata. Cross-volume moves and version-specific container
metadata remain manual.

## App Store Connect credentials and TestFlight

On the release Mac, the existing credentials are configured in
`${MUSTER_APPLE_CREDENTIALS_FILE}`: `APPLE_NOTARY_KEY_ID`,
`APPLE_NOTARY_ISSUER_ID`, and `APPLE_NOTARY_PRIVATE_KEY` (a path to the `.p8`
file). Despite the variable names, these credentials also authorize the
Muster Agents App Store Connect record (`6817180898`, bundle identifier
`dev.linsheng.muster`). Read them locally without printing their values;
never commit the environment file or private key.

The development-signed OTA IPA is not a TestFlight upload. Export a separate
`app-store-connect` IPA with Xcode, team `89LGY6BD53`, automatic signing and
`manageAppVersionAndBuildNumber=false`. Pass the existing key through
`-authenticationKeyPath`, `-authenticationKeyID`, and
`-authenticationKeyIssuerID`, with `-allowProvisioningUpdates`. Verify the
exported bundle version, strict signature, production push entitlement and
disabled debugging before uploading it with `xcrun altool --upload-package`.
The upload tool accepts the private key's path with `--p8-file-path`.

An upload acknowledgment alone is not release acceptance. Read back the build
and its prerelease version through App Store Connect; require processing
`VALID` and internal state `IN_BETA_TESTING`, and confirm access through the
existing Internal beta group. Do not claim physical iPhone installation from
these server-side states. Xcode Cloud onboarding is a separate operation and
is not implied by a successful API-authenticated upload.

## Packaging

Use Velopack CLI **1.2.158**. A macOS bundle must already have all nested helpers
signed with their proper entitlements. `package-macos.sh` preserves nested
signatures with `--signDisableDeep`; provide `SIGN_IDENTITY`, `VPK` and
`MUSTER_RELEASE_KEY`. Set `MUSTER_NOTARY_PROFILE` for notarized publication.
No script unlocks a Keychain. Interactive Keychain unlock, if required, belongs
to the user.

`package-macos.sh` first runs `verify-release.sh`, which refuses the bundle
unless its daemon:

- carries exactly one `MUSTER-BUILD-COMMIT:<sha>` stamp (written by the musterd
  build script and printed in the daemon's startup log) that is neither
  `-dirty` nor `unknown`, equals the packaging tree's `HEAD`, and that tree
  has no uncommitted tracked changes;
- was built from a source that contains every commit in
  `deploy/desktop/released-commits.txt`;
- contains every string in `deploy/desktop/release-fingerprints.txt`, each of
  which stands for a fix an earlier release shipped.

After publishing, append the new version and its stamped commit to
`released-commits.txt`. A version number alone proves nothing about content:
1.0.20 was built from a line that never received 1.0.16–1.0.18.

`package-windows.ps1` requires the **new** `Muster.exe` plus `muster_ffi.dll`,
a Windows signing template and feed key. The new Windows shell must call
`muster_desktop_update_startup()` before creating windows/runtime, render
`DesktopUpdateView`, send `DesktopUpdateAction`, stop its own daemon on
`PrepareDesktopUpdate`, and answer the effect. On `FinishDesktopUpdate`, exit
cleanly when `exit=true`, otherwise restore the daemon. These are OS lifecycle
callbacks; verification and update decisions stay in Rust.

Publish package files first, then signed feed and signature. A transient
feed/signature mismatch fails closed and is retryable. Keep the private key
outside the repository and site. Existing plain `.app` installations need a
one-time signed Velopack portable-bundle installation to bootstrap metadata
and the updater. Building or signing alone does not prove a live update.

For attended portable-bundle installation, keep the Rust desktop-update lease
open through the daemon stop and replace only the verified app bundle. Start
with normal Launch Services. A background `open -g` can leave the process alive
without materializing the SwiftUI window task that registers musterd (observed
on Mac-C with 1.0.74). If the expected daemon/socket is still absent after a
bounded initial wait, issue **one** normal `open /Applications/Muster.app`, then
wait for the expected daemon version, unchanged identity, relay and Vnet. Do not
infer startup from `open` returning zero, loop GUI relaunches, force launchd, or
repeat installation when only post-install verification is incomplete. A
protected-session refusal still stops installation before bundle replacement.

Two conditions look alike here and are not the same, which stalled the 1.0.94
installation on Mac-A. Launch Services needs a moment to pick up a replaced
bundle and returns -609 until it has; that is why
`MusterApp/scripts/install-mac.sh` retries `open` at the same point. The "one
`open`, no relaunch loop" rule is about a *launched* app that fails to
materialize its window task and register its daemon. So wait long enough before
the single launch — ten seconds was enough on all three Macs, three was not —
and do not relaunch afterwards. A script that treats the two as one condition
either launches too early and stops, or relaunches when it must not.

Do not drive a fleet installation over the Vnet address. Stopping musterd tears
down the virtual network, so an SSH session addressed to a member's `192.168.13.x`
address is cut at the daemon stop, mid-script: during the 1.0.94 rollout Mac-B
was left with a replaced bundle, no running daemon and no route back, and had to
be reached over its physical bridge address. Address each member by an address
that does not depend on Muster. `launchctl asuser <uid> open` is not an
alternative from such a session; it needs root and fails with "Could not switch
to audit session".

`xcrun stapler validate` can also fail transiently on a bundle whose ticket it
must fetch from Apple, while `spctl -a -t install` on the same bytes reports
"accepted, source=Notarized Developer ID". Treat a single stapler failure as
inconclusive, re-check with `-v` and against `spctl`, and never weaken the gate
to get past it.

For a provider send acceptance through the bundled CLI, `muster send` and
`muster messages` need the **full** session id. `muster agents` prints ids
truncated to eight characters and a truncated id returns `NotFound`, which reads
like a missing session rather than a malformed argument; the full ids come from
the `agent_list` RPC. Only sessions whose `native_app` column is set can be
addressed, a session listed with `none` returns the same `NotFound`, and the
listing can show a session that no longer exists. The CLI addresses only its own
daemon, so a cross-device send is not reachable this way.

Release destination (confirmed 2026-09-30): connect as `${MUSTER_RELEASE_HOST}`
over SSH port `${MUSTER_RELEASE_SSH_PORT}` and publish to `${MUSTER_RELEASE_ROOT}/desktop/`, served at
the fixed HTTPS source above. Subsequent Mac releases use this distribution
path. The publisher keeps only the latest installable release. Verify public
downloads and any current feed signatures after publishing. The separate iOS OTA directory is
`${MUSTER_RELEASE_ROOT}/ios/`; it must not be overwritten by desktop packaging.
The unified download page is `https://linsheng.dev:8443/muster/`. Its source is
`package-ota.py`'s generated `index.html`, which includes both the Apple-silicon
Mac download and registered-device iOS installation. The page shows the latest
versions and links to the fixed `manifest.plist` and `Muster.ipa` aliases.
It does not send visitors to an independently older TestFlight release.
On the server, `${MUSTER_RELEASE_ROOT}/index.html` links to `ios/index.html`, and
`desktop/index.html` links to `../ios/index.html`. All entry pages and artifacts
use `Cache-Control: no-store, no-cache, must-revalidate, max-age=0`; install
`MusterApp/deploy/nginx-ios.conf` and reload nginx when changing this policy.

The iOS manifest still binds its asset to the exact hash-named IPA. Upload the
new IPA and manifest first, then replace the fixed aliases and page. Publish
all six packager outputs. Historical hash-named manifest URLs redirect to the
latest fixed manifest, including links from a previously opened page. Old IPA
and desktop package URLs return 410 after removal. Never serve different bytes
under an existing hash-named package URL.

After verifying the new public aliases, run:

```sh
python3 prune-releases.py ${MUSTER_RELEASE_ROOT} --apply
```

The script in `MusterApp/scripts/` checks current IPA hashes/build and desktop
version before removing superseded packages/manifests. It also removes an old
macOS feed and its metadata if no feed for the latest portable version was
published. Missing signing credentials must not leave an older version offered
as the current update. In that case in-app updates are unavailable; the signed
latest portable download remains available. Do not retain publisher backups of
old releases. These latest-only rules supersede retention in historical records.
Retired version-page bookmarks redirect to the latest download page, and their
`install.plist` links redirect to the current iOS manifest.
Public acceptance follows the actual page install link through the manifest to
the IPA, checks both package versions/hashes and cache headers, and verifies that
old manifests resolve the latest build and old packages cannot be downloaded.

Each macOS publication also publishes a corresponding iOS build, unless the
user explicitly requests otherwise. Report the two platforms separately.

## Acceptance boundaries

The macOS UI and shared updater are implemented. Windows production shell
migration (architecture P4), Windows signing and a real Windows upgrade need
separate verification; the old preview is not a usable self-updating client.
Cross-platform SDK support is not a Windows end-to-end acceptance result.

### 2026-09-29 Mac verification

- 18 core unit tests passed (live download test is opt-in); existing core/ABI
  integration tests and four Swift rendering tests passed. Clippy passed.
- Developer ID build and nested strict signature verification passed. This
  release is **not notarized**; no notarization profile was configured.
- An isolated signed 1.0.9 copy was replaced by the actual Velopack 1.2.158
  updater with 1.0.10. The resulting version and strict signature passed.
  The test used `--norestart`; it is not a live UI install/relaunch acceptance.
- Signed 1.0.10 bootstrap installed on Mac-A, Mac-C and Mac-B. Daemons returned
  relay registered after restart. Mac-C reports `not_installed` for iTerm2;
  Mac-A and Mac-B report its permission granted. Locked Use reports ready.
- Windows delivery is deferred at the user's request pending the new shell.

### 2026-09-30 conversation fixes (desktop 1.0.15 / iOS build 12)

- Codex asynchronous single-choice cards advance/send when the option is
  pressed. The AX adapter no longer waits for a radio selection or performs a
  second final submit. Custom answers recognize the scoped Send button. Only
  the exact provider receipt confirms success; disappearance alone does not.
- Rust owns per-conversation text drafts and receipt-based clearing. iOS edge
  gestures animate page movement and emit the existing Back intent.
- 106 Rust unit tests and 10 Swift rendering/ABI tests passed; Clippy passed.
  iOS UI compilation and both Release builds passed. Developer ID and iOS
  signatures verified; desktop notarization was not performed.
- Published to the existing `linsheng.dev:8443` iOS and desktop directories via
  SSH port `${MUSTER_RELEASE_SSH_PORT}`. Public downloads, SHA-256, and the desktop feed signature were
  verified. Prior releases were retained for rollback.
- Mac-C and Mac-B installed the signed 1.0.15 portable distribution downloaded
  from that domain. Both daemons report relay registered, vnet active,
  Accessibility granted and the console locked. Installed daemon SHA-256:
  `06228f4ca39de55d9cbe8eb4179651e96bfe5edda7bc64052c82520e3b80606b`.
- iOS installation and a new phone-authorized Locked Use answer, draft restore,
  and physical swipe gesture remain user-device end-to-end acceptance steps.

### 2026-10-01 queued messages (desktop 1.0.16 / iOS build 13)

- Codex queued follow-ups appear above the composer, with a right-aligned
  Send now button per message. Shared Rust reads the native local queue and
  app-server SQLite queue, owns ordering and action validation, and pushes
  changes independently of transcript writes. Swift renders that projection.
- Send now uses the existing session-bound, Presence-authorized Locked Use
  operation to press the exact native queue row once. A provider transcript
  receipt confirms execution; ambiguous rows and attachment-only messages
  cannot be submitted. Ordinary sends also recognize new queue admission as
  acceptance, without claiming the model has executed the message.
- 112 Rust unit tests, five service integration tests and 11 Swift tests
  passed; Clippy and both Release builds passed. The 390-point phone queue
  rendering was inspected. Developer ID and iOS signatures verified;
  desktop notarization was not performed.
- Desktop 1.0.16 and iOS build 13 were published through `linsheng.dev` via
  SSH port `${MUSTER_RELEASE_SSH_PORT}`. Public feed signature, package hashes and iOS manifest/build
  were verified. Prior releases and installation backups were retained.
- Mac-B and Mac-C installed the signed portable distribution downloaded
  from the release domain. Both report relay registered, vnet active,
  Accessibility granted and a locked console. Installed daemon SHA-256:
  `1392ad326cda30bbc68b0276123eee78f18fb2b5d6dc1017ef0c270ed8f2954c`.
- iOS build 13 installation and a real queued-message Send now operation
  with phone Presence remain end-to-end acceptance steps. Neither machine
  had a queued item during read-only inspection; no live message was sent
  as a test.

### 2026-10-01 working sends and owner scheduling (desktop 1.0.17)

- Codex composer submit discovery now recognizes Queue/Steer after text
  echo, excludes Stop and queued-message row actions, and requires a
  complete AX traversal and one composer-scoped submit control.
- The Locked Use owner has an independent executor and timer driver.
  Heartbeat, observation freshness and authorization deadlines are unchanged.
  Queue errors distinguish a closing transaction from a different requester;
  slow OS observations and relevant submit labels have dedicated diagnostics.
- 72 agent tests, 12 platform tests and 20 Locked Use owner tests passed,
  including RPC-executor starvation, queue rejection and working composer
  controls. Clippy, Rust Release and the arm64 Xcode Release build passed.
  Developer ID signing and the detached release feed signature passed;
  no notarization was performed. iOS build 13 remains the current client.
- Published 1.0.17 to the existing domain and verified public feed/package
  signature, size and hashes. Mac-B's direct download hit a DNS failure, so
  the same verified public portable ZIP was transferred over SSH. Mac-B
  installed 1.0.17 and reports relay registered, vnet active, Accessibility
  granted and a locked console. Its signed daemon SHA-256 is
  `31bac3fa2f28b37e501a2bcc953a4a79dcd195281f64bea0e8e2808f94224d0c`.
- Mac-C still has the old failed transaction's protection marker and
  watchdog. Installation is deferred until local logout/login recovers the
  console; neither its marker nor watchdog was forcibly removed. A fresh
  phone-authorized working send remains unverified on both machines.
- Subsequent three-device deployment: Mac-A upgraded from 1.0.16 to 1.0.17
  using the verified public portable distribution. Its localhost SSH forward
  became unavailable, so deployment used the existing authorized iTerm2
  service with bracketed-paste input and script/archive hash checks. Strict
  installed signature, version and daemon hash passed; relay registered,
  vnet active and Accessibility granted. Its unlocked console was preserved.
  Mac-B's existing 1.0.17 signature/hash and healthy runtime were reverified
  without a redundant restart. Mac-C's package is staged and verified, but
  its unchanged login audit session still has the failed protection marker;
  installation remains pending local logout/login. No new build or phone
  end-to-end send was performed during this deployment-only follow-up.

### 2026-10-01 protection cleanup and durable drafts (desktop 1.0.18 / iOS build 14)

- The independent watchdog reports a brief unlock through its private pipe,
  with monotonic observation time, console and input generation. Rust accepts
  it only for the consumed transaction. This evidence can settle cleanup after
  plugin Allow and a newer locked observation; it never grants execution or
  substitutes for a missing plugin terminal. Existing deadlines stay intact.
- Same-requester operations arriving while an owner closes wait up to two
  seconds before ordinary fresh admission. An already accepted operation is
  never replayed. Persistent quarantine still refuses new answers/sends.
- Draft edits and successful receipt-based clearing use synchronous atomic
  writes with file/directory synchronization. A delayed cache batch cannot
  overwrite a newer draft. Reopening restores each conversation's saved text;
  failed/unknown sends and edits newer than the submitted revision survive.
  Write failures retain the in-memory text and report a persistence error.
- Old installations with an existing protection marker still require local
  recovery before replacement. New watchdog evidence cannot reconstruct an
  unlock that an old watchdog never recorded. In particular, merely observing
  a locked screen does not authorize clearing an old consumed grant.
- Verification: 31 core Locked Use tests, 24 owner tests, 32 app-core tests,
  12 platform tests, one remote access integration test and 11 Swift tests
  passed. Clippy passed. Both Rust targets and both Xcode Release builds
  passed; the first iOS attempt lacked a sandbox-blocked generated Info.plist,
  resolved by regenerating the project with build permissions.
- Developer ID and iOS development signatures passed; the Mac release was
  not notarized. Both versions were published through the existing SSH `${MUSTER_RELEASE_SSH_PORT}`
  destination, with old releases backed up. Public HTTPS feed signature,
  package hashes, iOS manifest and IPA build were verified.
- Mac-B and Mac-A installed 1.0.18 with strict signature and version checks.
  Both report relay registered, vnet active, Accessibility granted and a
  locked console. The installed daemon SHA-256 on both is
  `6a219da227e2b748788af6211037b15c097a32ca93edb3c1498409fba523772e`.
  Mac-A used the existing authorized terminal service; neither host received
  a test Agent message or answer. Installation backups remain available.
- Mac-C remains on 1.0.16 with its old protection marker in audit session
  100014. The signed 1.0.18 package is staged locally, but installation is
  deferred until local logout/login safely recovers that transaction. No
  protection marker or watchdog was forcibly removed. iOS build 14 install,
  phone-authorized send/answer, draft restoration on the phone, and a live
  in-app self-update during protection remain end-to-end acceptance steps.

### 2026-10-01 evening: 1.0.20 dropped earlier fixes (repaired for 1.0.21)

- Desktop 1.0.20 was installed on all three Macs at 19:03–19:05 CST with
  daemon SHA-256
  `78c6bb8ededda8bfa6ccf51c6812193d6d504ecf665f179862715eda85b7f3d6`. Its
  strings match `main` (09498c46 or later) and none of the fourteen
  `codex/chats-catalog` commits behind 1.0.16–1.0.18: the Locked Use owner
  runtime, the blocking AX worker, AX diagnostics, the Mac-C lock UI reveal,
  update draining, Codex queued messages and the PR #6 draft boundaries. No
  release record for 1.0.19 or 1.0.20 exists in the repository.
- Between 19:06 and 20:30 CST the operation ledgers recorded three failed
  send attempts with no ERROR in any log: two "the app did not confirm the
  draft" rejections (Mac-C 19:36:21, Mac-B 20:20:07) and one send left
  pending after an owner heartbeat loss during sidebar selection (Mac-C
  20:19:17), the same failure 3ca6525e had fixed on 2026-09-30. Each message
  was delivered by one manual retry; none was delivered twice.
- The repair merges that line back into `main`, adds the release check above,
  and the Locked Use changes recorded in LOCKED_USE.md. It has not been
  built, signed, published or installed; device acceptance is still required.


### 2026-10-01 complete PR baseline (desktop 1.0.21 / iOS build 18 / HarmonyOS)

- Source: `feb74060862e8d7de1e9e3a998e6360a9e97ecc2`, the merge of PR #15.
  All 21 merged GitHub PR merge commits, including the stacked PR #6, are
  ancestors of this commit. No open PRs or newer main commits remained at
  publication. The primary checkout stayed on main; builds used an isolated
  release worktree with no tracked source changes.
- Both Apple Release builds and the HarmonyOS Rust/NAPI/ArkTS build passed.
  The packaged Mac passed the clean-source stamp, all prior release ancestry
  and fingerprint checks. Developer ID nested signatures, the P-256 desktop
  feed signature, iOS registered-device signature and HarmonyOS debug HAP
  signing passed. macOS was not notarized.
- Tests: 14 Swift tests and 48 HarmonyOS host checks passed. Core tests
  (55), platform/daemon tests and their integration checks passed, as did
  agent, app-core and FFI tests. Five network integration tests covering
  hostile relay, peer links/build exchange and mailbox passed; the privileged
  two-host TUN test remained ignored. The network unit suite was **not green**:
  `fatal_socket_error_rebuilds_same_node_and_rejects_retired_connections`
  failed with connection loss in both parallel and serial suites (30 other
  tests passed), then passed in isolation. This intermittent recovery test
  remains unresolved; live device health is not a substitute for it. The
  initial test runtime-path and Swift cache-alias build errors were resolved
  through build environment settings without source changes.
- Published desktop 1.0.21 and iOS 1.0.21 build 18 via SSH port `${MUSTER_RELEASE_SSH_PORT}` to the
  existing linsheng.dev directories. All ten public HTTPS files were read
  back and matched local SHA-256 values; the desktop feed signature and the
  IPA's embedded version/build were verified. No old-program backups were
  created. Prior separately versioned desktop packages were left in place.
- Mac-C, Mac-B and Mac-A installed the same verified public portable package.
  Strict installed signatures and daemon hashes matched on all three:
  `9cedf9e39e0761ffd38a7e23391c660f2e1ad9cb4374a4cb4bc42e5c81cbe591`.
  Each system Locked Use plugin matched the bundled plugin. All report
  version 1.0.21, relay registered, vnet active and Accessibility granted.
  Mac-A also queried both peers successfully through its newly installed
  daemon. Mac-C and Mac-B were locked at final verification; Mac-A was
  unlocked. Existing identities and network membership were retained.
- The same source built a signed HarmonyOS HAP and replaced the app on the
  connected HUAWEI Mate X7. `hdc install -r` and ability startup succeeded;
  a running process was observed, and the phone rejoined the existing network
  over a direct link. The unchanged HarmonyOS manifest still reports 0.1.0
  (version code 1); identify this delivery by its HAP hash:
  `b48cb8d2bc1e03a8db71cb1d5c85ab0847702bdcf81c05d4854eb64d14842137`.
  USB disconnected after installation, preventing a follow-up screenshot.
- Remaining device acceptance: iOS installation, phone-authorized send/answer,
  real queue control and physical fold/gesture interactions were not exercised.
  No test messages or input were sent to real sessions during deployment.


### 2026-10-02 morning: desktop 1.0.22 / iOS 1.0.22 build 19

- Source: `35c341566ac849c1c9f66a5ce2548e24b6702f22`, the merge of PR #23.
  This includes native controller observation on the long-lived service
  runtime, replacement of confirmed ended controllers, enrolled-phone
  device-key authorization for agent operations, background mobile display
  watch suspension, and relay outage retry backoff. The primary checkout
  remained on main; Release builds used a clean session worktree.
- Rust daemon, CLI, browser host and both Apple FFI Release builds passed,
  followed by macOS and iOS native Release builds. The prior fix turn's
  targeted core Locked Use (34), app-core (38 passed, one ignored) and secure
  relay (7) checks passed, as did affected-crate Clippy with warnings denied.
  This publication did not rerun the full network suite; the intermittent
  recovery test recorded for 1.0.21 remains unresolved.
- The packaged Mac passed the clean-source stamp, prior-release ancestry and
  fingerprint checks. Strict nested Developer ID signatures, the P-256
  desktop feed signature and the iOS registered-device signature passed.
  The iOS provisioning profile covers three devices and expires 2027-09-25.
  macOS was not notarized; the iOS publication is OTA, not TestFlight.
- Published both platforms through SSH port `${MUSTER_RELEASE_SSH_PORT}` to the existing desktop and
  iOS directories. All ten public HTTPS files were downloaded again and
  matched their local SHA-256 values. The public desktop feed signature,
  desktop 1.0.22 feed, iOS build 19 manifest and IPA version 1.0.22/build 19
  were verified. No old-program backups were created; prior separately
  versioned desktop packages remain in place.
- Artifact SHA-256 values:
  - Mac portable: `83f88fde75fa1e7e123d6ca1d944d2b10b0ebedd43574b545363b46f162e85b5`.
  - Mac full update: `49e23a326728cc12ca3ad040ca3064740af6f5e2e8cf83580b3fe895f7b46eab`.
  - Packaged daemon: `bbd9e0fb7f57baff1ac079f6bd1a0fee92b15325e8430c3be8700107bc777450`.
  - iOS IPA: `952c81ab2e307e9dd5960c576aa7fc6972e3b2bfdb3d07d464163b071e019b30`.
- This turn published the Apple updates only. It did not install or restart
  any device. Runtime health, physical send/answer without repeated local
  authentication and iOS thermal improvement remain device acceptance steps.


### 2026-10-02: iOS build 10 observation and OTA URL correction

- After publishing build 19, the connected iPhone's CoreDevice app inventory
  still reported `dev.linsheng.muster` version 1.0/build 10. The App's version
  label reads its own bundle metadata; this was an old installed bundle, not
  a hard-coded display value. The live OTA page, manifest and downloaded IPA
  reported build 19. There is no captured device installer error establishing
  why the prior OTA attempts left build 10 installed; caching is not confirmed
  as the root cause.
- An in-place CoreDevice installation of the already-signed build 19 succeeded
  without uninstalling the old App or resetting its data/pairing. A fresh
  inventory read confirmed version 1.0.22/build 19, and launching the bundle
  succeeded. This confirms the installed version and launch, not a Safari OTA
  installation or send/answer/thermal acceptance.
- The packager now gives both the IPA and install manifest content-addressed
  URLs instead of reusing fixed URLs in the install page. The fixed aliases
  remain available. This removes cross-release cache reuse on the new install
  route; an already-open old page must be reloaded.
- Repackaged the same signed build 19 and verified every ZIP entry against the
  previously published IPA. Published all six files; all public SHA-256 values
  matched, and the actual page -> manifest -> IPA chain resolved to 1.0.22/19.
  The repackaged IPA SHA-256 is
  `4aa138983c5ea96b21b5a5631cb48f7f399036b5b12e9f1b6668cff866b74ab8`.
  The application contents and signatures are unchanged; this is an iOS OTA
  packaging correction, not another macOS or iOS application build.


### 2026-10-02: 1.0.22 installed on Mac-B, Mac-C and Mac-A

- Installed the verified public portable package on Mac-B, then Mac-C, then
  Mac-A. Before stopping each App, the installer held the existing daemon's
  `DesktopUpdateLease` stream and required `DesktopUpdateReady`, so Rust
  drained console operations and blocked new writes through daemon shutdown.
  No pending protection marker was removed and no process was force-killed.
  Only Muster's App and user daemon were restarted. No old-program backup
  was created.
- All three installed Apps passed strict nested signature verification and
  report version 1.0.22. Their daemon SHA-256 matches the published bundle:
  `bbd9e0fb7f57baff1ac079f6bd1a0fee92b15325e8430c3be8700107bc777450`.
  The running daemon's version was checked separately from the bundle version.
- Each existing root helper refreshed the system Locked Use plugin. Its
  installed bytes match the signed App's plugin on all three machines:
  `842d5063f72962bce166afc81bd2e975c392b83f09f146f2ac9777cb6d7f56fc`.
  The plugin signatures passed. No Keychain unlock or password handling was
  used.
- Mac-A, Mac-B and Mac-C each retain their prior device identity, virtual
  address and locked console state. Relay registration, vnet and Accessibility
  are healthy on each device. After its own restart, Mac-A successfully read
  both peers' live device info through Muster.
- Mac-A's eight and Mac-B's five iTerm2 session IDs remain unchanged. Mac-C's
  terminal API was unavailable before deployment because iTerm2 was not
  running or its API was disabled; this was not treated as a deployment fault.
- The paired Apple release remains iOS 1.0.22/build 19, installed and launched
  in the preceding turn. This Mac deployment sent no test messages or answers;
  per-device phone send/answer behavior and thermal improvement remain real
  usage acceptance steps.


### 2026-10-02: 1.0.23 published with manual question confirmation

- Source `4d921f244c4877f51fd27d8d076a13b343f2bf3c` (PR #28) removes
  post-click selection/receipt waits for any question count and returns a
  message asking the user to confirm in the original conversation. It retains
  the exact session/question lookup and operation authorization boundaries.
- Published signed macOS 1.0.23 and iOS 1.0.23/build 20 through the documented
  server. All 12 public files matched their local SHA-256 values, the desktop
  feed signature verified, and the actual OTA page/manifest/IPA chain resolved
  to build 20. macOS portable SHA-256:
  `e075049c87bbe3cbe5668f573decafce55953896ee67c6080c1f30086587b3c2`;
  iOS IPA: `973cd3240fbb106a3d3548ee6516dc32f1bfde4311b77c5768faa138a7157c1b`.
- Platform tests (14), daemon tests (31), question integration tests (6,
  including 1/3/16 questions), and affected-crate Clippy passed. Both Apple
  Release builds and strict bundle signatures passed. Notarization, TestFlight,
  and physical answer acceptance were not completed.
- Publication completed before the user added the historical turn-result
  visibility fix. No device was installed with 1.0.23; installation was deferred
  to the combined release to avoid repeating device restarts.


### 2026-10-02: 1.0.24 / iOS build 21 published and installed

- Source `2d0b3b37934b67d05db91e0bb706e11d63af3ecc` was merged by PR #29
  (`ec69a6c66470437429caa5f38ed996b1a649a7bb`). It includes PR #28's manual
  answer-confirmation feedback and retains each completed turn's result outside
  Worked, including historical turns without usage footers, multipart replies
  and explicit Claude results followed by other records. Rust owns the grouping.
- Nine timeline/render tests, 15 Claude-related tests, and all-target Clippy for
  `muster-agent` / `muster-app-core` passed. macOS and iOS Release builds passed;
  the iOS Rust build retains the existing unused `PrepareUpdate` warning.
  There were no configured CI checks or other open PRs at release preflight.
- Both packages were published through `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}`. Independently
  downloaded all 12 public files and verified their SHA-256 values, the desktop
  feed signature, and the actual OTA page -> immutable manifest -> IPA route.
  The IPA reports version 1.0.24/build 21. Final bundle signatures and desktop
  release ancestry/fingerprint preflight passed.
- Artifact SHA-256 values:
  - macOS portable: `fafddac61eac02eeb9ca1dbf9d74bda58594e92b32be44f83d8f289c94cfe420`.
  - daemon: `b86490e72df0f4f3fa4f2e25d1bdf4a81f013954954b10be98c0562ad410d560`.
  - iOS IPA: `16e14aa4a42cdd68cfc062b8451903dda92ec67f96339198708b7deffe8cd430`.
- Installed on Mac-B, Mac-C, then Mac-A using the verified public portable ZIP.
  Each installer required Rust's `DesktopUpdateReady`, normal process exit,
  strict nested signatures, exact daemon bytes/build stamp, preserved device
  identity, a matching installed Locked Use plugin, relay registration, vnet
  and Accessibility. All three report running version 1.0.24. No old-program
  backup, forced termination, Keychain unlock or pairing reset was used.
  After Mac-A's restart, read-only Muster CLI queries to both peers succeeded.
- Installed `dev.linsheng.muster` in place on the connected iPhone. A fresh
  CoreDevice inventory confirmed version 1.0.24/build 21, and launch succeeded.
  This validates direct installation, not Safari OTA installation.
- No real pending answer was submitted by this deployment. Physical per-device
  answer delivery, final-result rendering and iOS thermal improvement remain
  usage acceptance checks. macOS notarization and TestFlight were not performed.


### 2026-10-02: 1.0.25 / iOS build 22 code-block copy action

- Source `47988eb69711dea9d3d6348ae1d80217c4484a46` (PR #31, merged as
  `8e86b20fdd1b3d0ffa15517134b1d326bf5ce746`) adds a top-right copy button to
  Markdown code blocks in the shared Apple UI. The button stays outside the
  horizontal scroll area, copies complete text with whitespace intact, uses a
  44-point iOS touch target and briefly shows a checkmark. No provider or
  authorization state moved out of Rust.
- Swift syntax verification and both Apple Release builds passed. The final
  macOS and iOS signatures passed strict verification, as did desktop source
  ancestry/fingerprint preflight. No other open PRs remained before publication;
  the repository has no configured PR CI checks.
- Published through `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}`, independently downloaded all
  12 public files, verified hashes and desktop feed signature, and followed the
  actual OTA page/manifest/IPA route to version 1.0.25/build 22.
  Portable SHA-256: `107a8ae2e0e24213ed525ac38ae9e1f1fe8003020a93a352232ca38eb41c9012`;
  daemon: `f532eeb3b5fd4a92978aa87705846ffed544fec85d36a02ab0d80321ab5f0160`;
  IPA: `98d3ef5f643e5fdb052be3c55e93e713430649184a6f4e087bc14eeacd7a6f77`.
- Mac-B's initial SCP transfer timed out before the installer was available.
  Logs and running version confirmed no replacement had occurred. Both remote
  Macs then downloaded the public package directly; the installer required its
  exact hash before proceeding. Installed on Mac-B, Mac-C and then Mac-A with
  Rust's update lease and normal process exit, without an old-program backup.
  All three passed signature, daemon version/hash, unchanged device identity,
  system plugin, Accessibility, relay and vnet checks. Mac-A queried both peers
  successfully after its own restart.
- iPhone installation and fresh inventory confirmed `dev.linsheng.muster`
  version 1.0.25/build 22. Automatic launch was denied because the phone was
  locked; launch and physical copy-button verification remain unverified.
  No clipboard contents were changed remotely for testing. macOS notarization,
  TestFlight and a physical Safari OTA install were not performed.


### 2026-10-02: 1.0.26 / iOS build 23 question dispatch and display lifetime

- Source `f42e3354ab7300f55a195c86ac33a462bdddb206` (PR #33, merged as
  `f3e85f37bfde8138f1238292300cd50f5084d36d`) addresses the new Mac-B failures
  observed on 1.0.25. At 12:24:05 CST, macOS logged display sleep followed by
  `kLWLockFromDisplayDim`, interrupting the second attempt after two choices.
  The earlier 12:23 attempt timed out finding the next question; its exact cause
  remains unproven. Neither attempt had a matching native answer receipt.
- Shared Rust now owns a watchdog display assertion bounded by the existing
  Locked Use deadline, with explicit release and OS timeout release. The brief
  lock-screen wake still ends before authorization. Manual lock, input,
  revocation and cover checks retain their existing behavior. Final immediate
  choices with a separate Submit control now wait for that actionable control
  and dispatch it, without waiting for selection echoes or answer receipts.
  Read-only inspection observes the form for five seconds under the live guard.
- All 90 relevant Rust tests and affected all-target Clippy checks passed. The
  actual IOKit test passed explicit release and automatic timeout release.
  macOS and iOS Release builds and strict final signatures passed; the existing
  iOS unused `PrepareUpdate` warning remains. PR #33 had no configured CI checks.
- Published both platforms through `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}`. Independently
  downloaded all 12 public files, verified hashes and the desktop feed signature,
  and followed the actual OTA page/immutable manifest/IPA route to 1.0.26/build 23.
  Portable SHA-256: `7a6ef51a7a58b743e341232a204e2741c4332f5e3b8cca79a7d3df5130b271c7`;
  daemon: `77bf00bbf24a1fb8cbcd6d4dc0158c68b0e2bf623e107fb0a7f011633983c33e`;
  IPA: `9a0b5e01e2381da0fd727756bd6a7a5c145690f81c8ef78867a8e8994f96dcd8`.
- Mac-B's initial public download failed DNS resolution, and an SCP transfer
  timed out before replacement. HTTPS download using the freshly resolved
  server address with normal hostname/certificate verification succeeded.
  Mac-B, Mac-C and Mac-A then installed the exact verified public package using
  Rust's update lease and normal process exit, without old-program backups.
  Each passed exact daemon hash/version, signature, unchanged device identity,
  installed plugin, Accessibility, relay and vnet checks. Mac-A's fresh CLI
  queries to both peers succeeded after its own restart.
- Mac-B's read-only diagnostic observed the same pending question for 5,140 ms,
  with unchanged initial/final forms. The operation completed normally and
  relocked; `PreventUserIdleDisplaySleep` returned to zero. No choice or Submit
  was pressed. This validates that observation window, not real answer delivery.
- Direct iPhone installation and fresh CoreDevice inventory confirmed
  `dev.linsheng.muster` version 1.0.26/build 23. Launch was denied because the
  phone was locked. User retry and a matching native receipt remain required
  for answer-delivery acceptance. Notarization, TestFlight and physical Safari
  OTA installation were not performed.


### 2026-10-02: 1.0.27 / iOS build 24 covered question AX refresh

- Source `69bf4cfa8d128a7c9b8dbfda7c53955370a1a685` (PR #35, merge
  `e57d21e1bab2ed5d55423761970c046adbb1343e`) follows two failures on the actual
  1.0.26 Mac-B at 12:59 CST. The first operation read question 0 while awaiting
  question 1; the second read question 1 while awaiting question 2. Both hit the
  three-second readiness deadline. Neither was interrupted by screen locking,
  and the native transcript still contained no matching answer tool result.
  A later read-only inspection showed question 2. Display wake alone had not
  fixed this stale-control behavior.
- The adapter now issues a read-only application-scoped AX hit test when the
  next question or enabled Submit cannot be found. The coordinates are clipped
  to the verified web-area/window intersection, and the result is discarded.
  Ordinary Chromium AX attribute reads use the browser cache; hit tests route
  to the renderer and force dirty accessibility serialization. The existing
  opaque cover, authorization, input-generation and readiness deadlines remain.
  No answer press is replayed, and no selection/receipt confirmation is added.
- A compositor-hint experiment at source `d57dc0c6` was tested on Mac-B but did
  not change the black test window's observed occlusion. It was reverted, Mac-B
  was restored to signed 1.0.26, and the final package was rebuilt from the
  source above. The withdrawn candidate never entered the public update feed;
  its hash-verified staging files were removed after the final release.
- Seventeen platform tests, 35 Locked Use tests and affected all-target Clippy
  passed. Both Apple Release builds and strict final signatures passed; the
  existing iOS unused `PrepareUpdate` warning remains. The PR had no configured
  CI checks. Mac-B's final signed candidate accepted the read-only renderer
  query (`renderer_query=true`) during a 5,165 ms inspection and completed the
  normal covered relock at 13:23:25 CST. A separate subsequent Codex send was a
  different operation and does not validate Claude question delivery.
- Published macOS 1.0.27 and iOS 1.0.27/build 24 through
  `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}`. Independently downloaded all 12 public files,
  verified their hashes and desktop feed signature, and followed the actual
  OTA page/immutable manifest/IPA chain. Artifact SHA-256 values:
  - portable: `9bce13d0b44bdf0bfa012aa32fc951b6344f735c8abe1ef91790f3810a036a61`;
  - daemon: `98b8f80bdd91084896d0a28892be7fcea9ceb6f292cf24c45fa272f9e4997715`;
  - IPA: `757eb25dd44b0f6c127098edc3671b02a33f27cb81f0d709c76374fb96dd0481`.
- Mac-B, Mac-C and Mac-A each run the exact final daemon bytes. Each installer
  required Rust's update lease, normal process exit, unchanged device identity,
  matching installed Locked Use plugin, strict signatures, Accessibility,
  relay registration and vnet. No old-program backup was created. Mac-A's
  read-only queries to both peers succeeded after its update. The iPhone's
  fresh inventory reports `dev.linsheng.muster` 1.0.27/build 24; launch succeeded.
- No real answer was replayed for testing. A user retry and its matching native
  receipt are still required for answer-delivery acceptance. macOS notarization,
  TestFlight and physical Safari OTA installation were not performed.


### 2026-10-02: notarized 1.0.28 and iOS TestFlight build 25

- Released source `2b24d82515adaa5619dbd4278755935fc094526e`, including PRs
  #37, #39 and #40 for bounded iOS diagnostics, built-in performance monitoring
  and log export. The source was already committed, pushed and merged. Open
  PR #38 changes only worktree maintenance and was excluded from this release.
- Both Apple Release builds and strict signatures passed. Three Swift
  performance-log tests and three Python diagnostics-tool tests passed. Desktop
  source ancestry, fingerprint and build-stamp preflight passed. This release
  did not rerun the full Rust suite or validate physical thermal behavior.
- Apple notarization accepted submission
  `b3d1c8c1-41fa-4ea9-a9b7-b71a7cbe1b28`. The ticket was stapled and validated;
  Gatekeeper accepted the final app as Notarized Developer ID. Both the
  portable ZIP and updater package include the ticket. Signed entries and
  symlinks were preserved, and package hashes and feed signature regenerated.
  Existing API credentials were used directly; no Keychain unlock occurred.
- Published macOS 1.0.28 and iOS OTA 1.0.28/build 25 through
  `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}`. All twelve publicly downloaded files matched their
  expected SHA-256 values, the desktop feed signature verified, and the actual
  OTA page -> immutable manifest -> immutable IPA resolved to 1.0.28/build 25.
  Historical immutable artifacts were retained.
- Exported a separate App Store Connect IPA and uploaded it successfully.
  TestFlight build `bba9ea4c-eb95-575c-8801-5d660e67bc18` reports version
  1.0.28/build 25, processing `VALID`, internal state `IN_BETA_TESTING` and
  `usesNonExemptEncryption=false`. The existing Internal group has automatic
  access to all builds. External beta review was not submitted.
- SHA-256: portable ZIP
  `2891a45a5bba97daeee215ca9d28e0d533c39e77bdacd943a2b7464e91abe03c`;
  full updater package
  `0ce6fb828f58512b7cb4505a01fc30207fa4214ccd0bf05983195f95acdf1312`;
  OTA IPA
  `f76ad568455e00329a932244a8c969a369fd20bf30268c9e439764805bab85ad`.
- This turn published artifacts without replacing installed Mac or iPhone
  apps. Device installation, runtime and physical end-to-end checks were not
  performed. HarmonyOS wireless installation remains a separate pending task.


### 2026-10-02 evening: 1.0.29 and 1.0.30 installed on three Macs (not published)

- 1.0.29 was built from `118523b4e0bdab18e2ff71077c0906dcdbcdfcb2` (PR #45: unconfirmed
  send reconciliation, not-sent ledger classification, pre-unlock refusal of
  command-line Claude sessions, Codex async question retirement, diagnostics).
  1.0.30 was built from `a93a70308a6d57518828b49c9cb79a91af8fdbe4`. It adds PR #46:
  Claude prompts queued during a running turn now confirm the send. That is the
  Mac-B `example-session` failure, which 1.0.29 did not cover.
- Both builds: Developer ID signed, release preflight passed, Velopack 1.2.158
  portable package, and Apple notarization accepted (`21aa78fc-ae62-4adf-a3a1-1c5c9b1ad988`
  for 1.0.29, `5214b597-ae50-4abb-8811-8ab94404652d` for 1.0.30). Ticket stapled;
  Gatekeeper reports Notarized Developer ID. Notarization used the existing API
  key, with no Keychain unlock.
- Installed on Mac-B, Mac-C and Mac-A in that order. The installer held each
  running daemon's update lease (Rust drained console operations), stopped only
  Muster's App and daemon, used no forced kill and made no backup. Relayed vnet
  SSH copies were retried until the hash matched; remote installers ran detached,
  so the vnet drop during the daemon restart could not interrupt them.
- Final state on all three: 1.0.30, daemon SHA-256
  `deb072d6af2f78e22b606ee696e09ab6b9a2f606d38da1be8514236d91a7634b`. Startup log
  stamp `a93a7030`. Same device identity. Accessibility granted, relay registered,
  vnet up. The system Locked Use plugin matches the bundle. Mac-A reaches both
  peers.
- Under 1.0.29, real user sends were confirmed: three Claude sends on Mac-B
  `example-session`, one Claude and one Codex (queue admission) on Mac-C. No send has
  been observed on 1.0.30 yet. The deployment restarts dropped the in-memory
  unconfirmed latches of the 13:42 (`01a0fa37`, Mac-A) and 18:57 (`example-session`,
  Mac-B) presses. Their ledger rows remain `unknown` and were not replayed.
  The 18:57 prompt had been absorbed by Claude; the user's later retry of the
  same text is the only `user` record of it.
- The public update feed, iOS OTA and TestFlight were not changed; this is not a
  public release. `released-commits.txt` is unchanged for that reason. Physical
  end-to-end checks remain open on the installed build: a send while Claude is
  busy, a late receipt after a timeout, CLI-session refusal and the new CoverLost
  diagnostics.


### 2026-10-03: 1.0.32 installed on Mac-B only (not published)

- Built from `cb390932354cd8ff0b4ec98775c0533026094134` (PR #54), which adds to
  1.0.31 (`e2b06dde`) PRs #50–#54: the declarative app-profile proposal, one
  transcript copy per chat view, the `update.rs` and `locked_use.rs` splits,
  CatDesk session viewing and AX profile (PR #52), and the desktop app registry
  (`muster_core::apps`).
- Clean detached worktree; release preflight passed (single non-dirty stamp
  equal to `HEAD`, prior-release ancestry, fingerprints). Developer ID signed
  and timestamped, Velopack 1.2.158 portable package (SHA-256
  `a62e5d7b6b442b1324420153d695ee7442dcc2c7c6ffd1c64dfb0d9685f8397d`; daemon
  `fb45d75f86efa7867a37cbf968a146807c3e6934fc9aaef44db42410fa48d038`). Not
  notarized and the feed was not signed: this Mac has neither the notarization
  key nor the offline feed key, and direct installation uses neither. Velopack
  needs `DOTNET_ROOT=/opt/homebrew/opt/dotnet/libexec` here.
- Copied to Mac-B over its LAN address (vnet bulk copies stall, see 1.0.31) and
  verified by hash there. The detached installer held the daemon's update lease
  until `DesktopUpdateReady`, stopped only Muster's App and daemon, replaced the
  bundle and relaunched; no backup, no forced kill.
- Final state: 1.0.32, startup stamp `cb390932`, same device identity
  (`56JI-LPAW-R3GS-ZRCE`), Accessibility granted, relay registered, vnet up,
  console locked. The bundled root helper, now built from
  `src/native/apple/NetHelper` (PR #48), reinstalled and verified the Locked Use
  plugin. Mac-B's AX allowlist is unchanged (Claude, Codex); CatDesk is not
  allowlisted there.
- Mac-A and Mac-C stay on 1.0.31. No message was sent. The public update feed,
  iOS OTA and TestFlight were not changed; `released-commits.txt` is unchanged.

### 2026-10-03: 1.0.33 installed on Mac-B only (not published)

- Built from `5614785c985fe2b0da2ce561e95942c79b8f72cd` (PR #57), which adds to
  1.0.32 PRs #55 and #57: app profiles step 2 (composer, send, stop and
  approval selectors as data; Chat mode's send, stop, new chat, dialog close
  and settings exit), and agent-conversation Stop matched by name with
  interrupt pressing the same control. These change how Stop is found and need
  device acceptance.
- Same procedure as 1.0.32: clean detached worktree, `verify-release.sh`
  passed (daemon stamp equal to `HEAD`), Developer ID signed and timestamped,
  Velopack 1.2.158 portable package (SHA-256
  `1147868c068a0b7ad437ef61bee6e75dbe2b9bd7d159d65ca25ca64f8d370796`; daemon
  `cc2b6546e3b1b34e287527c6c249e73a2cc24ab504dae4b591309c4ba5c68f87`); not
  notarized, feed not signed. Copied over Mac-B's LAN address (now
  192.0.2.235), verified by hash, installed by the detached installer
  holding the update lease; no backup, no forced kill.
- Final state: 1.0.33, startup stamp `5614785c`, same device identity
  (`56JI-LPAW-R3GS-ZRCE`), Accessibility granted, relay registered, vnet up,
  console unlocked by the user at the time. The Locked Use plugin in
  `/Library/Security/SecurityAgentPlugins` is this build's, Developer ID
  signed. AX allowlist unchanged (Claude, Codex).
- Mac-A and Mac-C stay on 1.0.31. No message was sent. The public update feed,
  iOS OTA and TestFlight were not changed; `released-commits.txt` is unchanged.

### 2026-10-03: 1.0.34 installed on Mac-B only (not published)

- Built from `41abb527219b714e5d10f09d0304a34026370510` (PR #61), which adds to
  1.0.33 PRs #58–#61: the rest of app profiles step 2 (queue, question cards,
  session list, drafting and Chat-mode strategies as data), AX fixtures of
  running turns, a question card, drafts and Chat mode, `ax-dump` reading up
  to 20,000 nodes, and a draft without Stop read as unknown with an explicit
  interrupt refusal.
- Same procedure as 1.0.33: clean detached worktree, `verify-release.sh`
  passed, Developer ID signed and timestamped, Velopack 1.2.158 portable
  package (SHA-256
  `1d14415d5643bd253bef35a6a1a7957f4b8d673f0981745e7b7cc5d2668d9c77`; daemon
  `479da2d85872a5d6050b1800be5d7da5dc16b4a6ee86531b83a4cad3c93ad7a7`); not
  notarized, feed not signed. Copied over the LAN, verified by hash, installed
  by the detached installer holding the update lease.
- Final state: 1.0.34, startup stamp `41abb527`, same device identity
  (`56JI-LPAW-R3GS-ZRCE`), Accessibility granted, relay registered, vnet up,
  console locked. The Locked Use plugin is this build's, Developer ID signed;
  locked control allowed; AX allowlist unchanged (Claude, Codex).
- Mac-A and Mac-C stay on 1.0.31. No message was sent. The public update feed,
  iOS OTA and TestFlight were not changed; `released-commits.txt` is unchanged.

### 2026-10-03: 1.0.36 on Mac-B and Mac-C; iOS build 26 and the Mac download published

- Built from `59428d6fc923e4fcd9a187797b286fdb5e28e4db` (PR #65), which adds to
  1.0.35:
  - Claude sessions archived in Claude.app are hidden and refused before
    unlocking (#64);
  - terminal copy and paste on iOS and macOS (#65).
- **macOS 1.0.36.**
  - Same procedure as 1.0.33: clean detached worktree, `verify-release.sh`,
    Developer ID signed and timestamped, Velopack 1.2.158 portable package
    (SHA-256
    `d4772e3141c68374946c1a5ffad96800fc50db4a23ba4275a66d8386ead9c750`; daemon
    `3c8e3523ba61d0900ad1be68cf143bead72736e209ee3fbd2f24c508f1914070`).
  - Not notarized. This Mac has neither the notarization profile nor the
    offline feed key.
  - Installed with the detached installer on Mac-B (23:46, LAN) and Mac-C
    (23:53, locally). Both report 1.0.36 with the same identity,
    Accessibility granted, relay registered and vnet up. Both consoles were
    locked at the end. The Locked Use plugin is this build's.
  - Mac-A stays on 1.0.31.
- **Mac download on the page.** At the user's request, only
  `desktop/Muster-osx-arm64-Portable.zip` was replaced with 1.0.36.
  - The previous file is kept in `~/muster-desktop-backup/` on the server.
  - The signed update feed (`releases.osx-arm64.json` and `.sig`) still offers
    1.0.28, so in-app updates are unchanged.
  - The public link was read back: the hash matched, the version is 1.0.36 and
    the strict signature is valid. Gatekeeper reports "Unnotarized Developer
    ID", so a downloaded copy needs Open Anyway the first time.
- **iOS 1.0.36 build 26.**
  - Built from the same commit, with `CFBundleShortVersionString` 1.0.36 and
    `CFBundleVersion` 26 set in the release worktree only.
  - Signed with Apple Development and the `dev.linsheng.muster` team profile
    (3 registered devices, expires 2027-09-30).
  - Packaged with `package-ota.py` and published to `${MUSTER_RELEASE_ROOT}/ios/`:
    the hash-named IPA and manifest first, then the aliases and the page. Build
    25's four alias files are kept in
    `~/muster-ios-backup/build25-20261003235034/`.
  - The public route was verified: page install link, then manifest
    `manifest-14a61ad9….plist` (build 26), then IPA SHA-256
    `14a61ad94a8ad5f139dd73461eab2efe89ab5cf243dc9ddb3c49cea41be39c23`, version
    1.0.36 (26), Apple Development signature.
  - Installation on the iPhone is pending. TestFlight was not changed.

### 2026-10-04: 1.0.37 on Mac-B and Mac-C; iOS build 27 and the Mac download published

- Built from `3d668c9c0227f705a6367c6bbd0e612ae9c261ef` (PR #70), which adds to
  1.0.36:
  - the Mac version on the OTA page (#67);
  - fewer iPhone re-renders and released hidden screens (#68);
  - the phone network profile: relay only, QUIC first with TLS fallback, no
    probes (#69);
  - performance-log upload from iOS Settings to a Mac (#70).
- **macOS 1.0.37.** Same procedure as 1.0.36 (Velopack portable package
  SHA-256 `2bc0615cb8925ead83641361c078d1db0b6a24fd00cd5c7d004d438e7bc484fc`;
  daemon `794c92769814cf86b2f3d31d02ddeca14fcab42ebfe4308ffe41afac91a0c1b0`).
  Not notarized. Installed with the detached installer on Mac-B (11:11 CST,
  LAN) and Mac-C (11:12, locally); both reported 1.0.37, Accessibility
  granted, relay registered and vnet up.
- **Mac download.** `desktop/Muster-osx-arm64-Portable.zip` replaced with
  1.0.37 (1.0.36 kept in `~/muster-desktop-backup/`). The signed update feed
  still offers 1.0.28.
- **iOS 1.0.37 build 27.** Published to `${MUSTER_RELEASE_ROOT}/ios/` as for
  build 26 (build 26's aliases in
  `~/muster-ios-backup/build26-20261004111247/`). The public route was read
  back: page, manifest (build 27), IPA SHA-256
  `562591bb21158252ee537c98e5ce2cea46be13636b86b5a2d9c5b157374e1883`, version
  1.0.37 (27); the page shows Mac 1.0.37.
- **Relay.** The relay of this commit (the server side of the sparse QUIC
  liveness in #69) was built on the server in
  `~/muster-relay-3d668c9c/`. Installing it into `/opt/muster/bin` and
  restarting the service needs sudo and is left to the user; the running
  relay is still the 2026-09-29 build.

### 2026-10-04: 1.0.38 on Mac-B and Mac-C; iOS build 28 and the Mac download published

- Built from `2d80c0d1d595fe406f599bf75a65dc2dd326755f` (PR #72), which adds to
  1.0.37:
  - an unconfirmed-send latch ends when the app dropped the write, or when
    the user releases it from the phone (#71);
  - the iOS terminal key bar pastes through the system paste control, with no
    "Allow Paste" prompt (#72).
- **macOS 1.0.38.** Same procedure (Velopack portable package SHA-256
  `a29dfd33f936257c776f0b7bfc1b04e866e1c002bea43e469cb1df0ca02d48db`; daemon
  `57cced7dd5b3a8701fdad98d92c2986ac3af007a2b62d801965893556dcedb7b`).
  Developer ID signed and timestamped, not notarized. Installed with the
  detached installer on Mac-B (14:26 CST, LAN) and Mac-C (14:27, locally);
  both report 1.0.38 with Accessibility granted, relay registered and vnet up,
  and the Locked Use plugin is this build's.
- **Mac download.** `desktop/Muster-osx-arm64-Portable.zip` replaced with
  1.0.38 (1.0.37 kept in `~/muster-desktop-backup/`). The signed update feed
  still offers 1.0.28.
- **iOS 1.0.38 build 28.** Published to `${MUSTER_RELEASE_ROOT}/ios/` (build
  27's aliases in `~/muster-ios-backup/build27-20261004142949/`). The public
  route was read back: page (iOS 1.0.38 build 28, Mac 1.0.38), manifest
  `manifest-03806ebe….plist` (build 28), IPA SHA-256
  `03806ebeafd32f1772c9077fe257c8cd2069e58add5dc99a13d06bae1e1c5c53`, version
  1.0.38 (28), Apple Development signature; the Mac download matched the
  package hash, version 1.0.38, strict signature valid.
- The first iOS build failed with "No space left on device": the system volume
  had 117 MB free. Scratch build caches were removed and the build rerun.
- The relay is unchanged (still the 2026-09-29 build); installing the 1.0.37
  relay still waits on the user's sudo.

### 2026-10-04: 1.0.39 on Mac-B and Mac-C; iOS build 29 and the Mac download published

- Built from `a065d22ff95bdb1aec8889265c61a5f9c65d95e7` (PR #74): Ghostty
  terminals, and Locked Use terminal sessions that keep a locked Mac
  unlocked under the cover while a phone shows a Ghostty terminal.
- **macOS 1.0.39.** Same procedure (Velopack portable package SHA-256
  `a9c9f93c0a8e054c22997a2d1599fd4a7d4818cde556279696fddf3da5aa4b16`; daemon
  `313a051997459b3542eaa30cc55997845c2831e8787eb86fa4d3306193bde2b8`).
  Developer ID signed and timestamped, not notarized.
  - Installed with the detached installer on Mac-B (15:40 CST, LAN) and
    Mac-C (15:41, locally). Both report 1.0.39 with Accessibility granted,
    relay registered and vnet up.
  - Mac-C has only Ghostty, so `--terminal auto` now offers Ghostty there.
    Before the Automation consent its terminal list answers "Muster may not
    control Ghostty yet", without prompting. Mac-B keeps iTerm2.
- **Mac download.** Replaced with 1.0.39 (1.0.38 kept in
  `~/muster-desktop-backup/`). The signed update feed still offers 1.0.28.
- **iOS 1.0.39 build 29.** Published (build 28's aliases in
  `~/muster-ios-backup/build28-20261004154150/`). Read back: page (iOS 1.0.39
  build 29, Mac 1.0.39), manifest (build 29), IPA SHA-256
  `564854ef2f5f394611264b60fbba8eb2a71e5f701d5ad4485621f4e19d18e52d`, version
  1.0.39 (29); the Mac download matched its hash, strict signature valid.
- **Acceptance pending.** It needs, on Mac-C, the one-time Automation consent
  (Muster's Mac setup, "授权并检查 Ghostty…"). Then the phone opens a Ghostty
  terminal on the locked Mac, types, leaves, and the Mac relocks.

### 2026-10-05: 1.0.40 on Mac-B and Mac-C; iOS build 30 and the Mac download published

- Built from clean `origin/main` commit
  `9217c0380c224937f0f640759ae2e55dafb0d19f` (through PR #79). Changes since
  1.0.39 include TOML app profiles and transcript grammars, mobile half-fold
  layouts and the Muster keyboard, and Claude Code web / Codex cloud sessions.
- **Checks.** 476 Rust tests passed, seven ignored; 53 Swift tests and five OTA
  packaging tests passed. Rust formatting, layer validation and strict workspace
  Clippy passed. Both Apple Release builds succeeded. Clippy initially selected
  Homebrew's incompatible driver; installing the matching rustup component and
  rerunning resolved that toolchain error without source changes.
- **macOS 1.0.40.** Release preflight passed (clean source, exact daemon stamp,
  prior-release ancestry and fingerprints). Velopack 1.2.158, Developer ID
  signed and timestamped; nested strict signatures verified. Not notarized:
  no notarization profile was configured. Portable ZIP SHA-256:
  `7484c36d1068f4dd64c9f15dcc077b6ffcea7f5c672d2849e7815872db22972d`.
  Signed daemon SHA-256:
  `caabb723b5f35d30e970f95d04c711e7c94a40bd641d3a971a13878bb21e0407`.
- **Installation and runtime.** Mac-B and Mac-C installed 1.0.40 while holding
  the daemon's update lease. Prior Apps were retained under each user's
  `Library/Application Support/Muster/Backups/`. Mac-C's Apple Events quit
  returned cancellation; the first attempt stopped safely, and a retry used
  normal SIGTERM after acquiring a fresh lease. No SIGKILL was used. Both
  devices retain their identities, report Accessibility granted, relay
  registered, vnet active and a locked console. Installed and bundled Locked
  Use plugins have matching hashes. Mac-A's localhost SSH port 2222 refused
  the connection, so it was not updated.
- **Public Mac download.** Published through `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to
  `${MUSTER_RELEASE_ROOT}/desktop/Muster-osx-arm64-Portable.zip`. Public download
  bytes match the verified signed package. The existing signed update feed
  remains at 1.0.28 because the offline feed key was not available; its bytes
  and signature were reverified after publication. This release does not
  become available through in-app updates.
- **iOS 1.0.40 build 30.** Apple Development signed for the existing three
  registered devices; provisioning expires 2027-09-30. Published all six OTA
  files to `${MUSTER_RELEASE_ROOT}/ios/`, immutable IPA/manifest first. Public
  page → hash-named manifest → IPA verified, including embedded version/build
  and SHA-256:
  `4bc9c75ee3cdcdb7add9e95d89db1805ea2d963380edff57c1034bca3e4a77de`.
  The page reports Mac 1.0.40. Both platforms' previous public aliases are
  retained in `~/muster-{desktop,ios}-backup/before-1.0.40-20261005014422/`.
  TestFlight was not updated; physical iPhone installation remains pending.
- **Relay.** Latest source compiled on the server; staged executable
  `/home/developer/muster-relay-9217c038/muster-relay`, SHA-256
  `26479379c68b92e8b9945718f5b1e7cfd48508d23068ebbaf80db5300ab5c046`.
  Installing/restarting remains blocked by server sudo permissions. The live
  service still reports its 2026-09-29 start time. The public relay probe
  passed signed registration on both TLS and QUIC; this validates the existing
  service, not installation of the staged binary.
- **End-to-end boundary.** No user messages were sent. Phone installation,
  cloud-session interaction, keyboard/half-fold behavior and real Locked Use
  operations still require device acceptance.

### 2026-10-05: latest-only download policy

- User requested that the download page install only the latest release and
  retain no old publisher versions. The root and desktop page aliases had no
  cache policy, while historical hash-named OTA packages remained installable.
- Updated the generated page to show direct latest downloads, removed the
  independently older TestFlight promotion, and pointed installation/IPA links
  at the latest fixed aliases. nginx now sends `no-store, no-cache,
  must-revalidate, max-age=0` for every `/muster/` page and artifact, disables
  ETags/If-Modified-Since reuse and redirects historical manifests to the latest
  manifest. Removed packages return 410.
- Installed and syntax-checked the nginx configuration, then reloaded it.
  Verified all six directory/index page URLs, old-manifest redirection to build
  30, latest macOS/iOS hashes and old-package refusal over public HTTPS.
  No app binaries, signatures, device installations or runtime state changed.
  Physical phone installation remains unverified.
- Removed 45 obsolete public files and the publisher's two old-release backup
  directories. This includes the stale signed 1.0.28 desktop feed: in-app update
  checks are unavailable until a latest signed feed can be published. The
  signed 1.0.40 Mac download and iOS build 30 remain available.
- Added a validated, dry-run-by-default pruning tool to make this part of each
  subsequent release. Nine packaging/pruning tests passed. Historical records
  above describe prior retention and do not override the new policy.

### 2026-10-05: standalone 1.0.40 download page

- Published `https://linsheng.dev:8443/muster/1.0.40/` at the user's request.
  The source page, independent `install.plist` and checksums are under
  `MusterApp/deploy/releases/1.0.40/`. The page explicitly offers iOS 1.0.40
  build 30 and macOS 1.0.40. Its unique manifest resolves the existing verified
  hash-named IPA with a dedicated release query; it does not redirect through
  the general latest-manifest alias. Its Mac link is a fixed 1.0.40 ZIP.
- Public page-to-manifest-to-IPA verification checked the embedded version and
  build, and both package SHA-256 values match the previously signed release.
  No app rebuild, new signing, notarization or device installation occurred.
  This separate URL does not establish that the reported iPhone update from
  1.0.27 has completed; that remains a device-installation verification step.
- Ten publishing/pruning tests passed. The pruning tool now removes known files
  from superseded standalone version directories on the next release, keeping
  the latest-only publication policy and leaving unrelated files alone.

### 2026-10-05: iOS 1.0.40 build 30 available in TestFlight

- **Code.** Reused the verified Release app from source
  `9217c0380c224937f0f640759ae2e55dafb0d19f`, already published as OTA
  1.0.40/build 30. Assembled an export archive from that app and exported a
  separate App Store Connect IPA with Xcode; application source and version
  numbers did not change. This was an API-authenticated upload, not an Xcode
  Cloud build.
- **Tests.** No new application compilation or unit-test run for this re-export.
  The earlier 1.0.40 release's checks remain the source-validation record.
  Documentation whitespace checks passed.
- **Signing and upload.** Xcode export succeeded using the existing credential
  file documented above. The exported IPA passed strict deep signature
  verification in the normal system trust environment; its version is
  1.0.40/build 30, push entitlement is `production`, and debugging is disabled.
  No Keychain unlock was performed. Debug symbols were not uploaded.
  `altool --upload-package --wait` completed successfully with no errors;
  delivery/build ID is `03790d16-bc2a-5341-9ed2-f68d736e3b19`.
- **TestFlight.** App Store Connect readback confirms version 1.0.40, build 30,
  processing `VALID`, internal state `IN_BETA_TESTING`, not expired, and
  `usesNonExemptEncryption=false`. The existing Internal group automatically
  receives all builds; a readback of that group lists build 30 as `VALID`.
  External beta review was not submitted.
- **Installation and runtime.** Physical iPhone installation and runtime remain
  unverified. TestFlight availability is verified independently of the OTA
  website, whose publication had previously left TestFlight at 1.0.28/build 25.
- **macOS and web.** The existing macOS 1.0.40 distribution and the dedicated
  `https://linsheng.dev:8443/muster/1.0.40/` page were not republished in this
  TestFlight-only operation. Xcode Cloud remains unconfigured.
- **App Store Connect IPA SHA-256:**
  `483fc296b75e7e8acb5c9ccbd004fe3eb54a21c636c99c600b839b76bb23b0e9`.


### 2026-10-05 keyboard and live conversations (1.0.41 / iOS build 31)

- **Code.** Built from `56d32b6c00fd61be5be19c3c81c41ced49380977` (PR #84,
  merged). Layout selection is in Settings → 页面布局. UIKit focus waits for
  window attachment; the composer survives layout transitions; explicit phone
  key width shares prevent missing letters/controls; the composer offers a
  keyboard chooser. Rust refreshes history on entry, retains actively watched
  readers, invalidates replaced/truncated transcripts, and refreshes watched
  cloud transcripts every two seconds. Growing replies publish a content
  revision independent of message count. Provider availability still bounds
  cloud freshness.
- **Tests.** Agent/app-core suites: 241 passed, 5 ignored. Clippy passed for
  agent/app-core/musterd; the layer check passed. All eight hosted iOS simulator
  tests passed, including custom/system keyboard switching, focus across layout
  changes, and following a growing reply. Keyboard screenshots were inspected.
  GitHub reported no CI checks for PR #84; these are local verification results.
- **Signing and notarization.** macOS nested strict signatures passed. Apple
  notarization `567e7b44-d886-4640-b464-c7470da9761c` was Accepted; stapling,
  staple validation and Gatekeeper assessment passed (Notarized Developer ID).
  iOS OTA uses the existing three-device development profile. The separate
  App Store Connect export passed strict signature verification with production
  push and debugging disabled.
- **Publication.** Both platform downloads were published through
  `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to the documented desktop/iOS directories. Public
  readback followed the actual page installation link, verified iOS 1.0.41 (31),
  macOS 1.0.41, package hashes and no-store headers. Old 1.0.40 packages/pages
  were removed; its IPA returns 410, its page redirects to the latest page, and
  both old manifest forms resolve build 31. The desktop signed feed remains
  unavailable (404) because the offline feed signing key is unavailable; the
  latest notarized portable download is the supported update route.
- **TestFlight.** Build `a9d9becb-a8df-4135-80e2-7e96a0541703`, version 1.0.41
  (31), returned `VALID`, `IN_BETA_TESTING`, not expired, no non-exempt
  encryption. Readback confirmed access in the existing Internal beta group.
  Xcode Cloud remains unconfigured; this release used API-authenticated upload.
- **Installation and runtime.** Mac-C and Mac-B now run 1.0.41 with matching
  daemon SHA-256, strict signatures and relay registration. Physical iPhone
  installation and live peer-to-iPhone end-to-end behavior remain unverified;
  simulator tests and TestFlight acceptance do not establish those states.
- **Published SHA-256.** macOS portable:
  `10225273f408641ab9578581e13d6128622c652365f4ac51ba4957c0bb4c2d74`;
  iOS OTA:
  `a67ea11adfd9d386d2321fdaf3ce3cb8e7869764d5101f59ab6d35a65cfc28f3`;
  TestFlight IPA:
  `a4ce95bf667d7e71fb8067ceee461bc4a245dfc97b1f553ed7afec9dfe927bb8`.


### 2026-10-06 iPhone Mac keyboard and persistent stacked input (1.0.42 / build 32)

- **Code.** `c682da2ca6fa222a35c4637ab36b5f3adb9e998e` (PR #86, merged).
  All iPhone widths now render the Mac ANSI rows, including Tab, both Shifts,
  Control/Option/Command and the inverted-T arrows. Narrow screens use compact
  legends, preserving the same key positions and relative widths. Function
  keys are active for terminal targets and disabled in ordinary text fields.
  In stacked conversations the built-in keyboard is a persistent lower-pane
  view; adding an image, losing focus or switching sending state cannot dismiss
  it. Key presses restore editable-field focus. Presentation and UITextInput
  adaptation remain native UI code; Shared Rust business behavior is unchanged.
- **Tests.** 22 keyboard/layout unit tests and all 10 hosted iOS simulator
  tests passed. New fixtures cover attachment/focus changes and input guards;
  actual Mac keyboard and stacked-with-attachment screenshots were inspected.
  GitHub reported no CI checks for PR #86; these are local results.
- **Signing and website.** iOS archive and separate App Store Connect export
  succeeded. Strict signatures passed; the latter uses production push and
  disables debugging. OTA uses the existing three-device development profile.
  Published through `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to `${MUSTER_RELEASE_ROOT}/ios/`.
  Public readback verified 1.0.42 (32), hashes, no-store headers, old manifest
  redirection to build 32 and old iOS package removal (410). Superseded 1.0.40
  and 1.0.41 iOS staging packages, plus the 1.0.40 Mac staging archive, were
  also removed from the publisher after checking their embedded versions.
- **TestFlight.** Build `a911b269-e021-5cac-b24b-48841dc1e4c0`, version 1.0.42
  (32), returned `VALID`, `IN_BETA_TESTING`, not expired and no non-exempt
  encryption. The existing Internal group can access it. Xcode Cloud remains
  unconfigured; this was an API-authenticated upload.
- **macOS.** This is an iOS UI release. The existing notarized macOS 1.0.41
  download is unchanged and its public hash was verified. No new Mac install
  or notarization was performed. The signed desktop feed remains unavailable.
- **Installation/runtime boundary.** Physical iPhone installation and live
  device-to-device end-to-end verification remain outstanding. Simulator
  acceptance does not establish those states.
- **SHA-256.** OTA IPA:
  `8f8ffd4bdeaf8cef3124c2af0fcbfbe3da7e2d3cc30a7845e269c3629550969f`;
  App Store Connect IPA:
  `172e37abc59eb776a7330a1ec5eecc2e5a2c1212259a9d414b32f0067967caf6`.

### 2026-10-06 automatic Rust/Swift high-CPU stacks (iOS 1.0.43 / build 33)

- **Code.** `8de9f8ca` (PR #88), merged as `f7e6e8fa`. iOS performance
  reports automatically include native execution stacks for hot Rust and Swift
  threads. The monitor selects interval CPU deltas at >= 80% process CPU and
  >= 20% per thread, captures up to three threads three times, and limits bursts
  to once per minute. A bounded C/Mach frame-chain walker resumes threads before
  Swift formatting and image lookup, and records acquisition/resume errors and
  partial-stack stop reasons. No debugger connection is required. This release
  gathers evidence; it does not fix the underlying high CPU.
- **Build and symbols.** Apple Rust builds retain frame pointers; release FFI
  flags were checked in Cargo's fingerprint. Xcode emits matching dSYMs. The
  released binary and dSYM UUID are both
  `2d488988-bb45-5216-ae6c-bb8c0668e9ca` (arm64); actual release Rust and Swift
  addresses resolved to source locations. The archive is retained on Mac-C at
  `build/releases/iOS-1.0.43-33/Muster.xcarchive` in the primary checkout, outside
  `/tmp` and session worktrees. The offline symbolication tool requires matching
  UUID/architecture and demangles both languages. See
  [performance diagnostics](PERFORMANCE_DIAGNOSTICS.md).
- **Tests.** Eight native Swift/log tests, six hosted iOS simulator tests and
  seven Python tests passed. The simulator test burns CPU until the existing
  monitor automatically writes stacks to its exported log. An optimized mixed
  Rust/Swift fixture captured both workers, resumed them and recovered function
  names/source lines using the matching dSYM. Budget-limited walks are explicitly
  tested as partial. Physical iPhone acceptance remains outstanding. GitHub
  reported no CI checks for PR #88; these are local validations.
- **Signing and website.** The archive and separate App Store Connect export
  passed strict signature verification. The distribution export has production
  APNs and debugging disabled; OTA retains the three-device development profile.
  Published through `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to `${MUSTER_RELEASE_ROOT}/ios/`.
  Public readback verified version/build, package hashes, no-store headers,
  latest-manifest redirection and HTTP 410 for the previous IPA. Checked 1.0.42
  artifacts were also removed from the publisher's private staging directory.
- **TestFlight.** Build `a8592d5b-b1ec-553e-8321-55bff7bbdd2f`, version 1.0.43
  (33), returned `VALID` and `IN_BETA_TESTING`, not expired and no non-exempt
  encryption. The existing Internal group includes this build. This was an
  API-authenticated upload; Xcode Cloud remains unconfigured.
- **macOS.** Existing notarized macOS 1.0.41 remains published and its public
  package hash was verified. No new Mac release, notarization or installation was
  performed. The signed desktop feed remains unavailable.
- **Installation/runtime boundary.** The iOS distribution and simulator
  verification do not confirm installation or successful stack capture on the
  user's physical iPhone. Await a schema-2 report from build 33 for acceptance.
- **SHA-256.** OTA IPA:
  `620adc4f5baecfdd43e06c8ae200c08076b274df4ded81b7beddfc1e3804456d`;
  App Store Connect IPA:
  `ce9b92f697d396263b0509c91e0b05b14ce9cb861904ab2ddf09265c5fd6741e`.

### 2026-10-09 Codex composer recognition (1.0.45 / build 35)

- **Source.** `f268e47102f3464abe0781c4b3bfc543c7c01dc1` (merged PR #96).
  Shared Rust recognizes the current Codex `Work with Codex` composer and
  localized variants while retaining the exact AXTextArea guard. The release
  preflight passed with clean source and prior release ancestry. Regression
  fixtures cover the new labels; the source test results are recorded in #96.
- **Build/signing.** Release Rust, native macOS and iOS archive/export builds
  succeeded. The final Mac bundle has both CFBundleShortVersionString 1.0.45
  and CFBundleVersion 35. An initial package with default Xcode bundle version
  metadata was rejected by the installer before replacement; it was corrected,
  signed, notarized again, stapled and republished before any device upgraded.
  Final Apple notarization `b6d502d2-dfe8-4524-bfe4-2709cae64fd1` is `Accepted`;
  staple validation, strict signatures and Gatekeeper assessment passed.
- **Publication.** macOS portable ZIP and paired iOS OTA were published through
  `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to the existing desktop/iOS directories. Independent
  public downloads matched their hashes and embedded versions. The unified
  download page is <https://linsheng.dev:8443/muster/>. Only the notarized
  portable ZIP is offered for Mac; the signed updater feed remains unavailable
  because the existing feed signing key is unavailable.
- **Mac installation/runtime.** M1 Max upgraded from 1.0.36; M4 Pro and M4 mini
  upgraded from 1.0.41. All three report 1.0.45, the source commit above in their
  startup logs, and the same daemon hash below. Installation used the Rust
  desktop update lease and preserved device identities without backing up old
  programs. All three pass strict signature and notarization assessment, have
  accessibility granted, virtual networking active and relay registered.
  Installed system Locked Use plugins match the bundled plugin. Remote `info`
  RPCs from M1 Max to both other Macs passed. Actual conversation sends and
  locked-screen control were not replayed; their end-to-end state is unverified.
- **iOS.** 1.0.45 (35) OTA and App Store Connect exports passed signature checks;
  the latter uses production push and disables debugging. TestFlight build
  `5b037406-6c4c-55aa-9453-1cf44fe4aee9` is `VALID`, `IN_BETA_TESTING`, and
  included in the existing Internal group. External state is
  `READY_FOR_BETA_SUBMISSION`; no public beta review submission or physical
  iPhone installation was performed in this release.
- **SHA-256.** Mac portable ZIP:
  `b93304022e618584a1a23cfc4f022d6a6fb7815379ebbbc41d63e0dadf189196`;
  installed daemon:
  `58954b651930795dc6cc8a42cbe53e2f07a83b0edefd8456c9768835ed2912d3`;
  OTA IPA:
  `b8f643fda511227a49a1eb9dd1048d650b64a9d9faf84b355b9b6349dd61ed40`;
  App Store Connect IPA:
  `8b1aea3a7dc06ab8968171653d46dc592d55729aaaa1191f19f5a1de29057a48`.


### 2026-10-09 subagent viewing (1.0.46 / build 36)

- **Source.** `b0c935a0832a207b1921e68b933ad4e648350943` (merged PR #98).
  Shared Rust discovers Codex and Claude subagents and exposes bounded direct
  parent/child relationships, read-only transcripts, paging and live updates.
  Native macOS, iOS and HarmonyOS views expose these relationships; control
  actions remain denied by Rust for subagent transcripts. The release preflight
  passed against clean source with prior-release ancestry. Open PRs #95 and #44
  were not merged into this release.
- **Checks.** The feature validation recorded 246 passing tests and five ignored
  tests, plus strict Clippy, formatting and layer checks. The existing
  `invitations_require_admin_and_disappear_after_demotion` failure was reproduced
  on unmodified main and remains unresolved; the full suite is not all green.
  Release Rust, native macOS, iOS archive/export and signed HarmonyOS HAP builds
  succeeded.
- **macOS signing/publication.** Bundle version 1.0.46 (36). Apple notarization
  `45f36c8d-e53f-4505-936d-231313475bb7` is `Accepted`; stapling, staple validation,
  strict signatures and Gatekeeper assessment passed. Published through
  `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to `${MUSTER_RELEASE_ROOT}/desktop/`. An independent
  public download matched the final stapled ZIP hash. Only the notarized
  portable ZIP is offered; the signed updater feed remains unavailable because
  its existing signing key is unavailable.
- **Mac installation/runtime.** M1 Max, M4 Pro and M4 mini upgraded from 1.0.45
  through the Rust desktop update lease, without backing up old programs. All
  three report 1.0.46, the source stamp and daemon hash below, retained device
  identities, granted accessibility, active virtual networking and registered
  relay. Installed system Locked Use plugins match their bundled plugins.
  Installed signatures and notarization passed. Remote `info` RPCs from M1 Max
  to both other Macs passed. M4 Pro's initial public download timed out resolving
  DNS before replacement; direct transfer of the identical verified ZIP succeeded.
- **Subagent runtime acceptance.** Read-only RPCs on all three Macs returned real
  Claude and Codex parent/child relationships, read-only flags and five messages
  from a child transcript for each provider. Sample child counts (Claude/Codex)
  were M1 Max 2/23, M4 Pro 15/7 and M4 mini 8/8. No conversation sends were issued.
  This verifies daemon discovery and reading; physical UI navigation, send and
  locked-screen end-to-end behavior were not replayed.
- **iOS.** 1.0.46 (36) OTA and App Store Connect exports passed strict signature
  checks. The store export has production APNs and debugging disabled; OTA uses
  the existing three-device development profile. Published OTA through
  `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to `${MUSTER_RELEASE_ROOT}/ios/`; the public download
  page, manifest and downloaded IPA passed version/hash checks. TestFlight build
  `12c0d099-2d0c-51a6-b0a5-05fbbb7e5459` is `VALID`, `IN_BETA_TESTING`, and present
  in the existing Internal group. External state is `READY_FOR_BETA_SUBMISSION`;
  no external beta review submission or physical iPhone installation was
  performed in this release. The unified Mac/iOS page is
  <https://linsheng.dev:8443/muster/>.
- **HarmonyOS.** Version 1.0.46 (versionCode 46) signed HAP built successfully and
  passed independent HAP signature verification. Published through the same SSH
  host to `${MUSTER_RELEASE_ROOT}/harmony/`; public download hash verified. The
  package at <https://linsheng.dev:8443/muster/harmony/> uses a registered-device
  development profile and requires debugging installation; public hosting does
  not make it a general browser-installable package. No USB target was available,
  so physical phone installation/runtime remain pending.
- **SHA-256.** Mac portable ZIP:
  `245354073f2b93f614e8703913f4e892e651a3b9bacd437bee61a864d79e5f2f`;
  installed daemon:
  `7e57877dd3cdbf205458bf6900d7fe8e15d0425eab12258836af88994e082bad`;
  OTA IPA:
  `ad647092d17ea28a0ba6957c985afb8dfde309be3b8a5505585bc2bd1541beef`;
  App Store Connect IPA:
  `7923b8a1761b32759539b1b9def9410c751acfa74328a78855e4dc8d4492eeac`;
  HarmonyOS HAP:
  `37e536825dd8f1e1fb037782cf730c36cf8b60702de390535c9911858052babb`.

### 2026-10-09 session display follow-up (source fixes; not yet released)

- Move Subagents into the conversation's upper-right actions beside Files/Git
  (toolbar on macOS; the corresponding action menu on mobile), preserving
  same-device Rust-owned child navigation and the parent return action.
- Give the Apple queued-message scroll view a measured, bounded viewport inside
  the fixed-size composer. Its intrinsic-height layout could collapse the
  message area. Native render tests cover one and multiple queued messages.
- Read Claude's persisted enqueue/dequeue/remove events incrementally in Rust
  and publish pending-message changes through the existing interaction stream.
  Claude queue entries are read-only; no unsupported Send now action is offered.
- A sampled M4 Pro Codex child displayed real content in the installed Mac UI.
  Other inspected child records contained metadata and an encrypted compaction
  without readable messages. Shared Rust now reports this specific condition
  through `transcript_notice`; the shells display it instead of a generic empty
  conversation. The notice clears when readable messages arrive. This does not
  decrypt or recover content absent from the local transcript.
- Validation: 169 Rust tests passed across agent unit/service/subagent and
  app-core watch suites; four ignored. The credential-isolation test passed in
  the sandbox; socket tests passed with local socket access enabled. Two native
  queue render tests passed and their images were visually checked. Strict
  Clippy, formatting, layer checks and macOS/iOS UI compilation passed. The
  HarmonyOS shell built an unsigned validation HAP; no new signed release was
  published. Physical acceptance of these source changes remains outstanding.
- **HarmonyOS 1.0.46 installation completed.** The previously published signed
  HAP (SHA-256 `37e536825dd8f1e1fb037782cf730c36cf8b60702de390535c9911858052babb`)
  was installed with replacement on the USB-connected Mate X7, retaining app
  data and UID. After the user unlocked the phone, EntryAbility started
  successfully, bundle readback reported 1.0.46 / versionCode 46 and the app
  process was running. This installed package predates the display fixes above;
  physical cross-device send/view end-to-end acceptance was not performed.


### 2026-10-09 foreground performance and session refresh (1.0.47 / build 37)

- **Source.** `f69aec77fbe6d4a98279bb54694a902a3fda2a9a` (merged PR #101),
  including PR #100. Relay QUIC uses the error-propagating UDP adapter and
  retires cancelled endpoints. Large cache gaps load the newest 500 messages
  after at most two successful history reads per catch-up attempt; small gaps
  retain validated deltas and live events. Connection and catalog-response
  deadlines are separate. Subagents placement, transcript availability notices,
  and Apple/Claude queued-message display fixes from #100 are included.
  Unmerged draft PRs #95 and #44 are excluded.
- **Checks.** PR #101 recorded 93 passing app-core/network tests (two existing
  ignored tests), strict Clippy, formatting, layer checks and an iOS arm64 FFI
  check. macOS/iOS Release Rust, native macOS build and iOS archive/export passed.
  Final native daemon and both FFI archives carry the clean source stamp above.
- **macOS.** Version 1.0.47 (37) is Developer ID signed. Apple notarization
  `f0db28c8-380d-44db-86da-b20ac037acc7` is `Accepted`; the final portable app
  passed staple validation, strict signatures and Gatekeeper assessment.
  M1 Max, M4 Pro and M4 mini upgraded through the Rust desktop-update lease,
  retaining identities/data without old-program backups. All three independently
  passed version, daemon hash/source, signature/notarization, Accessibility,
  active vnet, registered relay and installed Locked Use plugin checks. Read-only
  `info` RPCs from the updated M1 Max to both other Macs passed. No live prompts
  or locked-screen conversation operations were replayed.
- **iOS.** The 1.0.47 (37) archive and separate App Store Connect export passed
  signing checks; the store export has production APNs and debugging disabled.
  TestFlight build `ce42e653-69dd-58c9-9375-89e1b68e60ce` is `VALID`,
  `IN_BETA_TESTING`, and present in the existing Internal group. It is also in
  Public Beta with English/Chinese testing notes. External review was submitted
  and read back as `WAITING_FOR_REVIEW` / `WAITING_FOR_BETA_REVIEW`; public beta
  availability awaits Apple approval. Physical iPhone installation and lower
  CPU/memory or faster refresh on-device have not yet been verified.
- **Symbols.** The archive and matching dSYMs are retained outside temporary
  worktrees at `build/releases/iOS-1.0.47-37/Muster.xcarchive` in the primary
  checkout for offline performance-log symbolication.
- **Website.** Published through `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to the established
  desktop/iOS directories. Independent public downloads matched both hashes
  and embedded versions. The unified page and fixed OTA manifest report build
  37, use no-store headers, and the old build-36 manifest resolves to build 37.
  The signed desktop updater feed remains unavailable; Mac distribution uses
  the notarized portable ZIP. HarmonyOS remains at its installed 1.0.46.
- **Cleanup boundary.** Automatic approval review blocked deleting six old
  hash-named IPA/manifest files for 1.0.43, 1.0.45 and 1.0.46, even after a
  read-only dry-run confirmed the scope. Those historical public files remain
  pending separate user authorization; current aliases offer 1.0.47. No source,
  app data or current release was removed by that blocked cleanup.
- **SHA-256.** Mac ZIP:
  `55ff2b40f24744920a63079f39bbb4f4b54761b63c9c9917e95b5540747ed660`;
  installed daemon:
  `f7ea07e429bc8a2ea20254cc2eb2e0bcc63b3a740f7c5e8259ef71a95623d350`;
  OTA IPA:
  `e3e7ffb2d8707f3b6212691fe72f67f0a72f6d8168149cf365d5298dfeea2f75`;
  TestFlight IPA:
  `ada70805a6429efa98c91d5054a95d573ffae725fabb7e2b7f1097b07720e290`.

### 2026-10-10 independent Accessibility step timeouts (1.0.48 / build 38)

- **Source.** `fed1d1c0edc1708a188517c99fd07eb0c1c7e04e`, including merged
  PRs #103–107: native model/effort controls, Locked Use requirements for Chat
  loading/sending, optional-question panel handling, and independent deadlines
  for Accessibility workflow steps. Apple project settings now share marketing
  version 1.0.48 and build 38; this also fixes Xcode Cloud continuing to upload
  the obsolete 1.0.44 marketing version.
- **Checks.** Rust release builds for the native daemon/CLI/browser and both
  macOS/iOS FFI targets passed. The daemon and both FFI archives carry the same
  clean source stamp above. Native macOS Release build, iOS archive and App Store
  Connect export passed. Project/plist validation and whitespace checks passed.
  This packaging turn did not rerun the unit suites recorded for PRs #103–107.
- **macOS.** Developer ID signed 1.0.48 (38); Apple notarization
  `bebd4c14-5645-4a08-b292-93521ec280a5` is `Accepted`. The final portable app
  passed staple validation, strict/deep signature verification, Gatekeeper and
  the release ancestry/fingerprint guard. The notarized ZIP is published.
  The signed desktop updater feed remains unavailable; distribution uses the
  portable download. Existing Mac installations were not upgraded in this turn.
- **iOS / Internal.** OTA 1.0.48 (38) is signed for the three registered devices.
  The separately exported App Store Connect IPA passed signature validation,
  has production APNs and debugging disabled. TestFlight build
  `69943df9-2d42-58ae-8f9b-bd7464fb8581` is `VALID` / `IN_BETA_TESTING` and
  membership in the existing Internal group was independently read back.
- **Public Beta.** The same build is present in Public Beta, with English and
  Simplified Chinese testing notes. Apple review was submitted and read back as
  `WAITING_FOR_REVIEW` / `WAITING_FOR_BETA_REVIEW`; automatic tester notification
  is enabled. Public installation of 1.0.48 still requires Apple approval.
  The existing public link is https://testflight.apple.com/join/Fh6ZkP8a.
- **Website.** Published via `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to the established desktop
  and iOS directories. Public downloads, both embedded versions, fixed/hash IPA
  aliases, latest page and no-store headers passed independent verification.
  The eight superseded IPA/manifest files from 1.0.43, 1.0.45, 1.0.46 and 1.0.47
  were removed after a scoped dry-run and latest-package validation. All four
  historical IPA URLs return 410; historical manifests resolve to build 38.
  This completes the old-public-file cleanup left pending by earlier releases.
- **Installation / runtime / end-to-end.** No new live Mac installation, physical
  iPhone installation or conversation replay was performed in this release turn.
  Server distribution and signing checks do not establish device-level behavior.
- **Symbols.** The archive/dSYMs and release evidence are retained outside linked
  worktrees at `build/releases/Apple-1.0.48-38/` in the primary checkout.
- **SHA-256.** Mac ZIP:
  `76ec806099b86edede488144d77d022ef85105c5cb9a3a102571537d8b2b4931`;
  OTA IPA:
  `0c50d4ffd9f7c2db27adf4ee0dfed6afcc56852ac4b49c8560ae4bc00699af1a`;
  TestFlight IPA:
  `74a4a1237e0764cd4daf527266ed3950c0884d49af4ee10946160510483b5a3d`.

### 2026-10-10 terminal paste publication and Mac installation (1.0.49 / build 39)

- **Source.** `7705cefe3f0956440de693f45583f627a1a69479`, including merged
  PR #109. iOS/Android terminal Paste stays visible with either keyboard or
  the keyboard hidden; macOS adds an explicit toolbar Paste button. Clipboard
  input still uses the existing Shared Rust intent and validation. Apple
  marketing/build settings are 1.0.49 (39).
- **Code/tests/build.** PR #109 passed iOS and macOS UI compilation and the two
  existing Rust paste tests (Unicode/newlines/control filtering, SSH bracketed
  paste and size limits). This release passed native daemon/CLI/browser and
  both Apple FFI release builds, native macOS Release build, iOS archive and
  App Store export. Project/plist and whitespace checks passed. Android source
  is included, but no Android package was built; the local SDK is unavailable.
- **Signing/notarization.** macOS Developer ID signatures, staple validation,
  strict/deep signature verification, Gatekeeper and the release source guard
  passed. Apple notarization `4e20cdf6-27ad-45dd-9e78-7d0f965516c6` is `Accepted`.
  iOS OTA signing is valid for three registered devices; the separate store IPA
  has production APNs and debugging disabled and passed signature validation.
- **Website.** Both platforms were published through `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}`
  to the established directories. Public downloads verified both embedded
  versions, hashes, fixed/hash IPA aliases and no-store headers. The two old
  1.0.48 IPA/manifest files were removed after dry-run and new-package validation;
  the old IPA returns 410 and its manifest resolves to build 39. The signed
  desktop updater feed remains unavailable; distribution uses the notarized ZIP.
- **TestFlight.** Build `c2bf8b60-fe76-4429-965d-0e0f48d37641` is `VALID` and
  `IN_BETA_TESTING` in Internal. The same build is assigned to Public Beta with
  English/Chinese notes and auto-notification enabled. Public review is
  `WAITING_FOR_REVIEW` / `WAITING_FOR_BETA_REVIEW`; public 1.0.49 installation
  awaits Apple approval. The uploader retried transient Apple storage TLS
  failures and ultimately reported successful delivery.
- **Installation.** M4 Pro and M4 mini upgraded from 1.0.47 via the Rust desktop
  update lease, preserving device identities/data and without retaining old
  program backups. Both installed bundles report 1.0.49 (39), have the expected
  daemon hash/source and pass signatures/Gatekeeper. M1 Max was unreachable
  through its old forwarded SSH port and registered vnet address, including a
  final retry; no M1 Max installation is claimed.
- **Runtime/end-to-end.** Both updated Macs have running App/daemon processes,
  granted Accessibility, active vnet and registered relay. Read-only `info`
  from M4 mini to M4 Pro returned the updated device/version successfully.
  Physical iPhone installation and actual terminal paste UI execution were not
  exercised; these checks do not establish paste end-to-end acceptance.
- **Symbols/evidence.** Matching archives/dSYMs, notarized app and verification
  logs are retained outside linked worktrees in primary checkout
  `build/releases/Apple-1.0.49-39/`.
- **SHA-256.** Mac ZIP:
  `b08234cef70a6ac8d52d525cbbe98154016bd97a03aed973407cf8ffd467c08c`;
  installed daemon:
  `cd11c034edd18ca49dc09365d9b08f13ba0f18e8e29c9520fa337750e3f7280b`;
  OTA IPA:
  `7b97afa1be9c6f550a6a8ce091a19bac0c24151ef995a51712f78b4256184b58`;
  TestFlight IPA:
  `699128aa434a648b6dca28e8878670ca1074a87dc9490f12e94251669dc8ae7a`.

### 2026-10-10 — 1.0.50 (41), mobile draft input and manual Locked Use

- **Code.** Released source `04f6cc082a92e81fc1fb1ecbf25034fc393c656a`
  (PR #111). Codex Code/Chat draft entry now uses focused selection replacement
  with exact fresh-echo confirmation. The mobile device list offers an explicit
  signed, host-maintained Locked Use cover, independent of Accessibility.
  Phone backgrounding does not stop the manual lease; local takeover,
  cancellation, revocation or failed protection relocks before uncovering.
  Cloud requests remain independent; owner terminal I/O keeps its own grants
  and drain barrier. See `LOCKED_USE.md` for the trust/lifecycle contract.
- **Tests/builds.** 43 core gate, 160 Agent, 58 app-core, 9 protocol, 45 daemon,
  and 2 watchdog tests passed (317 distinct selected tests; four existing tests
  ignored). Clippy with warnings denied, rustfmt and the 16-crate layer check
  passed. Final macOS/iOS Release builds passed. HarmonyOS SDK 26 ArkUI and
  unsigned HAP packaging passed using the existing Rust archive; no HarmonyOS
  release/install is claimed. Android protobuf configuration was fixed, but
  compilation remains blocked by the absent local Android SDK.
- **Signing/notarization.** Developer ID signing, strict/deep verification,
  staple validation, Gatekeeper and release-source verification passed.
  Apple notarization `7b4ce680-b334-493c-80af-1fce9a600982` is `Accepted`.
  OTA signing covers three registered iOS devices. The separate TestFlight IPA
  has production APNs, debugging disabled, and passed signature validation.
- **Website.** Both platforms were published through `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}`.
  Public downloads verified version/build, hashes, fixed/hash aliases and
  no-store headers. Superseded 1.0.49 IPA/manifest files were removed after
  dry-run; the old IPA returns 410 and its manifest resolves to build 41.
  The signed desktop updater feed remains unavailable; the notarized ZIP is
  the desktop distribution path.
- **TestFlight.** Build `f1f75afa-acd7-43dc-bb42-d20bd541774d` is `VALID`, assigned
  to Internal and Public Beta with English/Chinese notes and auto-notification.
  Internal is `IN_BETA_TESTING`. Public review was submitted and is
  `WAITING_FOR_REVIEW` / `WAITING_FOR_BETA_REVIEW`, so public installation still
  awaits Apple approval. Apple storage TLS errors retried successfully. This
  turn's superseded intermediate build 40 was not published to the website and
  is expired after final build 41 became available.
- **Installation/runtime.** Mac-C and Mac-B upgraded from 1.0.49 to 1.0.50 (41)
  through the Rust desktop update lease, preserving identity and data without
  keeping old program backups. Installed source/hash/signatures/Gatekeeper,
  running App/daemon, granted Accessibility, active vnet and registered relay
  were verified on both. A real read-only Mac-C → Mac-B info RPC succeeded and
  reported the new `locked.device.v1` capability. Mac-A remains unreachable via
  both its forwarded SSH port and registered vnet address; no installation there.
- **End-to-end limits.** Physical iPhone installation, repeated real native sends
  and real manual-cover switching have not been exercised. Simulated gate/owner
  tests and installed runtime checks do not establish those UI acceptance results.
- **Evidence.** Final archive/dSYMs, notarized app, hashes and verification logs
  are under `build/releases/Apple-1.0.50-41/`; reusable Xcode derived data remains
  under the intermediate `Apple-1.0.50-40/` directory.
- **SHA-256.** Mac ZIP:
  `46fad92c455d70515fa69075f48e80cb7d89fc9aa932b0183a771474c91309b7`;
  installed daemon:
  `0eb036ea14fdd05fa8104945c1bc1fca2768fa3086a9a64ae38e2d3aacd55746`;
  OTA IPA:
  `477324806265db38f453391930c2c8a508ef6270bf2486bf90a86615367182fa`;
  TestFlight IPA:
  `c416fe14fcad498c5cdd0f209180723d581b914fd86aea5a57d5509d5900bf0a`.

### 2026-10-10 — 1.0.51 (42), evidence-based member presence

- **Code.** Released source `84428431dc0a8f31c2339adde4c15b2b07f1ca34`
  (PR #112 plus the version bump). Shared Rust `muster_net::presence` owns
  member online state. A live link or recent peer traffic makes a member
  online; a lost link or failed dial is `reconnecting` for at most two bounded
  retries before it is offline, so one RPC timeout no longer flashes a Mac
  offline. An explicit relay `ABSENT` marks the member offline immediately;
  concurrent callers share that verdict and nobody redials or polls it until
  `PRESENT`, new traffic, a network change or a user refresh. Offline members'
  sessions are hidden without deleting cached data. Member QUIC keepalive is
  5 s with a 15 s idle timeout. On a network change the phone verifies a live
  QUIC relay carrier with one probe instead of resetting it. See
  `ARCHITECTURE.md` §3.5.
- **Tests/builds.** PR #112 added presence state-machine, loopback relay/node
  and app-core tests (one lost link then recovery, retries exhausted, absent
  member not redialed by concurrent callers, late answers, new-traffic
  recovery, background/foreground, offline session filtering). The full
  workspace test run, clippy with warnings denied and rustfmt passed. The
  existing `node::recovery_tests` and secure-relay failover tests are flaky
  under parallel load on `origin/main` as well. Final macOS/iOS Release builds
  passed.
- **Signing/notarization.** Developer ID signing, strict/deep verification,
  staple validation, Gatekeeper and release-source verification passed.
  Apple notarization `9a1130b0-8142-43c9-9c85-34316e29f8a7` is `Accepted`.
  OTA signing covers three registered iOS devices. The separate TestFlight IPA
  has production APNs, debugging disabled, and passed signature validation.
- **Website.** Both platforms were published through `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}`.
  Public downloads verified version/build, hashes, fixed/hash aliases and
  no-store headers. Superseded 1.0.50 IPA/manifest files were removed after
  dry-run; the old IPA returns 410 and its manifest resolves to build 42.
  The signed desktop updater feed remains unavailable; the notarized ZIP is
  the desktop distribution path.
- **TestFlight.** Build `5a135177-8b46-400c-9e63-658fb3dd5cfe` is `VALID`, assigned
  to Internal and Public Beta with English/Chinese notes and auto-notification.
  Internal is `IN_BETA_TESTING`. Public review was submitted and is
  `WAITING_FOR_REVIEW` / `WAITING_FOR_BETA_REVIEW`, so public installation
  awaits Apple approval.
- **Relay.** A `muster-relay` built on the server from the same source is staged
  at `~/muster-relay-84428431/muster-relay` but not installed; installation
  needs the operator's sudo. Production still runs the 2026-09-29 relay, which
  neither sends `ABSENT`/`PRESENT` nor answers `WATCH`. Until it is replaced,
  clients rely on bounded retries, then `unreachable` with a 60 s probe.
- **Installation/runtime.** Mac-C and Mac-B upgraded from 1.0.50 to 1.0.51 (42)
  through the Rust desktop update lease, preserving identity and data without
  keeping old program backups. Installed source/hash/signatures/Gatekeeper,
  running App/daemon, granted Accessibility, active vnet and registered relay
  were verified on both, and a read-only Mac-C → Mac-B info RPC succeeded.
  On Mac-C the offline Mac-A went from two failed dials to `unreachable`
  within 5 s; Mac-B's daemon restart during its own upgrade showed two failed
  retries, `unreachable`, then online 4 s later on its new link. The relay's
  dropped forwards fell from about 75 to about 33 per 30 s once both Macs ran
  1.0.51. Mac-A remains unreachable via both its forwarded SSH port and
  registered vnet address; no installation there.
- **End-to-end limits.** The physical iPhone has not installed 1.0.51; its
  online/offline display, session hiding and background/foreground behavior
  are unverified on the device. The relay `ABSENT` path is unexercised in
  production until the new relay is installed.
- **Evidence.** Archive/dSYMs, notarized app, hashes, release scripts and
  verification logs are under `build/releases/Apple-1.0.51-42/`.
- **SHA-256.** Mac ZIP:
  `3ebff997335c33f098ac40c3889a5d248f4f3a453906222103d517c5d9ed2f77`;
  installed daemon:
  `8798fc6a18499f49a7dd19f6f72ea7d6c3bc6c73255a1c702ab1b5016b41d6a9`;
  OTA IPA:
  `1704418677d1a267f029fa5c8fc696f199690ada5a3fc61bfe36ddb79d9321fc`;
  TestFlight IPA:
  `ecb14b4fa5eec5eec7e4c99f1152b76dc618f0a06347c78608e8356e194d4cd9`;
  staged relay:
  `42766f2f0627b30163c1f186e7b9f8acae6251c4d7321f1365f7511c33231715`.

### 2026-10-12 — 1.0.52 (43), native CLI integration

- **Source.** Release `de8828751b3506ad07475bc2de7f60811564b425` contains
  PRs #114–#116: the embedded Shared Rust CLI runtime, native session rendering,
  real-diff normalization and receipt-based composer clearing. All 21 recorded
  desktop release commits are ancestors. The user authorized deployment while
  Claude account-dependent acceptance remains open; the limits in
  `validation/CLI-LIVE-2026-10-12.md` still apply.
- **Build/signing.** Release SEP daemon, macOS/iOS Rust archives, Apple Silicon
  Developer ID Mac App and iOS archive built successfully. Strict/deep signatures,
  Mac source/fingerprint verification, and both version/build values passed.
  The iOS development profile covers the same three registered devices.
- **iOS OTA published.** Published the six OTA files through
  `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to `${MUSTER_RELEASE_ROOT}/ios/`, immutable files first
  and the installation page last. Existing immutable releases were retained.
  The public unified page's actual install link, manifest and hash-named IPA
  resolve to 1.0.52 (43); the fixed IPA alias matches. Page/manifest/IPA responses
  advertise no-store. IPA SHA-256:
  `a58eefe886c2ba32a26d91d64f5cdfbd0f4ac0f15732a625aadcf599e87a1572`.
- **Mac installation/runtime.** Mac-A, Mac-C and Mac-B all installed 1.0.52
  (43) using the Rust desktop update lease. The signed Velopack 1.2.158 updater
  was preserved in the bootstrap bundle. No old program backups were retained;
  identity, pairing and user data were preserved. All three installed bundles
  pass strict/deep signature verification, report `agent.orca.v1`, and run the
  new App and daemon with Accessibility granted, vnet active and relay
  registered. Mac-A-to-Mac-C and Mac-A-to-Mac-B read-only info RPCs passed;
  both links reported direct. Installed daemon SHA-256 on all three:
  `f3c816e811e22113f456368773f4d4e3ae6d4708000526e0c32469b18af8fff2`.
- **Notarization/public Mac download.** After the user restored Mac-C Remote
  Login disk access, Apple accepted submission
  `dac2dbe5-26b8-5986-b523-458b5d75214e`. The exact bootstrap installed on the
  three Macs was notarized; its pre-stapling ZIP SHA-256 was
  `6340cc29a01f65a10d39c12eccee640e651fc884782b520a0765fc4edf7740f1`.
  All three installed Apps now have valid stapled tickets and pass strict/deep
  signature verification and Gatekeeper (`Notarized Developer ID`). The App and
  daemon remain online, with vnet and relay registered; remote info RPCs pass.
  The stapled ZIP and checksums replaced the public desktop download through
  `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}`; the unified page now shows 1.0.52 for both platforms.
  A fresh public download passed version/build, SHA-256, ticket, signature and
  Gatekeeper checks. Public Mac ZIP SHA-256:
  `0db56bd7703ac2fdfeb99e4817739eb581a62fdd02ab8596c2c1f7cc0fe2d1ec`.
- **TestFlight.** App Store export passed strict/deep signature verification,
  production APNs and disabled-debugging entitlement checks. IPA SHA-256:
  `8cb7150a94ac069e62fa9b6576963ba5717c9d2561670c6d3a9b8860f5f18542`.
  Upload completed successfully and build
  `385fb42b-722e-504f-9cb6-e91e957d698d` is `VALID`; internal state is
  `IN_BETA_TESTING`. Build 43 belongs to the existing Internal and Public Beta
  groups, with English/Chinese release notes and notifications enabled.
  External review submission was rejected with
  `ENTITY_UNPROCESSABLE.ANOTHER_BUILD_IN_REVIEW`: 1.0.52 build 25
  (`b5d23e5d-063f-5447-955a-45da4be925ea`) is already
  `WAITING_FOR_BETA_REVIEW`. That separate submission was left intact.
  Build 43 remains `READY_FOR_BETA_SUBMISSION`; external availability is not
  established and submission must be retried after the other review finishes.
- **Acceptance limits.** Physical iPhone installation and rendering acceptance
  remain unverified. The previously recorded Claude account hold and outstanding
  CLI acceptance cases remain open; release publication does not clear them.
- **Evidence.** Local source worktree `/private/tmp/muster-release-native-cli`;
  build logs, OTA files, downloaded public artifacts and
  `public-release-final.json` are under `build/releases/Apple-1.0.52-43/`.
  Mac-C release evidence is in
  `/Volumes/Workspace/Projects/Muster/build/releases/Apple-1.0.52-43/`, including
  `notary.json`, the stapled `desktop/` package and App Store export/upload logs.
  No Keychain unlock command was used and no Keychain password was requested.

### 2026-10-12 — 1.0.53 (44), iPhone CLI acceptance and publication

- **Source and tests.** Clean source `ba1f2310725826cbd4778983fa9fc9d6bcee8a6d`
  (PR #120, merged as `6fb7960182ed07e16ccbdcf0fbd40c64845802ae`) prefers the
  desktop app's native Codex, displays bounded provider errors, removes duplicate
  desktop discovery of Muster-owned CLI sessions, and restores durable activity
  time after restart. The release source/fingerprint check passed. Local agent
  tests passed: 169 unit tests across the sandboxed suite and isolated socket
  test, plus 7 protocol tests; 4 opt-in tests stayed ignored. Strict agent Clippy,
  workspace formatting, macOS Release and iOS archive builds passed. GitHub
  reported no CI checks on the implementation PR; no CI pass is claimed.
- **Physical iPhone acceptance.** The final 1.0.53 (44) candidate was installed
  on the paired iPhone and Mac-A. A phone-created Codex CLI session read and
  patched a test file; Chinese Markdown, a table, fenced code, numbered red/green
  diff rows and code copying were inspected on the physical screen. Interruption,
  continuing after interruption, and recovering the same session/history after
  App/daemon replacement passed. The final resumed command read the file and
  returned `IPHONE_CLI_OK` on the phone. The restored list contained one CLI
  entry with its original activity time. These results cover Codex, not Claude
  inference or live approval-card interaction; protocol fixtures cover approvals.
- **Notarization.** Apple accepted submission
  `93016533-ef08-4765-bfc4-bdb4688f9e48`. Pre-stapling ZIP SHA-256:
  `1269915c72923a369bb1e7f26475b26044b59e74ff4da8a3c73842c08c682230`.
  The stapled app passed strict/deep signature, ticket validation and Gatekeeper
  (`Notarized Developer ID`). Public ZIP SHA-256:
  `b6dcd270f0a8b806ff2bff7ebbc7b7faecefd37809fc994c84ea63ac3a126273`.
- **Publication.** The Mac portable download and all six iOS OTA files were
  published through `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to the existing desktop/iOS
  directories. Immutable OTA files were published before aliases and the page;
  previous immutable files were retained. The public page's actual install URL,
  manifest and IPA resolve to 1.0.53 (44), and the page's Mac download passed
  version/hash/signature/ticket/Gatekeeper checks. OTA IPA SHA-256:
  `8618dc9afccf28eeeada242b0c15bb34b8f175a318fa224abb0018eef1f71eb6`.
- **All Mac devices.** Mac-A, Mac-C and Mac-B installed the exact notarized
  distribution using the Rust update lease, with no old-program backups and
  unchanged device identities/pairing. Mac-B's automatic relaunch did not restore
  its daemon within the installer's timeout; the user opened Muster, after which
  installation and runtime checks passed. All three report 1.0.53 (44),
  Accessibility granted, vnet active and relay registered. Installed signatures,
  stapled tickets and Gatekeeper pass; Mac-A's read-only remote info RPCs to both
  other Macs succeed. All three daemon hashes are
  `edf7f7012ed609255401250a024054ac0e884df2c5b384fae0e1124340cd1ead`.
- **TestFlight.** The App Store export passed strict/deep signatures, production
  APNs and disabled-debugging checks. IPA SHA-256:
  `cf361718da1b5f827fc7cece09f2a464635920a09d433a46653e7f648e530381`.
  Build 44 (`4780ddf8-59a1-51f9-aecd-0d4cd993a57c`) is `VALID` and
  `IN_BETA_TESTING`, with confirmed membership in the existing Internal and
  Public Beta groups and English/Chinese release notes. External review was
  rejected with `ENTITY_UNPROCESSABLE.ANOTHER_BUILD_IN_REVIEW`: the same 1.0.53
  train's build 27 (`37597c3b-8044-523d-a4f5-d7ae435af28a`) is
  `WAITING_FOR_BETA_REVIEW`. Its review was left intact. Build 44 remains
  `READY_FOR_BETA_SUBMISSION`; Public Beta group membership does not establish
  external availability.
- **Evidence.** Final artifacts, public download checks and per-device JSON
  records are under `/private/tmp/muster-iphone-cli-validation/build/releases/Apple-1.0.53-44/`.
  Apple submission/export records are under Mac-C's
  `/Volumes/Workspace/Projects/Muster/build/releases/Apple-1.0.53-44/`.
  No Keychain unlock command was used and no Keychain password was requested.


### 2026-10-12 — 1.0.54 (45), whole-composer replacement

- **Source and builds.** Clean release source
  `f2285fd396f1539a9d4ba749c661e218b6cc5b27` includes PR #122
  (`66058a78c6efd623418aa9e42206eeabe28779aa`). The shared Rust Codex
  profile replaces the complete composer value and still requires an exact
  draft echo before submitting. The implementation's 169 agent unit tests and
  layer check passed (4 opt-in tests ignored). Release Rust/FFI builds,
  macOS Release build, iOS archive and release ancestry/fingerprint preflight
  passed. This release turn changed Apple version metadata and this ledger.
- **Final macOS package.** Velopack 1.2.158 produced the complete bundle,
  including `sq.version` and `UpdateMac`. Apple accepted notarization
  `c75e3433-1e50-4063-9c33-489dd854c845`; the stapled app passed strict/deep
  signature, ticket validation and Gatekeeper (`Notarized Developer ID`).
  Public ZIP SHA-256:
  `465384d840e0c67ea0022391ff64e2e8531af8da8ea041b2195365f0656d0ba5`.
  An intermediate raw Xcode package lacked updater metadata; it was replaced
  in the public download and on Mac-B by this final Velopack package.
- **Publication.** Final macOS ZIP/checksum and the six iOS OTA files were
  published through `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to the existing desktop/iOS
  directories. Independent public downloads match the final hashes and
  1.0.54 (45) metadata; the actual page install URL resolves to the expected
  OTA manifest and IPA. OTA IPA SHA-256:
  `edc048dfa6fdc9e282fef79e5afa11714407ebd3c188e64813ce6a2512d771e3`.
- **Mac installations.** Mac-C and Mac-B installed the final notarized package
  through the Rust update lease, without old-program backups. Their device
  identities are unchanged; both report 1.0.54 (45), Accessibility granted,
  active vnet and registered relay. Signature/ticket/Gatekeeper checks and
  Mac-A-to-device info RPCs passed. Both daemon hashes are
  `2c08a0c0e68b867e78027056ba37f7580d9ada054c5631d01fd68629510440c8`,
  with release source stamp `f2285fd396f1539a9d4ba749c661e218b6cc5b27`.
  Mac-A remains on 1.0.53 (44): its update lease was denied because an earlier
  protected terminal session has not drained. The installer left its app and
  data untouched; ending that session is required before installation.
- **TestFlight.** The App Store export passed signature, production APNs and
  disabled-debugging checks. Store IPA SHA-256:
  `13c39f4bfe895d1d5880ea5e4a561cad9e707ecb3c5e1ca2a2048ba842b446b4`.
  Build 45 (`30c10666-3be9-597a-84d3-14f966063178`) is `VALID`,
  `IN_BETA_TESTING`, and belongs to Internal and Public Beta groups with
  English/Chinese notes and notifications enabled. External submission is
  `WAITING_FOR_BETA_REVIEW`; external availability is not yet established.
  Existing build 44's review was left unchanged.
- **Acceptance limits and evidence.** Live Codex composer/send E2E and physical
  iPhone installation were not verified in this release turn. Local artifacts,
  public downloads and per-device records are under
  `/private/tmp/muster-release54/build/releases/Apple-1.0.54-45/`;
  Apple records are under Mac-C's
  `/Volumes/Workspace/Projects/Muster/build/releases/Apple-1.0.54-45/`.
  No Keychain unlock command was used and no Keychain password was requested.

### 2026-10-12 — 1.0.55 (46), recovery from Codex settings

- **Source and builds.** Clean release source
  `d4d81b889ef4a6b16843594409b1dcc5f80f4b1a` includes PR #124
  (`51f4e5b9459e4b436a3494f25a605044df968d2d`). Shared Rust returns from
  settings through a uniquely identified Back to app control, with a bounded
  Home fallback scoped to the application sidebar, and then selects the exact
  target conversation before editing the composer. Synthetic regression tests
  cover the observed failure shape; these are not captured live AX fixtures.
  Targeted tests (20 passed, 1 ignored), platform check, strict Agent/Platform
  Clippy, formatting and 16 architecture checks passed in the implementation
  turn. The full agent suite had 172 passes, 1 failure and 4 ignored tests:
  `credentials_follow_the_profile_paths_and_expiry` also failed on the unchanged
  previous release because the local Keychain supplied credentials after the
  test removed its file. No full-suite pass is claimed. Rust/FFI release builds,
  macOS build, iOS archive and clean-source release preflight passed. An initial
  Apple timestamp-service failure cleared on retry.
- **macOS signing and publication.** Velopack 1.2.158 included `sq.version`
  and `UpdateMac`. Apple accepted notarization
  `4f005d7f-9bc4-4c85-b901-8a0b89d81e9e`. The final stapled app passed
  strict/deep signatures, ticket validation and Gatekeeper (Notarized Developer
  ID). ZIP SHA-256:
  `fd7aa9fd7074163e4d02576a3c25cf0036f30064ab6cf9f2119676245666b8b0`.
  The portable ZIP/checksum and six iOS OTA files were published through
  `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to the existing desktop/iOS directories. Independent
  public downloads matched hashes and 1.0.55 (46) metadata, including the actual
  page install URL, manifest and immutable IPA. OTA IPA SHA-256:
  `791497ef4e0f6cb24dd8c3e4813b2733e91662afb10f14efa32583755ddac187`.
  Existing immutable artifacts were retained; no desktop update feed is claimed.
- **Mac installations.** Mac-C and Mac-B acquired the Rust update lease,
  installed the final notarized bundle without old-program backups and retained
  their device identities. Both run 1.0.55 (46), with Accessibility granted,
  active vnet and registered relay; signature/ticket/Gatekeeper and Mac-A-to-peer
  info RPCs passed. Installed daemon SHA-256:
  `3d7c2cf781420607904f32bc996da3dc6e367ee981e75df94fcc71e28717ff34`.
  Mac-A remains on 1.0.53 (44): a lease-only check returned "Mac protection has
  not finished". Its application was not changed; the user was asked to end
  the protected session and restore the normal console before installation.
- **TestFlight.** The App Store export passed strict/deep signature, production
  APNs and disabled-debugging checks. Store IPA SHA-256:
  `29333ee7f7edcf106268dd77cba55bff0d5d29450dd46e53959a1e8f72e60483`.
  Upload completed with delivery UUID `173efe34-1fda-5b3b-999f-d9cdf9fe04b1`.
  Build 46 (`173efe34-1fda-5b3b-999f-d9cdf9fe04b1`) is `VALID` and
  `IN_BETA_TESTING`, with confirmed Internal and Public Beta membership and
  English/Chinese release notes. External review submission was rejected with
  `ENTITY_UNPROCESSABLE.SUBMISSION_LIMIT_REACHED`; no existing review was
  withdrawn, and external availability is not established.
- **Acceptance and evidence.** Actual iPhone-to-Codex sending, settings-page
  recovery and physical iPhone installation were not verified in this release
  turn. Installation and remote info do not establish send E2E. Local artifacts,
  public checks and device records are under
  `/private/tmp/muster-release55/build/releases/Apple-1.0.55-46/`; Apple records
  are under Mac-C's
  `/Volumes/Workspace/Projects/Muster/build/releases/Apple-1.0.55-46/`.
  No Keychain unlock command was used and no Keychain password was requested.


### 2026-10-14 native send reliability (desktop 1.0.56 / iOS build 47)

- **Source and checks.** Built from clean commit
  `0739de95dedb9ab1989bf7d49decdfb6fecd96e7`, containing PR #126's
  `4b0aca63f4567128b296098b36fc889c4e63b49e` merge. That repair filters explicitly
  disabled duplicate submit controls, refreshes/focuses the exact composer
  before draft replacement, waits for brief input activity during read-only
  preparation, and reconciles late provider receipts into the operation ledger.
  The repair turn passed 247 local tests, formatting and static checks; the
  release turn passed both Apple Release builds and `verify-release.sh`.
- **Signing and publication.** Developer ID signatures, Apple notarization
  (`ef25c9c0-efa5-4d5a-a052-880114d98959`), ticket stapling and Gatekeeper passed.
  Published through `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to the existing desktop/iOS
  directories. Public page-to-manifest-to-IPA downloads and the desktop download
  match 1.0.56 (47), their hashes and the required no-cache headers. Desktop ZIP
  SHA-256: `64f4614cd08403bf8e26259b888b93417ba630b5a6a3a86fd1383398354b09d4`.
  OTA IPA SHA-256: `0fa8031f1b7112ff4a30f80fda704f178dd6ab0524688cc27712dab7096625ea`.
  No desktop update feed was published. After explicit user confirmation,
  `prune-releases.py` removed the ten listed superseded iOS packages/manifests;
  the previous IPA returns 410 and its manifest resolves to build 47.
- **Three Mac installations.** Mac-A, Mac-B and Mac-C acquired the Rust update
  lease and installed the notarized portable bundle without old-program backups.
  All retain their device identities and run 1.0.56 (47), with Accessibility
  granted, active vnet and registered relay. Installed signature, stapled ticket
  and Gatekeeper checks passed on every Mac. Peer info RPCs also passed.
  Daemon SHA-256:
  `126b3bd5daf1cc2714e01fb6d3224f41f906abf849a66451a833c4ca98d9adb2`.
- **Internal TestFlight only.** The App Store IPA passed strict signatures,
  production APNs and disabled-debugging checks. SHA-256:
  `07572b1b09391091e31f8b441716e56e1772bdeeddaf6fde8b8290ebe6783ccf`.
  Build 47 (`793443c9-4fb7-5fff-a0e2-c9e832be1869`) is `VALID` and
  `IN_BETA_TESTING`. API read-back confirms membership only in Internal;
  no external/Public Beta group was added and no external review submitted.
  External state remains `READY_FOR_BETA_SUBMISSION`.
- **Native sending: all three Macs passed.** Installed-daemon `agent_send`
  RPCs drove the actual Codex native composer on Mac-A, Mac-C and Mac-B. Each
  had an exact draft echo, successful button press, provider admission and an
  `applied` ledger row. Each idle conversation contains exactly one user test
  message and its exact assistant marker. Identical operation replay returned
  `duplicate` without another user message. Mac-B additionally passed native
  queue admission and duplicate replay while a separate task was running;
  final-reply acceptance used an idle session instead of interrupting that task.
  The original Mac-C failure session had an unsent draft, which was preserved;
  a different idle session was tested. Mac-B initially rejected attempts before
  writing because of continuing HID keyboard/mouse events. After the user paused
  local input, sending passed with the normal protection unchanged. The extra
  busy-session diagnostic message was left in the native queue.
- **Evidence and limits.** Artifacts and device acceptance records are under
  `/private/tmp/muster-release56/build/releases/Apple-1.0.56-47/`; Apple upload
  records are under Mac-C's
  `/Volumes/Workspace/Projects/Muster/build/releases/Apple-1.0.56-47/`.
  The iPhone was unavailable, so physical iOS installation and phone-authorized
  Locked Use sending are not verified. Long-delayed receipts and Claude sends
  were not reproduced in this release acceptance. Historical unknown ledger
  rows from before the repair are not retroactively reconciled.


### 2026-10-16 Claude card compatibility (desktop 1.0.57 / iOS build 48)

- **Source and builds.** Clean source `3a4906a60f0f4da0c1aa449456de2b530542f691`
  contains PR #129 and #130, including blocking reminders, wrap-up variants,
  scoped native choices, and the additional Claude Desktop card formats.
  Both Apple Release builds and the release ancestry/fingerprint check passed.
- **macOS publication.** Developer ID signatures, Apple notarization
  (`638966bd-67dc-4928-bcc2-391c50b378e4`), stapling and Gatekeeper passed.
  Published through `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to the existing desktop directory.
  A fresh public download passed version/build, hash and signature/ticket checks.
  ZIP SHA-256: `227da1de843018b3bbbb21df80d245ac03670f4dc8af4adf3377a294d0d5a279`.
  The portable bundle includes Velopack 1.2.158 metadata and UpdateMac.
  No signed desktop update feed was published.
- **iOS OTA publication.** Published the six OTA outputs through the same host
  to the existing iOS directory. The public page's install link, manifest and
  immutable IPA resolve to 1.0.57 (48); hash and no-cache headers passed.
  IPA SHA-256: `3e72313c221e3b1c816fdb875210f8d0f10cfd8d05e3b7d72ca52156f0b36b0d`.
  The previous build 47 IPA now returns 410 and its manifest resolves to build 48.
- **Mac installation.** Mac-A acquired the Rust update lease and installed the
  public notarized package without an old-program backup. Device identity and
  pairing remain unchanged; version 1.0.57, Accessibility, relay and vnet were
  verified. Daemon SHA-256:
  `fc030fd8eb7cc6824c195b1c8c76ee4dacd1ffddc044254aaa2c621fd8001394`.
- **Internal TestFlight.** The App Store export passed strict signatures,
  production APNs and disabled-debugging checks. Upload/build ID
  `a252afa7-d5e7-57e2-81de-e271fe517d9c` is `VALID` and `IN_BETA_TESTING`;
  API read-back confirms membership in Internal only and bilingual release
  notes. No external review was submitted. Store IPA SHA-256:
  `499667df97f2863e37f0bd114787a85dd0e34bd1d16c2c2e3b5149b1bcfff4ec`.
- **Physical iPhone installation.** After USB connectivity recovered, build 48
  installed successfully on the connected iPhone 17 Pro and launched through
  CoreDevice. The earlier error 4016 did not persist.
- **Controlled macOS card acceptance.** In isolated Claude Desktop session
  `3e9c6153-3ed4-57ae-8024-981cd2f7ec47`, the blocked B3 card displayed its
  question and choices, accepted selection and submission in Muster, and
  resolved with `CARD57_B3_OK` visibly rendered. Operation
  `app-1792134735688617000-14` is `agent.respond: applied`; Claude recorded
  `CARD57_B3_ACCEPT` at 07:12:17.208 UTC and the reply at 07:12:22.986 UTC.
  Earlier direct Claude selections without matching Muster ledger entries
  are excluded from acceptance.
- **Physical iPhone card acceptance.** On the installed 1.0.57 (48), Device Hub
  showed both representative native `wrap_up` cases. Ordinary W2
  (`blocked: false`) expanded from optional answers; blocked B4
  (`blocked: true`) displayed its required question and choices immediately.
  Both allowed selection and Submit answers on the phone, then visibly showed
  the selected message and expected assistant reply with the card resolved.
  W2 operation `app-1792135725049197000-2` and B4 operation
  `app-1792135871735693000-3` are `agent.respond: applied`, targeting their
  respective native question IDs. Claude recorded exactly one
  `CARD57_W2_ACCEPT` at 07:28:46.860 UTC and `CARD57_W2_OK` at 07:28:49.817 UTC;
  B4 likewise recorded exactly one `CARD57_B4_ACCEPT` at 07:31:13.096 UTC and
  `CARD57_B4_OK` at 07:31:14.467 UTC. These submissions were performed through
  Muster on the physical iPhone, not by selecting in Claude.
- **macOS ordinary wrap-up follow-up.** On Mac-A running 1.0.57 (48), a fresh
  W3 native card (`blocked: false`) displayed both choices after expanding
  optional answers. Selecting Mac Continue visibly highlighted the choice;
  Submit answers resolved the card and Muster displayed `CARD57_W3_ACCEPT`
  followed by `CARD57_W3_OK`. Operation `app-1792136893599043000-20` is
  `agent.respond: applied`, targeting `toolu_01Y2KcwtFSw2WVPVViBewHXq`.
  Claude recorded exactly one answer at 07:48:14.852 UTC and one expected
  reply at 07:48:16.325 UTC. The preparation send is separately recorded as
  `app-1792136835636884000-19`, `agent.send: applied`.
- **Acceptance scope.** macOS and physical iPhone now each passed a
  representative ordinary wrap-up and blocked-card display/selection/submit/
  reply cycle. This does not exercise every supported schema variant on each
  device. The earlier macOS W2 session-visibility rejection remains a failed
  attempt; W3 closes the desktop ordinary-card acceptance gate with a new
  controlled submission. W3 preparation initially hit a local-input conflict
  before ledger creation, with no provider receipt; after the user paused
  local input, preparation and submission passed. Input protection remained
  unchanged. These acceptance follow-ups change documentation only and do
  not rebuild or republish either platform.
- **Evidence.** Build, public-download and acceptance records are under
  `/private/tmp/muster-release57/build/releases/Apple-1.0.57-48/`, including
  `acceptance/iphone-card-acceptance.json` and
  `acceptance/mac-ordinary-card-acceptance.json` with operation and receipt
  records; Apple export, upload and processing records are on Mac-C under
  `/Volumes/Workspace/Projects/Muster/build/releases/Apple-1.0.57-48/`.


### 2026-10-16 Provider serialization release (desktop 1.0.58 / iOS build 49)

- **Source and validation.** Release source
  `0470a3652c26ce094fd57e62cc17b2afb26c3d7e` includes merged PR #134
  (`58906ff8`), which retains provider exclusion until cancelled OS work
  finishes. Serialization covers one desktop operation through confirmed
  submission, not the provider's entire model response. Both Apple Release
  builds passed; the three OTA packaging tests passed. The lock regression
  results are recorded in PR #134; this release changes versions only.
- **macOS publication.** Developer ID signing, notarization
  (`dc66a064-576e-4ad5-9607-748d5e3ff0d1`), stapling and Gatekeeper passed.
  Published through `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to `${MUSTER_RELEASE_ROOT}/desktop`.
  An independent public download passed version/build, hash and signature
  checks. Portable ZIP SHA-256:
  `dc694bbe45916b83b30d5e06f06a9d6a67132cb2b8f63db016811224ba0d291a`.
  No signed desktop update feed was published.
- **iOS publication.** The six OTA outputs were published through the same
  host to `${MUSTER_RELEASE_ROOT}/ios`. The public install page, manifest and
  immutable IPA resolve to 1.0.58 (49); download hash and no-cache checks
  passed. OTA IPA SHA-256:
  `6716a6d097c86b83486234bf0b33a7e64412e190844d206e1bdb666d502b0bbb`.
  App Store export passed signatures, production APNs and disabled-debugging
  checks. Store IPA SHA-256:
  `32aab56c5467a60f9fc21c78c616137f4e4e27a8ab54b148cdbec0b6dd29af38`.
  Build `b5180edd-4f80-5eb6-871f-fe290f8baa82` is VALID and IN_BETA_TESTING;
  API read-back confirms Internal-only distribution and bilingual notes.
  No external review was submitted.
- **Installation and runtime.** Mac-A installed the public notarized bundle
  through the Rust update lease, without an old-program backup. Its identity
  and pairing are unchanged; Accessibility, relay, vnet and daemon source
  `0470a365` were verified. Installed daemon SHA-256:
  `4de29c541eeacfdbedf794e2b54b2b613b2ab94061404fb43a6dc9b6d2701a30`.
  The connected iPhone 17 Pro installed and launched 1.0.58 (49); CoreDevice
  read-back verified the version, and Device Hub showed the running Muster
  session list. Mac-C and Mac-B were not upgraded in this release operation.
- **Concurrent acceptance remains blocked.** During the attended window,
  local Muster CLI and Mac-C's existing Muster CLI simultaneously targeted
  the same Mac-A Claude acceptance session. Attempt 1 failed in navigation
  with zero AX web/navigation areas. After selecting the verified acceptance
  session and clearing only its previously prepared test draft, attempt 2
  failed with `local input changed during chat navigation`. No UI actions
  were performed by the test controller while the RPCs ran. Input protection
  remained enabled; the source of the input-counter change is unresolved.
  All four operation IDs are absent from the ledger and neither SERIAL58
  marker appears in the provider transcript. These are pre-submission
  failures, not successful serialization or delivery evidence. The requests
  were not retried after the attended window ended.
- **Remaining card coverage.** No-button blocked text, Other/custom answers,
  label-only second choices and multi-select/multiple-question submissions
  remain unverified on this release. The representative macOS and iPhone
  card results above belong to 1.0.57 and are not counted as 1.0.58 acceptance.
- **Evidence.** Build, signing, public-download, installation and failed
  acceptance records are retained under
  `/private/tmp/muster-release58/build/releases/Apple-1.0.58-49/`.
  `acceptance/concurrent-clients-attempt1.json`, `concurrent-clients.json`
  and `concurrent-verification.json` retain request times and the negative
  ledger/transcript check; `acceptance/testflight.json` records distribution.
  Apple export/upload records are on Mac-C under
  `/Volumes/Workspace/Projects/Muster/build/releases/Apple-1.0.58-49/`.


### 2026-10-16 Native navigation readiness (desktop 1.0.59 / iOS build 50)

- **Source and regressions.** Source `22c67c7bd13f3454b976af1486ca6278f425ac93`
  includes merged PR #136 (`995672c2`) and the prior provider serialization
  repair. Shared Rust now re-observes transiently missing navigation content
  and discards read-only snapshots across input changes before any action.
  Once navigation or writing begins, fixed-generation checks still reject
  input changes. No press or send is replayed. Agent (198), platform (27) and
  daemon (46) unit tests passed; four existing tests remain ignored. The
  existing Keychain-dependent credential test failed under host credential
  state and was explicitly excluded from that final regression run. All-target
  Clippy passed. Both Apple Release builds and three OTA packaging tests passed.
- **macOS delivery.** Developer ID signing, notarization
  (`3bfb0153-afce-4c50-9e46-30c879d492ce`), stapling and Gatekeeper passed.
  Published through `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to the existing desktop directory;
  an independent public download passed version/build, hash and signature
  checks. ZIP SHA-256:
  `d1d225c8e15f4dfc3a08dee5285727bd2d6e96f667cd0d2edec4f1e1451631aa`.
  No signed desktop update feed was published. Mac-A installed the public
  package through the Rust update lease without an old-program backup.
  Identity/pairing were preserved; version, Accessibility, relay and vnet
  were verified. Daemon SHA-256:
  `2652f3bca802779b99e7cc2d0aeb462a1d79828c1910a78836c1ce197bbd959f`.
- **iOS delivery.** The public OTA page, manifest and immutable IPA resolve
  to 1.0.59 (50), with matching independently downloaded bytes. IPA SHA-256:
  `2a9af5bcfb15019480ca58fbc475bc8180df4f025930b0cfb8ca10d1db526105`.
  The App Store export passed signatures, production APNs and disabled-debug
  checks; Store IPA SHA-256:
  `940826297df1898ef6349f09c234b358bab8b90d2bc937ecff31de36d58016f5`.
  Build/delivery ID `9987f02a-4eac-59dc-8982-a081f74173f3` is VALID and
  IN_BETA_TESTING. API read-back confirms Internal-only distribution and
  bilingual notes. No external review was submitted.
- **Device installation.** After reconnecting and unlocking the iPhone, the
  public OTA app installed successfully. CoreDevice read back 1.0.59 (50),
  launch succeeded, and the physical screen displayed Muster's device and
  session list. The initial CoreDevice 4016 failure is retained as attempt 1.
- **Concurrent clients.** Mac-A's installed 1.0.59 CLI and Mac-C's existing
  1.0.56 CLI sent concurrently to the same Claude session on Mac-A. Requests
  started about 1.5 ms apart. Native traces show local selection/send/receipt
  completing before remote selection/send/receipt began; both ledger entries
  are `applied`. Claude received each marker once and replied
  `SERIAL59_LOCAL_OK` and `SERIAL59_REMOTE_OK`. The second desktop operation
  completed before the first model answer, matching the desktop-operation
  lock boundary rather than a whole-model-turn lock. Sampled input counters
  were unchanged throughout the concurrent send. Input protection remained
  enabled; no session-location or input-conflict error occurred in this run.
- **Card acceptance.** All answers below were submitted through Muster UI
  in the isolated session `3e9c6153-3ed4-57ae-8024-981cd2f7ec47`, with
  `agent.respond` ledger status `applied` and matching provider receipts.

  | Platform | Card case | Submitted answer | Provider reply |
  | --- | --- | --- | --- |
  | macOS 1.0.59 | Blocked `wrap_up`, recap only, no ask/actions | `CARD59_T1_CUSTOM` | `CARD59_T1_OK` |
  | macOS 1.0.59 | Ordinary `wrap_up`, Other text | `CARD59_T2_OTHER` | `CARD59_T2_OK` |
  | iPhone 1.0.59 (50) | Blocked `wrap_up`, label-only actions, second option | `CARD59_T3_SECOND` | `CARD59_T3_OK` |
  | macOS 1.0.59 | `AskUserQuestion`, two questions, multi- and single-select | First `A,C`; second `Y` | `CARD59_T4_OK` |
  | iPhone 1.0.59 (50) | Blocked `wrap_up`, Other text | `hello` | `CARD59T5OK` |

  The phone displayed its second-option and Other replies after submission.
  Initial Device Hub typing did not reliably commit IME text; enabling its
  Capture Keyboard control let Return commit `hello`, which was visually
  checked before submission. Capture Keyboard was restored off afterward.
  No new card compatibility defect was reproduced. This is the per-platform
  coverage above, not every case repeated on both platforms. Earlier 1.0.57
  acceptance remains separately versioned. Stable input samples and this
  successful run do not identify the source of earlier input-counter changes.
- **Evidence.** Artifacts and acceptance records are retained under
  `/private/tmp/muster-release59/build/releases/Apple-1.0.59-50/`;
  `acceptance/acceptance-results.json` records scoped receipts and operation
  IDs, while `concurrent-clients.json`, `concurrent-order.log` and
  `concurrent-input-counters.txt` record overlap, serialization and input state.
  the worktree is locked to retain ignored signed artifacts. Apple export and
  upload records are on Mac-C under
  `/Volumes/Workspace/Projects/Muster/build/releases/Apple-1.0.59-50/`.


### 2026-10-16 Claude composer placeholder (desktop 1.0.60 / iOS build 51, not published)

- **Source.** `8943fa9efacde9767f2d5c314e225a971859c778` adds the version bump
  to merged PR #139 (`8e54819f`): the Claude conversation style's composer
  placeholder is now Claude.app's new-chat text, "How can I help you today?",
  on macOS and iOS. The Codex style is unchanged.
- **macOS installation.** Developer ID signed and timestamped Release build;
  `codesign --verify --deep --strict` and `verify-release.sh` passed (single
  stamp equal to `HEAD`). Not notarized and not packaged with Velopack. Installed
  on Mac-A with the `install-mac.sh` steps: Locked Use policy check, stop the App
  and `launchctl bootout` of the daemon (no update lease), replace
  `/Applications/Muster.app` without a backup, relaunch. The installed bundle no
  longer carries Velopack's `UpdateMac` and `sq.version`. The App and its launchd
  daemon were running afterward. Daemon SHA-256:
  `2e9b86a473a1ffd3ffe3b8797b87a94ad25c6ca7319d3658634f3ec29ae9c2af`.
  Identity, pairing, Accessibility, relay and vnet were not checked.
- **iOS installation.** Development-signed Release build installed on the
  connected iPhone 17 Pro with `devicectl`; read-back reports 1.0.60 (51) and
  the app launched. The iPhone shows the new placeholder only in the Claude
  conversation style.
- **Not done.** The OTA page, desktop download, update feed and TestFlight were
  not changed, so `released-commits.txt` is unchanged. Mac-C and Mac-B were not
  upgraded. The placeholder was not visually checked on either device.
- **Evidence.** Build logs are under
  `/private/tmp/muster-release60/build/releases/Apple-1.0.60-51/` until that
  worktree is swept.


### 2026-10-17 Codex composer suggestions (desktop 1.0.61 / iOS build 52, not published)

- **Source.** `0b81348c2c51f615e0f04c949ca4849d0f5aec6f` adds the version bump
  to `4826e33f`. For a watched Codex session, muster-agent now asks a
  short-lived `codex app-server` for the suggested next message after each
  completed turn (see APP_PROFILES.md, "Codex composer suggestions"). The
  composer shows that suggestion, or the provider app's own empty-composer
  text, on macOS and iOS.
- **Validation.** muster-agent, muster-app-core, musterd and muster-proto tests
  passed (402). The host-dependent
  `cloud::tests::credentials_follow_the_profile_paths_and_expiry` was skipped:
  it fails on this Mac because Claude credentials are present. Workspace clippy
  and rustfmt passed. Both Apple Release builds succeeded.
- **macOS installation.** Developer ID signed and timestamped; `codesign
  --verify --deep --strict` and `verify-release.sh` passed. Not notarized and
  not packaged with Velopack. Installed on Mac-A with the `install-mac.sh`
  steps; the App and its launchd daemon were running afterward. Daemon SHA-256:
  `5f99e9d629fca2a2c7861969635ae0f7aaa981299d4fec9136aa9569e127b20d`.
- **Live check.** With the installed daemon, `muster agent-watch` on the Codex
  thread `01a1344e…` (idle after a completed turn) returned
  `composer_suggestion` "把这些修改提交，并部署到 iPhone 真机验证一下。". No
  `codex app-server` process remained afterward. The `muster agent-watch` output
  of the suggestion comes from a CLI built from the later `a77e2a17`.
- **iOS installation.** Development-signed Release build installed on the
  connected iPhone 17 Pro with `devicectl`; read-back reports 1.0.61 (52) and
  the app launched.
- **Not done.** The placeholder was not visually checked in either app. The OTA
  page, desktop download, update feed and TestFlight were not changed, so
  `released-commits.txt` is unchanged. Mac-C and Mac-B were not upgraded.

### 2026-10-20 Three-Mac notarized release and iOS OTA (1.0.62 / build 53)

- **Source and build.** Clean source
  `ce77021558ae8ff43d06d583936ef67fd63934d4` includes merged PR #143
  (native-send recovery) and PR #144 (editable composer suggestions, mobile
  composer layout and Rust-backed native iOS navigation). Both Apple Release
  builds passed, including the iOS device archive. Three OTA packaging tests
  passed. `verify-release.sh` passed on both the packaged app and an independent
  public download, including earlier release ancestry and binary fingerprints.
- **macOS publication.** Developer ID signing, Apple notarization
  (`46582457-6286-4942-964d-75f97dac6e69`, Accepted), ticket stapling, strict
  signature verification and Gatekeeper passed. The portable ZIP was published
  through `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to `${MUSTER_RELEASE_ROOT}/desktop/`.
  Public ZIP SHA-256:
  `e7344b81b69fd35ac99c68d5da977c28422376b4d264476440fbae5aa5807ab1`.
  No desktop update feed was published.
- **Three-Mac installation and runtime.** Mac-A, Mac-C and Mac-B installed the
  same public package using the Rust desktop update lease. Each installed bundle
  passed stapling and Gatekeeper checks. All three report version 1.0.62 (53),
  daemon commit `ce770215`, the original device identity and SEP key backend,
  Accessibility granted, relay registered, vnet up and Locked Use setup ready.
  Daemon SHA-256 on all three:
  `6d5edf9e903dead6dac3dfa06757394bdda2f0878f10bd8979d157713474db4e`.
  No old-program backup was created; application data and pairing state were
  preserved. Mac-C's first SSH package transfer timed out; a fresh public
  download matched the expected full hash before installation.
- **iOS publication.** The existing public OTA page, its installation manifest
  and immutable IPA were independently downloaded and verified as 1.0.62 (53).
  The IPA signature passed; its provisioning profile covers three registered
  devices and expires on 2027-09-25. IPA SHA-256:
  `98b8b4795c871499fc2df26cc76c251cf5f8f93d95afb79c9471b7f5609815e4`.
  Publication used `${MUSTER_RELEASE_ROOT}/ios/`; the unified entry remains
  `https://linsheng.dev:8443/muster/`. No TestFlight upload was requested.
- **Acceptance boundary.** This release verifies build, signing, notarization,
  public download, three-Mac installation and daemon readiness. It does not
  claim a new provider-send end-to-end run, physical-iPhone installation or
  visual acceptance of the composer/navigation changes on this build.
- **Retained evidence.** The locked release worktree contains ignored artifacts
  under `build/releases/Apple-1.0.62-53/`: `notary.json`, `sign-notary.log`,
  `public-check/result.json`, both Apple build logs and per-host installation,
  before/after identity and detailed Rust device-state records in `acceptance/`.

### 2026-10-20 OTA unchanged-install investigation and retry entry

- After the 1.0.62 (53) publication, the user reported that iOS still showed
  the old version. Mac-A's latest stored iPhone heartbeat reported 1.0.61 (52),
  commit `0b81348c`; that peer was subsequently unreachable. CoreDevice also
  reported the registered iPhone unavailable, so this is a cached runtime
  observation, not a live installed-app inventory.
- A new independent public download still contained 1.0.62 (53) and matched
  IPA SHA-256 `98b8b4795c871499fc2df26cc76c251cf5f8f93d95afb79c9471b7f5609815e4`.
  The page, manifest and IPA returned no-store headers. Old `manifest-<hash>`
  URLs redirect to `manifest.plist`; no server-side old-IPA fallback was found.
  Server access logs were not readable with the available account, and no
  device installation error log was available. A cache or installation failure
  remains a hypothesis, not a confirmed root cause.
- The OTA page now offers an explicit retry for the displayed build through
  `install-<IPA hash>.plist`. This content-addressed route returns 200 directly,
  avoiding both the mutable manifest alias and historical-manifest redirect.
  It references the unchanged signed IPA. The standard latest-version entry
  is retained. Packaging emits this additional manifest, and pruning preserves
  the current retry manifest while retiring superseded ones.
- Public read-back verified the refreshed page, a direct non-redirecting retry
  manifest, build 53 and the exact verified IPA asset. All 15 Apple release-script
  tests passed. The user was asked to retry and report the resulting version or
  installation error. Physical-device installation and the cause of the first
  failed update remain unverified.

### 2026-10-20 IPA packaging defect reproduced and corrected (1.0.62 / build 53)

- **Remote evidence.** Read-only Nginx access logs, accessed through the
  account's configured passwordless shell permission, show iPhone18,1's
  `com.apple.appstored` downloading all 11,255,652 bytes of IPA `98b8b479...`
  at 10:26, 10:33, 10:34 and 10:38 China time. The phone subsequently reported
  1.0.61 (52), commit `0b81348c`, while online. The retry-manifest workaround
  therefore did not solve the problem, and stale download URLs do not explain
  these attempts. Device installation logs remain unavailable remotely.
- **Confirmed packaging defect.** `package-ota.py` used `ditto -c -k` without
  `--norsrc`. It emitted 17 AppleDouble `._*` entries into the IPA, including
  entries inside the signed app and resource bundles. Ordinary ZIP extraction
  followed by `codesign --verify --deep --strict` failed with "a sealed
  resource is missing or invalid" and listed the sidecars as added files.
  Removing only those entries made the unchanged application signature pass.
  Previous verification used `ditto -x`, which consumes AppleDouble metadata
  and hid this defect. The older 1.0.59 OTA also contains these sidecars; its
  documented wired installation does not establish OTA installation success.
- **Independent comparison.** Xcode's successful `debugging` export from the
  exact same archive uses `ditto --norsrc` and has no AppleDouble entries. Its
  signing certificate, profile and entitlements match the published app. The
  original signed executable embeds `ce77021558ae8ff43d06d583936ef67fd63934d4`;
  the iOS version comes from Bundle Info.plist at startup, not a persisted
  previous release value. No certificate, provisioning rights or application
  content needed to change for the packaging repair.
- **Repair and regression.** The packager now uses `--norsrc`, rejects metadata
  entries and verifies the final IPA after ordinary `unzip` extraction. A real
  ad-hoc signed fixture with extended attributes reproduces the failure under
  the old invocation and passes with the fix. All 17 release-script tests
  passed. This verifies a packaging defect and its removal; the phone's
  original OS installation error code is still unknown.
- **Republished artifact.** The corrected 1.0.62 (53) IPA is published through
  the existing SSH destination and OTA page. SHA-256:
  `fcebd67d2c97dced44a455ac3e0d96d048fea3881a8169f25af9ba052316ab95`.
  Independent public download verified hash, version, zero AppleDouble entries,
  ordinary-extraction signatures and byte equality of every retained entry with
  the original signed app. The defective IPA and its retry manifest were
  withdrawn. Current retry manifests return directly; retired retries redirect
  to the latest manifest. `nginx -t`, reload and public redirect/read-back passed.
  macOS artifacts are unchanged. The user was asked to retry remotely; successful
  physical-device installation is not yet confirmed in this record.
- **Evidence.** Local package comparisons, Xcode export and signature checks are
  under `/private/tmp/muster-ota-export-check/` and
  `/private/tmp/muster-ota-investigation62/`. The locked release worktree retains
  the corrected publication under `build/releases/Apple-1.0.62-53/ios-repacked/`.

### 2026-10-20 Claude historical cards, Tab suggestions and authorization timing (1.0.64 / build 55)

- **Observed failure.** Four Mac-A 1.0.62 proposal replies timed out locating
  the same historical card. The native tree exposed `Load earlier messages`
  but not the card. Loading two earlier pages exposed it; the dismissal was
  named `Dismiss suggestion <task title>`, which the old aliases did not match.
  Shared Rust now loads bounded history and matches that exact title-qualified
  control. An installed 1.0.63 inspection matched both controls on the original
  failing card without starting or dismissing the task.
- **Authorization repair.** Installed-device checks also reproduced owner
  heartbeat losses with 383–490 ms gaps during initial authorization. The
  owner's dedicated single-thread runtime still ran synchronous lock-screen AX
  queries. Those queries now run on a separate worker; cleanup cancels its
  submission checks and joins it before continuing. The watchdog heartbeat
  and observation freshness limits remain unchanged. Stale ready reports can
  briefly await genuinely fresh evidence; their timestamps are never refreshed.
- **Suggestion input.** Mac and iOS expose Read suggestion for an empty Claude
  composer. A separately authorized native operation focuses the verified empty
  editor, posts one application-targeted Tab, reads its value, and confirms
  restoration of the empty draft. It never sends Return or submits the text.
  Existing drafts and intervening local input are preserved. The result is
  bound to the chat content revision and can be copied into the editable draft.
- **Source and checks.** Final release source is
  `b59bba8f8e2165cd6f4911098313920fee80b055`. App-core, platform, protocol,
  daemon and targeted agent tests passed, along with strict Clippy. The final
  daemon run passed 47 unit and 14 integration tests, including a 400 ms blocking
  query test that keeps the owner timer running and rejects submission after
  cancellation. The full agent run retains one existing host-dependent failure:
  its credentials test assumes no system Keychain credential on a logged-in Mac.
  With that test excluded, 204 passed and four were ignored. Both Apple Release
  builds and iOS archive/packaging checks passed.
- **Signing and publication.** Developer ID signing, notarization
  `8ba54bcc-02a1-4829-9163-5e8621006ae0` (Accepted), stapling and Gatekeeper
  passed. Both platforms were published through `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to
  the existing desktop and iOS directories. Independent public downloads
  verified 1.0.64 (55), signatures, source stamp, hashes and zero IPA AppleDouble
  entries. macOS ZIP SHA-256:
  `b4c74e6036ed63e34d51dc1e3c9101ef38666c46dc87276a0784fc1ce052b944`.
  iOS IPA SHA-256:
  `8f6d6c0c9cacf44c31d717d8f210d5636dafeeef5c0e2959c44277f2c60d1d96`.
  The registered-device OTA entry remains `https://linsheng.dev:8443/muster/`.
  No desktop update feed or TestFlight upload was made.
- **Installation and runtime.** Mac-A, Mac-C and Mac-B all installed the
  notarized 1.0.64 (55) package through the Rust update lease. Each retained its
  device identity and Accessibility grant, with relay registered and vnet up.
  All three daemon hashes are
  `8e523d6b64c0f20824b028dc18da0f9cd9b56a671453f2c4fa65251c1bb79285`.
  No installed-program backups were retained and application data was preserved.
  Mac-A had transient network recovery delays during the earlier 1.0.63 installs;
  final 1.0.64 readiness was confirmed.
- **Acceptance limits.** The original card's controls were verified on installed
  1.0.63; 1.0.64 retains that parser unchanged. Its final protected inspection
  correctly refused entry because Mac-A was then unlocked. The new worker has
  regression coverage but its first locked-console authorization on Mac-A still
  needs acceptance. No real proposal action or message was sent for this
  investigation. Native Tab retrieval, mobile interaction and physical iPhone
  installation remain unverified.
- **Evidence.** The retained session worktree contains ignored
  `build/releases/Apple-1.0.63-54/` and `Apple-1.0.64-55/` directories with
  build, notarization, installation and public-download verification records.
  1.0.63 (54), source `6ce9101bb342c18e6f5d57529c28692cf7991bae`, was briefly
  published and installed before the additional authorization-thread repair;
  both public aliases now point to 1.0.64 (55).

### 2026-10-20 Composer crash, Claude receipts and Codex Tab (1.0.65 / build 56)

- **Crash evidence.** Mac-A's 1.0.64 crash reports at 12:32, 12:36 and
  12:38 show main-thread stack exhaustion while SwiftUI recursively derives
  an accessibility label. Opening the placeholder conversation and reading its
  accessibility tree reproduced the crash. The macOS multiline composer now
  supplies an explicit `Message` label. The patched development UI opened both
  that conversation and the card acceptance conversation, survived repeated AX
  reads, and accepted/cleared an editable test draft. However, the installed
  Release build still crashed on the original conversation at 13:08:46. The
  composer label alone did not fix the Release-only recursion.
- **Actual send and false failure.** On installed 1.0.64, operation
  `cli-1792471765066588000-27036` submitted the unique marker
  `MUSTER-Mac-A-SEND-1020-B` to the existing Claude acceptance conversation.
  Claude persisted one user message and replied with the marker, but Muster
  timed out and retained a pending operation. Claude Desktop 2.1.299 had put
  linked-session system notices in a separate leading text block in the same
  human message record. Comparing all blocks together missed the exact prompt.
  The receipt parser now skips only leading, wholly wrapped system-notice
  blocks in explicitly human Claude Desktop records and compares the remaining
  prompt exactly. It still excludes tool results and does not accept a quoted
  substring. The old operation was not resent or manually altered.
- **Tab support.** Shared Rust now admits both Claude and Codex and carries
  the correct provider through authorization. Codex's known empty placeholder
  AX values are accepted only when no submit control may be active. Cleanup
  writes an empty string rather than typing the placeholder back as a draft.
  Existing drafts and input/session changes remain guarded.
- **Checks and source.** Release source:
  `ad9649444ef0ad5a3f0decfa4895ea60559562bb`. App-core (73 unit tests),
  platform (33 unit tests), receipt regressions (5) and targeted completion
  regressions passed; strict Clippy passed for all three changed Rust crates.
  Both Apple Release builds and iOS archive/IPA validation passed. Packaging
  rejected an intermediate daemon built before the final fix; the published
  bundle was independently verified against the final source stamp.
- **Signing and distribution.** Developer ID signing, notarization
  `f8c73d8a-0a99-43d3-a0b7-eb77ca74ba31` (Accepted), stapling and Gatekeeper
  passed. The existing SSH publisher received macOS and iOS 1.0.65 (56).
  Independent public downloads verified source, versions, signatures, hashes
  and absence of IPA AppleDouble entries. ZIP SHA-256:
  `8ae1a927e791c2a620816fb13292a51affb01020fbe0ba7ba0786cfd59c876ee`.
  IPA SHA-256:
  `57b0f7691ae26cc912acbd3b68f3aea1393659f8617a22e9f67cab62733bc280`.
  Superseded public packages were pruned. No desktop feed or TestFlight upload
  was made.
- **Installation and limits.** Mac-A, Mac-C and Mac-B installed the same
  notarized build through the Rust update lease, preserving their identities
  and Accessibility grants; all recovered relay registration and vnet.
  Daemon SHA-256:
  `aa48103bd286d0c5fd76de3853e0acf8fa90c0f82597a60c9139efc48f841bfb`.
  Before installation, direct Claude Tab testing left the empty acceptance
  composer unchanged because that turn had no suggestion. After installation,
  the actual Muster Tab RPC initially refused because Mac-A was locked. After
  the user unlocked it, Tab completed with an empty result and did not submit.
  The unique Claude send marker `MUSTER65-Mac-A-1020-C` then received an exact
  provider reply; its operation was `applied`, and same-operation replay was
  `duplicate`. A preceding local-input rejection occurred before submission.
  Nonempty suggestion extraction remains unverified. The computer-use tool refused
  access to Codex, so its native Tab path is not claimed as physically tested.
  Physical iPhone installation is also unverified.
- **Evidence.** The retained session worktree has
  `build/releases/Apple-1.0.65-56/` with signing, public-download verification,
  three installation records and the reduced send/Tab acceptance evidence.


### 2026-10-20 Release accessibility regression fixed and accepted (1.0.66 / build 57)

- **Cause and fix.** The original placeholder conversation includes a queued
  message. In optimized macOS builds, its selectable `Text` plus a redundant
  `accessibilityLabel` recursed through SwiftUI/AppKit until stack overflow.
  Removing only that label override preserves native readable/selectable text.
  The shipped 1.0.65 executable reproduced in an isolated bundle; the corrected
  Release build and the final installed notarized 1.0.66 both opened the same
  conversation, exposed its original queued content, and survived AX reads and
  a temporary composer draft edit/clear without submission. Debug-only success
  is not used as the acceptance gate. No queued message or user draft was deleted.
- **Installed native acceptance on Mac-A.** A new message was sent from the
  Muster GUI to the existing Claude acceptance conversation. Operation
  `app-1792474200371316000-1` was `applied`; the provider transcript contains
  exactly one user message and one exact expected reply containing
  `MUSTER66-Mac-A-1020-D`. The GUI cleared the sent draft. Same-operation replay
  returned `duplicate`. Native Tab RPC succeeded with an empty suggestion;
  Claude's composer remained empty and no extra message was submitted.
  Claude did not offer a nonempty suggestion in this acceptance turn. Codex
  UI access was refused by the computer-use tool, so its native Tab path remains
  covered by Rust checks, not physical acceptance. Physical iPhone installation
  is also unverified.
- **Build and distribution.** Source `9c7e9e5054b5f90350adf8c661c79eadd7ebba07`.
  macOS Release and iOS archive/IPA validation passed. An initial Apple signing
  timestamp failure was retried without relaxing validation. Developer ID,
  notarization `fd06c6e3-ebd7-4a76-82f6-d5048aa01391` (Accepted), stapling and
  Gatekeeper passed. Mac-A, Mac-C and Mac-B run the same 1.0.66 (57) daemon,
  preserving identity and Accessibility access, with relay/vnet recovered.
  Daemon SHA-256:
  `427ed8b2588c1e438cfa25f98d99dbdc518cefe6d5749fd71f4c05fc2eaf655b`.
- **Public verification.** macOS and iOS were published through the existing
  SSH publisher. Independent downloads verified source, signatures, versions
  and hashes. ZIP SHA-256:
  `6520089872e2f4ced36da6fdebe1ee60ea57fb605a2f439615404e81b397b288`.
  IPA SHA-256:
  `7797134187695e8d7a5564b5f51b76874e3818ce189e77de4ee444d728795d4d`.
  OTA manifest build is 57. Superseded 1.0.65 immutable files were pruned;
  old manifest/install links resolve to the current no-cache manifest, while
  the old IPA returns 410. No desktop feed or TestFlight upload was made.
- **Evidence.** `build/releases/Apple-1.0.66-57/` in the retained session
  worktree contains build/signing logs, public downloads, installation records,
  and reduced acceptance evidence, including `acceptance/installed-results.json`.


### 2026-10-20 Descendant activity in session lists (1.0.67 / build 58)

- **Behavior.** Shared Rust aggregates verified child and deeper-descendant
  activity into `AgentSession.descendant_activity`. List rows stay working
  when the parent is idle and a child runs; approval takes precedence. Raw
  parent turn state, input authority and read-only child controls are preserved.
  Ordinary forks do not contribute, cycles terminate, and children beyond the
  200-summary limit still count. Claude inline children use their own sidechain
  turn records and their registered parent worker's verified launch; Codex
  continues using worker-bound rollout evidence. Completion, missing workers
  and records predating a replacement worker clear the aggregate.
- **Checks.** Six subagent integration tests, six process-state tests, 74
  app-core tests and 205 agent unit tests passed. Clippy with warnings denied,
  formatting and layer checks passed. One existing credential-fallback unit
  test fails against this Mac's signed-in Keychain and was excluded from the
  subsequent unit run. GitHub reported no CI checks for this PR. Apple macOS
  Release and iOS archive/IPA validation passed; an initial timestamp-service
  failure was retried without relaxing signing requirements.
- **Release.** Source `4017d71ecad47cbf83114cb4066765400fe975fa`.
  Developer ID signatures, notarization
  `ac800dba-8a8e-403c-ad3c-50a5684ef3f2` (Accepted), staple validation and
  Gatekeeper passed. Mac-A, Mac-C and Mac-B installed the same 1.0.67 (58),
  preserved identity and Accessibility grants, and recovered relay/vnet.
  Daemon SHA-256:
  `5816cc1ebe006e1d6bf9ff4e19397acb73ce9668dfe5597bb3e06736d502ffd2`.
- **Installed runtime evidence.** All three Macs returned the new field in
  real session-list RPCs (92, 142 and 111 visible sessions respectively).
  Mac-A returned an active Codex descendant. Mac-B returned a Claude parent
  with own state `idle` and descendant activity `running`; its child transcript
  records continued at 07:02-07:05 UTC, after the verified worker's 06:09 UTC
  launch. This is installed daemon/protocol evidence. Badge projection and
  control separation passed Rust tests; no new live UI transition or physical
  iPhone installation is claimed.
- **Public distribution.** Published both platforms through
  `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` into the existing desktop/iOS directories.
  Independent public downloads verified source, signatures, versions and
  hashes. ZIP SHA-256:
  `0cd5fbfd343051efe94c5ae8b134cde587221c65f99c94d6d781618783f9b3f3`.
  IPA SHA-256:
  `51cf81bf067b8a7984c4226ec1162fd8aa30a815ed27a85be3a55ab76eb98c95`.
  The OTA manifest names build 58. Superseded 1.0.66 immutable files were
  pruned; their manifest/install links resolve to the current no-cache
  manifest, and the old IPA returns 410. No desktop feed or TestFlight upload
  was made.
- **Evidence.** The session worktree's `build/releases/Apple-1.0.67-58/`
  contains build/signing logs, public-download checks, installation records,
  reduced per-host list observations and old-link verification.


### 2026-10-20 Claude reply completion and live work (1.0.68 / build 59)

- **Finding.** On Mac-A, the reported Claude conversation wrote an assistant
  `end_turn` at 07:13:59 UTC, while its Stop hook continued until 07:14:59.
  A reply boundary had incorrectly outranked the verified busy worker.
  The installed 1.0.67 UI also opened asynchronously loaded history at its
  oldest messages: a manual Jump to latest revealed the actual ongoing work.
- **Behavior.** Shared Rust preserves verified busy/waiting worker activity
  after reply completion. Completed history remains Worked; a separate live
  Working, Needs you or descendant activity marker covers work after a reply.
  Descendant-only state changes invalidate the timeline cache. Native apps
  render an empty activity marker without an empty disclosure control. Apple
  transcripts start at the bottom after asynchronous layout and reset their
  scroll identity when switching conversations; composer identity is preserved.
- **Checks.** Six process-state tests, six subagent tests, 205 agent unit
  tests and 75 app-core tests passed. Clippy with warnings denied, formatting
  and layer checks passed. The previously identified credential-fallback test
  was excluded because this Mac has signed-in Keychain credentials. Apple
  macOS Release and iOS archive builds passed. Android/Harmony render changes
  were source-reviewed, not platform-built. No live post-install UI transition
  or physical iPhone acceptance is claimed.
- **Release.** Source `1f62106f59288e9d5ccefc00ec7d99607a86da1f`.
  Developer ID signatures, notarization
  `463e612c-8728-4d48-bba2-2fdf0d6a9f0d` (Accepted), staple validation and
  Gatekeeper passed. Mac-C and Mac-B installed 1.0.68 (59), preserved device
  identity and Accessibility grants, and recovered relay/vnet. Their daemon
  SHA-256 is `29349c51f6907ec9e4b8149f47b44f86b6989055326260229510345eb92c9cee`.
  The installed Mac-B daemon reports a real Claude idle parent with a running
  descendant. Mac-A's Rust update lease refused replacement because an existing
  protected terminal console lease had not drained. Its installed 1.0.67 was
  left intact; the user was asked to end that connection before retrying.
- **Public distribution.** Both platforms were published through
  `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` in the existing desktop/iOS directories.
  Independent public downloads verified versions, hashes and signatures, plus
  macOS source stamp, staple and Gatekeeper. ZIP SHA-256:
  `916e2072841154a410a57b7ccb28ac6087d048841cf7ff1b830c235faea74e61`.
  IPA SHA-256:
  `fa5173121e430213aa55b5fc9716b4bc74f4324b6c60d5796aebda96f8255fb4`.
  The OTA manifest names build 59. Old 1.0.67 manifest/install links resolve to
  that no-cache manifest, and its retired IPA returns 410. No desktop feed or
  TestFlight upload was made.
- **Evidence.** The session worktree's `build/releases/Apple-1.0.68-59/`
  retains build/signing/public checks, per-host installation records, reduced
  runtime observations and old-link checks. The Mac-A installation failure is
  recorded in `install-mac-a.log`.


### 2026-10-20 Mac-A 1.0.68 installation and native UI acceptance

- **Installation.** After the user ended the protected terminal connection,
  the normal Rust update lease succeeded. Mac-A installed the same notarized
  1.0.68 (59) artifact recorded above. Signature, staple and Gatekeeper checks
  passed; device identity and Accessibility permission were preserved, and
  relay/vnet recovered. All three Macs now have this release.
- **Original conversation.** The originally reported conversation was matched
  by its stable session ID after its title changed. It was now idle. Opening
  it in the installed Mac-A app landed at the latest content (scroll value
  0.998994). Jump to first showed history items 0-100 of 378. Switching away
  and reopening returned to items 278-378 and the same bottom scroll value.
  No manual Jump to latest was used for either open.
- **Live descendants.** A real Mac-B Claude session reported own state idle
  and descendant activity running. On Mac-A, its list row displayed working
  and a native screenshot confirmed Subagents working beneath the completed
  reply. This verifies the installed UI projection across devices. No provider
  message was sent, and the original post-reply Stop hook transition was not
  recreated. Physical iPhone acceptance remains unverified; iOS OTA stays at
  the already verified 1.0.68 (59).
- **Evidence.** `build/releases/Apple-1.0.68-59/install-mac-a.json`,
  `install-mac-a-retry.log` and `mac-a-ui-acceptance.json` retain the installation
  and reduced UI observations. This resolves the Mac-A installation and native
  UI acceptance limitations in the preceding release entry.


### 2026-10-20 1.0.69 execution traces and Claude sub-session completion

- **Diagnosis and correction.** The Mac-B example used for 1.0.68's descendant
  UI acceptance contained 15 inline Claude agents that had already handed back
  their tasks. Each ended with a user tool result carrying `toolEndsTurn: true`,
  followed by a hook diagnostic. The old parser missed that explicit boundary
  and reported all 15 as running. The earlier screenshot verified the UI's
  projection, not that those descendants were actually still working.
- **Behavior.** Shared Rust now recognizes the provider handback boundary in
  both transcript and activity parsing. Resumed children can become active
  again. Claude desktop `spawnedFrom` relationships resolve desktop IDs to
  transcript IDs within the same organization/user registry. Independent
  sub-sessions retain their own worker/input authority; inline agents remain
  read-only. User forks are navigable but do not keep a parent working.
  Registry-only relationship changes also notify list subscribers.
- **Tracing and navigation.** Apple, Android and Harmony content pages expose
  Conversation, Tracing, Subagents and Sessions tabs, with parent navigation.
  Rust pairs calls/results only by unique provider tool-call IDs, projects
  recorded status and approximate duration, and preserves unknown outcomes.
  Traces use loaded history and existing pagination. Each child can open its
  own history and trace.
- **Checks.** 206 agent unit tests, six process-state tests, seven subagent
  integration tests and 77 app-core tests passed. Tests cover handback,
  resumption, nested completion, fork exclusion, registry-only refresh and
  synchronized list/content projection. The previously identified signed-in
  Keychain credential-fallback test remains excluded. Formatting, layer checks
  and Clippy with warnings denied passed. macOS Release, iOS archive and
  Android `:app:compileDebugKotlin` passed. Harmony was source-reviewed and
  protobuf bindings regenerated, but no Harmony platform build was run.
- **Release and installation.** Source
  `1a986d5311d130bdf96f17bc329ad1f2d06e8c08`, version 1.0.69 (60).
  Apple timestamp service intermittently failed; normal timestamped signing
  eventually passed without relaxing validation. Notarization
  `e3c4cd99-7070-4ec1-bac2-f039c6e3c8fe` was Accepted; strict signatures,
  staple validation and Gatekeeper passed. Mac-A, Mac-C and Mac-B installed
  the same artifact through the normal Rust update lease, retained identities
  and Accessibility grants, and recovered relay/vnet. Daemon SHA-256:
  `687b35e9d9cc627548e17cd8854745df3487e5632f50f673a959abb110e75d0d`.
- **Installed runtime acceptance.** Before upgrade, the Mac-B example reported
  15 running inline agents. After upgrade, all 15 reported idle; independent
  list and selected-content snapshots both reported parent idle with no
  descendant activity. A later settled snapshot confirmed no reversion.
  Three independent sub-sessions resolved to the parent and remained writable.
  This verifies existing completed work after upgrade. A newly started live
  child-to-completion transition was not driven in the provider UI. Mac-A was
  locked and native UI access was refused, so the new tabs' installed visual
  acceptance awaits manual unlock. Physical iPhone acceptance is unverified.
- **Public distribution.** Both platforms were published through
  `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` in the existing directories. Independent public
  downloads verified build/version, hash, signatures, and macOS source stamp,
  staple and Gatekeeper. ZIP SHA-256:
  `122be5c484dbc6bcab1c00dbecee2098a68c944265245286647bca4b1f647d69`.
  IPA SHA-256:
  `47e7e692fe166e14b9663c941208e0768071cbcf5558659a178b1a8e16120217`.
  The OTA manifest names build 60. Retired 1.0.68 manifest/install links
  redirect to the no-cache current manifest; its removed IPA returns 410.
  No desktop feed or TestFlight upload was made.
- **Evidence.** `build/releases/Apple-1.0.69-60/` retains build/signing logs,
  per-host installation records, before/after/settled reduced runtime snapshots,
  independent list verification, public download checks and old-link checks.


### 2026-10-20 Mac-A 1.0.69 native tracing acceptance

- After manual unlock, installed Mac-A 1.0.69 reported an unlocked console,
  Accessibility granted, relay registered and vnet active. Native UI inspection
  confirmed Conversation, Tracing, Subagents and Sessions tabs.
- The target Mac-B parent displayed idle in the Mac-A list; its latest content
  had no residual Working marker. Subagents showed all 15 children idle.
  Opening one child displayed Worked, read-only history and no message composer.
  Its Tracing page displayed idle, tool calls and approximate durations.
- The parent Tracing page displayed tool events, timestamps and durations, and
  a tool detail disclosure expanded successfully. Sessions displayed three
  independent sub-sessions. Opening one preserved its editable Message field
  and exposed its own Tracing page. Parent navigation returned to Conversation
  successfully from both inline and independent children. Native screenshots
  confirmed the tab/trace layout and absence of a stale activity marker.
- A fresh Mac-B protocol snapshot agreed: parent idle, empty descendant
  activity, 15 idle inline children and three independent sub-sessions. No
  provider message was sent and no new live child-completion transition was
  driven. Physical iPhone acceptance remains unverified. This resolves the
  locked-screen limitation on the new tabs' installed Mac visual acceptance;
  no application change or additional release was needed.
- Reduced observations are retained in
  `build/releases/Apple-1.0.69-60/mac-a-ui-acceptance.json` and
  `after-unlock-mac-b.json` alongside the earlier release evidence.


### 2026-10-20 Mobile conversation workbench and Mac-C acceptance (1.0.72 / build 63)

- **Source.** `e9cdbf4b01eef58ca51cce72f15caa6855f61c07` adds Chat / Activity /
  Terminal navigation, native iOS review sheets, and confirmed shell-code
  execution with automatic result return. Shared Rust owns command validation,
  durable records, origin binding, return consent, and delivery. The workbench
  contract is in [CONVERSATION_WORKBENCH.md](CONVERSATION_WORKBENCH.md).
- **Problems reproduced and fixed.** On Mac-C 1.0.71, a fast command completed
  before the provider's preceding turn; automatic return was rejected with
  `Wait for the current turn, answer its prompt, or interrupt it`. The client
  now waits for fresh provider idle evidence. Captured output also contained
  tmux's retained-pane exit banner; the command window now disables that banner
  before releasing execution. The rejected 1.0.71 return was not replayed.
- **Checks.** 80 app-core, 18 terminal, and 48 daemon tests passed (three explicit
  environment-dependent tests ignored). A separate real-tmux regression passed,
  including exact output, nonzero exit status, deduplication, and reconnect/end
  persistence. Clippy, layer checks, Apple builds, and Android Kotlin compilation
  passed. iOS simulator layout tests rendered 320/402-point light/dark layouts;
  the narrow confirmation sheet displays the complete wrapped command.
- **Signed release.** macOS 1.0.72 passed strict signature verification,
  notarization `4a5e9a6b-b34b-4182-bce1-4bdb4ec49644` (Accepted), stapling,
  Gatekeeper assessment, and source-stamp verification. Its portable ZIP SHA-256
  is `3abb0f4c1b6d59a43b72db60e95c0af6ac11217b6db252a2fc18e56ac7ff2414`.
  iOS 1.0.72 (63) archived and packaged successfully; IPA SHA-256 is
  `703d4aa487bd1dfdf052e8376dbf7589ff6af60934d43fc02875ed9d7f9c9210`.
  Both were published through `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to the existing desktop
  and iOS directories. Public downloads matched these hashes; manifest build 63,
  embedded version, iOS signature, and macOS notarization were independently
  checked. OTA: <https://linsheng.dev:8443/muster/>.
- **Mac-C installation/runtime.** Installed through the existing Rust update
  lease, without retaining an old app bundle or changing app data. The installed
  daemon SHA-256 is `a4380b403fdd2a737880bbaf8bcc415a2f5916a3a81df567efe03fb04d0754d3`.
  Device identity was preserved; installed version 1.0.72, accessibility granted,
  unlocked console, registered relay, and vnet recovery were verified. Mac-A and
  Mac-B were not upgraded in this validation pass.
- **Mac-C actual execution and return.** Used a dedicated independent Codex CLI
  adapter conversation in `/Volumes/Workspace/Projects/Muster`, through the same
  Shared Rust intents as the mobile UI. Exact `printf` / `pwd` / `exit 7` commands
  were previewed and confirmed. Normal run
  `4361bd3337c5ddbf6c3be75343496da8f5b9fe985603d26735312acd111329a8`
  returned after leaving and reopening the original conversation. A second run
  `8aaaffe5b447c2892ba0c761aedbc252dabb01e6d34b29063102c0a69405413d`
  deliberately kept the provider generating after its code fence: observed
  `waiting -> sending -> sent`, then one result message and one provider
  acknowledgement. Both returned the correct directory, exit code 7, and exactly
  two output lines. Ending the dedicated terminal preserved both completed
  records with applied receipts. Separate RPC checks verified repeated operation
  IDs do not execute again and changed directories are rejected.
- **Acceptance limits.** Claude desktop on Mac-C was at its sign-in page, and
  Claude CLI was unauthenticated; native Claude return still needs manual login
  and separate acceptance. The Codex CLI result does not establish native desktop
  AX acceptance. Physical iPhone installation is not claimed. Android JNI/APK
  packaging is blocked by the incomplete local NDK; HarmonyOS build/device
  acceptance is unverified.
- **Evidence.** `build/releases/Apple-1.0.72-63/` retains build/signing logs,
  simulator images, public-download verification, Mac-C installation/RPC logs,
  core probe results, original-session messages and acknowledgements, and retained
  result records. Release 1.0.71 was only test-installed, not publicly published.


### 2026-10-20 Evening-log fixes and mandatory ADRs (1.0.73 / build 64)

- **Source/decisions.** `e4f48b8ec27eb75b3081323ea0c2866635076ccf` fixes Codex
  Cloud list requests that exceeded the server's limit of 20. Bounded cursor
  pagination preserves the prior 50-task coverage. Relay probes retry within the
  original three-second deadline; persistent loss still fails over. Native
  renderer/draft refusals now explain safe recovery without removing their guards.
  [ADR 0002](adr/0002-provider-limits-and-bounded-relay-probes.md) records the
  alternatives and evidence. [ADR 0001](adr/0001-require-adr-for-changes.md) requires
  a numbered ADR for every future change, enforced by `check-adr.sh` during
  `finish-session.sh`; AGENTS.md defines the rule. ADR 0003 retrospectively records
  the confirmed conversation workbench.
- **Checks.** 12 cloud and 11 relay tests passed, including real encrypted
  transport fault injection, unchanged failure deadline/fallback, trust, MTU,
  and peer RPC recovery. Clippy, 16-crate layer checks, shell syntax, and seven
  ADR-gate Git-history cases passed. On Mac-C, a read-only provider comparison
  returned HTTP 400 for limit 50 and HTTP 200 for limit 20; that account had zero
  current cloud tasks, so live multi-page task contents are not claimed.
- **Release.** macOS and iOS 1.0.73 (64) built successfully. Notarization
  `ec396a5b-ff92-4e7b-b37b-6cc56c3f6651` was Accepted; staple, Gatekeeper, strict
  signatures and embedded source stamp passed. Both platforms were published
  through `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to the existing release directories. Public
  downloads independently verified version 1.0.73/build 64 and signatures:
  macOS ZIP SHA-256 `6813751499c293bbfec848555c03a82f5ee1ed6aaed9832ca4ea8472eb03c6d5`;
  iOS IPA SHA-256 `0afe18289080db36f00e71aa7bb083040646b9204b9dc01ab1a94c442673f42d`.
  OTA: <https://linsheng.dev:8443/muster/>. The publisher's initial console summary
  retained a stale build-63 label; the actual IPA, manifest and independent public
  verifier confirm build 64. The script message was corrected without republishing.
- **Mac-C installation/runtime.** Installed with the Rust update lease, no old
  application backup, and unchanged app identity/data. Installed daemon SHA-256
  `0dc96a2dd5e64d4817ee83c8c610cff0c25da66242935d16439ed79f82eec326`.
  At 23:42:56 CST, 97 seconds after startup, accessibility, relay registration and
  vnet were healthy; SSH through 192.168.13.13 succeeded. No Codex Cloud 400,
  relay warning/error, or new Muster crash report appeared during that window.
  This observation is bounded and does not establish the root cause of every
  earlier network failure. Other Macs were not installed in this pass.
- **Manual/untested scope.** Claude Cloud still requires CLI sign-in on Mac-C;
  the app does not bypass or refresh credentials. Historical native renderer
  unavailability has not been reproduced; this change improves its recovery
  message rather than claiming a root-cause repair. Existing-draft protection and
  deliberate changed-directory test rejections remain correct. Physical iPhone
  installation and native AX send acceptance were not run for this release.
- **Evidence.** `build/releases/Apple-1.0.73-64/` contains build/notary logs,
  tests, live limit comparison, update-lease installation, vnet SSH, runtime review,
  and `public-check/result.json`.


### 2026-10-21 Network recovery release (1.0.74 / build 65)

- **Source and builds.** Clean release commit
  `210013f54ad087ace876bba75694c1992847c8de` includes the transport/Vnet repair in
  PR #158 and ADR 0004. macOS Rust/SEP and FFI builds, signed macOS app build,
  iOS FFI and Release archive succeeded. The preceding source verification
  (126 tests, strict Clippy, layer checks, fault suite on all three Macs) is
  detailed in ADR 0004; it is not being counted as installed-device testing.
- **Signing and publication.** Notarization
  `1073b2bd-86bb-414a-826a-bde519d3ffbd` was Accepted. Staple, strict signatures,
  Gatekeeper and release provenance passed. Published through
  `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to the existing desktop and iOS directories.
  Independently downloaded public artifacts:
  macOS ZIP SHA-256 `83ca6eeb7254a85bb3eda5dd29fe7ce8ac68621518a580608fcfd2374de682ee`;
  iOS IPA SHA-256 `c258eafb02fe74c4a7495ab13bd72aa70d0c95bb3eb6f4145c4b86190188f28c`.
  The page, fixed manifest, hash-bound install manifest and IPA report
  1.0.74 (65); mutable endpoints have no-store/no-cache headers.
  OTA: <https://linsheng.dev:8443/muster/>. This publishes the notarized portable
  download; no signed Velopack feed was published in this pass.
- **Three-Mac installation.** Mac-A, Mac-C and Mac-B acquired the Rust update
  lease, installed the same notarized app and retained their device identities.
  Their daemon SHA-256 is
  `65fec14cc3724ebb34d595ae6bf127d31ff4a9936aa8fda6e169135488275634`.
  Each installed bundle passed signature/staple/Gatekeeper and source-stamp
  checks; each daemon reports 1.0.74, accessibility granted, relay registered and
  Vnet active. Mac-C needed the normal-open startup fallback described above;
  its initial installer timeout is retained as evidence, followed by independent
  successful installed/runtime verification. No old app bundle was retained.
- **Mac-B installed acceptance.** The 25-second bounded process I/O pause caused
  observable RPC and virtual-IP failure; peer links recovered immediately after
  resume, without a service restart. Two subsequent same-relay replacements
  advanced transport generations 1→2→3 with one/two-second backoff and retained
  the same daemon, utun7, VIP and one NetHelper TUN descriptor. RPC and real
  virtual-IP traffic passed after each replacement. All six directed RPC/ICMP
  paths passed at the end; Mac-A/Mac-C also verified virtual-IP SSH to Mac-B.
  Detailed timeline, control-connection interruption and limits are in
  [NETWORK_RECOVERY.md](NETWORK_RECOVERY.md).
- **Limits and pending cleanup.** Physical iPhone installation and native chat
  sending were not tested. Installed socket-error injection and the original
  external kernel/the adjacent network extension trigger were not reproduced; real socket/pump faults
  remain isolated regression evidence. Historical public iOS artifacts were
  retained after automatic approval review rejected deletion of 12 obsolete
  IPA/manifest files; latest aliases are verified and unaffected. No security
  component or networking policy was changed.
- **Evidence.** `build/releases/Apple-1.0.74-65/` retains build/notary/public
  checks, each device's installation log, independent final installed checks,
  network matrices and fault/rebuild results. No credentials or runtime state
  are committed.


### 2026-10-21 Desktop Presence release (1.0.76 / build 67)

- **Source.** Release source `25da95a5eca56f860a9e23a7c7ab7ea4be302db1`
  contains desktop candidate registration, administrator-approved revocable
  bindings, console-state routing and bounded cancellation receipt preservation.
  [ADR 0006](adr/0006-desktop-presence-and-console-routing.md) and
  [enrollment operations](PRESENCE_ENROLLMENT.md) describe the trust boundary.
  258 Rust tests passed, one existing test was ignored; strict Clippy, the
  16-crate layer check and ADR gate passed. The diagnostic follow-up reran
  80 app tests successfully (one existing ignored) and strict Clippy.
- **Installed defect found and repaired.** 1.0.75 (66) was initially published
  and installed, but Mac-B native startup logged `keychain error -34018` and
  no desktop registered a Presence candidate. 1.0.76 adds the matching
  Developer ID G2 profile and application/keychain entitlements. The release
  guard rejects the old artifact and accepts the final package; it also verifies
  the profile's certificate and validity. No software key substitutes for Presence.
- **Build and release.** Both Apple builds succeeded. Notarization
  `ea746209-b006-4371-b540-c5c59c5313c0` was Accepted; staple, Gatekeeper,
  strict signatures, source provenance and Presence signing checks passed.
  Published both platforms through `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to the existing
  directories. Public macOS SHA-256:
  `3a391969a0839717dd92a43605556fa87a54e69b1e86e3b838b13114b86983a9`;
  iOS SHA-256:
  `2d2d8b3abe86d57d7e7bed3393b2ba84e91c520e7c5a9fd774f7e2ae0fd6a242`.
  The OTA page and manifest report 1.0.76 (67), mutable URLs are no-store,
  superseded manifests resolve to build 67 and the 1.0.75 IPA returns 410.
  OTA: <https://linsheng.dev:8443/muster/>. The portable download is notarized;
  no signed Velopack feed was published.
- **Installed runtime.** Mac-A, Mac-C and Mac-B acquired update leases and
  installed the identical daemon, SHA-256
  `a972c2c0c10426e728a98e139f1bfe5f1f5c74854afcfb3ce7d9749b84b6a0a7`,
  with unchanged device identities. Each independently passed signature,
  staple/Gatekeeper and version checks; accessibility, relay and Vnet are healthy.
  All three now report `candidate_registered=true enrolled=true`; Mac-B's
  missing locked.control was explicitly granted without changing other scopes.
  Six directed RPC and six directed virtual-IP ICMP checks passed.
- **Desktop acceptance (2026-10-21, 13:02 CST).** Mac-B's installed Muster
  desktop submitted one message to Mac-A Claude session
  `3e9c6153-3ed4-57ae-8024-981cd2f7ec47`, with both consoles unlocked. Target
  operation `app-1792558929277155000-1` is `agent.send / applied`; the native
  send trace confirms a provider receipt at 05:02:11 UTC. Claude replied
  `MUSTER76-Mac-B-Mac-A-1021-A 收到。`; Mac-B displayed the reply and cleared
  the composer. No retry was issued.
- **Locked desktop acceptance (2026-10-21, 13:09 CST).** After an earlier
  challenge expired without execution, the user explicitly requested another
  attempt. Mac-B submitted the retained dedicated Claude draft once. Target
  operation `app-1792559367600704000-3` passed native Presence verification,
  temporary unlock and provider receipt, becoming `applied` at 05:09:33 UTC.
  Claude replied `MUSTER76-Mac-B-Mac-A-LOCKED-1021-B 收到。`; the Mac-B composer
  cleared. The queue and AX work drained, the gate completed relocking and
  released the display guard; a fresh OS observation confirmed Mac-A locked.
  No unknown-outcome message was retried and the 15-second challenge was unchanged.
- **Acceptance boundary.** Physical iPhone installation and reuse of an existing
  compatible Locked Use owner remain untested on devices in this release.
  Temporary-owner cleanup above does not establish existing-owner preservation.
- **Evidence.** `build/releases/Apple-1.0.76-67/` retains source/build/notary
  logs, public checks, signing rejection regression, per-device installation and
  enrollment evidence, `installed-final.json` and `network-matrix.json`.


### 2026-10-21 Desktop manual Locked Use and delayed NetHelper (1.0.77–1.0.78)

- ADRs [0007](adr/0007-desktop-manual-locked-use.md) and
  [0008](adr/0008-complete-delayed-vnet-startup.md) record decisions, tests and
  separate installed acceptance, including the earlier misdirected test.
- 1.0.77 (68), source `bc122a0e3dbbcedcd66062d9d1448c8af060bf3b`, exposed desktop
  manual Locked Use through native Presence. All three Macs installed it; the
  Mac-A Vnet acceptance failed because the old retained helper job had no
  automatic consumer. It is superseded by 1.0.78, not counted as a network fix.
- 1.0.78 (69), source `a299568898ccb9699b5edad4381f3b5f736ffdf0`, gives Vnet one
  tracked startup task that completes after caller timeout. Shutdown cancels
  its observer and rejects late publication while owned descriptors clean up.
  All 51 musterd library tests, strict Clippy, layer and ADR gates passed;
  helper regressions additionally passed in isolated processes on both remote Macs.
- macOS and iOS release builds passed. Apple notarization
  `710f39a4-d2a2-4e98-9405-aff6d8bd2a6c` was Accepted. The desktop portable package
  passed strict signing, Presence provisioning, source/fingerprint, staple and
  Gatekeeper checks. Public downloads matched version/build and these hashes:
  macOS `a5f302b5e7313d3c92125ebb3c3a5ad7ed33c6e94424daab11f4c44d401f7e79`;
  iOS `2b15139c4f0c16535da0ef4fb4250ee41ec87d13dcbe99e333c09e33d99aff31`.
  Installed daemon SHA-256:
  `b9e96d9eed92308a19f8585c36647292859eb6485dbaa4c354ac38b84d905c52`.
- Published through `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}` to the existing desktop/iOS
  directories. OTA is https://linsheng.dev:8443/muster/. The three obsolete
  1.0.76 files and later the three obsolete 1.0.77 files were each deleted only
  after the user's separate explicit approval; 1.0.78 and aliases were retained.
- Mac-A, Mac-C and Mac-B installed 1.0.78 with their identities unchanged,
  Accessibility granted, relay registered and Vnet up. Mac-B's CloudKit DNS
  lookup failure used the verified archive plus native Gatekeeper notarization
  assessment as described in ADR 0008; no DNS or security setting changed.
- Mac-A actually exceeded the three-second helper deadline and completed at
  3.257 seconds automatically. All six RPC/ping paths passed. Four SSH paths
  targeting Mac-C/Mac-B passed. Mac-A's local SSH service was not listening;
  the other two SSH paths remain unavailable, independently of successful Vnet ping.
- On Mac-B, two dedicated Claude sends received matching replies under one
  unchanged manual owner/nonce. Explicit end confirmed Mac-A locked, owner
  cleared and cover released. See ADR 0007 for operation IDs and timestamps.
- No physical iPhone installation/UI acceptance, new original-kernel-fault
  reproduction, or new Velopack feed/TestFlight publication is claimed. Existing
  transport recovery decisions remain ADR 0004; delayed helper completion is
  the new installed fault scenario accepted here.


### 2026-10-22 Native sidebar release (1.0.79 / build 70)

Published the merged PR #168 corrections on both Apple platforms. All three
Macs installed 1.0.79 through the Rust update lease; public iOS OTA build 70
was verified at <https://linsheng.dev:8443/muster/>. Signing, notarization,
source stamps, runtime and network results, plus untested native-send and
physical-iPhone scope, are recorded in [ADR 0010](adr/0010-native-sidebar-metadata.md#published-and-installed-1079-2026-10-22).

### 2026-10-22 Native answer and approval scope (1.0.80–1.0.81)

1.0.80 (71) shipped the merged Claude focused-Submit repair and Codex approval
labels. Mac-B -> Mac-A produced an actual answer receipt and continued reply;
Mac-C's actual Calculator permission card resumed after its explicitly approved
conversation grant. That test found an unrelated review-pane Reject button
being offered as a permission action.

1.0.81 (72) excludes workspace review/sidebar controls from Codex permissions
and revalidates scope and enabled label before dispatch. All three Macs installed
the same notarized artifact; the paired OTA page/manifest/IPA was independently
verified. All six RPC and virtual-IP ping directions passed. The real Mac-C
card retained its consent choices while excluding the review Reject; Deny ended
the new scoped test without granting another app. See [ADR 0017](adr/0017-claude-question-submit-activation.md)
and [ADR 0018](adr/0018-codex-approval-conversation-scope.md) for source hashes,
notary IDs, precise versioned acceptance and limitations. Hosted Actions remains
disabled by owner request; physical iPhone and new Locked Use tests are not claimed.

The subsequent 1.0.81 Claude repeat delivered its prompt but rejected Submit
before activation, including with the window raised. It is not a successful
answer/continuation acceptance. [ADR 0019](adr/0019-submit-focus-sampling.md)
records the investigation and the six explicitly authorized old OTA deletions.


### 2026-10-22 Focus readiness and desktop startup (1.0.82 / build 73)

Published source `366a199fa346e6288578023f79d96c0dc365730b` on macOS and iOS.
Apple notarization `6a38fa3f-6cc6-4304-a124-1840bb3b0881` was Accepted. Public
page -> manifest -> IPA and macOS download checks verified versions, signatures,
source stamp/fingerprints, stapling, Gatekeeper and matching hashes:

- macOS ZIP: `e654317b07baae6e2d837b5ee4b2e4330faa37849bbff4e51c72b9ae2f26e426`
- daemon: `7ffa668b45eec039938b40bec5b9ef9982c026d9bb6b6dd92421a69f9d1e6e98`
- iOS IPA: `0f75c3afb151565981ad56789a94fd8e63776384adf4b70bbcfd8888cdea2468`

Mac-A, Mac-B and Mac-C installed this artifact with identities unchanged;
Accessibility, relay registration and Vnet passed on each. All six RPC and six
virtual-IP ping directions passed. Mac-B -> Mac-A answered the original pending
Claude card C once; provider receipt and continued reply both arrived. See
[ADR 0020](adr/0020-submit-window-focus-proof.md).

The local release driver initially leaked its CLI MUSTER_DIR override into the
GUI, disabling managed-service startup by design. Removing it from the GUI
launch environment fixed the reproduced failure. Keep CLI socket selection
local to CLI invocations (prefer `--socket`) and launch the installed GUI with
`env -u MUSTER_DIR open /Applications/Muster.app`. Wait for the old GUI to exit
before swapping its bundle. The native installer now uses the same isolation;
custom developer daemon directories continue to be manually managed. Initial
startup and Try again share the bounded Rust preparation described in
[ADR 0021](adr/0021-desktop-startup-retry.md).

Actions remains disabled. iOS OTA publication is not physical-iPhone acceptance.
No new production retry fault injection, kernel-network fault injection,
Locked Use cycle, or Mac-C consent grant is claimed here; earlier permission
acceptance remains ADR 0018. No additional old OTA files were deleted.

### 2026-10-26 Web window and ADR renumbering on three Macs (1.0.94 / build 85)

Published source `fdb9c024bf02215dbaa777338a51946a750ad31f` on macOS and iOS,
the merge of the two branches that were open against `main`: the web window
([ADR 0049](adr/0049-web-window-onto-one-origin.md), renumbered from a taken
0026) and the ADR renumbering of
[ADR 0025](adr/0025-ignore-the-scheduler-token-in-the-working-tree.md).
Releases 1.0.83–1.0.91 were recorded in their own ADRs rather than here;
1.0.92 (83) and 1.0.93 (84) were built but never published.

Apple notarization `60fa0735-122b-4901-9e97-da756cbac003` was Accepted for the
signed bundle, and `c9802de5-1457-4818-a0a5-a0d292d9dc62` for the Velopack
re-sign, which is a second ticket because packing re-signs the outer bundle.
`--signDisableDeep` preserved the daemon's own signature byte for byte, so the
daemon in the published package is the same binary the notarized ZIP carries:

- macOS portable ZIP: `45cba2a152ff77514bf9b1c40d91d8ad9c85ef39bb1a9fc91e12776b65273453`
- macOS full nupkg: `3281f8fc8e310b4c3023853ad700b0f5b972053032034a340c49c7f582e4cdc7`
- daemon: `458c572cb2a016b749ae9943e2a369c6e9df66d82d9415285edb70671e9f98dd`
- iOS IPA: `cf890b3b61d61ace9a4eee96b1e6269a05e59e5841c88536bdd6b636b9843629`

**The signed desktop feed is back.** `releases.osx-arm64.json` had answered 404
since the 1.0.91 publication, because no .NET runtime was available for `vpk`
and `prune-releases.py` removes a feed that does not match the latest portable
version. In-app desktop updates were unavailable fleet-wide for that whole
period. The feed signature was verified the way the updater verifies it, over
`UTF8(filename) + LF` and the exact served JSON bytes against the compiled-in
`release-public.pem`: "Verified OK", with a version-altered feed rejected.

Mac-A, Mac-B and Mac-C all installed the published portable bundle. Each one's
installed daemon hashes to the value above, so all three run the published
binary and not a local rebuild — which matters, because before this release the
three machines ran two *different* builds under the one version string 1.0.92.
Every device identity, fingerprint and Secure Enclave key backend is unchanged;
Accessibility is granted, Vnet up and the relay registered on each; each machine
sees the other two as `direct`. All six virtual-IP ping directions pass, 24 of
24 packets with no loss. Both merged changes are demonstrably in the shipped
binary rather than only in source: web-window string literals are present in the
installed daemon, and it advertises the `agent.send.status.v1` service that
1.0.92 did not.

The paired iOS OTA build 85 was published and verified through the public page:
both IPA aliases hash to the value above, the manifest binds build 85 to the
hash-named IPA, cache headers are `no-store`, historical manifest URLs redirect
to the latest and removed package URLs answer 410. 36 superseded iOS files were
removed on explicit authorization, leaving the seven current ones.

Not claimed. No provider conversation was exercised, so there is no send,
receipt or continuation evidence for this build, and the web window has no
acceptance of its own on any device. Physical iPhone installation remains
unverified. Hosted Actions remain disabled, so the absence of hosted checks is
not a CI pass. Three operational findings from this rollout — the Launch
Services versus relaunch distinction, never installing over the Vnet address,
and transient `stapler validate` failures — are recorded under "Packaging"
above, and the release itself in [ADR 0050](adr/0050-paired-release-1-0-94.md).
