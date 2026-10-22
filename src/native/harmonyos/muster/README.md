# Muster for HarmonyOS

ArkUI around the Rust core (ARCHITECTURE §3.7). The NAPI bridge
(`entry/src/main/cpp/muster_napi.cpp`, about 300 lines) carries bytes
between ArkTS and `muster.h`; view models and effects cross through
threadsafe functions. The key callbacks never cross into ArkTS: the core
asks for them on whichever thread it is on, the ArkTS thread included (it
starts there and signs its link certificate while starting), so the public
points are handed over at `create` and the device key signs through HUKS's
synchronous C API in the bridge.

```
entry/src/main/cpp/            muster_napi.cpp, CMakeLists.txt (links libmuster_ffi.a), types/libmuster
entry/src/main/ets/core/       MusterCore.ets (typed intents/effects), Keys.ets (HUKS keys), MusterModel.ets (effects)
entry/src/main/ets/proto/      muster.js + muster.d.ts, protobufjs static module (scripts/gen-protos.sh)
entry/src/main/ets/display/    FoldDisplay (OS posture/preferences), WorkspaceLayout (presentation geometry)
entry/src/main/ets/input/      in-app keyboard: Keyboard, KeyboardFields, MacLayout, PinyinEngine, TextEdits
entry/src/main/ets/pages/      Index.ets: one component per screen
entry/src/main/ets/view/       Workspace (tabs), NetworkViews, AgentViews, ChatViews, DesktopChatViews,
                               Markdown(+Parse), TerminalViews (canvas grid), WorkTreeViews, BrowserViews
```

## Build

Install DevEco Studio 26 (it bundles HarmonyOS SDK 26, hvigor, ohpm and
Node) or Huawei's Command Line Tools 26.0.0, and a rustup-managed Rust
toolchain. Then:

```sh
sh scripts/build.sh          # Rust core, bridge and HAP
sh scripts/build.sh --ui     # ArkTS and bridge only, once the Rust archive exists
sh scripts/run.sh [target]   # install the newest HAP with hdc and start it
```

`scripts/env.sh` uses `HARMONYOS_COMMAND_LINE_TOOLS` when set, otherwise
DevEco Studio at `DEVECO_STUDIO` (default `/Applications/DevEco-Studio.app`)
and its bundled JBR for `JAVA_HOME`. The build checks SDK 26, builds
`muster-ffi` for `aarch64-unknown-linux-ohos`, installs ArkTS dependencies and
builds the debug HAP. It selects rustup's compiler explicitly, so a Homebrew
Rust installation earlier in PATH cannot hide the installed OHOS standard
library. Rust outputs stay in `src/muster/target`, as CMake expects.

The project targets HarmonyOS 26.0.0 and keeps API 20 as the minimum version.
The entry profile builds the NAPI bridge for ARM64 and links the Rust archive;
a missing archive stops packaging.

### Signing

Only `build-profile.example.json5` (no signing) is tracked; `build.sh` copies
it to the ignored `build-profile.json5`. For a phone, open this directory in
DevEco Studio and turn on File → Project Structure → Signing Configs →
Automatically generate signature (Huawei account); DevEco writes certificate
paths and passwords into the local file, and the build then also emits
`entry-default-signed.hap` (the product already names the `default` signing
config; without one the build only warns and skips signing). Open this
directory itself, not `src/native/harmonyos`: there DevEco reads no bundle
name and renews the certificate without issuing a profile. Automatic debug
profiles name the bundle (`dev.linsheng.muster`) and the phone's UDID and
expire after 14 days. The emulator installs the unsigned HAP.

### Emulator

DevEco's emulator CLI can create a Mate X7 running HarmonyOS 7.0.0 (API 26):

```sh
E=/Applications/DevEco-Studio.app/Contents/tools/emulator/Emulator
$E -install -deviceType foldable -osVersion "HarmonyOS 7.0.0(26.0.0)"
$E -create "Mate X7 API26" -deviceType foldable -osVersion "HarmonyOS 7.0.0(26.0.0)" -screenProfile "Mate X7"
$E -start "Mate X7 API26"
$E -instance "Mate X7 API26" -foldedState close    # or open, half-open
```

