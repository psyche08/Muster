# ADR 0050: Land the outstanding branches and release them as 1.0.94 (85)

- Date: 2026-10-26
- Status: Accepted
- Related: [ADR 0049](0049-web-window-onto-one-origin.md);
  [ADR 0025](0025-ignore-the-scheduler-token-in-the-working-tree.md);
  [ADR 0047](0047-transcript-reading-anchor-parity.md);
  [ADR 0048](0048-admitted-send-acknowledgement.md);
  [desktop updates runbook](../DESKTOP_UPDATES.md)

## Context

Two reviewed branches were open against `main` and neither could merge on its
own: the ADR renumbering of PR #2 and the web window of PR #6. Both conflicted,
and PR #6 claimed request field `68` in `muster.proto`, which `main` had already
given to `agent_send_status` in a build that is installed on all three Macs.

At the same time the release ledger had fallen behind the devices.
`released-commits.txt` ends at 1.0.91 (82). 1.0.92 (83) was built and installed
but never published; the three Macs are in fact running two different 1.0.92
builds (`66a39fbf` on one, `93529e00` on the other two), which is exactly the
hazard the ledger exists to catch. 1.0.93 (84) was built, notarized and recorded
in ADR 0047 with exact hashes, but was never published and its retained
artifacts no longer exist. The public download page still offers macOS 1.0.91
and iOS build 82, and `releases.osx-arm64.json` returns 404, so in-app desktop
updates are currently unavailable fleet-wide.

The owner asked for the outstanding work to be merged and the latest version
packaged and delivered.

## Decision

Merge both branches into one release line and publish it as paired version
**1.0.94 (85)**.

Resolve the conflicts so that no shipped wire number moves: `agent_send_status`
keeps request field `68` and the web window's `forward_open` takes `69`. Its
response variant `forward_opened` keeps `68`, which was free in that oneof. The
checked-in Swift protobuf is regenerated from the resolved `.proto` rather than
hand-merged. The web window's record is renumbered from the taken 0026 to
[ADR 0049](0049-web-window-onto-one-origin.md), and every reference that belongs
to it moves with it; references to the unrelated ADR 0026 stay as they are.

Do not reuse 1.0.93. ADR 0047 records that number against exact ZIP and IPA
hashes, and this tree's content differs from it. A version number must keep
meaning what it meant when it was recorded.

Publish what can be published honestly. Velopack 1.2.158 cannot run here: `vpk`
is installed but no .NET runtime exists on any machine in the fleet, and the
owner declined installing one for this release. So this release publishes the
iOS OTA half only, and the macOS half stops at a locally built, signed and
notarized artifact. The unified download page therefore keeps advertising the
macOS version that is actually published, not this one.

## Alternatives

Merging only PR #2 would have left the web window unreleased and its conflict
to be resolved again later against a moved target. Renumbering
`agent_send_status` instead of `forward_open` would have changed a field number
that installed daemons already speak. Hand-resolving the generated protobuf
would have risked a file that no longer matches its source. Publishing a plain
notarized ZIP under the existing `Muster-osx-arm64-Portable.zip` name would have
broken the Velopack metadata that installed clients bootstrap from. Claiming
1.0.94 on the download page without publishing it would have made a public page
state something untrue.

## Consequences

`main` carries the web window and a unique ADR number per record again. The
desktop fleet stays on 1.0.92 and in-app desktop updates stay unavailable until
a .NET runtime is installed and a signed Velopack feed is published again; the
ledger does not gain a 1.0.94 desktop entry, because no desktop package was
published. The web window ships to devices without ever having had a dedicated
on-device acceptance pass of its own, which remains open on ADR 0049. The split
1.0.92 installations are recorded here but not repaired by this release.

## Verification

Source verification on 2026-10-26, from the merged tree before the version bump:

- `cargo check --workspace --all-targets` clean; `cargo test --workspace` passed
  564 tests with 0 failures and 7 ignored.
- `src/muster/scripts/check-layers.py`: 18 crates follow the layer map.
- `scripts/check-public-content.py`: 706 tracked text files, 0 findings.
- `scripts/check-adr.sh origin/main`: records present.
- `src/browser/agenthalo-extension/adapter.test.cjs`: 8 pass, 0 fail.
- The Swift protobuf was regenerated with `scripts/gen-protos.sh` (protoc with
  `protoc-gen-swift`), not edited by hand.

Build, signing, notarization, publication and installation are recorded below as
they happen. Nothing about them is claimed by this commit.

### Built, signed and notarized macOS 1.0.94 (85) — not published, not installed

