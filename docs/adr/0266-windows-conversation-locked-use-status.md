# ADR 0266. Show pending Locked Use in Windows conversations

- Date: 2026-11-21
- Status: Accepted
- Related: [ADR 0007](0007-desktop-manual-locked-use.md), [ADR 0200](0200-optional-requester-locked-use-auth.md), architecture §3.7

## Context

The Mac conversation shows the shared status while a protected operation
waits for local confirmation or for the Mac to relock. Windows rendered
neither the status nor the offered cancellation. The shared user-cancel
handler also drained unrelated reads when a delayed click arrived after
the pending operation had finished.

## Decision

Show the status projected by Shared Rust beside the conversation composer.
Offer Cancel when the core advertises it. Dispatch the existing user-cancel
intent and leave the panel's state to the next projection. In the shared
reducer, handle an explicit user cancellation only while a Locked Use
operation is pending. Keep lifecycle and navigation cancellation independent.

## Alternatives

Inferring a phase from a Windows prompt cannot report a target Mac's relock.
A transient toast scrolls away and offers no cancellation.

## Consequences

The user can withdraw the pending operation through the same reducer as on
Mac. Windows still owns its native confirmation prompt. Closing the panel
or receiving a click supplies no proof of a remote outcome.

## Verification

Test both pending phases, revocation of the cancel affordance and removal
of the status in a real Windows control. In the shared reducer exercise an
active cancellation and a late click after completion with a newer read in
flight. Verify installed GUI and physical confirmation separately.
