# ADR 0257. Allow MSYS2 tmux to restart its server on Windows

- Date: 2026-11-21
- Status: Accepted
- Related: architecture §3.6.3, ADRs 0033, 0255 and 0256

## Context

After ending the last shell, the private tmux socket can remain on disk.
With the bundled MSYS2 runtime, querying this endpoint took about two seconds.
Creating the next detached session succeeded, but collecting its result took
just over five seconds. The adapter's five-second deadline reported a failure
to the user even though the new shell existed.

## Decision

Allow up to fifteen seconds for a Windows tmux command that creates a shell
in the validated working directory. Keep a bounded wait and issue the command
once. The existing host owns the socket, generation guard and session identity.

## Alternatives

Removing an apparently stale socket based on a failed probe could disconnect
a reachable or newly starting server. Keeping an idle tmux server running
would extend its lifetime past its last session. Automatically retrying a
mutation could duplicate work after an uncertain result.

## Consequences

A successful cold start can report its result after the old deadline. A truly
unresponsive creation can take longer to fail. Inventory queries keep their
shorter deadline.

## Verification

Use the portable runtime on a private socket. Create and end the last managed
shell, start another, reject the old handle and verify the new shell executes
input. Repeat through the Windows conversation interface on an isolated node.

The new regression reproduced the new-session timeout on the preceding
implementation. With the startup deadline applied, one reopen returned a new
generation, the old handle rejected input and the new shell returned the
expected arithmetic result. The 19 default Windows library tests also passed.

On an isolated software node built from the release source, the Windows GUI
opened the synthetic conversation from Agents after its final shell ended.
Selecting Terminal once opened a replacement. Text entered through the
WinForms grid produced the expected computed line in its accessible screen.
An independent RPC run received a full frame, resized the replacement, read
its computed output and queried its live generation. In one separate
diagnostic run, an external tmux list-sessions probe on the private socket
hung while the node still answered DeviceInfo. Its cause is unresolved.
These runs used an isolated development GUI and did not switch the live node.
