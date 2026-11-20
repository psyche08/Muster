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
