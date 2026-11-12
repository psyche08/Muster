# ADR 0190 – Admit desktop operations on the executor without taking foreground

- Date: 2026-11-12
- Status: Accepted
- Related: [0189](0189-reuse-native-create-windows.md), [0115](0115-target-scoped-local-input.md)

## Context

Both Multica creations and peer requests can reach an unlocked executor while
the owner types in a different application. Native creation and exact session
selection previously used Launch Services to activate the provider. An
installed Codex directory-route probe with background launch requested still
made the foreground test editor resign active.

## Decision

Use the executor's keyboard-focus owner. An uncovered task may operate a
verified, empty native creation form in the requested local directory with
target-scoped input guards. Hold foreground-dependent navigation in the
provider queue until the provider is foreground or a suitable surface becomes
available. Recheck at the OS action boundary. Empty or unknown focus is
insufficient for foreground routing. Keep the existing task authorization,
receipt, cancellation, directory and covered console checks.

## Alternatives

Calling open with its background flag still allowed the provider to activate
itself. Restoring the previous foreground app after routing leaves a time
interval in which a user's keystroke could go elsewhere.

## Consequences

A new task directory may require the provider's own foreground folder flow.
A waiting request remains subject to its caller's lifetime and timeout.
Already visible sessions can be resolved without selecting another window.

## Verification

Exercise policy and provider tests, then use an independent foreground editor,
system-delivered test input, window order, pointer and activation observations.
Report real service outcomes and each task's receipt separately from module tests.

Initial device trials observed two independently assigned runs enter the wait
on an unlocked Mac. The foreground editor received 174 of 174 and 226 of 226
alternating system-delivered characters. The observer saw no activation loss,
normal-layer window order change or pointer movement in those intervals.
After the operator released the editor and selected the provider, both runs
stopped before drafting during a new-window transition. Use the workspace's
frontmost application for the activation check. A temporary lack of an AX
focused element does not establish that another app became frontmost. A
verified empty Codex home already focused within the provider can also be
claimed for routing after the operator selects it.
