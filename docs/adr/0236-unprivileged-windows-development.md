# ADR 0236. Run Windows development builds as the signed-in user

- Date: 2026-11-20
- Status: In development
- Related: ADR 0233, ADR 0235 and the shared startup ABI in ADR 0227

## Context

Replacing the machine-wide MSI closes the installed desktop and daemon. Opening
Wintun after that replacement requires a new elevation prompt. Client changes
can be exercised against the existing daemon through its owner-checked pipe.
A second production daemon would contend for the same TPM identity lock even
if given another state directory.

## Decision

Give the development client an explicit attach-only entry point. Use the
unmanaged startup ABI and a separate client cache. Build its shell, FFI and CLI
in a user-writable directory. For daemon development, provide a Cargo example
using the existing test signer injection, a fresh temporary store, a loopback
bind and no driver. Keep this example out of the MSI. The Windows production
daemon continues to open its TPM device identity and enforce exclusive use.

## Alternatives

Installing the MSI for every change interrupts the live node. A privileged
service that launches workspace builds would give writable binaries system
privileges. The existing local IPC interface already supplies the client with
the installed daemon's network services.

## Consequences

The installed node must be running for live integration. The isolated example
has its own device identity and starts outside the production network. Driver
and per-machine installer changes still need device acceptance.

## Verification

Run an installed client and daemon, then launch the development client with a
standard user token. Check their separate paths and confirm the installed
daemon and tunnel process IDs survive a development client restart. Exercise
the isolated daemon over its own pipe, compare identities, and verify its vnet
is off. Check a missing daemon produces a retryable client error. Verify the
release MSI separately.

## Windows acceptance on 2026-11-20

Under a non-elevated Windows token, the development client built, attached
to the installed daemon and wrote its own view cache. Closing it and opening
a second build preserved the installed daemon process. The isolated daemon
answered a CLI request on its own socket with a different identity and vnet
off, and exited on stdin closure. With a missing socket the shared startup
check reported that the daemon was unavailable without invoking a service
callback. The installed tunnel was offline during these checks, so continuity
of an active tunnel across a development-client restart remains unverified.