The emulator has no lock-screen credential, so HUKS refuses the presence key
and the shell runs with the HUKS device key only (Settings says so). Scan
Kit neither scans nor draws QR codes there (createBarcode never settles):
join with Paste an invite and copy invites as text.

## Icon and theme

The app icon is the Apple apps' (`src/native/apple/MusterApp/Design/AppIcon.svg`):
`scripts/gen-icons.py` renders it with `rsvg-convert` into a layered icon
(ink background, parchment-and-clay mark at the same proportions) for the
app and the ability, and a rounded tile for the start window on ink.
`MusterTheme` makes the Apple AccentColor (clay `#A0644B`, dark `#B5775D`)
the brand colour of every system component, so toggles, selects, buttons
and emphasis match. App-drawn colours are `app.color` resources with dark
values from the Codex chat palette (`ChatStyle.swift`): user bubbles,
composer, code blocks, borders, links and the blue stop button; selection
highlights are a tint of the accent.

## Keys

- Device key: HUKS ECC P-256, `HUKS_AUTH_STORAGE_LEVEL_ECE`, signs without
  the user. `Keys.ets` creates it and exports the point (HUKS's exported key
  is a SubjectPublicKeyInfo; the last 65 bytes are the point the roster
  carries); `muster_napi.cpp` signs with the same alias and parameters
  through `OH_Huks_InitSession` / `OH_Huks_FinishSession`, since the ArkTS
  HUKS API has no synchronous sessions.
- Presence key: the same with `HUKS_TAG_USER_AUTH_TYPE`; each signature
  runs `userAuth` over HUKS's challenge and passes the token to
  `finishSession` (`Keys.ets`).

## Verification status

2026-10-01, HarmonyOS 7.0.0.107 emulator (Mate X7 profile, API 26), unsigned
debug HAP:

- Launch: the Rust core loads from `libmuster.so`, creates its HUKS device
  key and signs its link certificate through the native bridge (key backend
  `platform`). Two startup crashes were fixed on the way: the protobuf module
  import and the V1 `AppStorage` holding the V2 model.
- Network: setting `muster.linsheng.dev:9443` registered with the production
  relay over TLS; creating a network made the emulator its admin; invites
  are issued as text.
- UI: Chat / Agents / Terminals switcher, device tabs, network menu, Network
  settings, device details, invite, folded (cover) and unfolded screens, the
  system back gesture.
- Host tests: `tests/fold-workspace.test.cjs` (14) and
  `tests/markdown.test.cjs` (9) pass.

### Testing against a fake desktop

`tests/fakemac/fakemac.sh` runs a second musterd on the Mac ("FakeMac") with
its own state, invented Claude Code and Codex sessions and a canned PTY shell,
so the phone exercises the real relay, pairing, authorization and agent and
terminal RPCs without real data. It refuses to start with Accessibility.

```sh
(cd ../../../muster && cargo build -p musterd -p muster-cli)
sh tests/fakemac/fakemac.sh run                   # foreground; Ctrl-C stops it
sh tests/fakemac/fakemac.sh muster create test-net
sh tests/fakemac/fakemac.sh muster invite         # paste it on the phone
sh tests/fakemac/fakemac.sh muster pending
sh tests/fakemac/fakemac.sh muster approve <device> --scope agents --scope agents:claude \
  --scope agents:codex --scope terminals
sh tests/fakemac/fakemac.sh muster policy --offer-agent true --offer-terminal true
sh tests/fakemac/fakemac.sh clean                 # forget it
```

A locked Mac refuses remote use unless `--allow-locked-control true` is also
set on FakeMac. Sending, answering and desktop chats need Accessibility, so
against FakeMac they fail with the core's error; that path is what they test.

2026-10-01, the same emulator against FakeMac on the production relay:

- Joining by pasted invite, waiting for and receiving approval.
- Agents across devices and per device, Claude and Codex conversations:
  Markdown replies (headings, lists, code, quotes, links), Worked groups
  with commentary and titled tool groups whose rows pair calls with output,
  tool activity,
  changed files opening coloured diffs, Current file with the core's error,
  turn usage footers, jump to latest, half-fold "Always" preview.
- A send refused by FakeMac shows the core's error and keeps the draft;
  toasts dismiss themselves.
