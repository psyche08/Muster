# ADR 0247. Open the session terminal from a Windows conversation

- Date: 2026-11-20
- Status: Accepted
- Related: [ADR 0033](0033-managed-terminal-compatibility.md), [ADR 0245](0245-windows-interactive-terminal-grid.md), architecture §3.6.3

## Context

The Windows terminal catalog can operate an independent terminal. It cannot
connect from an agent conversation to that session's managed shell. Shared
Rust already projects the working directory, connection state, errors and
lifecycle actions. The Windows target daemon currently reports that the
tmux session service requires a Unix host.

## Decision

Add a terminal pane to the Windows conversation. Use SessionTerminalAction for
start/resume, detach, restart and end. Display the shared TerminalView using
the existing terminal grid. The only local selection is the visible pane,
keyed by device and conversation. Keep the conversation draft on pane changes.
Show the exact working directory and failure from Shared Rust. Display and
copy only the host attach commands returned by the service. Use its advertised
native sources for explicitly requested native views.

## Alternatives

Opening the general ConPTY terminal would lose the conversation binding and
its working directory. Starting a local shell for a remote conversation would
execute input on the wrong device. A private native Windows multiplexer
requires a separate host lifecycle design and device acceptance.

## Consequences

The existing host resolves the session and checks authorization. Returning to
the conversation detaches the client view and preserves the shell on compatible
hosts. End and Restart are distinct explicit actions. Windows does not launch
WSL or install software as a side effect of opening the pane.

## Verification

Exercise available, starting, attached, ended and refused projections using real
WinForms controls. Check the exact lifecycle and native-attach intents, grid
input and the return to the correct conversation. Recheck the Rust target and
late-result tests. Report host implementation and access restrictions
separately from client verification.

On Windows, the 192 DPI controls test exercised the lifecycle and native view
intents, terminal paste and stale menu invalidation. A separate full GUI
opened a synthetic session through an isolated daemon's named pipe, displayed
the exact working directory and the Windows host's tmux refusal, and returned
to its unsent draft. The existing reducer lifecycle test now supplies the
repository's synthetic signer so the live Windows identity stays available
to its owner. A real managed shell on a compatible remote host still needs
an authorized device test.
