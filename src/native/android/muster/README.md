# Muster for Android

Compose around the Rust core (ARCHITECTURE §3.7). `core` is the thin
shell: `muster_jni.c` over `muster.h`, `MusterCore.kt` (typed intents, effects,
`StateFlow<ViewModel>`, the composer draft source), `KeystoreKeys.kt`
(StrongBox/TEE P-256 device key, biometric-bound presence key), and the
protobuf messages generated from `src/muster/crates/muster-proto/proto` at
build time. `app` renders.

```
core/src/main/cpp/          muster_jni.c, CMakeLists.txt (links libmuster_ffi.a)
core/src/main/kotlin/.../core/  MusterCore.kt (typed intents/effects), MusterNative.kt (JNI), KeystoreKeys.kt
app/src/main/kotlin/.../app/    MusterModel.kt (effects), MainActivity.kt (prompts, links), MusterRoot.kt (one
                                composable per screen), Workspace.kt (tabs), NetworkScreens.kt, AgentScreens.kt,
                                Markdown.kt, DesktopChatScreens.kt, TerminalScreens.kt (canvas grid),
                                WorkTreeScreens.kt, BrowserScreen.kt
```

## Build

```sh
cargo install cargo-ndk
export ANDROID_NDK_HOME=$HOME/Library/Android/sdk/ndk/27.2.12479018
scripts/build-ffi.sh          # libmuster_ffi.a for arm64-v8a and x86_64
./gradlew :app:installDebug
```

The Gradle protobuf plugin runs protoc; CMake links the JNI library
against the archive by ABI (`core/src/main/cpp/CMakeLists.txt`).

## Pieces

- QR scanning: Play Services code scanner (no camera permission in the
  manifest; the module downloads on first use).
- Presence proof: `BiometricPrompt` with a `CryptoObject` around the
  Keystore key's `Signature`, so the key is unlocked for exactly one
  signature (`MainActivity.authorize`). The `LocalAuth` effect is the same
  prompt without a key (`MainActivity.authenticate`).
- `ActivateNetwork`: one TCP connect to the endpoint the core names, within
  its deadline (`MusterModel.activateNetwork`); only a retry hint.
- The composer's draft rides along with every intent while a conversation
  is open (`ComposerDraftSource`, `ComposerAction`): restored from
  `ChatView.draft`, a new revision per keystroke, cleared only for the
  revision a send receipt names.
- Terminal paste reads the clipboard on a tap of the key bar's Paste button
  and sends `TerminalPaste`; the core types it while `TerminalView.can_paste`
  holds. Paste stays visible with the Muster keyboard, system keyboard, or
  keyboard hidden; only the extra special keys depend on keyboard mode.
