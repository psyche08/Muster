# ADR 0189 – Reuse desktop windows for native session creation

- Date: 2026-11-12
- Status: Accepted
- Related: [0150](0150-remote-desktop-session-creation.md), [0154](0154-wait-for-slow-codex-windows.md), [0183](0183-locked-codex-creation-stages.md)

## Context

Creating several Codex sessions presses New Window for each task, including
when the preceding task left a usable window. Claude's code/new route supports
its main shell. The owner wants independent tasks to share a desktop window.

## Decision

Supersede the per-task Codex window allocation in ADR 0183. Keep one retained
AX window allocated by Muster for the lifetime of the Codex process and the daemon.
Remember it as soon as the menu-created window is identified, including when
later readiness or routing fails. On each creation match that exact window in
the live signed process before reuse. A closed window or a new process allows
one replacement. A sole empty Codex home created during app activation can
serve as the initial window when its verified project matches the request. A restored thread is ineligible. On Claude, prefer the uniquely identified main shell before
dispatching its code/new route. Use the existing activation/creation discovery
when no main shell is present or the provider itself opens a distinct window.

Check focus before sending a provider-wide URL. Keep the draft, process,
physical-input, directory, form and delivery checks. On a reused Codex window the routed form must show the home heading,
local mode and the requested project picker before draft or submission.
Each operation requests
a new session and resolves its own transcript receipt. A failed raise or
unconfirmed route ends the attempt without opening a fallback window.

## Alternatives

Choosing the frontmost or first Codex window would allow automation to take
over an unrelated conversation. Persisting an accessibility reference beyond
the provider or daemon lifetime cannot establish the identity of a live window.

## Consequences

The first Codex creation may open a dedicated window. Later tasks can use it
across directories. The signed provider still controls how it handles a URL.
Existing user windows remain open. Window reuse does not guarantee background
focus preservation.

## Verification

Exercise repeated selection, closed windows, process restart and ambiguity.
Check the native agent and macOS adapter suites. Physical acceptance requires
two distinct creation receipts in one retained window per provider.

The agent library suite passed 398 tests with four ignored. All 52 platform
library tests and Clippy passed on macOS. The selection tests covered
reordered windows, a closed target, a replaced process and a route-created
window. Consecutive real provider submissions and installed acceptance are
pending.
