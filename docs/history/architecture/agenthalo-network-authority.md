# Network authorization and member services

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Status: 2026-09-21. Architectural decoupling is **partially implemented; the production path remains incomplete**. Evidence from the original investigation is retained below; this section defines the current increment.

## 2026-09-21 user confirmation: the creator only authorizes device enrollment

After enrollment, members access one another as peers. Any device going offline (including the creator) does not affect access among other online devices through Muster. When online, the creator is an ordinary service target too. Offline status is not revocation and does not change other members' permissions. The creator is not a required online node for ongoing authorization, directory, forwarding, or credential maintenance. Pending enrollment approvals may wait; existing-member access may not.

The former “creator centrally manages all permissions” proposal is no longer confirmed product design. Post-enrollment permission-change/revocation governance will be defined separately. The network layer still uniformly enforces service scopes; removing centralized dependencies does not justify weaker signature, expiry, or revocation checks.

**Implementation gap:** `NETWORK_DISTRIBUTION_LIFETIME` is currently 90 days, `distribution_step` requires Master renewal, and access still checks distribution expiry. The implementation therefore establishes independence from an online Master only within valid-record windows, not indefinite independent operation. Member credential maintenance, cold-start discovery, and listener lifecycle must remove creator-online dependencies; the specific protocol needs separate design and validation. This round corrects design only, without changing these runtime mechanisms.

Master renewal, global permission governance, and field results in older sections describe historical/current implementation and do not override this section's latest product principles. All platform pages follow the [Devices-page design](agenthalo-device-page.md).


## 2026-09-21 source progress and Devices-page constraints

Reviewed baseline `1ba39750` (not deployment evidence):

- `00a9793d` adds a `member-enrollment` broker slot written by App/read by backend. Listener bootstrap remains unwired, so joiner-only devices cannot be claimed readable. **No new member-material producer is needed**: production `publish_network_certificate` already fills that role; see the two 2026-09-21 corrections below. `e6ff2703` reverted two commits based on the mistaken premise.
- v3 distributions in `58c10fff` carry scopes **per requesting member**, not the per-member-pair policy targeted below. Legacy v2 signed bytes and three AgentHalo permission semantics remain intact.
- `1ba39750` adds scoped drafts, device-network C ABI, and Swift bridging; automatic `distribution_draft` still drafts v2. Low-level interfaces neither prove complete platform UI, signed persistence, and propagation nor grant permanent creator management power.
- `POST /terminal` now registers `TerminalConnect`, so “no route entry” below refers to the older baseline. However, the network branch of `resolve_verified_mst2_authorization` still maps only read/write/approval; full remote-terminal authorization/execution needs wiring.

The new [cross-platform Devices-page candidate](agenthalo-device-page.md) accordingly separates membership, service readiness, permission coverage, and actual access. Current granularity does not support per-target permission matrices; missing Shared capabilities must not yield fake UI actions. New design and old software tests establish neither signing, installation, nor real multi-device acceptance.

Parallel commit `1ba39750` already includes those scoped interfaces. This round updated design only, without validating their runtime chain. Distinguish the low-level distribution signer from the product enrollment role; existing Master implementation constraints must not become new central-dependency product requirements.

## In one sentence

`owner authority` currently combines three responsibilities that should be separate: issue grants, **decide whether a device can be read**, and **carry the only fine-grained permissions**. The latter two belong in muster network.

## Evidence (all observed locally, not inferred)

### Only owners run product listeners

| Device | owner runtime/registration | consumer enrollment | 8765 |
| --- | --- | --- | --- |
| mac-c | Present | Absent | **up** |
| mac-a | Present | Absent | **up** |
| mac-b | Absent | Present (joined mac-a) | **down** |

mac-b backend remains blocked at:

```
agenthalo-backend remote startup blocked: owner broker: existing hardware identity file is unavailable
```

It lacks `SecurityKeys/e93653de….sepblob` (SHA256 of `dev.linsheng.agenthalo.owner-authority.signing.v1`).

