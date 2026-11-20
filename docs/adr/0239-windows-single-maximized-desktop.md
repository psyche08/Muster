# ADR 0239. Open one maximized Windows desktop per user session

- Date: 2026-11-20
- Status: In development
- Related: ADRs 0233 and 0236

## Context

The installed desktop opened at a fixed size and launching a development or
installed build could create another main window. The requested experience
is one GUI per interactive Windows user session, maximized on first launch.

## Decision

Use a stable Windows mutex scoped to the current user and session before
constructing the desktop or Rust client. Mark the primary window handle for
activation by a repeated launch. Restore a minimized owner and preserve a
manually resized window. Use native maximization with normal window chrome
and taskbar. Installed and development GUI builds share ownership. The CLI,
daemon and vnet helper have separate lifecycles.

## Alternatives

A process-name check would also match the CLI and could race with a second
launch. Borderless full screen would remove the normal title bar. A launcher
shortcut would not cover every entry point.

## Consequences

Close the current GUI normally before testing a different client binary.
The installed daemon and tunnel can stay running through that change. An
older installed GUI predating the marker can be recognized by its installed
path during migration. Older binaries themselves cannot enforce this rule.

## Verification

Use concurrent test processes, check creation and reactivation, minimize and
restore, and check a new launch after the owner exits. Check the actual GUI,
daemon and traffic over the existing tunnel separately from installer results.

On the Windows host four simultaneous fixture launches created one form. Its
first window was maximized with the native title bar and sizing frame. A
repeat launch restored it from minimized, preserved a manually restored size,
and a new owner started after closing the first. The installed previous build
remained online with its tunnel while these GUI checks ran. Launching the
new development executable with the older installed GUI already open returned
successfully and left that same GUI running.
