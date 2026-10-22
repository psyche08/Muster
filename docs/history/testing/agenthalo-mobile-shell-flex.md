# Unified Mobile Shell and Half-Fold Layout Validation Record

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Date: 2026-09-20. Scope: native iOS, HarmonyOS, Android UI. Shared baseline: [visual specification](../design/agenthalo-mobile-style.md) and [JSON tokens](../design/mobile-style-tokens.json). This round adds `shell` (four destinations) and `layout` (modes/half-fold geometry) contracts; see [unified mobile shell and half-fold layout](../architecture/agenthalo-mobile-shell.md) for design/implementation details.

## Scope of Changes

- All three platforms use four destinations, “Sessions / Terminal / Devices / Settings,” with token-defined keys/order/titles validated against source.
- Shared layout resolution adds compact / expanded / flex modes based on window dimensions and OS-reported posture; each platform has an equivalent implementation/unit tests.
- Half-fold (flex): transcript above the crease, composer/execution parameters/session pending approvals below; each half scrolls independently and keyboard covers only the lower half.
- Posture sources: Android `androidx.window` `FoldingFeature`, HarmonyOS `display` fold status/crease region; iOS has no posture API and always reports unknown. None infer posture from window dimensions.
- Settings adds “Half-fold layout” (Automatic / Always half-fold / Off), affecting presentation only, initially session-scoped.
- iOS moves “Security and permissions” and “Developer connection records” from Devices to Settings; Android's two-column breakpoint changes from 760dp to shared 600dp, with proportional list width clamped to 240–310dp.
- Terminal lists blocking approvals/questions and `terminal/*` provider sessions. Approvals remain in the originating session too: iOS/HarmonyOS already showed them in transcripts, while Android adds an in-session approval area this round.
- Authorization, pairing, networking, state machines, route keys, handle identity, durable recovery, and Shared ABI unchanged. Existing disable conditions, read-only notices, simulated sources, DEBUG warnings, errors, and recovery actions retained.

## Completed Independent Checks

- Shared validation: `node docs/design/check-mobile-style.mjs …` passed. Seventeen common text/background pairs meet 4.5:1; shell defines 4 destinations/7 layout constants; each platform's 16 semantic theme colors and 7 layout constants match, covering all four destinations.
- iOS: `bash src/ios/CoreTestHarness/run-mobile-shell-host-tests.sh` ran 13 host tests, no failures/skips (3 theme + 10 layout/shell). Full App source type checks under iOS 26 Simulator target/Swift 6 (embedded/nonembedded core client configurations each run once) left only unrelated existing errors, detailed below.
- Android: `./gradlew :app:testDebugUnitTest --rerun-tasks` passed 129 JVM tests in 19 suites, no failures/skips (including 8 new layout/posture-mapping tests). `:app:assembleDebug` and `:app:assembleDebugAndroidTest` compiled; development signing is not release signing.
- HarmonyOS: complete `AGENTHALO_HAP_SIGNING=skip bash src/native/harmonyos/build.sh` passed, including host regressions, real SDK AST theme/layout checks, OHOS Rust / N-API, ArkTS, unsigned HAP. `FoldableLayout` covers resolution/crease mapping; `MobileStyle` AST adds structural assertions for four-destination navigation, both flex halves, approval lists. Unsigned HAP SHA256: `a6145a0124b8c7b8fb90932437c4c68281a6f4fd3b32ad9c7ecf8b312ca8c3ec`.

## Half-Fold Failure Fallback

Half-fold does not mean “split every foldable screen.” Resolution falls back to single or side-by-side columns under any condition below, avoiding unusably narrow transcript/Send areas:

- Window height below `flexMinHeight`, upper half below `flexMinTranscript`, or lower half below `flexMinComposer`.
- Vertical crease (book posture): side-by-side columns.
- Unmeasurable width/height (NaN / Infinity / nonpositive): always single column.
- User chooses Off. Always half-fold still obeys these minimum heights.

OS-reported crease position clamps between `flexMinTopFraction` and `flexMaxTopFraction`; unavailable creases use `flexDefaultTopFraction`, without guessing. Each rule has corresponding assertions on all three platforms.

## Second Round (Same Day): Preference Persistence and iOS Columns

