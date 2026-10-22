# AgentHalo Mobile Visual Specification

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Scope: native iOS, HarmonyOS, and Android UI. Retain the dark workspace and blue accent, with consistent information hierarchy and action density; platforms need not copy the same native controls.

## Colors

Machine-verifiable values are in [mobile-style-tokens.json](mobile-style-tokens.json). Platform themes reference the same semantic roles; business models do not interpret colors or styles.

| Role | Value | Purpose |
| --- | --- | --- |
| background | `#0D1117` | Page background |
| surface / raised | `#161B22` / `#202732` | Cards / secondary containers |
| border | `#303B4A` | Noninteractive decorative dividers |
| text / secondary / muted | `#E9EDF4` / `#A8B2C0` / `#8793A4` | Body / secondary explanation / metadata |
| accent | `#8DAFFF` | Links, selection, icons |
| primaryFill / onPrimary | `#31517F` / `#FFFFFF` | Primary action buttons |
| success | `#79D89D` | Success feedback, accompanied by text or an icon |
| warning / warningSurface | `#F1B66B` / `#332719` | Warnings and read-only notices |
| danger / dangerSurface | `#FF9A8E` / `#351D22` | Failures, rejection, and destructive actions |
| userBubble | `#263451` | User messages |

Color alone must not convey connection, authorization, or operation results; continue displaying Shared's original projections. The darker border is decorative and must not be the only visible boundary for inputs or focus. Do not pair light-blue accent-filled buttons with small white text.

## Layout and Typography

- Use spacing of 4 / 8 / 12 / 16 / 24 / 32; horizontal phone padding is 16, or 24 on wide screens.
- Card radius and internal padding are 16; input radius is 12. Avoid repeatedly nesting the same information in multiple card layers.
- Page titles: 24–28; section titles: 18–20; body: 16–17; supporting text: 14; metadata: at least 12. Retain system font scaling and do not compress line height in Chinese paragraphs.
- iOS action areas are at least 44; prefer 48 on Android / HarmonyOS. Use minimum heights for multiline text instead of fixed heights that truncate.
- At logical widths of 320 / 360 / 390 / 430, titles and actions may wrap, and approval options and parameters use wrapping containers. Do not squeeze the entire page horizontally to preserve one line.
- Allow long device names, errors, and recovery hints to wrap. Session titles may truncate, but device identity, error reasons, and recovery actions remain accessible.
- The composer respects system safe areas and the keyboard; message lists retain independent scrolling and existing navigation identity.
- For fixed header + body + composer layouts, set limits using the session's actual available height: header at most 30%, composer at most 45%, reserving about 25% for the body (excluding dividers). Overflowing supporting content scrolls within its region; do not clip Send, errors, or recovery actions to gain space. These ratios do not replace native keyboard testing.

## Unified Shell

All three platforms use the same four destinations; order and keys are in `shell.destinations` in [mobile-style-tokens.json](mobile-style-tokens.json):

| Key | Title | Content |
| --- | --- | --- |
| `sessions` | Sessions | Session list and transcript |
| `terminal` | Terminal | Currently blocking approvals and structured questions, plus `terminal/*` provider sessions |
| `devices` | Devices | Pairing, Muster device directory, and connection state |
| `settings` | Settings | Display preferences such as half-fold layout, capability explanations, and data provenance |

Destinations name pages, not capabilities, authorization, or reachability; those still come from Shared projections. Approvals and questions must also appear in the sessions that raised them; Terminal is an aggregate entry, not the only entry.

## Layout Modes and Half-Fold

`layout` tokens provide geometry constants shared by all three platforms. Resolution is a pure function; unmeasurable inputs always fall back to a single column:

| Mode | Trigger | Structure |
| --- | --- | --- |
| compact | Width < `expandedMinWidth` | Single column: session list or one pushed session |
| expanded | Width ≥ `expandedMinWidth`, or a vertical crease (book fold) | Side-by-side list + session; list width is `listPaneFraction`, clamped between `listPaneMinWidth`/`listPaneMaxWidth` |
| flex | System reports a half-fold with a horizontal crease, or user selects Always Half-Fold | Top/bottom halves: transcript above the crease; composer, execution parameters, and pending approvals for that session below |

- Use half-fold only when window height ≥ `flexMinHeight`, top half ≥ `flexMinTranscript`, and bottom half ≥ `flexMinComposer`; otherwise fall back to compact/expanded instead of compressing content or Send into unusable strips.
- Clamp the system-reported crease position between `flexMinTopFraction` and `flexMaxTopFraction`; use `flexDefaultTopFraction` if the crease cannot be read, without guessing its position.
- Posture comes only from the system (Android `androidx.window` `FoldingFeature`, HarmonyOS `display` fold status and crease region), never inferred from window size. iOS currently has no fold-posture API, so posture is always unknown.
- The Half-Fold Layout preference has Auto / Always Half-Fold / Off options in Settings. It affects display only and persists per installation in each platform's display-preference storage (iOS UserDefaults, Android SharedPreferences, HarmonyOS Preferences), never business or trust state. Unknown values fall back to Auto. Always Half-Fold remains subject to the minimum heights above.
- In half-fold mode, the software keyboard covers only the bottom half. The top transcript and bottom approvals/composer scroll independently; splitting panes must not hide or clip blocking states, error reasons, or recovery actions.

## Devices Page

**New candidate design, not implemented, 2026-09-21.** iOS / Android / HarmonyOS share the [device-page architecture](../architecture/agenthalo-device-page.md) and the same network roles, states, permission direction, and recovery semantics as the [desktop specification](agenthalo-desktop-devices.md). The [first implementation record](history/agenthalo-device-page-2026-09-21-v1.md) is historical; its build results cannot establish completion of the new design.

