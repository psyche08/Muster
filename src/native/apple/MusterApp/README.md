# MusterApp (macOS + iOS)

One SwiftUI package for both Apple apps (ARCHITECTURE §3.7). The Mac app
talks to `musterd` over its socket; the iPhone app embeds the core's own
link (decision D8).

```
Sources/CMuster          the C ABI (symlink to src/muster/crates/muster-ffi/include/muster.h)
Sources/MusterCore       MusterRuntime (bytes over the ABI), MusterCore (typed intents/effects),
                       Keys (Secure Enclave device + presence keys), TerminalStyles,
                       Generated/ (SwiftProtobuf models from app.proto + muster.proto)
Sources/MusterUI         MusterModel (effects), MusterRootView, one file per screen group,
                       TerminalGridView (Canvas + key capture), QR scan/render
Sources/MusterDev        `swift run MusterDev` for development
Apps/{macOS,iOS}       Muster.app's entry points per platform (XcodeGen, project.yml)
Tests/MusterCoreTests    the wrapper against the real Rust core
```

## Conversation drafts and navigation

Shared Rust saves unsent composer text per network and conversation in its
local cache. Agent drafts also include the host; desktop Chat drafts follow
the conversation across hosts. Returning to a conversation restores its text.
Only an applied/duplicate send receipt clears the exact submitted revision;
failed or uncertain sends and newer edits retain their text. Attachments remain
in the current composer and are not persisted with text drafts.

Codex queued messages appear above the composer with a right-aligned Send now
action. Rust reads both the app-server SQLite queue and older desktop follow-up
storage without modifying either. Send now uses the native queue row's Steer /
Send now control through the existing session-bound Locked Use response path;
it does not copy the message into the composer. Queue admission confirms a
normal send was accepted, while Send now still waits for a provider receipt.

On iOS, edge-back gestures animate the outgoing page with the finger. A short
or cancelled gesture returns it to place; a completed gesture emits one Rust
Back intent and reveals the destination when the core supplies it.

## Build

```sh
# 1. The Rust core for the Apple targets
scripts/build-ffi.sh

# 2. The Xcode project
brew install xcodegen
xcodegen generate            # → Muster.xcodeproj: targets MusterMac and MusteriOS, both Muster.app
open Muster.xcodeproj
```

