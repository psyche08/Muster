# Owner registration and pairing readiness

The production Mac must not issue a QR merely because a local Owner and Relay
URL exist. The Relay's signed-ACK directory broker requires that Owner to be
registered. Previously the virtual-phone test created that record through a
test-only bootstrap route, hiding the missing production path.

## Ownership and effects

`OwnerRegistration` in Shared Rust borrows the **existing, exact-durable**
`AuthorityPairingRuntimeV2` identity and local device. It does not create another
authority, perform another approval ceremony, or trust a platform readiness flag.
ABI 1.49 exposes opaque handles and typed effects:

1. Generate the publisher capability and registration request ID; seal, persist,
   and independently read back the exact state before signing or networking.
2. Sign the domain-separated registration proof with the same Owner key; again
   persist and read back the signed request before sending it.
3. Register through the production Relay handler and verify the bound receipt.
4. Read the publisher's current directory. Stage and sign a publication when
   necessary, with the exact local device descriptor from the authority runtime.
5. Persist the signed publication before PUT. Independently GET the publication,
   verify its Owner signature, content, revision and ETag against the saved
   publication, then persist the committed state before releasing readiness.

Mac adapts hardware signing, sealed storage, and bounded HTTP only. Registration
uses the same Muster helper, configured origin, DNS and physical-egress policy
as pairing. The additional allowlist is exactly POST `register`, PUT `publish`,
and GET `published` under `/v2/owner-directory/`; it is not a generic HTTP proxy.
No ordinary-startup LocalAuthentication is added. Explicit device-management
authorization and first-install authorization remain separate OS entry points.
The legacy authority publisher is not started beside the new production path.

The Mac's production local-device loader advertises no product service until a
real Shared listener is ready. Registration must not invent a service merely
to make an empty service list look successful. Pairing completion and remote
service/listener availability remain distinct acceptance gates.

## Registration protocol

POST `/v2/owner-directory/register` carries schema 1, `trust_anchor`,
`directory_endpoint`, `publisher_capability_sha256`, `request_id`, and
`signature_der_hex`. Owner and key IDs must be respectively `owner:<digest>` and
`key:<digest>`, where digest is SHA-256 of the 65-byte P-256 SEC1 public key.

The signing input is `muster-owner-registration-v1\0`, followed by six strings,
each UTF-8 encoded and prefixed by its unsigned 32-bit big-endian byte length:
Owner ID, key ID, SEC1 public-key hex, canonical HTTPS origin, publisher digest,
request ID. The handler compares the signed origin to configured PublicEndpoint,
never a Host or forwarded header. P-256 DER signatures are verified. The Shared
capability is 32 random bytes with an `odp_` prefix and hex encoding; only its
SHA-256 digest reaches registration or the Relay ledger.

First registration returns 201; an identical authorized tuple returns 200 even
after restart or a lost response. ECDSA signature randomness is not a new tuple.
A changed key, capability, request ID, or origin cannot overwrite an existing
Owner. An administratively provisioned old Owner is not silently taken over.
Explicit key rotation remains the only rotation path. The digest and original
signed registration remain bound through the persisted rotation chain.

There is intentionally no freshness expiry on the immutable registration proof:
replay can only recreate the exact same Owner/key/capability tuple, not obtain a
credential or authorize a different identity. This preserves lost-response
recovery. Readback at GET `/v2/owner-directory/published` requires the publisher
bearer; a phone's ordinary read capability cannot access that route.

The public self-registration route proves key possession, not human identity or
account admission. Its 4 KiB request and 4096-Owner capacity bounds limit resource
use but do not prevent a malicious party generating many keys and exhausting
registration capacity. No account/allowlist policy is silently introduced by
this pairing repair. This is an operational admission-risk boundary to consider
before enabling the new server binary on an exposed service.

## Crash recovery and errors

Every preparation/invitation readiness check restores the sealed registration
slot and replays registration plus signed readback. A serialized Ready flag,
HTTP 200, or known origin cannot authorize a QR. Exact sealed CAS prevents two
platform instances from replacing each other's publisher capability.

An ambiguous PUT retains the exact signed envelope. A matching GET commits it
without signing again. If the saved pending envelope has expired, a verified
exact predecessor (or a missing first publication) permits
burning its revision and staging a greater revision. A revision is never reused
for different signed contents. Near-expiry pending envelopes are still retried
exactly. Superseded signed envelopes are retained until their successor commits,
so a delayed older PUT or a lagging Relay clock cannot strand recovery after
the revision is burned. This unresolved history is bounded to 32 entries and
fails closed at its limit. Missing previously committed content, unknown
signed content, wrong ETag, wrong origin, regressed clock and wrong key fail
closed. Expired committed snapshots are verified as historical evidence only;
they cannot release current readiness without renewal and fresh readback.

This does not restore the separately declined independent external rollback
protection requirement. Existing ACK signatures, grant bindings, local durable
consistency and pairing recovery rules remain in force.

Harmony errors preserve the actual completion/ACK stage and ACK HTTP status.
A delivery GET 200 followed by an ACK 409 is reported as an ACK failure, not
`delivery_extract/http=200`. The exact saved ACK remains retryable through the
Shared completion journal. Mac waiting text covers both authenticated ACK and
directory authorization, rather than incorrectly blaming an unresponsive phone.

After registration is repaired, an existing ACK may be retried only via that
saved Shared state. If its Grant or checkpoint has expired, a new pairing may
be required. No state is cleared and no ACK or grant is manually marked done.

## Acceptance boundaries

- Shared tests cover entropy/persistence ordering, exact readbacks, lost replies,
  all restored publication phases, revision burning, forged receipts and content,
  cross-origin/key substitution, expiry and secret-free diagnostics.
- Go tests cover real registration, restart/idempotence/conflicts, proof validation,
  publisher-only readback, persistence faults and the helper's narrow allowlist.
- The virtual-phone TLS test starts with an empty Owner directory and has no
  bootstrap control route. It calls production registration, publication and
  readback, then exercises signed ACK, directory authorization and both terminal
  pairing journals. Its software keys/platform service fixture are not hardware
  or real-listener evidence.
- Swift tests must execute the ABI 1.49 plan and actual production coordinator
  adapter path; the fixture may implement only OS/storage/HTTP boundaries.
- Harmony tests cover the production binding/HTTP adapter and C++ N-API as well
  as lifecycle unit tests. HAP installation, signed Mac deployment and physical
  Mac–Harmony pairing remain separate gates.

For deployment, both the signed client origin and Relay PublicEndpoint must
canonicalize to the same origin, including port. Deploy the compatible Relay
before relying on the new client registration path. Do not modify other virtual
hosts or clear existing pairing/Owner state as an installation shortcut.