- Terminals: a new PTY window, typed commands, colours, OSC title, key bar.
- Chat tab: the core's "no Mac can show chats" error.
- Fixed on the way: terminal input resent the whole line per keystroke;
  terminal controls were invisible on black; the access editor could not
  scroll to Save; file previews had no diff colours.

2026-10-01, HUAWEI Mate X7 (HarmonyOS 7.0.0.109, API 26), signed debug HAP
with DevEco's automatic profile, joined to the owner's network of three
Macs:

- Install and launch; no crash logs.
- Scan Kit scanned the Mac's invite QR; the Mac approved the phone.
- Settings reports key backend `platform`: the core's link certificate and
  pairing were signed by the HUKS device key through the native bridge. The
  user-auth-bound HUKS presence key was created (the phone has a lock-screen
  credential).
- Links: a direct path to one Mac, relay paths to the others.
- Agents across three Macs with live working/idle states; a live Claude
  conversation with its Working group, user messages and usage footers;
  iTerm2 windows listed per Mac (one Mac's iTerm2 error shown inline) and
  one drawn read-only; the network menu omits Invite for a non-admin.
- Half-folded: Automatic split the agents and terminals side by side at the
  vertical crease; the layout button's Top and bottom turned the screen to
  landscape and stacked a live conversation at the now horizontal crease;
  Side by side turned it back and put the conversation or an iTerm2 window
  beside its list (the window shrunk to fit its 121 columns); Single pane and
  Automatic restored the system orientation; back closed the right pane.
- In-app keyboard in a conversation (top and bottom layout): pinyin `nihao`
  offered 你好 first and committed it, Chinese punctuation, EN mode letters,
  shift, the number page, delete and hide; nothing was sent. In a terminal
  the Esc/F1–F12 row, number row and Ctrl/Alt keys showed and Ctrl armed and
  released (no key was sent to the real window); a conversation's keyboard
  showed the number row without them.
- MacBook keyboard on the unfolded phone (773vp), in Settings' port field:
  English `tcp/22`, ← ← then `3` in the middle, Shift-← selected the `3`,
  command-A selected all and delete cleared it; caps lock tap switched 中/英,
  pinyin `nihao` gave 你好, `c` then `,` committed 从 before ，, holding caps
  lock typed `A`; nothing was added. A terminal (top and bottom, compact)
  showed the esc/F1–F12 row; no key was sent. Flip 180° turned the display
  from rotation 90 to 270 and back.
- Dark mode (the app forced dark by a temporary local build; the phone's
  setting untouched): the device chip and settings toggles and buttons in
  the dark accent, a conversation's user bubble (`#253454`, light blue
  text) and `#303030` composer, the MacBook keyboard, the Chat tab and the
  layout menu all rendered legibly. Pages keep the system's black, not the
  Apple apps' `#212121` canvas.
