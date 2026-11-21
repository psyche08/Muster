# ADR 0277: Bind remote desktop sharing to the selected member and local owner

- Date: 2026-11-21
- Status: Accepted
- Related: [ADR 0242](0242-muster-remote-desktop-engines.md), architecture §3.4 and §3.6

## Context

The existing remote desktop test uses an arbitrary handler predicate for admission. A paired device and a working transport alone do not express the owner's choice of viewer, input mode or the lifetime of screen sharing.

## Decision

Shared Rust holds one host offer, bound to the selected active member and a host-selected view or mouse mode. The host owns an opaque handle. Replacing or dropping an offer invalidates its sessions even when the next offer names the same member. The frame and mouse adapters check the live grant before using their platform capability. The viewer checks the live connection and membership as it receives data. Keep host lock and explicit stop independent of Agent Locked Use.

## Alternatives

A network-wide desktop permission would change signed roster decoding for older members. A viewer-provided input flag would let a client choose what it can do to the host.

## Consequences

Host restarts require a new offer. An outstanding handle can stop only its own offer. The engine host must still stop on lock or disconnect and enforce the mouse gate at consumption.

## Verification

Exercise view-only admission, mouse permission, refusal of other members, replacement, revocation and drop. Check the Mac host and Windows viewer separately for real capture and input. Passing transport tests does not establish device acceptance.

On Windows, the signed-peer transport fixture passed with view-only and mouse grants. A second test rejected an unpaired viewer, replaced a live grant on the same connection, dropped the stale handle, then revoked the new viewer while awaiting media. macOS compiled the vendored Moonlight C core and passed its loopback check. No product host registered a capture engine in these tests.
