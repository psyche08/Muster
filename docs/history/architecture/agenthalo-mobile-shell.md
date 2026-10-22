# AgentHalo unified mobile shell and half-fold layout

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

**Status:** The first version landed on iOS, Android, and HarmonyOS on 2026-09-20 and passed each platform's host/build gates. A second round that day added preference persistence and iOS side-by-side panes. No acceptance testing on physical foldables. See the [unified mobile shell and half-fold layout validation record](../testing/agenthalo-mobile-shell-flex.md) for evidence and the [mobile visual specification](../design/agenthalo-mobile-style.md) for visual and geometry conventions.

## Goals and non-goals

Goals:

- Present the same shell on all three platforms: four destinations, the same layout modes and geometry constants, differing only in native controls.
- Give foldables a half-fold (flex) mode: when half-folded with a horizontal crease, the transcript sits above it, while the composer, execution parameters, and this session's pending approvals sit below. The software keyboard covers only the lower half.
- Obtain posture only from the OS; resolve layout with a pure function and fall back to one column for any uncertain measurement.

Non-goals:

- Do not change route keys, handle identities, lifecycle, pairing, or authorization flows, or introduce business state.
- Do not integrate Shared Rust. Layout and display preferences belong to presentation, not the business logic that [AGENTS.md](../../../AGENTS.md) requires moving into Shared. Retain the existing “JSON tokens + semantically equivalent implementation per platform + source checks” pattern instead of adding an ABI. Preferences stay in each platform's display-preference store and never enter trust state.

## Shared conventions

[mobile-style-tokens.json](../design/mobile-style-tokens.json) adds two groups:

```json
"shell": {
  "destinations": ["sessions", "terminal", "devices", "settings"],
  "labels": { "sessions": "会话", "terminal": "终端", "devices": "设备", "settings": "设置" }
},
"layout": {
  "expandedMinWidth": 600,
  "flexMinHeight": 600,
  "flexMinTranscript": 220,
  "flexMinComposer": 220,
  "flexMinTopFraction": 0.40,
  "flexMaxTopFraction": 0.62,
  "flexDefaultTopFraction": 0.50
}
```

The label literals above mean “Sessions,” “Terminal,” “Devices,” and “Settings,” respectively.

[check-mobile-style.mjs](../design/check-mobile-style.mjs) adds a `--layout` argument group alongside its existing color/contrast checks: every subsequent source file must declare the seven layout constants with identical names and values using `static let` / `const val` / `static readonly`, and contain all four destination keys. Platform layout modules:

| Platform | Layout module | Posture adapter |
| --- | --- | --- |
| iOS | `src/ios/MusterIOS/MusterIOS/AgentHaloMobileLayout.swift` | `AgentHaloFoldPostureReader` in the same file (always unknown) |
| Android | `src/native/android/app/src/main/kotlin/dev/linexample/agenthalo/ui/MobileLayout.kt` | `ui/FoldPostureSource.kt` |
| HarmonyOS | `src/native/harmonyos/entry/src/main/ets/domain/FoldableLayout.ts` | `data/HarmonyFoldPosture.ets` |

The constants satisfy `flexMinHeight × flexMinTopFraction ≥ flexMinTranscript` and `flexMinHeight × (1 − flexMaxTopFraction) ≥ flexMinComposer`: even the shortest eligible window can fit two usable halves at either extreme crease position. The validator asserts both inequalities. 600dp excludes landscape phones while covering real half-fold devices (Z Flip-class devices are roughly 890dp tall, Mate X/Pocket-class devices taller) and iPhone 15 Pro dimensions (roughly 800pt available height).

## Layout resolution

All three platforms use the same algorithm. Inputs are container dimensions, posture reading, and user preference; outputs are mode and split ratio:

```text
resolve(width, height, posture, hinge, hingeCenterFraction, preference):
  if width/height is nonfinite or ≤ 0        → compact
  requested =
    preference == disabled  → false
    preference == always    → true
    preference == automatic → posture == halfOpened && hinge == horizontal
  if requested && height ≥ flexMinHeight:
    fraction = isFinite(hingeCenterFraction) ? clamp(hingeCenterFraction, min, max) : default
    transcript = round(height × fraction)
    if transcript ≥ flexMinTranscript && height − transcript ≥ flexMinComposer
                                            → flex(fraction)
  if width ≥ expandedMinWidth               → expanded(listPane = clamp(round(width × 0.38), 240, 310))
  else                                      → compact
```