### Only the owner path creates that key

Three SEP key labels in production:

| label | Creator |
| --- | --- |
| `consumer-signing.v1` | `AgentHaloConsumerEnrollment` |
| `local-mutation-signing.v1` | `AgentHaloLocalMutationController` |
| `owner-authority.signing.v1` | **Only** `AgentHaloOwnerAuthoritySecurity.swift:190/209` |

All callers reaching the last are owner-side (`prepareOwnerAuthority`, `liveInputs`, `OwnerDirectorySyncPlatform.publish`, `AuthorityOrchestratorPlatform.sign`, `desktopPairingResponse`). `AgentHaloExistingManagementKeys.owner()` only calls `loadExistingSigningKey`, never creates one.

### Consequently, joiners never start

```
Join another network → consumer enrollment exists
                     → single-network guard rejects owner preparation (AgentHaloSingleNetworkError)
                     → owner-authority key is never created
                     → load_existing_state cannot open owner broker
                     → product listener never starts → unreadable
```

mac-a and mac-c are readable only because **each was historically an owner**. mac-b is the first clean joiner-only device, so it hits this first.

## Key finding: the right mechanism already exists

**Members do not need new trust assembly.**

- `DeviceNetwork::authorize_remote(subject, target, now)` — “Authorizes the fixed AgentHalo policy for one exact directed member pair. Both endpoints must have accepted the same Master distribution locally; **no online Master** or display/roster JSON participates in this decision.”
- `network_certificate_v2.rs` — “A member's public TLS certificate bound to an exact Master distribution. The Master policy authorizes the pair; this proof binds certificate bytes and lifetime to the **member hardware key**. **It never creates an Owner Grant.**”
- `network_remote_listener_v2.rs` is even more explicit: “The existing local runtime remains a **hardware/lifecycle gate, not an alternate remote Grant authority**.”

This path is connected to backend, not dead code:

| Location | Function |
| --- | --- |
| `remote_tls_peer_resolver_v2.rs:246` | `resolve_network_remote_authorization_v2` |
| `remote_tls_generation_v2.rs:195/225/291` | `capture_network_certificate` |
| `network_management_service_v2.rs:127` | `network_certificate_publication_needed` |

All three `device-network-v2.sealed` files are **byte-identical** (6839 B). Master distribution is replicated throughout the group; mac-b already holds its authorization source.

**The only blocker** is `macos_remote_startup_v2.rs::load_existing_state` unconditionally requiring owner broker + owner runtime before building the listener, although neither authorization nor certificate comes from them.

## Second coupling: only owner grants express permissions

Upper layers **do not** authorize independently: they declare scopes and the sole resolver decides. This structure is correct:

```
agent | terminal     ← Each remote route declares a MusterScope
---------------------
muster network       ← DeviceNetwork::authorize_remote decides member pairs
```

But scope sources diverge:

```rust
RemotePrincipalV2::Owner(p) => p.has_scope(scope),          // Individual, fine-grained
RemotePrincipalV2::Network(_) => self.is_current()          // Three hardcoded scopes
    && matches!(scope, AgentHaloSessionRead
        | AgentHaloSessionWrite | AgentHaloApprovalRespond),
```

`TerminalConnect` / `ComputerUseExecute` / `LockedUseExecute` are outside the network-member set. Both `MusterScope::TerminalConnect` and the mapping `InteractionActionV2::TerminalConnect → MusterScope::TerminalConnect` exist, **but network members can never obtain it**.

→ **muster network cannot express “this device may open a terminal.”** That decision is hardcoded in `matches!`.

### Direct consequence for remote terminals

`remote_route_scope` has no `POST /terminal` entry, falling through to `_ => None` → `remote_forbidden()`:

```rust
// Network management is local-admin-only. Unknown routes and method
// mismatches are also denied so adding a local route cannot
// accidentally expose it remotely.
_ => None,
```

Therefore:

