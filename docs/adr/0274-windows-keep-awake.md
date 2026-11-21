# ADR 0274. Keep the Windows desktop awake while Muster is running

- Date: 2026-11-21
- Status: Accepted
- Related: [ADR 0084](0084-keep-awake-menu.md), [architecture §3.6.2](../architecture/agent-halo.md)

## Context

The Mac GUI can keep its host awake for paired clients while allowing the display to sleep. The Windows GUI has no equivalent choice. An ordinary user should be able to change this local OS preference even when the daemon is disconnected.

## Decision

Expose Keep Awake and Keep Screen On in a Power menu in the regular Windows desktop. Default to keeping the system awake with normal display sleep. Remember both switches for the current Windows user across portable updates. Disabling Keep Awake disables the screen switch and preserves its preference. Apply the request on the GUI thread with SetThreadExecutionState and release it on shutdown. The OS ties its lifetime to the thread. Attach-only and isolated development launches leave production power preferences alone.

## Alternatives

Changing the machine power plan affects other applications and outlives Muster. Having the daemon hold the request would tie it to the wrong lifetime. Like the Mac preference, this is local OS integration and needs no change to the Shared Rust ABI.

## Consequences

The OS can prevent idle system sleep for a running desktop. The operator can still lock the computer or explicitly put it to sleep. The user can opt in to keeping the display on. Windows battery and power policies may affect the observed behavior.

## Verification

Exercise the Windows menu against a disposable user preference. Inspect the actual GUI thread execution state after startup, toggling, relaunch and disposal. Verify the packaged GUI shows the defaults or saved preferences, and distinguish API verification from physical sleep tests.

The Windows form tests passed at 192 DPI. On the same STA thread the OS reported system-only, system plus display, and cleared states after menu clicks. Recreating the menu restored the disabled system choice and remembered the display choice. Disposal released the active request. Lid-close, battery policy and physical sleep were not exercised.
