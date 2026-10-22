# Desktop Presence enrollment

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Desktop Presence signatures prove interactive user authentication on the
requesting Mac. The ordinary daemon identity key is not a substitute. The native
shell creates/uses the hardware-backed Presence key; Shared Rust owns candidate
registration, administrator approval, binding verification and request routing.
Private key material never enters the daemon, roster, CLI or enrollment proof.

For Macs paired before this support, open the updated Muster app once. On that
Mac, `muster presence` reports whether its public candidate is registered and
matches the enrolled key. Registration alone grants no remote permissions.

On an enrolled administrator's local daemon:

```sh
muster enroll-presence Mac-B --grant-locked-control
```

This explicitly approves the candidate fetched from the authenticated Mac-B
member and adds only `locked.control`, preserving existing scopes. Omit the
flag when that grant already exists or when enrolling for a different high-risk
operation. Repeating approval for the same current key redelivers the same
binding without rotating its authorization epoch. If approval persisted but
network delivery failed, repeat enrollment; this is metadata delivery, never a
chat-send retry. The initiating desktop must still complete native user
presence authentication when signing a protected operation.

To invalidate an added binding without removing/re-pairing the device:

```sh
muster enroll-presence Mac-B --revoke
```

Revocation does not remove unrelated capabilities. Original pairing-time
Presence keys remain governed by the original record and scopes; this command
refuses to replace or revoke those immutable keys. Rotate a post-pairing key by
opening the app with its new hardware key and explicitly approving it again.

The administrator signs a public binding to network, member, key and an empty
DeviceUpdate epoch. Old clients see that ordinary update as a no-op and continue
learning subsequent grant changes and removals. Updated clients verify both the
accepted roster epoch and the separate signature. The existing removed-admin
rule applies to the anchor. New epochs invalidate old bindings; member removal
invalidates every binding. Normal name, role and scope updates preserve the
epoch, while permissions are always checked against current state.

RPC forwarding attaches the daemon's verified public binding and signed roster
evidence. Targets validate evidence in Rust through the ordinary roster
publication/revocation path and persist the binding. Cached bytes or an
`enrolled` display flag cannot authorize an operation. Old targets ignore the
optional envelope field and still require their original enrolled key; update
both desktop endpoints before accepting protected sending.

See [ADR 0006](adr/0006-desktop-presence-and-console-routing.md) for design and
[Locked Use](LOCKED_USE.md) for execution lifetime and acceptance limits.

The macOS release must carry an Apple Developer ID provisioning profile for its
existing team and bundle ID, plus the matching application identifier and exact
keychain access group. `check-presence-signing.swift` validates this before
packaging, and `package-macos.sh` preserves the expanded entitlements. A native
`keychain error -34018` with no candidate is a signing/configuration failure;
repeating enrollment cannot repair it. Never substitute the daemon identity key.

For a Developer ID Xcode build, set `MUSTER_MAC_PROFILE` to the installed profile
name (currently `Muster Developer ID Presence G2`). This selects the profile on
the macOS app target only; a global `PROVISIONING_PROFILE_SPECIFIER` incorrectly
applies to Swift package resource bundles. Match the profile's certificate to
the actual signing identity, including the Developer ID G2 certificate type.
