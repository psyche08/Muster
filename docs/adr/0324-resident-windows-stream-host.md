# ADR 0324. Keep the Windows stream host running and select the physical console

- Date: 2026-11-26
- Status: Accepted direction, physical-device verification in progress
- Related: [Physical surfaces](0306-physical-lock-screen-remote.md), [device owner](0318-windows-background-device-owner.md), [release installation](0310-windows-elevated-release-updates.md)

## Context

Disconnecting Remote Desktop can leave the enrolled daemon running in a different
session from the physical display. The private media engine inherits its
launcher's session. Moving the full daemon would also move its interactive
application adapters.

## Decision

Use the name `muster-stream-host` for the packaged host. On Windows the
automatically started SCM service tracks the OS physical console and requests
the protected release's fixed, least-privilege interactive worker through Task
Scheduler. The service runs as LocalService. The worker belongs to the enrolled
Windows account. The media engine starts only for an authorized streaming
session. The Rust daemon retains the existing identity, policy, encrypted
transport and interactive application adapters.

Apply the Windows session requirement to the worker that supplies the media.
Keep credentials, UAC and unknown surfaces outside the stream. A console with
no signed-in user has no ordinary interactive worker. Represent this condition
as unavailable until a supported surface-specific backend can supply it.

## Alternatives

Moving the full daemon to the console would interrupt unrelated functions.
Launching the full private media engine at boot would start work before a
viewer has an authorized session. The existing per-session certificate and
grant remain necessary for each stream.

## Consequences

The service can remain running during logoff or an RDP disconnect. SCM and
Task Scheduler manage host and worker lifetimes independently of the GUI.
A running service proves availability of the selector alone.

## Verification

Check the protected installation paths, service account, automatic startup,
worker task principal and privileges, session selection, idle state and
version replacement. Test a daemon outside the worker session and verify
revocation and cleanup. For locked-console acceptance record the actual first
frame and visible pointer response on both viewers and the physical display.