Deliberate choices:

- **A vertical crease is not flex.** A book-style fold (half-open Fold / Mate X) has a vertical crease and side-by-side halves, naturally `expanded`; only a horizontal crease (Flip / tabletop mode) creates an upper/lower split. The posture reading separates `hinge` from `posture` specifically to distinguish these two half-fold forms.
- **Always-flex cannot bypass minimum height.** `always` only sets `requested` to true; the height gate still applies. Users may preview flex on ordinary phones, but landscape or split-screen windows silently fall back rather than producing a 180dp transcript.
- **Crease position is clamped, not trusted.** OS `hingeCenterFraction` values outside [0.40, 0.62] clamp to an endpoint; unavailable values use 0.5. The ratio always stays within this range, never allocating 90% of the height to the keyboard.
- **Failure means one column.** NaN, Infinity, zero, negative values, and unmeasured initial areas all yield compact. Every platform starts in this state before `onAreaChange` / `onGeometryChange` / `BoxWithConstraints` reports geometry.

The `expanded` list-pane width is unified too: previously Android used fixed 310dp with a 760dp breakpoint, HarmonyOS a 600vp breakpoint, and iOS had none. All three now use a 600 breakpoint and 38% width clamped to 240–310. At width 600, the list is 240 and transcript 360, barely sufficient; this ensures unfolded foldables (roughly 670–840dp) enter two-column mode.

## Posture sources

Posture is **read, never inferred**: no platform treats a tall, narrow window as half-folded. Each expresses “raw OS values → posture reading” as a testable pure function; OS types appear only in thin adapters.

**Android** — `androidx.window:window:1.3.0`. `rememberFoldPostureReading(activity)` in `FoldPostureSource.kt` collects `WindowLayoutInfo` using `WindowInfoTracker.windowLayoutInfo(activity)`, takes the first `FoldingFeature`, and passes it with window bounds from `WindowMetricsCalculator` to this pure function in `MobileLayout.kt`:

```kotlin
foldPostureReading(posture, horizontalHinge, hingeCenterY, windowTop, windowHeightPx)
```

`FoldingFeature.State.HALF_OPENED → HALF_OPENED`, `FLAT → FLAT`, no FoldingFeature → `UNKNOWN`. `Orientation.HORIZONTAL` means a horizontal crease. Split ratio = `(hingeCenterY − windowTop) / windowHeightPx`; zero window height yields NaN rather than zero (which would place the crease at the top). Readings enter through `AgentHaloApp(posture = …)` and propagate via `LocalFoldPosture`.

**HarmonyOS** — `display` from `@kit.ArkUI`. `HarmonyFoldPosture.ets` calls `start(listener)` in `aboutToAppear`: first `isFoldable()`; nonfoldable screens immediately report unknown without subscribing. Otherwise map `getFoldStatus()` (`FOLD_STATUS_HALF_FOLDED → HalfOpened`, `EXPANDED`/`FOLDED → Flat`), obtain `getCurrentFoldCreaseRegion().creaseRects[0]` and `getDefaultDisplaySync().height`, and pass them to this pure function in `FoldableLayout.ts`:

```ts
foldPostureReadingFromCrease(posture, creaseWidthPx, creaseHeightPx, creaseCenterYPx, displayHeightPx)
```

A crease with `width ≥ height` is horizontal. Every API call is wrapped in try/catch, with failure yielding unknown. Subscribe via `display.on('foldStatusChange')` and call `off` at the end of `aboutToDisappear`.

**iOS** — No fold-posture API. `AgentHaloFoldPostureReader.posture` is always `.unknown`; `apply(posture:hinge:hingeCenterFraction:)` is only for tests and future OS sources. Thus “Automatic” equals “Disabled” on iOS; only “Always half-fold” enters flex. The settings explanation says so explicitly. Types and resolver names/semantics match the other platforms, so a future OS posture API requires replacing only this reader.

