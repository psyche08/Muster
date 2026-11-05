# ADR 0140: Resolve Cargo output through CARGO_TARGET_DIR

- Date: 2026-11-05
- Status: Accepted; macOS, iOS and Android builds verified in both modes
- Related: [ADR 0107](0107-l4forwarder-in-muster-app.md) (l4forwarder in the
  app bundle), [ADR 0121](0121-generated-xcode-project-follows-project-yml.md)
  (`project.yml` is the source of the Xcode project),
  [src/native/apple/MusterApp/README.md](../../src/native/apple/MusterApp/README.md)

## Context

Agent sessions build Rust into throwaway target directories of their own, and
nothing removes them; one reached 66 GB. The plan is to give every agent shell
one `CARGO_TARGET_DIR` per session and delete directories that have been idle
for a few days. Cargo already honors the variable, but the places in this
repository that read Cargo's output did not:

- the Xcode project linked `libmuster_ffi.a` from
  `src/muster/target/<triple>/<profile>/` and copied `musterd`, `muster`,
  `muster-panel`, `muster-browser-host` and `l4forwarder` into the bundle from
  `src/muster/target/release/` and `addons/l4forwarder/target/release/`;
- `scripts/build-ffi.sh` (Apple and Android), the Android CMake build, the
  Linux install scripts, the vnet namespace test and the HarmonyOS fake
  desktop named `target/` directly.

With the variable set, those consumers would miss the fresh outputs, or worse,
pick up a stale `libmuster_ffi.a` or binary left in the in-repo `target/`.
`scripts/dev-terminal.sh` was the one consumer that already used
`${CARGO_TARGET_DIR:-…}`.

## Decision

Every consumer of Cargo output resolves it as `$CARGO_TARGET_DIR` when the
variable is set and non-empty, and as the workspace's own `target/` otherwise.
With the variable unset, every path is the same as before.

- **Xcode.** `project.yml` defines two project-level build settings,
  `MUSTER_CARGO_TARGET_DIR = $(CARGO_TARGET_DIR:default=$(SRCROOT)/../../../muster/target)`
  and `L4FORWARDER_CARGO_TARGET_DIR`, which defaults to
  `addons/l4forwarder/target` instead. `xcodebuild` imports its environment
  as the lowest build-setting level, so a shell's `CARGO_TARGET_DIR` reaches
  them; Xcode opened from the Dock has none and falls back. Every
  `OTHER_LDFLAGS`, the bundling script and its input files use these
  settings. The settings have their own names because a project-level
  `CARGO_TARGET_DIR` would shadow the environment's. The project is
  regenerated from `project.yml`.
- **l4forwarder.** It is a separate Cargo workspace. With the variable set,
  both workspaces write into the one directory, so its binary is in
  `$CARGO_TARGET_DIR/release/`, not under `addons/l4forwarder/target/`.
- **Shell scripts** use `${CARGO_TARGET_DIR:-target}` from the workspace
  directory, or `${CARGO_TARGET_DIR:-$src/target}` where they compute it.
  A relative `CARGO_TARGET_DIR` resolves against the same working directory
  Cargo used. The Linux install scripts run under `sudo`, which drops the
  variable unless told to keep it; they then read the in-repo directory, as
  before.
- **Android.** `core/build.gradle.kts` always passes
  `-DMUSTER_CARGO_TARGET_DIR=<value or empty>` to CMake. Passing it even
  when empty keeps a value from an earlier configure from staying in the
  CMake cache. A changed value also changes the arguments, so the Android
  Gradle plugin configures CMake again. `CMakeLists.txt` falls back to
  `src/muster/target`.
- **CI.** The Swift test step sets `MUSTER_FFI_LIB` with the same fallback.
  CI does not set the variable, so it reads `src/muster/target` as before.

Left as they are:

- **HarmonyOS `build-ffi.sh`** already pins `CARGO_TARGET_DIR` to
  `src/muster/target` so the DevEco CMake build, which does not inherit a
  shell's environment, always finds the archive. Producer and consumer agree
  in both modes.
- **Xcode Cloud `ci_post_clone.sh`** pins the same in-repo directory for the
  same reason. Xcode Cloud's `xcodebuild` has no `CARGO_TARGET_DIR`, so the
  project falls back to that directory.