Both targets carry the bundle id `dev.linsheng.muster` (one product on
both platforms; it is also the APNs topic the relay's push hook uses).
Signing: set your team on both targets; the iOS target needs the
`aps-environment` entitlement only if you want push wake-ups (Settings →
the relay's push hook).

Development without Xcode (macOS or Linux):

```sh
MUSTER_FFI_LIB=$PWD/../../../muster/target/debug/libmuster_ffi.a swift test
swift run MusterDev            # macOS only; expects musterd's socket in ~/Library/Application Support/Muster
```

## Regenerating the protobuf code

`Sources/MusterCore/Generated` is checked in. After changing the schema:

```sh
brew install swift-protobuf    # protoc-gen-swift matching Package.resolved
scripts/gen-protos.sh
```

CI rebuilds the generator at the resolved version and fails if the files
differ.

## Where the files are (macOS)

The daemon ships inside the bundle and runs as a LaunchAgent the app
registers on first launch (SMAppService; approve it under System Settings
› General › Login Items if asked):

| | |
|---|---|
| `Muster.app/Contents/MacOS/musterd` | the daemon (copied by the MusterMac post-build script from `src/muster/target/release/musterd`; build it with `cargo build --release -p musterd --features sep`) |
| `Muster.app/Contents/Helpers/muster` | the command line, same origin; `alias muster=/Applications/Muster.app/Contents/Helpers/muster` (not next to the app executable: `muster` and `Muster` collide on a case-insensitive volume). Every Muster process on the Mac runs from the installed bundle, none from a build tree |
| `Muster.app/Contents/Library/LaunchAgents/dev.linsheng.musterd.plist` | its LaunchAgent (`Apps/macOS/dev.linsheng.musterd.plist`) |
| `~/Library/Application Support/Muster/` | store (`musterd.db`), socket (`musterd.sock`), the app's own state |
| `~/Library/Logs/Muster/musterd.log` | the daemon's log (`musterd --log`) |

The relay is set in the app (Settings › Relay) or with
`muster relay muster.linsheng.dev:9443`; the daemon remembers it.

Keys: the device key and the presence key are Secure Enclave keys whose
blobs sit in the data-protection keychain, owned by the app's team and
bundle id, so launching (or a rebuilt build) never asks for the login
keychain password. Touch ID / Face ID is asked only when the presence
key signs, which the core requests for high-risk actions on *another*
device (approving or removing a member, changing its scopes or policy);
a Mac administering its own daemon over the socket never prompts.

Development builds: sign the Mac target with your team
(`DEVELOPMENT_TEAM=… CODE_SIGN_STYLE=Automatic`); an unsigned build has
no application identifier, so the keychain refuses it and the app runs
with software keys. After every rebuild launchd still holds the previous
build's code requirement for the agent (`spawn failed`, EX_CONFIG); the
app re-registers it when the socket does not appear, and
`launchctl bootout gui/$(id -u)/dev.linsheng.musterd` before launching
does the same by hand.

## Installing on this Mac

```sh
scripts/install-mac.sh     # Release build + Developer ID signing + App replacement; root helper installs Locked Use
```

The App registers its bundled root helper with SMAppService. macOS may require
one-time approval in System Settings > General > Login Items. The Rust daemon
then requests and verifies Locked Use plugin installation through XPC; no sudo
command or package installer is used.

/Applications and a Developer ID signature give TCC and launchd a stable
identity: the Accessibility grant (System Settings › Privacy & Security ›
Accessibility › musterd) and the LaunchAgent survive rebuilds. A build
from DerivedData signed with a development certificate works too, but
every rebuild re-registers the agent and may lose TCC grants.

## Running it by hand

```sh
cd ../../../muster && cargo build --release -p musterd -p muster-cli -p muster-relay --features musterd/sep
target/release/muster-relay --udp 127.0.0.1:9443 --ws 127.0.0.1:9444 &
MUSTER_DIR=$HOME/.muster-dev target/release/musterd --relay 127.0.0.1:9443 --loopback &
MUSTER_DIR=$HOME/.muster-dev open build/DerivedData/Build/Products/Debug/Muster.app   # or Run in Xcode
```

With `MUSTER_DIR` set the app does not register the LaunchAgent and talks
to that directory's socket (keep the path short: macOS limits socket paths
to 104 bytes). In the iPhone simulator,
`xcodebuild ... CODE_SIGN_IDENTITY=-` (ad-hoc, not `CODE_SIGNING_ALLOWED=NO`):
the keychain refuses an unsigned app (`-34018`) and the Secure Enclave
device key then falls back to software. The simulator has no camera, so
the join sheet shows the paste field; `muster invite` prints the text.

## What is verified where

- Linux CI: `MusterCore` (ABI wrapper, config, styles, keys' software path)
  against `libmuster_ffi.a`: create a network, toasts, lifecycle, push token,
  SSH key line, navigation; the keyboard's pinyin engine, MacBook layout and
  caret arithmetic, and the half-fold workspace geometry.
- macOS (Xcode 27) and the iPhone simulator: both apps build and run;
  onboarding, create / invite / join / approve through a local relay,
  device and settings screens, the agent list and a live chat transcript
  read from the Mac, the Mac's iTerm2 sessions drawn on the phone at
  their own size (musterd built with `--features sep` and connected to
  iTerm2's API). The simulator holds a Secure Enclave device key but
  cannot create the presence key (`LAError -1020`), so presence-gated
  actions need a real iPhone.
- Only on a device: Face ID / Touch ID presence proofs, the camera
  scanner, push wake-ups, and the terminal grid's hardware-keyboard
  capture.

### Registered-device IPA download

An already signed iOS Release app can be packaged with its HTTPS OTA manifest:

```sh
python3 scripts/package-ota.py \
  build/LockedUseIOS/Build/Products/Release-iphoneos/Muster.app \
  /tmp/muster-ota-release https://linsheng.dev:8443/muster/ios/ \
  --mac-version 1.0.36
```

`--mac-version` puts the Mac download's version on the page (the packager
cannot see the desktop package); pass the version currently at
`desktop/Muster-osx-arm64-Portable.zip`. To change only the page of an
already published IPA, render it for that IPA's hash-named files with
`render_page` and its original instant (`--published-at` keeps the release
time when packaging again): a new `ditto` archive has a different hash.

Use the externally reachable HTTPS origin, including its port. For linsheng.dev
that is `https://linsheng.dev:8443`; the internal Nginx listener on 443 is not the
public install URL. Both the manifest URL and its IPA asset URL must use 8443.

The page shows the explicit Build number and a fixed release timestamp captured
when packaging. JavaScript formats that timestamp in the visiting device's local
timezone, including the timezone label; refreshing does not change the instant.

The public release address is fixed at `https://linsheng.dev:8443/muster/ios/`
(the packager default). Replace the four published files for subsequent builds;
do not create dated release directories. Verify the page, manifest and IPA
checksum after publishing. Include `deploy/nginx-ios.conf` in the site TLS
server to preserve public port 8443 when redirecting the address without a
trailing slash, and to revalidate cached files at the fixed URL.

Publish over SSH to `${MUSTER_RELEASE_HOST}` on port `${MUSTER_RELEASE_SSH_PORT}`; the iOS web directory
is `${MUSTER_RELEASE_ROOT}/ios/`. Stage and verify the signed package first, keep
only the latest version on the publisher, then publish the IPA
before the manifest, checksum and page. Read back the public HTTPS files and
verify the IPA checksum and manifest build number. Build 11 includes expanded
Working activity and wrapped, numbered, syntax-colored tool details.

Subsequent Mac releases also go through `linsheng.dev`, using the signed
desktop feed at `${MUSTER_RELEASE_ROOT}/desktop/` (public URL
`https://linsheng.dev:8443/muster/desktop/`). Follow
[`DESKTOP_UPDATES.md`](../../../../docs/DESKTOP_UPDATES.md) for packaging and
feed signatures; uploading an App alone does not update that feed.

The output directory must not exist. Publish `Muster.ipa`, `manifest.plist`,
`index.html` and `SHA256SUMS` together at that HTTPS directory. The script checks
code signing and the registered-device provisioning profile, without resigning
or expanding installation rights. Apple Development builds require a registered
UDID and Developer Mode; hosting an IPA does not make it installable by everyone.
Distribution for additional registered devices should use an appropriate Ad Hoc
profile; public testing should use TestFlight. A valid IPA/manifest is not proof
of successful Safari installation on a physical iPhone.

### Home navigation and session order

The list navigation row contains a **Chat / Agents / Terminals** segmented
control alongside network settings; it has no separate header. Switching to
Terminals immediately lists terminals across the current device filter. Both lists share
All devices / device tabs. Shared Rust aggregates results, attaches source device
IDs, rejects stale responses and routes opening a terminal to its source device.
Choose one device to create a terminal or SSH session. Phones remain clients.

Agent lists use the timestamp of the last user/assistant conversation message,
newest first, across both individual and all-device views. Transcript file
mtime, token accounting, tool results, and other metadata cannot bump a session.
Unknown timestamps sort last; equal timestamps have a stable ID tie break.
The daemon caches times for unchanged files and reads changed transcripts
backwards, without loading entire histories into the UI. This requires the
updated daemon on each source Mac, independently of the iPhone UI update.

Codex activity also uses read-only OS evidence: the signed desktop's current
worker must hold the exact catalog transcript open. The latest turn start/end
record distinguishes running from idle even when its AX page is hidden. Starts
from an older worker launch cannot revive a stale turn. Observations are sampled
every scan and expire after ten seconds; unavailable evidence remains unknown.
This observation never attaches a controller or grants input: sending still uses
Accessibility. Claude uses its desktop worker registration, verified against
the signed process and its UTC launch generation. The newer of the registration
status and transcript activity wins, with AX approval controls as a supplement.
A live Claude GUI alone never marks all previously visited sessions as running.
The list badge shows work activity, not mere process existence. Claude plain-text
replies stay visible while tool calls/results remain folded. Initial chat loading
and reconnect failures are rendered explicitly rather than leaving a blank page.

### Mobile conversation layout

Both iOS styles use a compact, growing follow-up composer. The Codex style adds
a blue user bubble and a title/device subtitle with secondary actions in one menu.
Shared Rust projects a running turn as an expanded `Working` block. Its prose
stays in transcript order, while consecutive tools share one collapsed summary
of commands, file reads, and searches. Expanding that summary reveals individual
calls; expanding a call reveals its arguments and recorded output.

Codex and Claude preserve provider call IDs through `AgentMessage`. Rust pairs
outputs only with a unique matching call in the same tool batch, including
out-of-order results. Missing or reused IDs remain separate; labels do not
claim tool success. Completed work defaults to collapsed with the final reply
outside it. The native shell owns user disclosure overrides and scroll position.
A bottom arrow returns to the latest message when reading history; new messages
follow only when already near the bottom. Every timeline block, including the
usage footer, reserves its full height as one measured row.

The footer reads: end time and model; one line with the turn's input, output
and cache hit; one tappable cache line with the Rust-provided TTL estimate and,
to the minute, when it lapses (or why the TTL is unknown), which opens the
details (requests, cache reads and writes, the exact expiry, first-request hit,
the estimate's basis); then the cost on its own line. HarmonyOS uses the same
layout. Missing expiry fields from an older daemon show
an explicit unavailable state; the client cannot recover a TTL from hit rate.
Both the phone app and the serving Mac daemon must be updated. A merged PR
does not update either installed binary, and Queue/Steer matching runs in the
Mac daemon rather than the phone app.

### Half-fold layouts and the Muster keyboard (iOS)

The iOS shell offers stacked and side-by-side workspaces with a Mac-style
in-app keyboard and the shared pinyin dictionary:

- **Layouts.** Settings → 页面布局 offers 自动, 上下布局, 左右分栏 and 单页; the choice is
  local display state (`UserDefaults` `flexPreference`). Top and bottom keeps
  the transcript (or the terminal grid) above an equal split and puts the
  composer, Locked Use status, queue and the provider's questions in a
  lower pane. Conversations keep a persistent Mac keyboard there. The composer
  keeps its UIKit identity outside the scrolling
  interaction cards when the available height changes, and takes focus with a
  compact keyboard. The reading pane can shrink to keep an attachment and
  keyboard on screen; choosing the stacked layout no longer silently falls
  back to a single pane when the viewport changes. Side by side puts the
  workspace's list on the left and the conversation, terminal, files or Git
  view on the right, split at the middle, each side 280 points or more; a
  row on the left replaces the right pane, so the title bar shows the
  switcher instead of Back. iOS exposes no fold geometry, so Automatic is one
  pane here. The geometry is `MusterCore/Display/WorkspaceLayout.swift`, the
  OS adapter `MusterUI/Display/FoldDisplay.swift`, and
  `MusterRootView` resolves the split for the window and hands it down as
  `\.workspaceLayout`.
- **Keyboard.** Settings → Keyboard → Muster 内置键盘 (off by default)
  replaces the system keyboard in every text
  field (`MusterUI/Keyboard/KeyboardFields.swift`: UIKit-backed fields whose
  input view is the keyboard) and, with its own switch that is on by default
  (Muster keyboard in terminals), in a terminal, where it types straight into
  the session and adds Esc, F1–F12, Ctrl and Alt, which the system keyboard
  lacks. The keyboard button beside the conversation input also selects and
  opens either keyboard. Text-view focus waits for window attachment, and
  changing keyboard type reloads UIKit input views after the UI transaction.
  Every width uses the MacBook ANSI rows (`MusterCore/Input/MacLayout.swift`):
  Esc/F1–F12, symbols/numbers, Tab and Q–P, Caps and A–L, both Shifts and Z–M,
  and fn/Control/Option/Command/Space with the inverted-T arrows. Narrow screens
  use compact symbol legends while retaining the same key positions and widths.
  Function keys are available to terminals; text fields leave them disabled.
  The terminal's Paste button stays visible on iPhone/iPad with either keyboard
  or the keyboard hidden; macOS exposes it in the toolbar alongside standard
  Command-V. Clipboard text goes through Rust's `TerminalPaste` intent for size
  limits, control-character filtering and supported bracketed-paste handling.
  In 上下布局 the keyboard is a persistent lower-pane view, independent of
  first-responder state, attachments and UIKit keyboard dismissal. It edits
  through the same UITextInput adapter and restores focus on a key press.
  Modifiers are sticky (tap: the next
  key; double tap: locked); in a field the arrows move the caret with the
  same arithmetic as HarmonyOS (`MusterCore/Input/TextEdits.swift`, UTF-16
  offsets through `UITextInput`), option by word, command and fn by line or
  to either end, command-A/C/X select all, copy and cut, control gives the
  macOS Emacs keys; in a terminal the keys go to the core as names
  (`escape`, `f5`, `ctrl-c`, `alt-b`, `shift-tab`…). The pinyin engine
  (`MusterCore/Input/PinyinEngine.swift`) splits letters into syllables,
  ranks whole words by frequency, composes a sentence when no word covers the
  letters, lets a shorter word take only its own letters, and raises words the
  user picks; space takes the first candidate, Enter types the letters. The
  dictionary (`MusterCore/Resources/pinyin.txt`, 55,320 GB2312 words from AOSP
  PinyinIME, Apache-2.0; `docs/third-party/aosp-pinyinime-NOTICE.txt`) is a
  package resource and loads off the first keyboard.

The engine, the layout data, the caret arithmetic and the workspace geometry
are pure Swift in `MusterCore`, so `swift test` runs their tests
(`PinyinEngineTests`, `KeyboardLayoutTests`, `WorkspaceLayoutTests`) on Linux
as well; they are ports of the HarmonyOS host tests and pass there. The
SwiftUI and UIKit parts are compiled and exercised by the hosted iOS simulator
tests below; physical-device keyboard acceptance remains separate.

`MusterMobileTests` runs inside the simulator app and captures light/dark chat
and expanded-tool screenshots. It also checks Working's initial expansion,
consecutive tool rows, the empty composer's height in both styles at 320- and
402-point widths, and the geometry of a streamed table/prose reply followed by
usage and cost. TTL snapshots cover known, unknown, and missing expiry data
at both widths. Long unbroken output stays within the phone width. Keyboard
fixtures cover focus before window attachment, switching system/custom input
views, preserving the composer through layout transitions, and following a
reply whose text grows without changing its message ID/count. Rust publishes a
content revision for that last case.

Opening a conversation refreshes its transcript before returning history. An
authorized session watch retains the reader until its last subscriber leaves;
file rotation/truncation invalidates existing history. Visible cloud sessions
refresh every two seconds, independently of catalog polling. Actual content
availability still depends on the provider publishing it.
Build the simulator core with the rustup toolchain, then from this directory:

```sh
xcodegen generate
xcodebuild -project Muster.xcodeproj -scheme MusterMobileTests \
  -destination 'platform=iOS Simulator,name=iPhone 18 Pro' \
  test CODE_SIGN_IDENTITY=- \
  OTHER_LDFLAGS="$PWD/../../../muster/target/aarch64-apple-ios-sim/debug/libmuster_ffi.a"
```

The test core uses an isolated temporary store and software keys. These layout
fixtures do not exercise real peer control, attachments, or physical-device
keyboard/scroll interaction. Simulator signing does not verify release signing,
notarization, or installation on an iPhone.

### Latest-only publishing

Follow the current policy in `docs/DESKTOP_UPDATES.md`: install
`deploy/nginx-ios.conf` for all `/muster/` entry points, publish the latest six
OTA files, then run `scripts/prune-releases.py "${MUSTER_RELEASE_ROOT:?set private release root}" --apply` on
the publisher. Historical manifests resolve the latest alias; obsolete package
URLs are removed. Do not retain an older desktop feed or publisher backups.

## High-CPU reports

The iOS performance monitor automatically records bounded native stacks for hot
Rust and Swift threads. See [capture and offline symbolication](../../../../docs/PERFORMANCE_DIAGNOSTICS.md)
for thresholds, limitations and matching release dSYMs.

## Xcode Cloud (iOS)

The generated `Muster.xcodeproj`, shared `MusteriOS` archive scheme, app plists
and Xcode Swift package lockfile are committed so Xcode Cloud can discover the
product before any build script runs. `project.yml` remains the source of truth;
run `scripts/generate-project.sh` with XcodeGen 2.46.0 after changing it, and
commit the generated configuration together. Do not commit `xcuserdata`.

`ci_scripts/ci_post_clone.sh` installs Rust 1.98.0 and builds the locked Rust FFI
archive for physical iOS devices with frame pointers retained. Xcode Cloud owns
signing and upload; the script needs no signing key, Apple API key, or Keychain
password. The workflow must archive **MusteriOS / iOS** (Release), not the Mac or
simulator targets. Simulator tests need their separate Rust target and are not
part of this archive workflow.

Use main-branch changes as the automatic start condition, and distribute the
archive to the Public Beta external TestFlight group. Distribution still
requires Beta App Review. The initial Cloud marketing version is 1.0.44, distinct
from the previously uploaded manual 1.0.43 (33); Xcode Cloud manages its build
number. A completed archive is not proof of TestFlight processing or installation.