- Good news: no “session-read becomes terminal-write backdoor” vulnerability. Default deny blocks it.
- Cost: all four [remote-terminal](agenthalo-remote-terminal.md) transport slices connect, but **the peer returns 403**. Closed routes are structurally enforced, not merely a convention.

## Decoupling direction

Two tightly coupled changes should happen together.

### 1. Member listeners no longer require owner identity

Bootstrap product listeners by role:

- **Member**: device hardware identity (`<device>.sepblob`, present on every device) + Master distribution (`device-network-v2.sealed`, replicated throughout the group).
- **Network authority**: retain the existing owner-runtime path.

Owner identity returns to its proper responsibility—**issuing grants and determining policy**, not being a prerequisite for readability.

**Verified findings (2026-09-21). First, a correction: this once said “all member-side primitives exist; only wiring remains.”
That was wrong: half a path had been read as a complete one. The checked state follows.**

### Already present (directly usable by members)

**Runtime provenance is already first-class.** `VerifiedRuntimeSourceV2` already has two branches:

```rust
bind_verified_runtime_state_v2(activation, grant, interaction)
    → VerifiedRuntimeSourceV2::ConsumerActivation(activation)
bind_local_owner_runtime_state_v2(runtime, grant, interaction)
    → VerifiedRuntimeSourceV2::OwnerAuthority(identity)
```

The first is live in production (`agenthalo-ffi/src/security_v2_runtime.rs:372`, used by consumer clients).

**Certificate-authorization verification is role-independent.** `verify_tls_certificate_authorization_v2` requires
`VerifiedAuthorityLedgerV2`, produced by `verify_authority_operation_log_v2(operations, paired_anchor:
&AcceptedOwnerAnchorV2)` from a **pairing anchor**—exactly what pairing provides. The owner path reaches
the same function through `runtime.durable_authority()`.

### Correction (2026-09-21): the producer exists and runs in production

**The previous version claimed “members have no TLS server-identity producer.” That was wrong.** Reading only
`network_management_service_v2.rs::prepare_delivery` had mistaken one path for the whole system.

In fact, `RemoteCertificateConsumerV2::publish_network_certificate` is that producer,
and `agenthalo-ffi/src/remote_consumer_v2.rs:403` already calls it.

```rust
let identity = self.network_local_identity(started)?;   // client_store().verified_current()
let plan = NetworkCertificatePublicationV2::begin(&state, &identity, started)?;
let signature = sign(hardware, identity.subject_spki_der(), &bytes)?;
plan.complete(&mut *network..., &signature, finished)
```

Paired members already produce and publish their own network certificates today.

### Correction (2026-09-21): the device-network path never reads owner authorization

This overturns the entire “owner issues it together during pairing” decision below. All member-certificate verification relies on:

```rust
// network_certificate_v2.rs:402
access.revalidate(network, now)?;               // Master distribution
proof.distribution == access.selector()          // Distribution selector
validate_certificate(&proof, expected, now)?;    // DER itself + member SPKI
verify_proof(&proof)?;                           // Member's own hardware signature
```

No `VerifiedAuthorityLedgerV2`, no quorum. `VerifiedTlsServerIdentityV2` inside `NetworkCertificatePublicationV2::begin`
is only a **carrier**: every field read is either recomputed from the certificate or checked against the device network,
and expiry is capped by `.min(access.expires_at_unix_seconds())`.
**Owner quorum signatures are never read on this path.**

`network_certificate_v2_tests.rs:134` itself says:

> `// Exercise the existing Owner certificate installer... Each device deliberately has a different Owner.`

Every device creates its own owner anchor and signs its own operation log, and the network accepts them. **Owner is a local procedure here,
not the trust decision**, so “owner signs member server-certificate authorization” buys nothing.

The corresponding two commits were reverted (`e6ff2703`).

### Still true: backend cannot read member material