- Half-fold preference becomes installation-persistent: iOS `UserDefaults`, Android `SharedPreferences`, HarmonyOS `Preferences`, identical keys/values, unknown values return Automatic. Each adds mapping tests; iOS uses an isolated `UserDefaults` suite to verify a new reader retrieves old values and does not guess unknown values.
- iOS renders `expanded` side-by-side list/detail columns sharing one `AgentHaloNavigationHistory`; list rows replace detail through new `select(_:policy:)`, blocked by `.blockedByMutation` during mutations. Five `AgentHaloExpandedNavigationTests` cover filling, replacement, same-session no-op, blocking, and shared history with compact push. `AgentHaloNavigation.swift` joins the host module.
- Reruns: shared validator passed; iOS 19 host tests (13 + 1 persistence + 5 expanded navigation), no failures/skips; full iOS 26 Simulator source type checks still show only two existing `AgentHaloSessionWindowABI` errors. Android `--rerun-tasks` 130 JVM tests/19 suites, no failures/skips, both debug APKs compile. Full HarmonyOS `build.sh` passed; unsigned HAP SHA256: `428af9dfdfe42f83e826d8dde9fd5a8c61203a7cee20533060ef612e58dda25a`.

## Third Round (2026-09-21): Devices Page Reworked to Match Desktop Structure

See [Devices page: desktop and mobile](../architecture/agenthalo-device-page.md) for design/implementation.

- All three Devices pages now follow the [desktop Devices page](../design/agenthalo-desktop-devices.md) hierarchy: This device (identity, pairing, scan; HarmonyOS also Master network card) → Devices (one row/computer: online/checking/unreachable + session count) → Advanced (authorizer set, raw directory fields, capabilities, protocol-state cards, developer simulations, architecture notes, collapsed by default). Pairing, verification codes, authentication, HUKS/Keystore flows unchanged.
- Each adds pure projections/unit tests: iOS `AgentHaloMobileDeviceDirectoryTests` (3, host module), Android `MobileDeviceDirectoryTest` (2), HarmonyOS `MobileDeviceDirectory.test.ts` (registered in build.sh). All assert multiple sources per device merge into one row, unmatched records stay separate, no invented status labels, summaries count actionable entries only.
- Reruns: iOS 22 host tests, no failures/skips (fixed prior `AgentHaloFoldPostureReader` tests leaking across runs via shared `UserDefaults.standard` by using isolated suites); full App type checking still has only existing errors. Android `--rerun-tasks` 132/20 suites, no failures/skips, both debug APKs compile (instrumentation updated to expand Advanced before asserting transport). HarmonyOS full `build.sh` passed, unchanged `DevicesLayout` AST gate (13 regression probes, 8 directory states, 7 pairing states) and `MobileStyle` (84 buttons, 260 font declarations) pass; unsigned HAP SHA256: `6746538eeeded97710b9fdf9a8f2792ba649f57e3ec0e0b9b19d1274eb19a486`.

## Unaccepted Boundaries

- No actual foldable device participated. Android `FoldingFeature`/HarmonyOS `display` posture adapters were compilation-checked only; tests cover posture→layout mapping with primitive inputs, not physical posture events.
- iOS currently has no fold-posture API: Automatic acts as Off, and only Always half-fold enters flex. iOS columns received type checks/history-policy unit tests only, not inspection on iPad/simulator.
- OS preference calls on all three platforms (paths beyond the isolated UserDefaults round-trip, including SharedPreferences / Preferences) were not executed on devices; tested parts are mappings and iOS's isolated suite.
- ArkUI AST is not pixel/font-scale/touch testing; Android Compose instrumentation compiled only; iOS host tests do not establish iPhone pages, Dynamic Type, or keyboard acceptance.
- All require physical checks: fold/unfold, flex keyboard/both scroll areas, widths 320–430, large system fonts, long device names/errors, Send/recovery accessibility. The Devices hierarchy/collapsed Advanced were not visually checked on hardware.
- No production client was release-signed, notarized, installed, or launched; no phones, pairing data, Keychain, Relay, mac-c / mac-b, or virtual network operated on.
- Existing gaps (unfixed here): `src/ios/CoreTestHarness/run-authenticated-catalog-host-tests.sh` and `MODULE=app src/ios/CoreTestHarness/run-core-tests.sh` omit `AgentHaloWorkspaceSessions.swift` from source lists and already fail on master; adding it collides with `MusterIOS`'s own `AgentHaloSessionRoute`. This round instead adds `run-mobile-shell-host-tests.sh` for pure presentation modules, also registering new tests in the catalog script for when those conflicts are resolved.
- Pairing, remote read/write, and recovery acceptance still follow the [cross-platform checklist](agenthalo-cross-platform-completion.md) separately; layout build results cannot replace them.
