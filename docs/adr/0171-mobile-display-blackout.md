# ADR 0171: Mobile control of Mac display blackout

- Date: 2026-11-10
- Status: Accepted, physical acceptance pending
- Related: [ADR 0007](0007-desktop-manual-locked-use.md), [ADR 0084](0084-keep-awake-menu.md), [ADR 0128](0128-the-panel-helper-keeps-its-own-identity.md)

## Context

A phone user wants the selected Mac to stay awake and unlocked with a pure black overlay on every screen until an authorized phone turns the mode off. The existing privacy cover follows the desktop UI and allows a local hotkey to remove it.

## Decision

Add a distinct device RPC guarded by Locked Control, target-local policy and the calling member's phone identity. The daemon persists the explicit on/off choice, supervises its own panel helper and publishes the helper's observed readiness separately. The phone renders the target's state. Closing either user interface or losing the phone connection leaves the daemon in charge.

The panel helper holds display and system idle-sleep assertions and maintains an opaque nonactivating panel per screen as the layout changes. It closes when the daemon exits. A restarted daemon reconciles the saved choice. Hardware brightness, OS authentication and the existing Locked Use lease have independent lifecycles.

## Alternatives

Hardware brightness on extended displays depends on monitor and connection. The operator selected a pure black overlay for every display. Reusing the existing cover switch would let the desktop hotkey clear the mobile mode.

## Consequences

The effect is visual privacy in the signed-in desktop session. System login, explicit lock, forced sleep and administrator policies remain under OS control. Each online authorized phone can end the mode, including a replacement for the phone that enabled it.

## Verification

Local Rust regressions passed for persisted mode recovery with a simulated panel, shutdown, and handset projection. Two real loopback RPC integration tests verified phone scope and local-policy enforcement, authorized release, and desktop and local caller refusals. The macOS panel executable linked. The Apple UI built for macOS and the iOS simulator, Android Kotlin compiled, and the HarmonyOS ArkTS UI compiled in an isolated UI-only build. Regenerating HarmonyOS bindings also synchronized intervening schema additions. The standard HAP build requires a separate native Rust archive.

Physical acceptance is pending for a paired phone, built-in and extended screens, hot-plug, disconnect/reconnect, observed power assertions and remote release. Signing, publication and installation are outside this source change.
