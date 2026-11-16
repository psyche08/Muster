# ADR 0214: Rename an enrolled device

- Date: 2026-11-16
- Status: Accepted
- Related: architecture §3.4 and ADR 0006

## Context

Enrollment records a display name. Administrators need to correct that name after pairing.

## Decision

Add a member rename operation to the device detail screen on Apple clients and to the CLI. Shared Rust authorizes an administrator, addresses the active member by its stable ID, and publishes a signed roster update containing only the trimmed display name. Reject empty, overlong and control-character names. Use the signed member name for enrolled device information. Remote administration uses the existing Presence policy.

## Alternatives

A client-local alias would give peers different names. Re-enrollment would disrupt existing membership.

## Consequences

Existing peers can fold the signed name update. The initiating daemon needs the new RPC. Virtual address, role, grants and pairing identity remain independent of the display name.

## Verification

Exercise the RPC against paired daemons, including invalid input, member authorization, synchronization and persistence. Check the Apple bindings and build. Verify the designated physical member and its virtual address separately.

The paired-daemon RPC regression passed on macOS. It verifies persistence on both peers, unchanged membership fields, remote and local member rejection, invalid names and repeating the same rename.
