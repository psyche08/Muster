# Mobile Layout Consistency Validation Record

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Date: 2026-09-12. Scope: native iOS, HarmonyOS, and Android UI. The [visual specification](../design/agenthalo-mobile-style.md) and its JSON tokens form this round's shared baseline.

## Scope of Changes

- Unified the dark background, blue accent, cards and input fields, text hierarchy, page spacing, and “Sessions / Devices” navigation; retained Android's Terminal and Settings.
- iOS retains native List / Form / navigation / safe-area composer; pairing, authenticated catalog, sessions, and startup failure pages use the same theme.
- Android fixes duplicate system-bar spacing, uses vertically grouped session lists on phones, and separates input from action rows; HarmonyOS increases text and button sizes and wraps actions on narrow screens.
- Authorization, pairing, networking, state machines, durable recovery, and the Shared ABI are unchanged. Existing disable conditions, read-only notices, simulated sources, DEBUG warnings, errors, and recovery actions are retained.

## Completed Independent Checks

- Shared validation: all 16 semantic colors on each platform match the JSON; 17 common text/background pairs have contrast ratios of at least 4.5:1.
- iOS: 32 host tests (29 existing + 3 theme tests), with no failures or skips; an independent rerun by the root task also passed. Unsigned Device SDK build and Simulator SDK App + XCTest `build-for-testing` passed; the latter did not execute the complete device test suite.
- Android: final total of 120 JVM tests (114 existing + 4 theme + 2 height-budget tests), 18 suites, no failures or skips; the root task forced all tasks to rerun and passed again. App and instrumentation Debug APK compilation passed; development signing is not release signing.
- HarmonyOS: real SDK AST theme checks passed, covering 79 multiline buttons with a minimum size of 48vp, 5 inputs, and 228 font-size declarations; existing device layout, lifecycle, and observation regressions passed. The root task independently reran the theme checks and 13 observation regressions successfully.
- SwiftUI: actual shared Card / Notice / Button components were rendered in an isolated macOS host and images inspected at widths of 320 / 360 / 390 / 430; long Chinese notices and buttons had no horizontal overflow. No product App / model / Core was constructed.

## Height Budget Review

Independent review noted that simply enlarging buttons and wrapping parameters increases the fixed composer height; diagnostics and recovery items in a fixed header can also squeeze out the transcript in a short viewport. The header and composer therefore need upper bounds based on the actual available session height, with overflow remaining scrollable and operable. Send and recovery buttons must not be hidden or clipped.

- HarmonyOS calculates a 30% header / 45% composer upper bound from measured session height. Both areas are independent Scroll containers, leaving at least 25% for the transcript. Native keyboard avoidance uses RESIZE, and Navigation uses the remaining height to avoid counting the top bar and DEBUG banner twice. New height State and presentation callbacks do not change existing runtime or authorization methods.
- Android calculates the same proportions inside BoxWithConstraints after system bars and IME have been deducted, subtracting dividers first; there is no fixed minimum composer height that can squeeze out the transcript. The header (including the offline notice) and the entire composer scroll independently, retaining every option and the Send button.
- HarmonyOS's real SDK AST executes the actual geometry helper, covering normal/short viewports, invalid input, height bindings for both Scroll containers, keyboard mode, and Navigation's remaining space. The root task reran this gate, 13 observation regressions, and device-page tests against the final source successfully.
- Android adds a real Compose fixture: width 320, height 320 → 200, 1.6× font scale, long runtime information, and a multiline draft. It asserts upper bounds for each area, a nonzero nonoverlapping transcript, and scroll access to Send and notices. This case and other instrumentation tests were compiled only, not run on a device.
- After the height fix, both Android Debug APK types and the full HarmonyOS `AGENTHALO_HAP_SIGNING=skip bash src/native/harmonyos/build.sh` completed again. The latter includes host regressions, OHOS Rust / N-API, ArkTS, and HAP packaging; final unsigned HAP SHA256: `75f4080253ca05ef69bacd6f86bbe85bb2cd3fe42ec403f149717f363bd5248f`.

## Unaccepted Boundaries

- No production client was release-signed, notarized, installed, or launched; no phones, pairing data, Keychain, Relay, mac-c / mac-b, or virtual network were operated on.
- Android's new Compose narrow-screen / large-font assertions were compiled only; HarmonyOS AST checks are not ArkUI pixel or touch tests.
- macOS host SwiftUI font metrics differ from iPhone and do not scale with iOS Dynamic Type; component previews do not establish acceptance of complete iPhone pages or large fonts.
- All three platforms still require physical-device checks: widths 320–430, system large fonts, long device names and errors, software keyboard, portrait/landscape and fold transitions, transcript scrolling, and access to Send and recovery actions.
- Functional acceptance of pairing, remote reads/writes, and operation recovery remains separate under the [cross-platform checklist](agenthalo-cross-platform-completion.md); this round's layout build results cannot substitute for it.