- Chat tab: the core stopped desktop-chat navigation because the Mac was in
  local use ("local input changed during chat navigation; nothing more was
  pressed") and the tab showed that error; not retried.

Not verified: a presence proof with face/fingerprint (nothing tested needed
one), drawing an invite QR on the phone (it is not an admin), sending,
answering, interrupting and typing into real sessions (not exercised against
real work), interaction cards and attachments, desktop chats while the Mac is
idle, the in-app keyboard side by side, and Markdown code blocks in dark mode.

2026-10-04, alignment with the iOS shell (not built here: the environment
has no DevEco Studio or HarmonyOS SDK):

- Added what the iOS shell had and this one lacked: the Chrome extension
  screen (`BrowserViews.ets`, from a device's details and from Settings on
  a device with services), Cover / Uncover its screen on a device's details
  while `DeviceActions.can_cover` holds, the composer's draft riding along
  with every intent (`ComposerAction`; restored from `ChatView.draft` and
  cleared only for the revision a send receipt names), the unconfirmed-send
  card (`ChatView.unconfirmed`, released by the next send), a terminal Paste
  button (the system paste control, while `TerminalView.can_paste`), and
  explicit answers to the `ActivateNetwork` (one TCP connect to the core's
  endpoint), `LocalAuth` (the system user-authentication prompt) and
  `Cover` (refused: a phone has no cover) effects, which were answered as
  unknown before.
- Names were checked against `ets/proto/muster.d.ts`; the ArkTS was not
  compiled and nothing ran on a device or the emulator. The iOS performance
  monitor (the `UploadDiagnostics` sender) has no HarmonyOS counterpart.

## Keyboard

Text fields type with an in-app keyboard (`entry/src/main/ets/input`) bound
with `customKeyboard()`, with pinyin Chinese and English; the choice
persists. From 560vp wide (the unfolded Mate X7, either way round) it has a
MacBook's layout (`MacLayout.ts`, US ANSI): `` ` ``1–0 `-` `=` delete, tab
Q–P `[` `]` `\`, caps lock A–L `;` `'` return, shift Z–M `,` `.` `/` shift,
then fn/🌐, control, option, command, the space bar, command, option and the
inverted-T arrows, with each key's width in proportion so the columns line
up; a terminal adds esc and F1–F12 above. Letter caps show capitals, symbol
caps both characters, and Shift types the upper one. As with macOS Pinyin, a
tap on caps lock switches 中/英 and holding it locks capitals; in Chinese the
punctuation keys type ，。、；：？！《》【】「」…… ￥ and alternating quotes, and an
apostrophe while composing separates syllables. Modifiers are sticky (tap:
the next key; double tap: locked). In a text field the arrows move the caret
(Shift extends the selection), option moves by word, command and fn by line
or to either end; option-, command- and fn-delete delete a word, to the line
start or forward; command-A/C/X select all, copy and cut; control gives the
macOS Emacs keys (A E B F P N H D K). Narrower windows (the folded outer
screen) get a phone layout: a number row over QWERTY with a 中/英 key, two
symbol pages with Chinese or English punctuation, shift and caps lock, a
repeating delete and a hide key. Caret arithmetic lives in `TextEdits.ts`;
`tests/keyboard-layout.test.cjs` checks it and the layout on the host.
In top-and-bottom layout the conversation's and terminal's field takes focus
on its own, so the keyboard is up from the start, in a compact size that
leaves the field below the crease; the layout counts the keyboard's height,
which the OS reports only for its own IME. In a terminal the keys send named
keys (`escape`, `f5`, `ctrl-c`, `alt-b`, `shift-tab`, `ctrl-alt-x`…) that
Shared Rust's `key_bytes` maps to xterm bytes, so any letter, digit or symbol
combines with control or option; as in macOS terminals fn-arrows are
Home/End/Page Up/Down, command-←/→ and command-delete send ^A, ^E and ^U,
and option-←/→ send ESC b / ESC f. The phone layout has a scrolling strip of
Esc, Tab, arrows, Home/End, PgUp/PgDn and F1–F12 with Ctrl and Alt keys
instead. Either way the terminal's own key bar stays hidden. The
pinyin engine (`PinyinEngine.ts`, plain TypeScript) splits letters into whole
syllables, a trailing partial syllable or bare initials (`zg` → 中国), ranks
whole-word matches by frequency, composes a sentence greedily when no word
covers the letters, lets a shorter word take only its own letters, and raises
words the user picks. Space takes the first candidate; Enter types the
letters. The terminal's keyboard types straight into the session. Settings →
Muster keyboard off returns every field to the system IME; a terminal has
its own switch, Muster keyboard in terminals (`terminalSystemKeyboard`, on
by default as well), since the IME has no Esc, F-keys, Ctrl or Alt.

The dictionary (`resources/rawfile/pinyin.txt`, 55,320 GB2312 words and
characters with frequencies) is converted by `scripts/gen-pinyin-dict.py` from
AOSP PinyinIME's `rawdict_utf16_65105_freq.txt` (Apache-2.0; notice in
`docs/third-party/aosp-pinyinime-NOTICE.txt`). It loads from the first
keyboard shown. `tests/pinyin.test.cjs` runs the engine against it on the
host.

## Half-fold workspace

The shell supports two half-fold workspaces. Top and bottom (horizontal crease,
laptop posture) in Chat and Terminal:
Chat keeps the transcript above the live crease and puts the composer, actions
and provider-supplied questions/approvals in a separately scrolling lower pane.
Terminal keeps its canvas above the crease and its key bar/input/status below.
The same component instances remain mounted through posture changes, preserving
local drafts, question answers, focus and scroll position. Resizing the terminal
continues to send the existing Rust resize intent.