- `muster://browser?device=…&request=…` (the Chrome extension's setup page)
  opens the browser screen once the roster is known (`MainActivity`,
  `MusterModel.openBrowserLink`).
- Push wake-up (`SetPushToken`, kind 2): not wired; add Firebase Messaging
  and call `core.setPushToken(2, token)` from its service.

## Half-fold layouts and the Muster keyboard

The shell has the HarmonyOS shell's two half-fold workspaces and its in-app
keyboard (the same code structure, the same dictionary):

- **Layouts.** The foldable-screen button in the title bar (on the workspace
  lists, a conversation and a terminal) and Settings → Half-fold workspace
  offer Automatic, Top and bottom, Side by side and Single pane; the choice is
  local display state (`muster-display` SharedPreferences, `flexPreference`,
  with `always` keeping its stored name for top and bottom). Automatic follows
  Jetpack WindowManager's half-opened `FoldingFeature`: a horizontal crease
  stacks, a vertical one splits side by side (`display/FoldDisplay.kt`, which
  also tracks the keyboard's height from the IME insets or from the in-app
  keyboard). Top and bottom keeps the transcript (or the terminal grid) above
  the crease, or above an equal split on a flat screen, and puts the composer,
  Locked Use status, queue and the provider's questions in a separately
  scrolling lower pane, where the composer takes focus with a compact
  keyboard; the keyboard's height is restored only to place the hinge, so
  typing never moves the upper pane. Side by side puts the workspace's list on
  the left and the conversation, terminal, files or Git view on the right,
  split at the crease plus 8 dp on each side or at the middle, each side 280
  dp or more; a row on the left replaces the right pane, so the title bar shows
  the switcher instead of Back and the system back gesture still closes the
  right pane. The geometry is `display/WorkspaceLayout.kt`; `MusterRoot`
  resolves the split for the window and hands it down as
  `LocalWorkspaceLayout`. The shell does not ask for an orientation while
  half-folded (HarmonyOS does).
- **Keyboard.** Settings → Muster keyboard (off by default until it has been
  verified on a device) replaces the system keyboard in every text field
  (`input/KeyboardFields.kt`: `MusterTextField`, a `BasicTextField` whose
  platform input is intercepted, so the IME never opens, and whose edits go
  through `FieldEditor`) and, with its own switch that is on by default
  (Muster keyboard in terminals), in a terminal, where it types straight into
  the session and adds Esc, F1–F12, Ctrl and Alt, which the system keyboard
  lacks. The keyboard is one composable
  (`input/Keyboard.kt`) drawn as the scaffold's bottom bar for whichever field
  or terminal registered a `KeyboardTarget` with `KeyboardHost`. Narrow
  windows get a phone layout (a number row over QWERTY with a 中/英 key, two
  symbol pages, shift and caps lock, a repeating delete); from 560 dp wide a
  MacBook's layout (`input/MacLayout.kt`: `` ` ``1–0 delete, tab Q–P, caps
  lock A–L return, shift Z–M shift, fn ⌃ ⌥ ⌘ space ⌘ ⌥ and the inverted-T
  arrows, with each key's width in proportion). Modifiers are sticky (tap:
  the next key; double tap: locked); in a field the arrows move the caret
  with the same arithmetic as HarmonyOS (`input/TextEdits.kt`, UTF-16
  offsets), option by word, command and fn by line or to either end,
  command-A/C/X select all, copy and cut, control gives the macOS Emacs keys;
  in a terminal the keys go to the core as names (`escape`, `f5`, `ctrl-c`,
  `alt-b`, `shift-tab`…). The pinyin engine (`input/PinyinEngine.kt`) splits
  letters into syllables, ranks whole words by frequency, composes a sentence
  when no word covers the letters, lets a shorter word take only its own
  letters, and raises words the user picks; space takes the first candidate,
  Enter types the letters. The dictionary (`app/src/main/assets/pinyin.txt`,
  55,320 GB2312 words from AOSP PinyinIME, Apache-2.0;
  `docs/third-party/aosp-pinyinime-NOTICE.txt`) loads off the first keyboard.

The engine, the layout data, the caret arithmetic and the workspace geometry
are plain Kotlin, so `./gradlew :app:testDebugUnitTest` runs their JUnit
tests (`PinyinEngineTest`, `KeyboardLayoutTest`, `WorkspaceLayoutTest`,
ports of the HarmonyOS host tests) without a device.

## Not verified yet

Nothing here has been compiled with the Android SDK, and nothing has run on
a device or an emulator; the environment this was written in has neither.

2026-10-04, alignment with the iOS shell: the shell was rewritten to the
current `app.proto` contract, mirroring the iOS shell screen by screen: the
Chat / Agents / Terminals workspace switcher and device tabs, the chat
timeline (Markdown replies, Working groups, titled tool groups with paired
output, turn changes opening diffs, usage footers, compact context), the
composer with attachments, drafts and the unconfirmed-send card, interaction
cards with the provider's questions, queued messages, the Locked Use status,
desktop chats and their model / effort sheet, the Chrome extension screen,
device actions from `DeviceActions` (cover, SSH, access, remove), the
resizable / fixed terminal grid with paste, diff previews with Current file,
the relay editor, and the `ActivateNetwork`, `LocalAuth` and `Cover` effect
answers. The removed `Log` / `KeepAwake` / `Haptic` effects and the
`PresenceGranted` intent are gone. Names were checked against the Kotlin and
Java that `protoc --kotlin_out=lite --java_out=lite` generates for the schema
(every getter and DSL builder the shell uses exists); the Kotlin itself was
not compiled. The iOS performance monitor (the `UploadDiagnostics` sender)
has no Android counterpart; `MusterCore.uploadDiagnostics` exists for one.

2026-10-04, half-fold layouts and the keyboard: the whole `app` and `core`
Kotlin was compiled on Linux with kotlinc 2.1.0 and the Compose compiler
plugin against the jars of the BOM this build declares (Compose 1.7.6,
Material 3 1.3.1, window 1.3.0, the Play Services and protobuf-lite
artifacts) and the Java and Kotlin `protoc` generates for the schema; it
compiles without errors or warnings, and the three JUnit suites pass with
JUnit 4.13.2 the same way. That check found nine `ListItem` calls from the
alignment above that passed the modifier positionally (Material 3 takes the
headline first); they were fixed. Gradle, the Android SDK, an emulator and a
device were still not available, so the keyboard, the fold tracking and the
split screens have not been run.