`activation.sealed` uses the App's Secure Enclave key, while broker originally had only five slots:
`owner-runtime` / `target-state` / `tls-state` / `owner-registration` / `device-network`.
The `member-enrollment` slot added by `00a9793d` is still needed—**this is the only missing connection**.
### Superseded (2026-09-21): owner issues server-certificate authorization during pairing

**Retained as history.** The two corrections above explain why this was superseded: the network does not read owner authorization,
so this decision addressed a nonexistent problem. The subsections below (signed body, quorum, ceremony rounds, renewal batches)
are superseded too. Their reasoning was sound but their premise wrong, so they remain as a record.

Original text:

The network authority issues a member's TLS server-certificate authorization **together with the grant during pairing**,
not in a later separate step. Reasons:

- Granting “this device may be read” and “this device may read others” is one trust decision. Splitting it leaves half-completed
  devices—exactly where mac-b is stuck today.
- Pairing already has user presence and SAS comparison, making it the right moment for this decision.
- One ceremony yields all required material; members need not contact owner at startup to fetch missing pieces.

Thus item 1 involves **two additions**, not wiring:

1. Add a member broker slot so App writes and backend reads member activation/ledger and server-certificate authorization.
   **Completed** — the `member-enrollment` slot in `00a9793d`.
   **2026-09-23 correction:** Initially this slot, like other remote slots, was sealed with the enclave key for
   `authority-pairing-runtime.v2`. Only devices with paired owner runtimes have that key; pure members (mac-b)
   do not. The first real enrollment failed writing with `protected state sealing failed`. It now uses the
   `device-network.v2` key every enrolled member has. The envelope remains remote-slot AHRB1 with this slot's context,
   mutually unreadable with network-record AHSB1 envelopes (covered by broker selftest). Broker still creates no keys.
2. Extend pairing issuance: owner signs TLS server-certificate authorization for the member hardware public key in the ceremony.

### Superseded: actual shape of step 2 (verified 2026-09-21)

This is not “extend existing issuance,” but **a new subject-parameterized issuance path**. Evidence is an asymmetry:

```rust
// Issuer binds itself to local hardware (local_target_bootstrap_v2.rs:276, local_multi_target_v2.rs:261)
if hardware_key_id != self.subject.facts().seal_key_id { return Err(...) }

// Verifier takes subject as a parameter (tls_identity_v2.rs:545)
verify_tls_certificate_authorization_v2(value, authority, subject_spki_der, ...)
```

“Multi-target” means multiple clients authorized to **connect in** (phone Grants), not multiple server identities:
TLS state restored by `server_tls_store` binds `snapshot.identity.local_device_id`,
and `next_server_certificate_request` obtains certificates only for the local key.

The addition is therefore: owner mints `TlsCertificateAuthorizationV2` for a given SPKI, signed by its authority
quorum. **The verifier needs no change**: members verify with `VerifiedAuthorityLedgerV2` derived from pairing anchors.

#### No new record type is needed

`TlsCertificateAuthorizationV2` **already describes a subject**. `local_device_id` /
`local_device_spki_sha256` are subject fields; the verifier takes and compares `subject_spki_der`:

```rust
if sha256_hex(subject_spki_der) != value.local_device_spki_sha256 { /* Reject */ }
```

“Local” is issuer-perspective naming, not a local-only constraint. Member certificate authorization is the same record,
filled with member device ID and SPKI. **Do not invent another record type**: that creates a second signed body to maintain,
while the existing one is already fixed by verifier, replay rules, and hardware proofs.

#### Signed body (copy existing `canonical_body`; do not change fields or order)

```
TLS_IDENTITY_V2_DOMAIN
schema_version                      u16
action                              1 byte  Issue=0 Rotate=1 Revoke=2
service_id                          string
local_device_id                     string   ← Subject
local_device_spki_sha256            string   ← Subject
key_id                              string
certificate_sha256                  string
predecessor_certificate_sha256      string
authority_set_id                    string
authority_operation_head_sha256     string
certificate_generation              u64
authority_epoch                     u64
issued_at_unix_seconds              u64
expires_at_unix_seconds             u64
```

