# Production pairing rollback-witness boundary

Status: **NOT PROVIDED — independent Relay rollback protection is not a deployment requirement.**
On 2026-09-07 the operator explicitly chose to deploy Relay without this capability. No independent
production witness provider is implemented or deployed. This decision removes only the Relay's
external-witness enablement prerequisite; it does not change Shared/platform authority recovery,
signed terminal ACK verification, exact request/Grant/delivery bindings, subject-latest checks,
local durable-state validation, or the physical pairing acceptance gate.

The local PairingTransport `.head` and owner-directory subject-latest index detect inconsistent
bytes and in-process stale revisions, not restoration of a complete, internally valid old backup.
`TestReleaseContractLocalStateAndHeadAreNotAnExternalRollbackWitness` deliberately reproduces that
limitation; its PASS must never be reported as proof of independent rollback protection. Restoring
the Relay's state and matching head together can restore old routing/authorization history. That
limitation is accepted for this deployment, not fixed or hidden by an always-success witness adapter.

## Immediate enablement policy

`musterd` enables the production pairing HTTP routes after its existing configuration and durable
store checks. Startup and validation explicitly report that independent external rollback protection
is not provided. The read-only, non-mutating `-validate-pairing` gate checks production pairing
configuration; success does not assert witness availability or physical pairing completion.

For isolated tests only, `rendezvous.pairing_transport.hermetic_loopback_only: true` is permitted when
both the actual configured listener and signed public HTTPS origin use numeric loopback IPs. A DNS
name, public listener, public origin, or the flag alone does not enable it. This mode is never a
production release gate. The existing test harness also instantiates handlers only against temporary
stores and explicitly labels its software keys and mock authentication.

Deployment must still identify the exact binary/configuration, preserve existing pairing data,
validate startup and broker compatibility, and separately verify real-device completion. Only the
Muster Relay unit needs replacement/restart; its active tunnels disconnect during restart. This
policy does not authorize unrelated service changes or clearing legacy trust state on validation
failure.

## Future independent protection (not part of this deployment)

The witness must reside outside the Relay host's rollback, snapshot and backup domain. It maintains
a linearizable compare-and-append head for each deployment/store namespace and returns a signed
receipt binding namespace, predecessor generation/hash, candidate generation/hash, state format and
operation nonce. Its signing key and current monotonic storage must not be writable or restorable
with the Relay. Normal reads verify pinned public keys and must not trigger platform authentication.

Any future claim of independent rollback protection must provide all of these:

1. An independently operated durable witness with authenticated, replay-resistant reads and CAS.
   Pin its identity separately from the Relay. Define key rotation, availability and recovery owners.
2. A Shared Rust verifier for the exact signed witness receipt and predecessor chain, exposed via
   opaque ABI types. Do not reuse freshness/authorization booleans or treat a generic HTTP 200 as
   a witness. Existing device-freshness envelopes have a different purpose and are not this protocol.
3. A crash-safe reservation/commit transaction: reserve the candidate head against the exact prior
   head, persist and read back the exact local candidate, then commit/verify the external head before
   exposing the transition. Persistently reserve the candidate so concurrent publishers cannot fork.
   A reservation that reached the witness but not local disk blocks the old state; recovery may only
   finish that exact candidate, never reset the witness or silently accept the predecessor.
4. Separate, bound namespaces for PairingTransport and owner-directory, with the existing signed-ACK
   cross-store ordering retained. Restore must check both live witness heads, not just a matching
   local `.head`. Witness unavailability blocks pairing while unrelated tunnels remain available.
5. Tests for whole-state-plus-head replay, both-store rollback, forked concurrent CAS, lost replies,
   witness outage, key rotation, and a crash at every reservation/local-write/commit/readback boundary.
   Add an independently observed deployed witness receipt/head to every G5/G6 run.

The external service location, administrative trust/key lifecycle, retention and recovery policy are
not inferable from the repository. A future implementation requires an explicit operational choice.
No same-directory file, local second encrypted slot, in-memory mock, configuration assertion or
always-success adapter may be represented as an independent witness.