```text
Devices                                      Refresh
My device network · This device is a member   Network details ›
Authorization valid · Last sync 17:20

Needs attention (only when applicable)
  Waiting to compare verification codes      Continue

This device
  My phone · Network member
  Can access network devices; no remote services offered

Network devices                All / Usable / Needs attention
  mac-c · Computer · Network member          ›
  Available: view sessions, send messages
  mac-b · Computer · Network member           ›
  Not ready to offer services

▸ History and compatibility records
▸ Diagnostics
```

Before enrollment, show Scan to Join Network. After enrollment, do not describe network identity as Paired with n Authorities or keep Add to Another Computer as a routine primary action. Show invitations only when Shared provides a valid invitation path. Phones cannot create a Master, and enrollment does not make them authorities.

Tapping a row opens device details: identity, services this device can use on the selected device, access the selected device has to others, connection and services, management, and technical details. Viewing sessions or selecting a new-session target is explicit within details; selecting a target neither affects existing sessions on other devices nor grants access. Back restores list filters and scroll position.

All members can view permissions; creator identity does not create a permanent permission-editing entry. Explain separately authorization granted but service not offered, no authorization, and current unreachability. Current scopes apply to accessor members; scope must be explicit, without false per-target editing. Where complete production paths such as terminal are not wired, show Not Supported in This Version. Show session counts only in details when explicitly supplied by the directory; cached counts do not establish online state.

Any device going offline, including the creator, affects only its own row. Other online members can still access one another through Muster, including an online creator. Creator unavailability must not disable the whole page or require waiting for its renewal; enrollment-authorization waits apply only to incomplete requests.

Use this document's layout tokens. At 320/360/390/430 use one column; tablets or landscape may use list/detail columns when enough width is available. The devices page does not inherit the transcript/composer top/bottom half-fold ratio, and creases must not split device rows or buttons. Large fonts naturally wrap states and actions without fixed card heights clipping content.

- iOS: NavigationStack within the Devices tab; scanning uses the existing camera adapter; iPad uses native split views; existing authentication and recovery gates remain.
- Android: List and details share the same navigation destination system; system Back retains position; TalkBack reads disabled reasons; JNI only consumes Shared projections.
- HarmonyOS: Combine the existing local pairing and Master cards into one network overview without double-counting members; retain HUKS transactions and directory error/refresh recovery entries; ArkTS no longer combines five fields to reconstruct service state.

Current iOS `AgentHaloMobileDeviceDirectory`, Android `MobileDeviceDirectory`, and HarmonyOS `domain/MobileDeviceDirectory.ts` are legacy platform projections. The new design first extends Shared/ABI, then replaces their business decisions; do not keep copying three algorithms. Retain real/simulated source indicators; simulated data belongs only in clearly marked development previews.

## Navigation and State

- Common navigation is consistently Sessions, Terminal, Devices, Settings.
- Loading, empty, offline, read-only, and error messages use the same text hierarchy. Slow operations retain progress and existing disabling conditions.
- Retain real/simulated provenance, read-only restrictions, and DEBUG authentication-bypass warnings. Nonactionable technical details may collapse; current blockers, unknown outcomes, and recovery actions must remain visible.
- Do not change page route keys, handle identity, lifecycle, pairing, or authorization flows; do not generate business state from color, caches, or HTTP success.

## Verification Boundaries

Record platform layout regressions and SDK builds separately from shared color and contrast checks. Source rules only check layout constraints; they do not replace rendering, Dynamic Type, software keyboard, or real-device touch acceptance. Layout changes do not complete existing remote-write, pairing, or authentication gaps, nor authorize signed installation, data cleanup, or remote migration.

Run shared semantic-color and 17 common text-contrast checks from the repository root:

```sh
node docs/design/check-mobile-style.mjs \
  src/ios/MusterIOS/MusterIOS/AgentHaloMobileStyle.swift \
  src/native/harmonyos/entry/src/main/ets/pages/AgentHaloMobileStyle.ets \
  src/native/android/app/src/main/kotlin/dev/linexample/agenthalo/ui/AgentHaloTheme.kt \
  --layout \
  src/ios/MusterIOS/MusterIOS/AgentHaloMobileLayout.swift \
  src/native/harmonyos/entry/src/main/ets/domain/FoldableLayout.ts \
  src/native/android/app/src/main/kotlin/dev/linexample/agenthalo/ui/MobileLayout.kt
```

The three files after `--layout` must declare the same `layout` constants and cover all four destinations. Each platform also has its own layout-resolution regressions:

```sh
bash src/ios/CoreTestHarness/run-mobile-shell-host-tests.sh
(cd src/native/android && ./gradlew :app:testDebugUnitTest)
# HarmonyOS FoldableLayout.test.ts runs with src/native/harmonyos/build.sh
```

An isolated macOS host preview uses the actual SwiftUI Card / Notice / Button components to render long Chinese text, errors, recovery, and disabled buttons at logical widths of 320 / 360 / 390 / 430. Images go to a separate temporary directory; it does not build or launch the product App / models / Core:

```sh
bash src/ios/LayoutPreviewHarness/render-mobile-style.sh
```

Host font metrics differ from iPhone, and macOS does not scale these semantic fonts using iOS Dynamic Type. This preview therefore verifies component colors and narrow-width wrapping only; it does not claim full-page, iOS large-font, keyboard, or real-device layout acceptance.

Design decisions, resolution algorithms, and platform wiring for the unified shell and half-fold layout are in [Mobile Unified Shell and Half-Fold Layout](../architecture/agenthalo-mobile-shell.md). Source, build, and unverified boundaries for that round are in the [validation record](../testing/agenthalo-mobile-shell-flex.md); the earlier color and height-budget round is in the [mobile layout validation record](../testing/agenthalo-mobile-layout.md).