Clean source `fdb9c024bf02215dbaa777338a51946a750ad31f`. The release daemon
carries exactly that one `MUSTER-BUILD-COMMIT` stamp and `verify-release.sh`
accepted the bundle: presence signing `89LGY6BD53.dev.linsheng.muster`, release
ancestry over `released-commits.txt`, and every string in
`release-fingerprints.txt`.

Signed with `Developer ID Application: Sheng Lin (89LGY6BD53)` and the installed
`Muster Developer ID Presence G2` profile, so the device identity and hardware
key backend are undisturbed. `OTHER_CODE_SIGN_FLAGS=--timestamp` put a secure
timestamp on the outer bundle as well as the nested helpers, so notarization was
attempted once. Apple notarization `60fa0735-122b-4901-9e97-da756cbac003` was
Accepted; stapling and `stapler validate` passed, `spctl -a -t install` reported
"accepted, source=Notarized Developer ID", and strict deep `codesign --verify`
passed after stapling.

- notarized and stapled ZIP: `d1c898b233148aaf37066241f56dbb4657ded29c8a62c83f934dafd1015ad076`
- daemon: `458c572cb2a016b749ae9943e2a369c6e9df66d82d9415285edb70671e9f98dd`

At this point no Velopack package or desktop feed existed yet, because no .NET
runtime was available for `vpk`. That was resolved later in the same day and the
desktop half was published; see "Desktop published with a restored signed feed"
below.

### Published and publicly verified iOS 1.0.94 (85)

The iOS Release archive is 1.0.94 (85), development-signed through automatic
signing for team `89LGY6BD53`, and passed strict deep verification. Its profile
covers 4 registered devices and expires 2027-10-25. `package-ota.py` prepared
the seven OTA outputs; both recorded checksums verified, each fixed alias is
byte-identical to its hash-named file, and the manifest binds build 85 to the
hash-named IPA.

- IPA: `cf890b3b61d61ace9a4eee96b1e6269a05e59e5841c88536bdd6b636b9843629`

Published to the iOS release directory in the runbook's order — hash-named IPA
and both hash-named plists first, then the fixed aliases, page and
`SHA256SUMS` — and verified from the public HTTPS URLs afterwards:

- all seven files download; both the fixed and the hash-named IPA hash to the
  value above;
- the fixed manifest names build 85 and the hash-named IPA;
- the page offers iOS 1.0.94 build 85 and, truthfully, macOS 1.0.91, which is
  the version actually published;
- `Cache-Control: no-store, no-cache, must-revalidate, max-age=0` on the entry
  pages, the fixed IPA and the manifest;
- two historical hash-named manifest URLs, including the previous build's,
  answer `302` to the latest fixed manifest, and a removed hash-named IPA URL
  answers `410`.

### Superseded iOS releases removed on the owner's explicit authorization

The owner authorized the cleanup after the publication checks above.
`prune-releases.py` was re-run read-only immediately beforehand: it revalidated
the live aliases, listed the same 36 files, and the new build's hash appeared
nowhere in that list. `--apply` then removed 36 superseded files on the
publisher — 12 hash-named IPAs with their 12 `manifest-` and 12 `install-`
plists — leaving exactly the seven current files. The desktop directory was not
touched; its 1.0.91 portable ZIP, `SHA256SUMS` and the `index.html` symlink to
`../ios/index.html` are unchanged.

Public behaviour was rechecked afterwards. The current IPA, both its aliases,
the fixed manifest, the install plist, the page and `SHA256SUMS` all still
serve, and both IPA aliases still hash to the value above. Three removed
hash-named IPA URLs answer `410`, and removed historical `manifest-` URLs answer
`302` to the fixed manifest, whose bytes are identical to what was published.
The current build's own `manifest-<hash>.plist` URL also answers `302` to the
fixed manifest, by the same nginx rule, and resolves to identical bytes; its
`install-<hash>.plist` still serves directly with `200`, which is what an
unchanged-install retry needs. All four entry points and the macOS download
answer `200`.

### Installed on Mac-B and Mac-C as well

Both remaining Macs installed the same published portable bundle, each with the
lease granted, the old GUI exited, its launchd service booted out and only the
verified bundle replaced. Both verify the way Mac-A did: the installed daemon
hashes to `458c572c…f98dd`, so all three machines now run the published binary
rather than a local rebuild. That is worth stating plainly, because before this
release the three ran two different builds under the single version string
1.0.92 (`66a39fbf` on Mac-A, `93529e00` on the other two); the fleet is now on
one artifact again.

