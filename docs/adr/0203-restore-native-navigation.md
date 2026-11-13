# ADR 0203: Restore a borrowed native session after an operation

- Date: 2026-11-13
- Status: Proposed for review
- Related: [ADR 0196](0196-foreground-waits-and-provider-queue.md), [ADR 0199](0199-local-dispatch-grant-for-locked-use.md), [ADR 0201](0201-background-claude-card-selection.md)

## Context

A send, creation or permission-card inspection can change the session shown on the target Mac. Preparation can fail after dispatching navigation. A receipt or a successful read therefore cannot be the only trigger for cleanup.

## Decision

Capture the foreground process and its lifetime, the provider process, its original main window and visibility, exact session/page identity and the system HID generation before changing the desktop. Restore only while the same provider still displays the operation's proven destination, focus is attributable to the operation and there has been no intervening HID input. An unknown original session permits only focus restoration. Never derive a restore route from a window title. Fresh console, deadline, identity, input and destination checks guard each restoration. Log skipped or failed cleanup independently of the operation result.

Group ordinary operations on the same target for a short idle interval. A covered operation attempts cleanup within its existing worker authority before relock, without waiting through a debounce interval. Prefer a live observation or an already known pending interaction for inspection. Defer navigation while the local user is active. A foreground client can retry a deferred inspection later without an error banner.

## Alternatives

An unconditional back action, activating the previously frontmost application on every exit, or restoring the last session matching a title can overwrite a local user's subsequent choice. Cleanup outside the covered operation cannot inherit permission to drive a relocked console.

## Consequences

Missing exact identity or a local takeover limits cleanup. Restoring never replays a send or an approval decision.

## Verification

Exercise success, failure, timeout, cancellation, changes of foreground or selected session, HID input, unknown originals, coalescing and busy inspection. Check the native adapter, daemon, and client. Record real paired-device validation separately from simulated host tests.

The ordinary inactivity window is 35 seconds. An inspect needing navigation
defers when system-wide HID idle is under three seconds. An attached visible
surface can still be read and a retained required interaction can be delivered
from the authorized watch. A deferred read schedules a single cancellable
35-second retry. Covered cleanup never extends or renews a lease.

Local verification used four Rust library suites (805 passing, nine ignored)
with four test threads and an 8 MiB test stack. Strict Clippy checked all
targets of the four crates. The repository layer checker also reports two
existing disallowed edges on main (muster-acp to muster-agent and muster-ffi
to muster-paths). Workspace rustfmt reports an existing difference in
muster-agent/src/questions.rs.

Device acceptance remains open. The target and the other paired Macs
reported locked consoles. The owner could not unlock the target for the
requested A-to-B-to-A scenario. No production daemon was replaced. Code and
Chat pages without an exact observable ID, and same-window pages without an
exact return route or link, can limit restoration. Physical create, cancel
and pre-relock restoration also remain unverified.
