# ADR 0255. Attach a Windows desktop client to the conversation shell

- Date: 2026-11-21
- Status: Accepted
- Related: architecture §3.6.3, ADRs 0033 and 0247

This extends ADR 0033's Windows frame decision for the desktop command.

## Context

Windows can start and reconnect a managed conversation shell. Its full frames
omit the desktop command that the existing conversation menu can copy. A user
cannot open a second view of that same shell through the Windows menu.

## Decision

Have the Windows host supply a PowerShell command using its own MSYS2 runtime.
Keep the exact socket, target, server-side generation comparison and ignore-size
attachment shared with the existing desktop client. Quote arguments for the
host shell and scope terminal environment changes to the child process. Emit a
command only for a full frame from the selected managed terminal.
The user runs the copied command on the host. Keep the iTerm2 field empty on
Windows. On the private Windows tmux server, advertise the static capabilities
of its ConPTY clients, including bracketed paste, focus and title. Device-attribute probes were observed to insert partial
terminal replies into an unfinished shell command. Preserve existing server
overrides when installing the ConPTY entry. The existing UI copies the host string on request.

## Alternatives

Launching an independent terminal would discard the running shell's state.
Building a command from projected fields on the viewing device would split
generation checks and OS path selection across clients.

## Consequences

An attached second client can read and write the same private shell. It cannot
take over sizing. Closing the client leaves the managed shell running.

## Verification

Exercise a real PowerShell client in a separate ConPTY and the packaged tmux
runtime on an isolated socket. Confirm shared output, ignored client size,
detachment and rejection of an old generation. Include shell-sensitive path
characters. Check the packaged daemon's full frame independently of the live
node and report production cutover separately.

Seven independent Windows PowerShell 5.1/ConPTY runs passed using the
portable runtime. Each preserved an unfinished command across attachment,
checked output and input in both directions, observed ignore-size and bracketed
paste, detached, restarted the fixture and rejected the stale command. The
Windows terminal library passed its 19 default tests. Full-frame RPC,
retained shell state and stale-handle checks passed across separate software
node processes in both short and deep Unicode directories. The existing
Windows terminal UI suite passed at 192 DPI. The running TPM-backed daemon
and a physical Unix client were outside these isolated runs.

A separate interactive Windows PowerShell terminal also ran the command
returned by the full frame of an isolated software node. The external terminal
displayed the result of a command started before attachment. The original
terminal then displayed the result entered from the external one. Restarting
the synthetic session closed the external client. Running the old command
again left the replacement session with zero external clients.