`key_proof_der` and `predecessor_key_proof_der` are **outside the signed body**: they are independent signatures
on the same body, as described below.

#### Quorum rules

```rust
fn quorum_for(active: usize) -> usize { if active <= 1 { 1 } else { 2 } }
```

For each signature, obtain `active_anchor_for_key` by `(owner_id, key_id)`, verify ECDSA P-256 SHA-256,
deduplicate/count by `owner_id`, and require ≥ `quorum_for(active_owner_ids().len())`. Members do this with
`VerifiedAuthorityLedgerV2` produced from pairing anchors through `verify_authority_operation_log_v2`,
using the same verification function as owners.

#### Hardware possession proofs determine ceremony rounds

`key_proof_der` is **the subject's own key signing the same `canonical_body`**. Therefore the body's
`certificate_sha256` must already be fixed:

```
1. Member generates certificate, calculates certificate_sha256, and fills all signable record fields
2. Member signs this body with its hardware key  → key_proof_der
3. Owner authority quorum signs the same body   → signatures
```

**Member proposes first, owner authorizes second**: two rounds. Owner cannot mint it unilaterally because it lacks
the member private key and cannot produce `key_proof_der`. Pairing must support a round trip, not just one-way delivery.

#### Superseded: renewal—owner presigns batches; relay releases them on schedule

Certificate validity is capped by Master distribution (`.min(access.expires_at_unix_seconds())`). Renewal republishes
a member-self-signed proof without owner or relay involvement. The measured relay-TPM findings in the next two subsections
remain valid, but no longer address this problem.

Original text:

**2026-09-21 user decision (superseding the initial idea below).** `expires_at_unix_seconds` remains mandatory.
Owner **presigns a batch of short-lived authorization generations during pairing**; relay only releases them on schedule.
When creator returns online, immediately re-sign and invalidate remaining batches.

**Relay returns to pure transport**: it can withhold delivery (availability loss), but cannot fabricate, alter, or extend anything.
Rule 6 (untrusted relay, holds no key material at rest) remains intact; relay needs no keys.

##### Why not “temporary relay renewal”

Relay host inspected on 2026-09-21:

```
Manufacturer INTC / "Intel"   ← Intel PTT firmware TPM in CSME, not a discrete chip
Firmware     0xB0008 → CSME 11.8
Algorithms   sha256 ✓ ecdsa ✓ ecc ✓        ← P-256 itself is supported
EK cert      ERROR → INTEL-SA-00086
```

Two independent reasons, either sufficient to reject “relay signs itself”:

1. **EK certificate unavailable (direct evidence, not inferred from an error).** Enumerating NV indices found only
   `0x1410001 0x1410002 0x1410004 0x1820002 0x1880001 0x1880011`. Standard EK-certificate indices
   `0x01C0000A` (ECC P-256) and `0x01C00002` (RSA) **were both absent**. Certificates therefore require Intel's
   online service, which rejects this path because of SA-00086.

   Without a certificate chain, members cannot distinguish “a TPM key” from “an arbitrary key.” The whole reason for using
   TPM was to justify the relay key under rule 6; without proof, the argument becomes “trusted because we put it there,”
   precisely what rule 6 prohibits.
2. **Firmware TPM flagged by SA-00086.** Keys live inside ME/CSME firmware; the advisory permits local attackers to
   execute code in ME, compromising the entire fTPM. `tpm2_getekcertificate` refusal is that flag.

A discrete TPM replacement (hardware change) and redefining the relay trust boundary (overturning rule 6) were both rejected.

##### One hard consequence of this decision

`key_proof_der` is **the subject's signature on the same signed body**, containing `certificate_sha256`. Thus every generation's
certificate in the batch **must exist during pairing**:

```
Pairing ceremony (batch version)
1. Member generates certificates for N generations and calculates each certificate_sha256
2. Member signs every body with its hardware key  → N key_proof_der values
3. Owner quorum signs those same bodies          → N signature sets
4. Relay receives N opaque authorizations and releases them by expires/issued windows
```

The member holds its private key and can perform steps 1/2. Owner still cannot create `key_proof_der`.
Batch length equals the outage period covered, a parameter still to choose.

##### Revocation: a valid certificate does not mean access

This makes revocation less constrained than it first appears. `authorize_remote` first calls
`require_distributed_access(subject, target, now)`: both subject and target must be active members of the **current distribution**.
Revoking a member therefore changes distribution, enforced locally by each peer, **regardless of remaining certificate validity**.
Certificates establish identity, not authorization.

But the same problem moves up one layer: **distribution itself requires Master renewal** (`NETWORK_DISTRIBUTION_LIFETIME`
90 days; `distribution_step` requires Master signatures). Distribution renewal during prolonged Master outages and killing active sessions
within ≤2s on revocation (rule 5) **remain undesigned**. This gap predates the decision, but presigned batches do not solve it
and must not be presented as doing so.

##### Still undecided

- Batch length N, per-generation lifetime, and their relationship to revocation latency.
- Relay release scheduling/idempotence and member degradation when relay is unreachable.
- Triggers/idempotence for “immediate re-signing” when creator returns, and invalidation of remaining batches (does it require reachable members or the distribution layer?).
- Advancement of `certificate_generation` / `predecessor_certificate_sha256` within batches: existing `validate_transition` assumes consecutive local self-issued generations. Batches predefine a sequence; who advances generations and when must be explicit.

#### Initial idea (superseded above): temporary relay renewal

**2026-09-21 user decision.** `expires_at_unix_seconds` remains mandatory. During prolonged creator outages,
muster relay temporarily renews; creator immediately re-signs on return and invalidates the relay version.

For this to work, the form must be something relay **cannot forge**. Relay is explicitly untrusted here
(`src/CLAUDE.md` rule 6: untrusted relay, holds no key material at rest). Letting it issue or extend authorizations
directly overturns that invariant.

**The existing form is freshness proof, not authorization.** `VerifiedFreshnessLeaseV2` binds subjects to authority state/generations
(`authority_set_id` / `authority_epoch` / `authority_operation_head_sha256` / `activation_generation` / `grant_ledger_generation`)
with a lifetime, and `RuntimeReadinessLeaseV2::IndependentWitness` is already a first-class source. Relay can prove
“as of T, I have seen no revocation and the head remains X,” without forging authorization or changing subjects.

Required properties (argue each explicitly during design, not by intuition):

1. Relay can only **extend survival of owner-signed material**, not create it, change subjects, expand scopes, or alter certificate hashes.
2. Relay renewal windows are **far shorter** than owner issuance windows and can be interrupted by revocation; otherwise an untrusted node determines the stolen-certificate window.
3. Relay denial of service can cause only **unavailability**, never excess privilege: without freshness proofs, expire and close.
4. Creator re-signing on return is **automatic**, not manually triggered; otherwise “temporary” becomes permanent.

**Undecided:** freshness-proof signed bodies and relay key provenance (with no persisted key material, who signs and where is the key?),
re-sign triggers/idempotence, and coexistence of revocation propagation during relay renewal with “kill active sessions within ≤2s”
(`src/CLAUDE.md` rule 5).

Cross-device advancement rules for `certificate_generation` / `predecessor_certificate_sha256` also remain undecided:
`validate_transition` assumes consecutive locally self-issued generations, so cross-device issuance must specify “who advances the generation,”
and relay renewal should not advance it.

Replay/renewal rules for `certificate_generation` / `predecessor_certificate_sha256` likewise remain undecided:
`validate_transition` assumes consecutive locally self-issued generations, so cross-device issuance must specify “who advances the generation.”

This is a protocol addition, not wiring, larger than the rest of this document. **Review this section before changing code.**

