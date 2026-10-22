# AgentHalo Pairing Release Gates

Pairing evidence is cumulative. A passing source or hermetic test does not replace artifact,
installed-runtime, deployed-Relay, or physical-device evidence. Every release report MUST publish
each gate independently as `PASS`, `FAIL`, `NOT_RUN`, or `BLOCKED`.

| Gate | Required evidence | Automation boundary |
| --- | --- | --- |
| G0 Shared contracts | Rust/Go/Swift/ArkTS unit, property, negative and persistence tests | Software fixtures are allowed and must be named as such |
| G1 hermetic loopback | Empty Owner ledger, production registration/publication/readback, Shared contracts and Relay handler/store with the virtual phone | Software P-256, mock OS auth/storage and temporary Relay; no test-only Owner bootstrap; direct Rust calls do not prove every platform ABI binding |
| G2 cross-adapter contract | macOS-issued subject/target inputs pass unchanged through the HarmonyOS production ABI binding | No fixture may reuse one device ID for the phone subject and an owner target; fake ports may not ignore inputs |
| G3 artifacts | App, HAP and Relay hashes identify the approved source revision; platform signature/notarization equivalents pass | Build or install success alone is insufficient |
| G4 installed platform runtime | Physical Secure Enclave/HUKS API path, one real local-auth prompt, exact protected-store readback, independent monotonic/witness evidence and restart recovery | Simulator, software key, boolean auth, in-memory persistence and encrypted bytes without an anti-rollback witness are insufficient |
| G5 deployed Relay | Running binary hash, unit/config, consumer-signed ACK verification at both the PairingTransport claim and authenticated GrantV2 broker, subject-latest revision/supersession enforcement and redacted durable-state summary match the release; independent Relay rollback protection is explicitly NOT PROVIDED | Process health, a public route probe, a bearer-only ACK or verifying an old still-valid owner signature is insufficient; local state/head checks do not detect a complete valid rollback |
| G6 physical cross-device closure | One correlated run reaches both Shared terminal journals, the current HUKS subject signs the exact terminal ACK, Relay ACK+directory authorization succeeds, and signed directory/service readback plus post-restart recovery completes | Mandatory release-promotion gate; no earlier gate substitutes for it |

The mandatory repository-local hermetic gate is:

```sh
cd src
make pairing-security-v2-hermetic-gate
```

Production enablement is additionally gated by `musterd -config <approved-config> -validate-pairing`.
This is read-only and does not open stores. As explicitly requested by the operator on 2026-09-07,
an independent external rollback witness is no longer a prerequisite for Relay deployment.
Production validation and startup report that this protection is not provided; they must not claim
otherwise. Existing signed-ACK, binding, subject-latest and local durable-state checks remain
mandatory. See the [accepted limitation and future witness plan](agenthalo-pairing-external-witness.md).
The hermetic-only loopback switch remains test-only and is not used to enable a public deployment.
This Relay policy change does not remove any Shared/platform authority recovery checks in G4.

The legacy-Relay approval canary and complete state-plus-head rollback limitation tests are part of
G0 (`go test ./internal/pairingtransport ./internal/relayd`). They prevent a source test report from
silently implying compatibility with the known old deployed Grant V1 / unsigned-ACK broker.

Before QR issuance, the ABI 1.49 Shared registration plan must verify registration and an exact
signed directory readback for the same authority and Relay origin. No legacy publisher capability,
local record flag, mock bootstrap endpoint, or HTTP 200 may substitute for that prerequisite.
Registration/publication reply-loss, delayed PUT, expiry and process-restart cases belong in G0/G1.
See [Owner registration and recovery](agenthalo-owner-registration.md), including the public
self-registration admission/capacity limitation and the requirement to preserve existing journals.

`make all` also executes the virtual-phone Relay test so the principal hermetic flow cannot be
silently skipped by the default target. CI logs and release summaries MUST retain the qualifier
`hermetic virtual-phone loopback`; they MUST NOT shorten it to `pairing E2E`.