## Preference persistence

The “Half-fold layout” preference persists per installation. Each platform uses its own display-preference store with matching keys and values (`automatic` / `always` / `disabled`). Missing or unknown values always return to “Automatic,” without guessing:

| Platform | Storage | Mapping |
| --- | --- | --- |
| iOS | `UserDefaults`, key `agenthalo.display.flexPreference`; `AgentHaloFoldPostureReader.preference.didSet` writes, `init(defaults:)` reads | `AgentHaloFlexPreference.stored(_:)` |
| Android | `SharedPreferences("agenthalo.display")`, `ui/FlexPreferenceStore.kt`; `MainActivity` reads the initial value for `AgentHaloApp(initialFlexPreference:onFlexPreferenceChange:)`; `rememberSaveable` still survives Activity recreation within the process | `FlexPreference.fromStored(_)` |
| HarmonyOS | `preferences.getPreferencesSync(context, { name: 'agenthalo_display' })`, `data/HarmonyDisplayPreferences.ets`; read in `aboutToAppear`, settings buttons call `putSync` + `flush()` | `flexPreferenceFromStored(_)` |

Any failed OS storage call falls back to a default or merely loses persistence; the selection still takes effect in the current session. Mapping functions are pure and unit-tested on each platform; storage adapters are outside unit-test coverage.

## Platform wiring

### iOS

- `AgentHaloRootView` presents `AgentHaloShellDestination.allCases` using `TabView(selection: $shell.destination)`. `AgentHaloShellState` (`@Observable`) holds the current destination and a one-shot “open conversation” request. The Terminal page calls `shell.open(conversation:)`; the sessions page calls `consumeRequestedConversation()` in `.onChange(of: shell.requestedConversation)` and pushes `.conversation(id)` onto **its existing** `AgentHaloNavigationHistory`. No second history stack; back policy, route keys, and composer drafts are unchanged.
- The sessions page reads `\.agentHaloMobileLayout`: `expanded` renders `HStack { NavigationStack { listRoot }.frame(width: listPaneWidth) | 1pt divider | NavigationStack(path:) { placeholder + navigationDestination } }`; otherwise it retains the original single stack. **Both layouts share one `AgentHaloNavigationHistory`**: compact treats it as a list-rooted stack, while expanded displays the same path in the right detail pane. Open conversations, route keys, and drafts therefore survive layout changes. Expanded list rows are `Button`s calling the new `AgentHaloNavigationHistory.select(_:policy:)`: populate an empty path, leave an already-open conversation unchanged, or replace the path **under the existing back policy** (`.blockedByMutation` during mutation, leaving the detail pane in place). Expanded mode immediately declines edge-back gestures; there is no poppable edge beside the list pane.
- The posture reader propagates via `.environment(posture)`; `.agentHaloResolvedLayout(posture)` (`AgentHaloMobileLayoutEnvironment.swift`) measures the root container with `onGeometryChange` and stores the result in `\.agentHaloMobileLayout`. SwiftUI code lives in a separate file so `AgentHaloMobileLayout.swift` depends only on CoreGraphics + Observation and can compile/test on a macOS 14 host (`onGeometryChange` requires iOS 18/macOS 15).
- `AgentHaloConversationView.body` wraps a `GeometryReader` and resolves again using **the conversation's own viewport**. Flex applies only if that viewport can fit both halves, regardless of total window size. The flex branch `flexBody` uses a `VStack`: `transcript(showsPendingInteraction: false).frame(height: transcriptHeight)`, a 1pt divider, then `flexInputHalf` (`ScrollView` of pending cards + `AgentHaloComposer`, with `maxHeight: .infinity, alignment: .bottom`). The non-flex branch remains `transcript(showsPendingInteraction: true).safeAreaInset(edge: .bottom) { AgentHaloComposer }`. `showsPendingInteraction` ensures approval cards appear in one place only.
- The Terminal page (`AgentHaloShellDestinations.swift`) lists `model.pendingInteractions` and conversations whose `providerID.hasPrefix("terminal/")`. iOS `respondToActiveApproval` / `answerActiveQuestion` operate only on the current conversation's `activePendingInteraction`, so this inbox **only lists and navigates**: each card offers “Go to conversation,” and the footer explains why. Look up the conversation using `providerID` + (`sessionID` or `nativeSessionID`); if absent, show an explanation rather than an unusable button.
- Settings takes “Security and permissions” (including capability details) and “Developer connection logs” from Devices, and adds a half-fold preference `Picker` and data-source information. Devices retains only pairing, the Muster device directory, and connection status. Capability availability/status labels move into `AgentHaloSecurityCapabilityStatus` and remain “product capabilities, not locally evaluated authorization.”