### Step 3 progress (measured 2026-09-22)

Connected:

- `bind_member_listener_bootstrap_v2` (`47ab294c`): members produce `VerifiedRemoteTargetBootstrapStateV2` through ConsumerActivation.
  The owner-grant admission set is **always empty**; Master distribution decides admission on a separate network channel.
- `member_enrollment_v2` (`18c5c173`): slot-byte format/readback. Readback re-evaluates everything: anchor + operation log rebuild ledger,
  Grant signatures are reverified, Activation rebound, and all ledgers restored under the rebuilt ledger.
  `subject` comes from live hardware held by backend, never from the record itself.
- `ActivationStateStoreV2::restored_runtime_state` (same commit): `verify_federation_activation_v2` can run only once;
  presentation/invitation proofs expire minutes after pairing and restarting devices can never replay them.
  This uses `verify_authority_pairing_activation_record_v2` to check surviving bindings.
- Backend role split (`34a32ac9`): only **absent owner runtime** takes the member path; present-but-corrupt state still fails as owner.
  Comparison strings become constants to avoid drift between two comparisons.
- `read_member_enrollment` moves from owner broker to **device hardware broker**. Behind owner identity it was unreachable
  for exactly the devices it served.

Not connected (precise next locations):

1. **App-side writes.** `MemberEnrollmentRecordV2` is defined in Rust; FFI/Swift writes are missing.
2. ~~Network admission channel is unwired~~ — **Connected (2026-09-22).** First correct the previous version:
   it described the unused `prepare_macos_distributed_remote_product_tls_v2` as the **cause** of empty admission sets. That was wrong.
   The actual cause was missing `CertificateExchangeHandlerV2` on the member path, so `bind_product_generations` never ran,
   `NetworkManagementServiceV2` had no listener binding, and `for_network` inside `prepare_delivery` never executed.

   Wiring: add `MacosMemberExchangeV2` (reads only `member-enrollment`), provide `NetworkManagementServiceV2::refresh_admission()`
   without directory devices (owner `refresh_delivery` requires `MusterDirectoryDevice` and publication lease; members have neither),
   plus a member-specific supervisor loop—the owner loop is entirely owner-specific (runtime bytes, certificate renewal, registration publication).

   `prepare_macos_distributed_remote_product_tls_v2` was removed, together with its sole callee `RemoteProductTlsV2::from_network_verified_parts`.
   The live path first creates non-network TLS, then upgrades via `for_network` → `with_network_admission`.
   The “construct network TLS from scratch” entry was never used.
3. **No happy-path record round-trip test.** All four current tests are rejection paths. A positive fixture must construct a committed
   Activation store (`dummy_verified_activation` in `activation_v2.rs` is module-private).

### 2. Master distributions carry scopes per member pair

Replace the three hardcoded scopes in `RemotePrincipalV2::Network(_)`. This means:

- Network authorization **becomes sufficient**: terminal and computer-use are expressible at the same layer.
- Upper layers still only declare scopes, with one decision point.
- The previously deferred “creator decides other devices' permissions” gains its proper implementation location here.

`POST /terminal` then receives a `remote_route_scope` entry mapping to `MusterScope::TerminalConnect`. Creator explicitly grants remote terminal access; session-read does not include it automatically.

### Two paths to avoid

- **Reverse topology** (each device hosts independently; mac-a joins each): cheap, but turns each machine into an independent governance domain, making “creator centrally decides permissions” impossible. It avoids the problem rather than solving it.
- **Relax the single-network guard** (allow simultaneous owner/joiner roles): retains the confusion described here and restores the dual-role state the guard prevents. That failure class has **no repair entry point** in code (see [muster-single-network.md](muster-single-network.md)).

## Related

- [muster-single-network.md](muster-single-network.md) — Single-network constraint and the invariant failures it prevents
- [agenthalo-remote-terminal.md](agenthalo-remote-terminal.md) — Four remote-terminal slices, blocked on item 2 here