## Physical run evidence

One G6 run ID binds all of the following without recording QR payloads, SAS values, bearer
capabilities, SPKIs, signatures, protected-state bytes, or opaque handles:

- source revision plus App, HAP and running Relay SHA-256 values;
- device/OS identifiers suitable for test inventory, not authorization;
- stable Shared stage names and journal generations from both Apps;
- Relay invitation phase, `delivery_acknowledged`, and `directory_authorized`;
- the verified owner-directory revision and expected `service_id=agenthalo` readback;
- the same terminal projections after both Apps restart without state clearing.

The run fails if either platform reconstructs trust from JSON, UI state, an HTTP success, a saved
boolean, or a Relay routing projection. Pairing is complete only after exact Shared-produced bytes
are persisted, read back, verified and projected as terminal on both sides.

The owner-completion checkpoint's expiry, predecessor binding and exact-idempotent Acked restore do
not constitute a hardware monotonic head. They prevent a replayed checkpoint from expanding a
Grant, but an old valid sealed slot can still revive stale UI/history or block a new invitation.
Authority/process restore therefore remains unavailable for production issuance unless the
platform supplies an independent live monotonic counter or witness accepted by Shared.

Until the Shared authority-issuance transaction, verified target-evidence handle and independent
rollback witness are all exposed by a production platform binding, that App MUST report pairing
issuance as unavailable before creating an invitation or performing local authentication. It must
not fall back to platform-generated Grant IDs, revisions, authority metadata, Activation fields or
delivery identifiers.

The older individually signed acceptance, Grant, Activation and delivery constructors are
compatibility/test surfaces, not a production issuance composition API. A production platform MUST
not call them independently to assemble a pairing. Before issuance is enabled, the platform ABI
must accept only the opaque `AuthorityPrepared` handoff (or equivalently make those legacy
constructors unavailable to product callers), so every published artifact is covered by the same
authority-ledger reservation and exact-readback transaction.

Production enablement also requires owner-completion restore to bind the current verified authority
generation/head, not only the unchanged owner key. Relay authorization must remember the latest
accepted Grant revision/supersession chain per subject and reject an older otherwise-valid signed
Grant; otherwise replaying a pre-successor checkpoint could resurrect stale policy.

Creating an owner publication plan is not itself a latest-head reservation. Before successor
issuance is enabled, Shared must durably reserve the active publication in the authority ledger (or
provide an equivalent atomic transaction), require exact readback, and block a predecessor plan
from publishing after a successor commits. A one-time latest-head check before returning an opaque
request does not close the later network-execution race.

The Grant broker must accept only the exact consumer-signed terminal ACK. Its signature binds the
invitation, selected request, recipient/read capability, delivery, subject SPKI, exact signed Grant,
completion ID, durable Committed generation and idempotency key; both PairingTransport and the
broker independently verify it. Bearer possession alone is never sufficient to authorize a Grant.

`delivery_acknowledged` in a Relay poll and the Relay's response receipt are still broker state, not
proof that the consumer subsequently persisted its Shared Acked journal, nor an external rollback
witness. A production terminal result therefore also requires the phone's exact Shared Acked
readback and an owner-signed directory/service readback. Relay routing metadata alone may update
transport diagnostics but must not establish device readiness.

## Report template

```text
G0 Shared contracts: <PASS|FAIL|NOT_RUN|BLOCKED> — <commands/evidence>
G1 hermetic virtual-phone loopback: <status> — software P-256, mock auth/storage, temp Relay
G2 cross-adapter contract: <status> — phone subject and owner target IDs are distinct
G3 artifacts: <status> — App/HAP/Relay revisions, hashes and signature results
G4 installed platform runtime: <status> — physical key/auth/readback/restart evidence
G5 deployed Relay: <status> — running hash/config/broker/state evidence
Independent Relay rollback protection: NOT PROVIDED — operator-accepted limitation
G6 physical macOS-HarmonyOS closure: <status> — correlated dual-journal/readback run ID
```