### Android

- `AppDestination` moves to `MobileLayout.kt` with `key`/`label`. Navigation is generated with `AppDestination.entries.forEach`, icons follow destinations, and Terminal shows a count when items are pending.
- `AgentHaloApp` holds `flexPreference` with `rememberSaveable(stateSaver = FlexPreferenceSaver)` and propagates it via `CompositionLocalProvider(LocalFoldPosture, LocalFlexPreference)`. `rememberSaveable` is used instead of `remember` because folding/unfolding recreates the Activity (the Manifest does not declare `configChanges`); preferences must not disappear on every fold.
- `WorkspaceScreen` and `ConversationPane` each call `MobileLayoutSpec.resolve` with `maxWidth/maxHeight` in `BoxWithConstraints`. The former distinguishes expanded/other only (the root has no transcript, so flex is equivalent to a compact list there); the latter has a flex branch:

  ```text
  Column(fillMaxSize)
  ├── Column(height = transcriptHeightDp)
  │   ├── SessionHeaderStrip(maxHeight = budget(transcript).headerMaxDp)   // Scrollable
  │   └── ConversationTranscript(weight 1)
  ├── HorizontalDivider
  └── Column(weight 1, Arrangement.Bottom)  testTag "flex-input-half"
      ├── SessionApprovals(weight 1, fill = false)                        // LazyColumn, scrollable
      └── Composer
  ```

  The upper header retains the original `ConversationViewportPolicy` height cap (30%), now based on the upper half's height.
- `SessionApprovals` filters with `PendingInteraction.belongsTo(session)`: present `deviceId`/`providerId` values must match, and `nativeSessionId ?: sessionId` must match one of the conversation's IDs. Missing remote fields do not broaden matching to other sessions. This is Android's first in-conversation approval display (previously only Terminal had one). Android `respond`/`answer` support any interaction, so cards are directly actionable.
- Terminal retains its global approval list and appends rows for `terminal/*` provider conversations in `sessionGroups`; Settings adds a half-fold `ChoiceMenu` and `flexStatusLabel` explanation.

### HarmonyOS

- `Index.ets.activePage` now uses `ShellDestination` string values. `showDestination(destination)` is the common entry point; sessions/devices still use `showAgentRoot`/`showDeviceNetwork`. Terminal does not pause conversation projection (its `pendingInteractions` come from the current conversation projection); Settings calls `pauseConversationProjection()` like Devices.
- Posture and preference are `@State foldPosture` / `@State flexPreference`. The Column containing `conversation()` records both `conversationAvailableWidth/Height` in `onAreaChange`; `conversationLayout()` resolves using them.
- `conversation()` splits into four `@Builder`s: `conversationHeaderScroll(maxHeight)`, `conversationTranscript(showsPending)`, `conversationComposerScroll(maxHeight)`, and `conversationPendingList()`. The single-column branch retains both `maxHeight` expressions (`headerMaxHeight(...) : '30%'`, `composerMaxHeight(...) : '45%'`) verbatim at their call sites, as asserted literally by `MobileStyle.test.cjs`. Flex branch:

  ```text
  Column
  ├── Column.height(transcriptHeight)  id flex-transcript-half
  │   ├── conversationHeaderScroll(headerMaxHeight(transcriptHeight))
  │   └── conversationTranscript(false)
  ├── Divider
  └── Column.layoutWeight(1).justifyContent(End)  id flex-input-half
      ├── conversationPendingList()            // Only with pending items; List, layoutWeight(1)
      └── conversationComposerScroll(flexComposerMaxHeight())
  ```

  `flexComposerMaxHeight()`: without pending items, the composer may fill the lower half; with pending items, it is capped at 55%, leaving at least 45% for approvals. Each scrolls independently.
