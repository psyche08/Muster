# Native shells

The app shells around the Rust core (ARCHITECTURE §3.7). Each one does
three things: sends Intents, performs Effects, renders ViewModels. Trust,
protocol and product state live in `src/muster`; a shell never reconstructs
them from what it displays.

| Platform | Where | Status |
|---|---|---|
| macOS + iOS | `apple/MusterApp` (SwiftPM: `MusterCore`, `MusterUI`, the `Muster.app` entry points, `project.yml` for XcodeGen; bundle id `dev.linsheng.muster`) | Builds with Xcode 27 on both platforms. Verified on a Mac + the iPhone simulator through a local relay: the Mac app (over musterd's socket) creates a network and approves the phone; the phone joins by pasted invite, lists the Mac's Claude / Codex sessions, follows a live transcript and views the Mac's iTerm2 sessions at their own size. Secure Enclave device key works in the simulator; the presence key (Face ID) and the camera need a real iPhone. On 2026-10-04 the iOS shell gained the HarmonyOS shell's half-fold workspaces (Top and bottom, Side by side, chosen from the title bar or Settings) and the Muster keyboard (pinyin and English, phone and MacBook layouts, named terminal keys; off by default in text fields, on by default in terminals): the engine, layout data, caret arithmetic and workspace geometry are pure `MusterCore` code whose tests pass on Linux, while the UIKit / SwiftUI parts were written without Xcode and are not yet compiled or run (`apple/MusterApp/README.md`) |
| macOS daemon shim | `apple/MusterMacShim` (SepSign static library for musterd's `sep` feature; the Locked Use console helpers `Console.m` built by `muster-platform/build.rs`) | Needs a Mac to build |
| macOS root helper | `apple/NetHelper` (`dev.linsheng.muster.helper`: utun, vnet route, Locked Use plugin installation; built into `Muster.app` by `MusterApp/project.yml`) | Ships in every desktop release |
| Locked Use plugin | `apple/MusterLockedUse` (system authorization plugin and installer) | See `docs/LOCKED_USE.md` |
| Android | `android/muster` (Gradle: `core` = JNI + Kotlin wrapper + Keystore keys + generated protobuf; `app` = Compose) | Written to the current contract on 2026-10-04, screen by screen after the iOS shell (workspace switcher and device tabs, chat timeline with Markdown and tool groups, composer with attachments, drafts and the unconfirmed-send card, interaction cards, queued messages, desktop chats and their options, the Chrome extension screen, device actions, the terminal grid with paste, diff previews, the relay editor, and the `ActivateNetwork` / `LocalAuth` / `Cover` effect answers); names checked against the Kotlin and Java `protoc` generates for the schema. On 2026-10-04 it also gained the half-fold workspaces (Automatic follows Jetpack WindowManager's fold, Top and bottom, Side by side) and the Muster keyboard, after the HarmonyOS shell. The whole Kotlin compiles on Linux with kotlinc and the Compose compiler plugin against the jars of the declared BOM, and its JUnit suites pass; **not built with Gradle and the Android SDK, and never run** (`android/muster/README.md`) |
| HarmonyOS | `harmonyos/muster` (hvigor: NAPI bridge + ArkTS wrapper + HUKS keys + generated protobufjs models; ArkUI pages) | Builds with SDK 26 and the ARM64 Rust core. Runs on the HarmonyOS 7.0.0 emulator: HUKS device key, production relay registration, network creation and the Chat / Agents / Terminals workspaces checked there; on 2026-10-01 a signed debug build ran on a HUAWEI Mate X7 against three Macs (see `harmonyos/muster/README.md`). `ets/proto` is generated with protobufjs-cli 1.3.3 (`scripts/gen-protos.sh`; 2.x rewrites the whole file). On 2026-10-04 it gained what the iOS shell had since (the Chrome extension screen, cover actions, composer drafts, the unconfirmed-send card, terminal paste, and the `ActivateNetwork` / `LocalAuth` / `Cover` effect answers); that change is not yet compiled or run |
| Windows | — | P4 (ARCHITECTURE §5); nothing yet |

The old AgentHalo apps (`apple/AgentHalo*`, `android/app`, `harmonyos/entry`,
`windows`, `../ios`) were removed on 2026-10-02 (ARCHITECTURE §8.5); recover
them from Git history with `git show acf23be7:<path>`.

## The contract every shell implements

- `muster_core_new(config_json, platform)` with five callbacks
  (`src/muster/crates/muster-ffi/include/muster.h`): `on_view`, `on_effect`,
  `hw_public_key`, `hw_sign`, `presence_public_key`.
- Intents, Effects, EffectResults and ViewModels are the protobuf messages
  of `src/muster/crates/muster-proto/proto/app.proto`; each shell generates its
  own bindings from that file (Swift: checked in under `Generated/`; Kotlin:
  at Gradle build time; ArkTS: checked in under `ets/proto/`).
- Two keys the platform holds: the device key (usable while locked, signs
  the link) and the presence key (one local authentication per signature,
  answers the `Authenticate` effect). Secure Enclave, Android Keystore
  (StrongBox when present) and HUKS respectively.
- Effects the platform performs: `ScanQr` (camera; a Mac reads the
  pasteboard or asks for the text), `Authenticate`, `Notify`, `OpenUrl`,
  `LocalAuth`, `ActivateNetwork`, and on a Mac `Cover`,
  `AuthorizeDesktopHelper`, `RestartDesktop`, `PrepareDesktopUpdate`,
  `FinishDesktopUpdate`.
- Decisions arrive in the ViewModel; a shell does not derive them from
  roles, kinds or states. For example `DeviceActions` (cover, SSH, edit
  access, remove), `ChatView.can_interrupt` / `can_choose_model` /
  `can_browse_files` / `provider_label`, `TerminalsView.can_create`,
  `DesktopSetupView.ready`, `can_go_back`, and display strings such as
  `JoinView.message`, `MemberView.link` and `DeviceInfo.key_label`.
- Terminal rows arrive as `TerminalLine{index, text, styles}`; `styles` is
  muster-terminal's 11-byte run encoding, decoded by each shell and drawn on
  a monospaced grid. Keys go back by name (`enter`, `ctrl-c`, `up`, …) or
  as text. `TerminalView.resizable` says whose grid it is: true for the
  shell's own SSH / PTY session (report the viewport with
  `TerminalResize`), false for somebody's iTerm2 window (draw `cols × rows`
  as given, shrink or scroll to fit, never resize).
- Selecting and copying is display-only: the shell maps a drag to grid
  cells and copies those cells of `TerminalLine.text` (Apple:
  `TerminalSelection`). Paste goes back as `TerminalPaste{text}` while
  `TerminalView.can_paste` holds; Rust turns line breaks into Enter, drops
  control characters (ESC included), brackets the text when the program
  enabled bracketed paste, and refuses more than 64 KB. A shell never types
  clipboard text as `TerminalText`.

## Building the Rust core for each platform

Each shell has `scripts/build-ffi.sh` producing `libmuster_ffi.a` under
`src/muster/target/<triple>/release`:

- Apple: `aarch64-apple-darwin`, `aarch64-apple-ios`, `aarch64-apple-ios-sim`
- Android: `aarch64-linux-android`, `x86_64-linux-android` (cargo-ndk)
- HarmonyOS: `aarch64-unknown-linux-ohos` (the OpenHarmony NDK's clang)

cargo also leaves a `libmuster_ffi.dylib`/`.so` beside the archive; the
shells link the archive by path so the static one wins.
