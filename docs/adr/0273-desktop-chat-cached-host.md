# ADR 0273. Verify the host before enabling a restored desktop chat directory

- Date: 2026-11-21
- Status: Accepted
- Related: [ADR 0246](0246-desktop-chat-history-availability.md), [ADR 0249](0249-windows-desktop-chat-sidebar.md), architecture §3.7

## Context

The desktop chat cache includes the host that last returned the directory. Restoring the whole view exposes its device ID while the current host probe is still pending. A cached row or New chat can then act on the previously selected host before Rust establishes that it is usable. This occurs on provider changes as well as after restarting the client.

## Decision

Restore cached catalog rows for display. Use the previous device ID only as a preference for probing reachable hosts. Publish the active ID and name after a successful live observation. Refreshes of an already verified, reachable host can keep showing it while updating. If the selected host is no longer reachable, clear the active route immediately.

## Alternatives

Hiding the cached list removes a useful directory during startup. Gating every refresh would interrupt browsing on an already verified host. A Windows-only button check would leave the shared intent path using the old route.

## Consequences

Previously saved catalogs remain readable. An unavailable host leaves the rows visible with the probe's error. A fresh local or paired host can enable the directory without changing the saved conversations.

## Verification

Hold the host probe while restoring a saved directory. Observe the cached rows, absent active host, and no premature open. Verify both the failed probe and successful recovery. Check a real Windows form after restarting against an isolated named-pipe fixture.

The regression test failed on the previous implementation when it published the cached host before the probe returned. All ten Shared Rust desktop chat integration tests passed after the change. Two Windows GUI processes reused one client data directory against successive synthetic hosts. The second form displayed the cached rows with opening disabled during the probe. After the replacement host returned the rows in reverse order, the form opened the selected stable ID and displayed its expected history. No provider message was sent.
