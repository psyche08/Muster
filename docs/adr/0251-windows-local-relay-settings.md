# ADR 0251. Show and configure the local relay on Windows

- Date: 2026-11-21
- Status: Accepted
- Related: architecture §3.5, ADR 0004 and ADR 0240

## Context

The Apple desktop shows the local relay and offers a configuration action.
Windows Settings shows the node version and network roster. A roster address
can remain assigned while the local tunnel is down.

## Decision

Render the local DeviceInfo endpoint, relay registration state, assigned VIP
and tunnel state from the shared view. Keep missing node state visible.
Offer the existing local SetRelay intent from Settings and onboarding. Seed
the editor with the current endpoint. Cancel, blank and unchanged input should
leave the node alone. Use a WinForms dialog owned by the desktop window so
keyboard navigation, cancellation and modal focus can be checked in isolation.
Disable submission for blank and unchanged input. Rust retains endpoint interpretation and reconfiguration.

## Alternatives

Reading adapter or process lists would give Windows its own definition of
node state. Editing the daemon store directly would skip the shared operation.

## Consequences

Operators can distinguish relay registration from packet transport after a
portable update. Applying a different endpoint can interrupt peer connectivity.

## Verification

Compare installed Settings with a local DeviceInfo read. Exercise a changed
endpoint with a disposable software-identity node. Verify cancellation keeps
the original endpoint. Record tunnel traffic and hardware enrollment separately.

A Windows software-identity fixture exercised the owned modal from Settings.
Cancel retained the original endpoint. Blank and unchanged input disabled
submission. A changed loopback endpoint with surrounding spaces was trimmed
and read back through DeviceInfo. Refresh returned the unjoined node to
onboarding; reopening Settings showed all four values from that node.
