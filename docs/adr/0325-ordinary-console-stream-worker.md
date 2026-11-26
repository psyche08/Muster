# ADR 0325. Lease the signed-in Windows user's physical desktop

- Date: 2026-11-26
- Status: Proposed, installed-device acceptance pending
- Related: [Windows device owner](0318-windows-background-device-owner.md), [physical console](0306-physical-lock-screen-remote.md)

## Context

A paired daemon can remain active in a disconnected RDP or background logon. Its own session cannot identify or capture the enrolled user's interactive display.

## Decision

The account daemon retains pairing, grant, occupancy and transport ownership. A non-elevated worker started in the physical session accepts a bounded media request through an account- and session-specific local pipe. Each endpoint authenticates the installed peer process. The worker checks the unlocked ordinary desktop, obtains its dimensions and starts the packaged engine for the lease. The broker pins the actual engine process for socket ownership checks. Pipe closure, process exit, revocation and loss of the ordinary desktop terminate the session. Only a paired and authorized viewer receives an availability reason.

## Alternatives

Launching in the daemon's logon session cannot follow a distinct physical console. A privileged capture process would expand access to secure desktops.

## Consequences

The user must be signed in at the physical console and the ordinary desktop must be active. Remote login, UAC and other secure desktop surfaces remain unavailable. Worker registration and protected installation belong to the service integration.

## Verification

Check the Windows build, local IPC trust, input and frame guards, process lifetime and unchanged Apple host behavior. Installed acceptance requires a daemon in another session, a signed-in ordinary desktop, and denial on lock, account switch, missing worker or sign-out. Record cross-compilation separately from execution on Windows.

The Windows package applies a pinned input-backend patch to the build copy of libvirtualhid and verifies its digest. It uses ordinary user-mode input, checks the input and thread desktop at dispatch and keeps capture threads on their original desktop. The worker checks the WTS console account against its token SID, reads primary display pixels and places the actual child in a kill-on-close job before sending its bootstrap. A confirmed lock or physical-session change ends the lease.

The Shared Rust desktop-session suite ran on macOS. The Windows daemon, worker and Windows test executable cross-linked for x64. The patched input backend cross-compiled and the bootstrap test ran natively on the build Mac. Full Windows media compilation and installed-device acceptance belong to integration.

The Windows build normalizes the reviewed dependency patch and its input to LF before applying it and verifying the resulting source digest. This supports Git checkouts using automatic CRLF conversion.