- **`muster-platform`'s `bundle::beside`** is not a build-tree lookup. It
  finds `muster-panel` next to the running executable, or in
  `Contents/Helpers` inside the app bundle. A daemon run from
  `$CARGO_TARGET_DIR/release/` finds the panel beside it. The
  `/src/muster/target/release/` paths in its tests are fixture strings.
  The running app does not need to know `CARGO_TARGET_DIR`.

## Alternatives

- **Symlink `target/` to a shared cache.** Every worktree would need its
  own link, and removing the cache would leave dangling links that the
  build scripts report as missing outputs.
- **A project-level `CARGO_TARGET_DIR` build setting.** It would shadow the
  environment, so the variable would never reach the project.
- **Pin every producer to the in-repo `target/`**, as the HarmonyOS
  script does. Correct, but agent builds would keep filling worktrees,
  which is the problem being solved.

## Consequences

- A shell with `CARGO_TARGET_DIR` builds the Rust outputs and the apps from
  that directory, and leaves nothing under the repository's `target/`.
- Xcode opened from the Dock, Android Studio, DevEco and CI keep reading the
  in-repo directories. A build made with the variable set is invisible to
  them, and they may link an older in-repo archive if one is there. Build in
  the same environment the app build runs in.
- Paths with spaces in `CARGO_TARGET_DIR` break `OTHER_LDFLAGS`, as a source
  path with spaces already did.

## Verification

Run on 2026-11-05 on an Apple silicon Mac with Xcode 27.2 and XcodeGen
2.46.0. Every build used fresh derived data.

- **Generated project.** `scripts/generate-project.sh` changed only the
  expected `OTHER_LDFLAGS`, input files, bundling script and the two new
  settings in `project.pbxproj`.
- **Build settings.** `xcodebuild -showBuildSettings` for `MusterMac`,
  `MusteriOS` (simulator and `iphoneos`) and `MusterMobileTests` resolves
  `MUSTER_CARGO_TARGET_DIR`, `L4FORWARDER_CARGO_TARGET_DIR` and
  `OTHER_LDFLAGS` to the variable's directory when it is set. When it is
  unset they resolve to the previous `$(SRCROOT)/../../../muster/target/…`
  and `addons/l4forwarder/target` paths.
- **Variable set** to a fresh temporary directory, with no `target/` in
  the worktree. Apple `scripts/build-ffi.sh` (three triples),
  `MusterMacShim/build.sh` and the `muster-cli`, `muster-browser`,
  `muster-panel` and l4forwarder release builds all wrote into it; the
  worktree still had no `target/` afterwards. Then:
  - `MusterMac` (Release, `CODE_SIGNING_ALLOWED=NO`) linked
    `aarch64-apple-darwin/release/libmuster_ffi.a` from that directory. The
    bundling script copied and ad-hoc signed `musterd`, `muster`,
    `muster-panel`, `muster-browser-host` and `l4forwarder` from it. Each
    bundled copy has the same Mach-O UUID as its source.
  - `MusteriOS` (Release, unsigned) built and linked the matching archive
    from it, for `iphoneos` and for the simulator. The simulator build used
    `ARCHS=arm64`. A generic simulator destination also links x86_64, for
    which no archive is built in either mode, so that build already failed
    before this change.
  - Android: `muster-ffi` was built for `aarch64-linux-android` with the
    NDK's clang set by hand, because `cargo-ndk` is not installed. Then
    `./gradlew :core:externalNativeBuildRelease` (arm64-v8a only) linked
    `libmuster_jni.so` against the archive in that directory.
- **Variable unset.**
  - The in-repo `src/muster/target` and `addons/l4forwarder/target` were made
    links to the same outputs. `MusterMac` linked through
    `…/MusterApp/../../../muster/target/aarch64-apple-darwin/release/libmuster_ffi.a`
    and bundled all five binaries through the in-repo paths, with the same
    UUIDs. The links were removed afterwards.
  - Android, in the same Gradle build directory as the set-mode run: CMake
    configured again with an empty `MUSTER_CARGO_TARGET_DIR`. It reported
    the in-repo `src/muster/target/aarch64-linux-android/release/libmuster_ffi.a`
    as missing, so no stale cached value was used.
  - The Android CMake fallback was also checked with `cmake -P` for an
    empty, set and absent value.
- **Not built.**
  - Signed or notarized macOS builds, and iOS builds with signing: these
    need the keychain.
  - The `x86_64-linux-android` ABI and Android's `build-ffi.sh` itself, for
    lack of `cargo-ndk`.
  - The Linux install scripts and the vnet test: they need root and
    systemd. These scripts were syntax-checked only.
