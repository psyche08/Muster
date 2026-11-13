# ADR 0200: Make requester authentication optional on macOS for Locked Use

- Date: 2026-11-13
- Status: Accepted (source and local tests)
- Related: [ADR 0120](0120-locked-control-without-a-per-operation-signature.md), [ADR 0007](0007-desktop-manual-locked-use.md)

## Context

Sending successive messages to a locked Mac opened a native authentication prompt on the requesting Mac for each new lease. The target already authorizes the requester and operation under ADR 0120.

## Decision

Default the requesting Mac's extra identity check to off. Add a local Settings switch to require it again. Shared Rust owns this preference and the continuation decision. Store opt-in outside the view cache, alongside the app's persistent configuration. Require authentication if the preference cannot be read.

Only a validated Locked Use execute continuation in the foreground can proceed on scope alone. Keep challenge binding, busy and cancellation checks, target permissions, the privacy cover and relock. Network administration continues to use its own Presence proof.

## Alternatives

Removing the prompt entirely would prevent owners from requiring a local check. Changing the target policy would grant access beyond the request.

## Consequences

A person using an unlocked, authorized requesting Mac can initiate a locked operation within its existing grants. Each requesting Mac can independently require authentication, including for manual Locked Use. An older target requiring a signature can refuse an unsigned operation. The requester can enable the switch for that target.

## Verification

The app-core unit suite passed on macOS (180 passed, one ignored). It exercised default dispatch, opt-in authentication and cancellation, preference reload, unreadable storage, and an administrative request with the switch off. The macOS MusterUI target built with regenerated Swift bindings. Physical two-Mac acceptance, signing, installation and distribution have not been performed for this change.