Each daemon reports version 1.0.94 and build commit `fdb9c02` with its device
id, fingerprint and `sep` key backend unchanged from the identity captured
immediately before its own replacement. Accessibility is granted, Vnet up and
the relay registered on all three, and each machine sees the other two as
`direct`. All six virtual-IP ping directions pass, 24 of 24 packets, no loss.

Three operational findings, all recorded in the runbook's "Packaging" section:

The Mac-B rollout cut its own connection. Its SSH route is its Vnet address, so
stopping musterd tore down the path mid-script, right after the launchd bootout.
The bundle replacement had completed, but the launch and verification had not,
leaving Mac-B with the new bundle, no running daemon and no route back. It was
recovered over its physical bridge address, which does not depend on Muster. A
fleet installation must address members by an address Muster does not provide.
`launchctl asuser <uid> open` is not a workaround from such a session: it needs
root and fails with "Could not switch to audit session".

Waiting ten seconds before the single `open`, instead of three, started the GUI
first time on both Mac-B and Mac-C. That confirms the diagnosis of the Mac-A
stall: the failure was Launch Services not yet having a usable registration for
the replaced bundle, not a launched app failing to register its daemon, and the
two conditions need different handling.

`xcrun stapler validate` failed once on Mac-C against bytes it had just verified
as the published daemon, then succeeded with `-v` after fetching the ticket from
Apple, while `spctl -a -t install` accepted the same bundle as "Notarized
Developer ID" throughout. The gate was left as it is and the installation simply
re-run; a transient ticket lookup is not a reason to weaken a signing check.

### Not claimed

No Mac ran this build, so there is no installed runtime, relay, Vnet, RPC or
ping evidence for 1.0.94, and no acceptance of the web window
([ADR 0049](0049-web-window-onto-one-origin.md)) or of
[ADR 0047](0047-transcript-reading-anchor-parity.md) and
[ADR 0048](0048-admitted-send-acknowledgement.md) on hardware. iOS OTA
publication is not physical-iPhone installation or acceptance; the phone was not
exercised. No TestFlight or App Store Connect upload was made. Hosted Actions
remain disabled, so the absence of hosted checks is not a CI pass. The split
1.0.92 installations (`66a39fbf` on one Mac, `93529e00` on the other two) are
recorded here and remain unrepaired.

### Desktop published with a restored signed feed

The owner authorized installing a .NET runtime on one machine so `vpk` could
run. Microsoft's official user-local installer put the .NET SDK under `~/.dotnet`
with no `sudo`, and Velopack CLI 1.2.158 was installed from nuget.org as a global
tool; a small wrapper pins `DOTNET_ROOT` so the pinned-version check in
`package-macos.sh` passes. Nothing outside `~/.dotnet` was touched.

Packaging ran against a tree detached at the build commit
`fdb9c024bf02215dbaa777338a51946a750ad31f`, so `verify-release.sh`'s
HEAD-equals-stamp rule held, and it accepted the staged bundle again: presence
signing, release ancestry and every fingerprint. `vpk pack` re-signed the outer
bundle with the same Developer ID identity under `--signDisableDeep`, so the
daemon's own signature was preserved byte for byte — the packaged daemon still
hashes to `458c572cb2a016b749ae9943e2a369c6e9df66d82d9415285edb70671e9f98dd`,
the same binary the notarized ZIP carries. Because the re-sign invalidates the
earlier ticket, Velopack notarized the bundle again:
`c9802de5-1457-4818-a0a5-a0d292d9dc62` Accepted, stapled, and verified with
`spctl`. The app inside the published portable bundle reports 1.0.94 (85),
carries the `fdb9c024` stamp, validates its staple, is accepted by Gatekeeper as
"Notarized Developer ID", passes strict deep `codesign --verify`, and passes
presence signing.

- portable ZIP: `45cba2a152ff77514bf9b1c40d91d8ad9c85ef39bb1a9fc91e12776b65273453`
- full nupkg: `3281f8fc8e310b4c3023853ad700b0f5b972053032034a340c49c7f582e4cdc7`

Published in the runbook's order — the nupkg, portable ZIP and `SHA256SUMS`
first, then `releases.osx-arm64.json` and its `.sig`. Verified afterwards from
the public HTTPS URLs: all five files answer `200`, the downloaded portable ZIP
and nupkg hash to the values above, the served feed names the nupkg with that
SHA-256 and a matching byte size, and `Cache-Control: no-store` is set on the
feed and both packages.