### Side by side

Across a vertical crease (book posture) the workspace's list sits on the left
and the conversation, terminal, files or Git view on the right, split at the
crease plus 8vp on each side; without a crease, at the middle. Each side needs
280vp. A row on the left replaces the right pane, so the title bar shows the
workspace switcher instead of Back; the system back gesture still closes the
right pane. The open row is highlighted, and an empty right pane says what to
choose. Someone's iTerm2 window keeps its own size: the terminal shrinks its
font to fit the window's columns instead of sending a resize.

### Choosing the layout

The foldable-screen button in the title bar (on the workspace lists, a
conversation and a terminal) and Settings → Half-fold workspace offer
Automatic, Top and bottom, Side by side and Single pane. The preference is local
display state, persisted in `muster-display` Preferences (`always` keeps its
stored name for top and bottom). Automatic follows the OS half-fold posture and
its one live crease: horizontal stacks, vertical splits side by side. Top and
bottom previews an equal split on ordinary/flat screens; Side by side splits
at the middle. A forced layout never overrides an unusable physical crease.
While half-folded, a forced layout also asks the window for the orientation
that puts the crease where it needs it (landscape for top and bottom,
portrait for side by side), so a phone in laptop posture with rotation lock
turns its screen. Top and bottom asks for a fixed landscape, since the
orientation sensor cannot tell which edge is up with one half lying flat:
the one auto-rotation settled on standing in laptop posture (display
rotation 90 on a Mate X7), or with Flip 180° (Settings, and the layout
button's menu while Top and bottom is chosen; persisted as `flexFlipped`)
the other one, for a phone standing on its other edge. Unfolding, folding, Automatic or Single pane hand the
orientation back to the system. No session, transport, authorization or
provider state is inferred by this display adapter.

`Display.getLiveCreaseRegion()` (API 20) is read on the window's display, with
pixel-to-vp conversion and both window and content offsets. Fold/display-mode,
rotation/display changes, window movement and foreground return refresh the
reading. Window teardown removes subscriptions, restores keyboard avoidance,
and rejects late window callbacks. Missing/invalid/ambiguous geometry falls back
to normal layout. The physical crease plus 8vp on each side remains empty; it is
never clamped to an arbitrary fraction. Each pane needs 220vp before typing;
with the resize-mode keyboard visible the lower scroll viewport may shrink to
96vp while the real hinge stays fixed. Smaller viewports use normal layout.

Host regressions (Node, with the installed SDK's TypeScript compiler):

```sh
TYPESCRIPT_PATH="$HARMONYOS_COMMAND_LINE_TOOLS/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript" \
  "$HARMONYOS_COMMAND_LINE_TOOLS/tool/node/bin/node" tests/fold-workspace.test.cjs
```

2026-09-30 verification for this change:

- Code: new `muster` shell only; the frozen legacy HarmonyOS shell is untouched.
- Tests: presentation geometry and real OS-adapter methods under explicit mocks
  pass, covering offsets/density, keyboard, small windows, invalid/vertical and
  multiple creases, preference persistence, events and subscription teardown.
- Compilation: complete build passed with Command Line Tools 26.0.0.851 /
  HarmonyOS SDK 26.0.0.105 and Rust 1.98.1. `muster-ffi` cross-compiles for
  `aarch64-unknown-linux-ohos`; CMake links NAPI/HUKS with no undefined symbols;
  ArkTS compiles and the unsigned HAP contains `libs/arm64-v8a/libmuster.so`
  and `libc++_shared.so`. The native ELF is AArch64 and exports the NAPI
  registration and Rust core entry points. The SDK 26 `fill` compatibility
  warning was removed by drawing the status dot with an API 20-compatible
  background. Existing Keys/ScanKit/dependency warnings remain.
- Signing/notarization: not performed (notarization does not apply to HAP).
- Installation/runtime: not performed; no device or emulator acceptance.
- End-to-end: not performed. Still check horizontal/vertical folds, rotation,
  floating/split windows, IME open/close, large fonts and long approvals on a
  physical foldable. Confirm draft/focus/scroll retention and real remote
  send/respond/terminal input separately on a signed installation.
