# ADR 0253. Display and paste network invitations on Windows

- Date: 2026-11-21
- Status: Accepted
- Related: architecture §3.4 (network membership) and §3.7 (platform effects)

## Context

The Windows desktop displayed the raw invite in a general settings panel,
requested a shorter lifetime than the shared default, and left the ScanQr
effect unanswered when the user selected Join without typing. Apple desktops
display a scannable invitation, expiry, a copy action and a paste entry point.

## Decision

Render the exact InviteView text as a local QR code with the pinned ZXing.Net
encoder. Display its reported expiry, allow copying and request a new invite
through the shared intent using the default lifetime. Clear the displayed text
and matrix when leaving the invite or losing the admin capability. Collect
pasted invite text in an owned dialog for the desktop ScanQr effect. Return
it to Rust for validation and joining. Display join progress and failure from
JoinView, including a way to return and try again.

## Alternatives

A web-based QR renderer would disclose the invite. Implementing the QR standard
in the shell would add unrelated encoding and maintenance work. Deriving
authority or TTL from native settings would duplicate shared decisions.

## Consequences

Windows can present invites to phone cameras and desktop clipboard workflows.
The node owns admission, validity and approval. A displayed countdown is local
presentation and the OS clipboard changes only when the user copies text.

## Verification

Decode the rendered QR pixels back to synthetic invite text, test replacement,
expiry, permission loss and copy. Exercise the paste dialog, cancellation and
invalid input with a disposable node. Verify generation and renewal from the
ordinary Windows settings entry without issuing an invite on the live network.

On Windows at 192 DPI the form test decoded the rendered QR pixels back to
the complete synthetic payload. Copy, renewal, expiry and clearing passed.
Using a loopback relay, the admin fixture generated and renewed a ten-minute
invite through Settings. The second fixture cancelled, retried after a failed
paste and joined using the renewed invite. Approval was issued by local RPC
after matching the guest fingerprint. Both nodes reported the same two-member
network. The installed network and system clipboard were not used for pairing.
