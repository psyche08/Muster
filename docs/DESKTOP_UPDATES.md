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

Dated release records from 2026-09-29 (desktop 1.0.15) to 2026-10-26
(1.0.94) are in [validation/DESKTOP-RELEASES-2026-09-29-TO-10-26.md](validation/DESKTOP-RELEASES-2026-09-29-TO-10-26.md).
Later releases are recorded in their own ADRs; add new ones there, not here.
