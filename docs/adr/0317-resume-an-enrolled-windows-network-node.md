# ADR 0317. Resume an enrolled Windows network node by full device ID

- Date: 2026-11-26
- Status: Accepted for the network-only executable. Background activation pending.
- Related: [Windows vnet](0232-windows-vnet-node.md), [Existing identity verification](0316-post-login-device-identity.md)

## Context

A session-independent network node must keep the paired hardware identity. The existing verification command exits after signing. The normal node startup can provision a missing key and create a database. The interactive daemon and the standalone network node use the same per-account ownership lock and roster store.

## Decision

Add `muster-vnet --resume-existing-device <full-device-id>`. Acquire the existing ownership lock exclusively, silently open the existing TPM key and match its complete ID. Use that same signer for subsequent handshakes. Open the existing database without the SQLite create flag. Require an active roster record with a virtual address and subnet before contacting the relay. Reuse the standalone node's QUIC and Wintun runtime and roster persistence. Apply the stored local network policy when present. Older network-only stores without a saved policy keep their existing closed-port defaults. The verify-only command remains available while the interactive daemon holds the lock.

## Alternatives

Calling the verify-only command and then launching the default executable opens the key twice and permits provisioning on the second open. Running two nodes for one device races the existing network owner.

## Consequences

An operator can resume an existing network installation after its previous owner exits. The operation uses the enrolled account's existing profile. Network-only mode serves the virtual network. Session-independent scheduling, privilege separation for Wintun and delivery of the physical desktop remain separate integration work.

## Verification

Check a missing database, a held owner lock, an absent or different key, inactive membership and conflicting CLI modes. Build for Windows. Device acceptance requires an exclusive owner and authenticated peer traffic from the resumed node. Repeat after sign-out, then verify a Remote frame and input independently.