- In `conversationTranscript.onScrollIndex`, the `isConversationAtLatest` offset no longer subtracts `pendingInteractions.length` when `showsPending == false`; otherwise flex “jump to latest” detection would be displaced.
- Terminal reuses actionable `pendingInteractionCard`s and then lists `terminal/*` provider conversations from `sessionSections`; Settings has three preference buttons + `flexStatusText()` + data-source information.
- Posture-subscription `stop()` sits at the **end** of `aboutToDisappear`, with optional chaining. `WorkspaceFiles.test.cjs` asserts `closeWorkspaceFiles()` appears within the method's first 320 characters, while `WorkspaceObservation.test.cjs` invokes it on a synthetic Page containing only some methods. A presentation-only listener should break neither.

## Half-fold invariants

- **Approvals render in one place only.** Compact: at the end of the transcript List (existing iOS/HarmonyOS behavior; newly above the composer on Android). Flex: in the lower half. The `showsPending(Interaction)` parameter is the only switch.
- **Blocking items remain reachable.** The lower approval area and composer scroll independently. The keyboard shrinks their scroll viewports, not their content. The upper header retains its 30% cap.
- **Switching preserves state.** Flex/compact changes only the layout tree, not `selectedSession`, drafts, attachments, or navigation history. Android preferences survive Activity recreation.
- **Preferences are not business state.** They never enter Rust or affect authorization projections, and only write platform display-preference stores (see “Preference persistence”); unknown values return to “Automatic.”

## Validation gates

| Gate | Command | Coverage |
| --- | --- | --- |
| Shared conventions | `node docs/design/check-mobile-style.mjs <three-theme-files> --layout <three-layout-files>` | Colors, contrast, matching shell/layout constants, destination presence |
| iOS | `bash src/ios/CoreTestHarness/run-mobile-shell-host-tests.sh` | `AgentHaloMobileLayoutTests` (resolution, clamping, failure fallback, no reader inference, preference persistence/unknown values, one-shot shell routing), `AgentHaloExpandedNavigationTests` (two-pane selection obeys back policy) + theme tests |
| Android | `cd src/native/android && ./gradlew :app:testDebugUnitTest` | `MobileLayoutTest` (same + `foldPostureReading` mapping + `fromStored`) |
| HarmonyOS | `bash src/native/harmonyos/build.sh` | `FoldableLayout.test.ts` (same + crease mapping + `flexPreferenceFromStored`), AST structural assertions in `MobileStyle.test.cjs`, ArkTS compilation and HAP packaging |

All three unit suites deliberately use the same cases (360×880 half-fold, 0.95/0.05 crease, height 599, vertical crease, NaN/zero/negative values…), exposing any platform resolver drift on the same case.

## Known boundaries and follow-up

- No physical-foldable acceptance testing; Android/HarmonyOS adapters were only compiled. Devices must verify: fold changes trigger layout, keyboard interaction with both scroll areas in half-fold mode, and alignment of `hingeCenterFraction` with the physical crease (Android bounds use window coordinates; HarmonyOS crease regions use screen coordinates and may shift in split-screen or floating windows).
- iOS two-pane behavior has only type checking and history/policy unit tests, with no iPad or simulator visual check. Selected-row highlighting depends on `listRowBackground` overriding container defaults and needs device confirmation.
- iOS Terminal approvals cannot be acted on directly because `WorkspaceModel` only serves the active interaction. Direct inbox decisions require interaction-targeted responses in the model first.
- HarmonyOS Terminal pending items project the current conversation, not all devices. This differs from Android global `pending` semantics and is stated in the UI copy.
- Preference storage adapters across the platforms (SharedPreferences / Preferences calls beyond UserDefaults) have not run on devices; tests cover mapping functions.
- `run-authenticated-catalog-host-tests.sh` and `MODULE=app run-core-tests.sh` already fail compilation on master (the source list omits `AgentHaloWorkspaceSessions.swift`; adding it conflicts with `MusterIOS.AgentHaloSessionRoute`). This round's tests are registered in the former, but currently run only through `run-mobile-shell-host-tests.sh`.
