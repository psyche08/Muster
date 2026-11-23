# 0294. Confirm replacement of the current desktop viewer

- Status: Accepted
- Related: [Desktop service authorization](0287-desktop-service-authorization.md), [idle host](0293-desktop-host-idle-and-viewer-window.md)

## Context

A host permits one desktop stream at a time. A second authorized device receives a generic refusal and cannot tell who is viewing or request a handoff.

## Decision

Admit a prospective viewer under the normal per-device desktop policy before disclosing the current viewer. Report the current viewer's roster identity over the authenticated session. The later viewer can cancel or explicitly request replacement on that same session. Compare the observed occupant generation under the host lock. Revoke the old stream before granting the new one and serialize native engine access. Apply the incoming viewer's own permission level.

Shared Rust binds the local confirmation to one pending request and host. The Windows viewer shows the controller name and a choice. Closing the prompt leaves the existing viewer connected.

## Alternatives

Automatic replacement interrupts a viewer without user intent. A separate disconnect RPC based only on a device name could interrupt a newer session.

## Consequences

Only one viewer reaches the media engine. Cancelling, losing the requesting link or presenting an obsolete confirmation leaves the current owner in place. Older viewers cannot confirm a takeover.

## Verification

Exercise cancellation, stale and competing confirmations, unauthorized requests, normal view-only admission and release of the old media lease. Check prompt presentation independently from multi-device acceptance.
