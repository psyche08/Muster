# ADR 0267. Preserve the cause and host of catalog failures

- Date: 2026-11-21
- Status: Accepted
- Related: [ADR 0264](0264-windows-agent-directory.md), architecture §3.7

## Context

A read-only watch of a connected peer returned an explicit refusal under
the target's local policy. The desktop prepended an availability failure to
the authorization error. Other peers kept streaming catalogs. The Windows
sidebar also clipped the reason to one line and combined separate notices.

## Decision

Pass the typed failure from the watch to the shared reducer. Format both
catalog and search failures with the reporting member's name and the
underlying reason. For an authorization error, display the permission and
the target's explanation once, including for older peers whose wire message
already contains the error category. Use the device ID if no name is known.
Format presentation text only after the RPC has classified the error.

On Windows, put each notice on its own line in a selectable, wrapping
read-only field. Limit its height and allow scrolling so sessions remain
reachable when several peers report failures. Show search and catalog
loading according to their respective projections.

## Alternatives

A tooltip requires a pointer and disappears while copying or comparing
notices. Changing permissions or restarting a node would modify a working
network while obscuring the actual reason for refusal.

## Consequences

A connected peer may deny access to its catalog while other catalogs
continue to update. Each host's next successful result clears its own
failure through the existing reducer.

## Verification

Test permission refusal and transport failure on separate members, recovery,
and an older wire message. Check multiple notices, selection and wrapping
at desktop DPI. On the installed client, compare the displayed reason with
a read-only watch of the connected peer and confirm that other peers still
supply snapshots.
