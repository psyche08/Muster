# 0303. Retry an isolated Apple UDP receive route error

- Date: 2026-11-24
- Status: Proposed
- Related: [0004](0004-transport-recovery-and-stable-vnet-owner.md), [0301](0301-desktop-gamestream-roles.md)

## Context

During a paired Mac upgrade, active connections to multiple peers came up and the shared QUIC receive driver then exited with Darwin EADDRNOTAVAIL. All peers lost their transport and a rebuild repeated the sequence.

## Decision

On Apple platforms retry one address-unavailable receive error in the current receive poll. Surface a repeated immediate error and all other fatal socket errors to the existing transport supervisor. Keep its independent local receive-health probe.

## Alternatives

Rebuilding immediately discards healthy connections to other peers. An unrestricted retry can spin on a permanently broken socket.

## Consequences

An isolated route error costs one receive attempt. Endpoint failure and recovery remain observable through existing diagnostics.

## Verification

Inject an isolated route error and a fatal error in the shared socket tests. Validate the installed Mac daemon with multiple real peers and the normal Remote workflow.
