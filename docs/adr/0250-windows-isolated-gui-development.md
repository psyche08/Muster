# ADR 0250. Run the Windows development GUI with its own local daemon

- Date: 2026-11-20
- Status: Accepted
- Related: [ADR 0236](0236-unprivileged-windows-development.md), [ADR 0239](0239-windows-single-maximized-desktop.md), [ADR 0240](0240-windows-portable-distribution.md)

## Context

A portable update keeps an already answering daemon and its live tunnel.
The client development command connects to that daemon. The standalone
development daemon has a fresh temporary store, a software identity and no
driver, but developers could only select its endpoint in the CLI. A change
spanning the GUI, FFI and daemon required a custom launcher.

## Decision

Add an Isolated mode to the standard-user development script. Build the GUI,
FFI, CLI and existing local-dev example from the same checkout and Cargo
profile. Start the fixture directly, enable its GUI argument only in the isolated development
build, read its fresh endpoint and connect the
GUI through the ordinary unmanaged startup path. Put the client cache and
logs under the fixture directory. Disable device authentication and presence
callbacks in this mode. Show the endpoint for explicit CLI commands. Closing
the GUI closes the fixture input so its daemon shuts down normally.

Use the same per-user, per-session desktop mutex. An isolated launch that
finds an existing GUI reports this condition without activating a live window
as if it were the development target. Production and connect-installed keep
their normal activation behavior.

## Alternatives

Replacing the installed daemon for every test would interrupt the live
network. Opening a second production daemon in a different directory still
uses the same hardware identity. A separate main window would violate the
single-instance desktop contract.

## Consequences

Developers can exercise changes across all three layers without UAC. Temporarily
close the existing GUI to use the isolated window. The installed daemon and
driver can continue serving the network. The fixture is an example executable
outside the portable release and discards its test state on shutdown.

## Verification

Run from a standard Windows token against the existing window, then with the
GUI closed. Inspect the isolated GUI, use the matching CLI to create and read
a synthetic network and check identity, vnet and cleanup. Reopen the installed
GUI. Verify the live processes and tunnel independently.

On Windows the development command ran with an ordinary user token. With the
installed GUI open the new launch reported the occupied window and discarded
its fixture. After closing just the GUI, the isolated window created a
synthetic network. The matching CLI read the network over the printed pipe,
reported a distinct identity and found no registered presence candidate.
Closing the window removed the temporary state. The installed GUI reopened
and both live networking processes retained their original process IDs.