The feed signature was verified independently, the way the updater does it: over
`UTF8("releases.osx-arm64.json") + LF + the exact served JSON bytes`, against
`muster-app-core/src/desktop_update/release-public.pem`, the key compiled into
the app. `openssl` reports "Verified OK". A negative control — the same
signature against a feed with the version altered — reports "Verification
failure". The private release key is not served: three plausible key paths under
the desktop directory answer `404`. In-app desktop updates are therefore
available again, after `releases.osx-arm64.json` had been absent (404) since the
1.0.91 publication.

`released-commits.txt` now records 1.0.94 against `fdb9c024`, because the
desktop package is published.

This release also fixes `package-macos.sh`, which could not have produced this
package as written: it passed `--signEntitlements` a bare `mktemp` path, and vpk
1.2.158 refuses an entitlements path that does not end in `.entitlements`. The
script now exports into a `.entitlements` file inside a temporary directory. It
was also checked in non-executable while its siblings were not, so it is now
`100755`. This release's package was produced by running the script's exact
steps by hand, with the fix applied only to the repository, so that the
packaging tree could stay clean and equal to the build commit.

### Installed and accepted on Mac-A

Mac-A installed the published portable bundle through the attended path. The
Rust desktop-update lease was granted (no protected session, console drained),
the old GUI exited, its user launchd service was booted out, and only the
verified bundle was replaced. Pairing and store state under Application Support
survived untouched.

The installed artifact is the published one, not a rebuild: the installed
daemon hashes to `458c572cb2a016b749ae9943e2a369c6e9df66d82d9415285edb70671e9f98dd`,
the same binary the notarized ZIP and the published portable bundle carry.
`/Applications/Muster.app` reports 1.0.94 (85), its Velopack `sq.version` reads
1.0.94 so the updater's recorded version agrees with the bundle, and the install
validates its staple, is accepted by Gatekeeper as "Notarized Developer ID",
passes strict deep `codesign --verify` and passes presence signing
`89LGY6BD53.dev.linsheng.muster`.

The running daemon was asked to describe itself rather than inferred from the
binary. It reports version 1.0.94 and build commit `fdb9c02`, with device id
`3508fbf9…6196d`, fingerprint `GUEP-X6MF-6FUR-UXBK` and key backend `sep`
(Secure Enclave) all unchanged from the pre-install baseline, so the device
identity and hardware key backend were not disturbed. Accessibility is granted,
`locked` false, `console` true, Vnet up on `192.168.13.11`, relay registered.

Both merged changes are demonstrably in the shipped binary, not merely in the
source: three distinct web-window string literals from
[ADR 0049](0049-web-window-onto-one-origin.md)'s `musterd/src/web.rs` are
present in the installed daemon, and the daemon advertises the
`agent.send.status.v1` service that [ADR 0048](0048-admitted-send-acknowledgement.md)
added, which the 1.0.92 baseline did not advertise.

Links to the other two Macs recovered and both outbound virtual-IP pings pass:
4/4 packets with 0% loss to `192.168.13.12` and `192.168.13.13`, minimum
round-trips 1.271 ms and 0.724 ms. Worth recording honestly: 21 seconds after
startup the Mac-C link still showed `relay 15ms`, and only after about 45
seconds did both settle to `direct` (9 ms and 23 ms). A link column read
immediately after a restart is not evidence of a transport regression.

Startup did not complete on its own. Three seconds after the replacement a
single normal `env -u MUSTER_DIR open` did not materialize the GUI, so no
daemon registered and the launchd service stayed absent; the owner then opened
Muster normally and everything above followed. This is the documented hazard of
inferring startup from `open`, and it exposed a contradiction worth writing
down: this runbook says to issue **one** `open` and not to loop GUI relaunches,
while `MusterApp/scripts/install-mac.sh` retries `open` every two seconds up to
five times at the same point, commenting that Launch Services needs a moment
after a bundle replacement or returns -609. Those address different conditions —
the prohibition is on relaunching the GUI when a launched app fails to register
its daemon, not on Launch Services having no usable registration yet — and a
script that conflates them stalls exactly as this one did.

### Not claimed

No cross-device RPC was exercised: the bundled CLI addresses only its own
daemon, so each machine's RPC was verified locally and the transport between
them by the six ping directions, not by a request crossing devices. No provider
conversation was exercised, so there is no send, receipt or continuation
evidence for 1.0.94, and the web window of ADR 0049 still has no acceptance of
its own on any device — it shipped to the fleet without one. Physical iPhone
installation and acceptance remain unverified, as for every earlier release. No
Locked Use cycle, fault injection or new consent grant is claimed here.
