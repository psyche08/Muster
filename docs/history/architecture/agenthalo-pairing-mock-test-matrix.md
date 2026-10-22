# AgentHalo Pairing Mock System API Test Matrix

## Boundary

The test-only `MockSystemApi` exercises the same Shared Rust boundary used by native Apps:

- P-256 hardware signing through `HardwareSigner`; test keys are software-generated and never ship
  in a release runtime.
- synchronous local-authentication results without exposing any pairing payload to the callback.
- a controllable OS clock.
- atomic persistence plus exact readback acknowledgement for the composite completion journal.
- deterministic failure injection before a write, after write/readback, and at every durable phase.

Native Apps remain thin. The mock does not reproduce Shared state in Swift, ArkTS, Kotlin, or C++.
It supplies only the OS evidence and storage behavior that production platform adapters supply.

The separate `pairing_virtual_phone_e2e` driver extends that boundary across a real loopback TLS
Relay. The Go E2E harness starts the production `rendezvous.Server`, PairingTransport store,
authenticated GrantV2 broker, and owner-directory store. A virtual phone then uses the production
Shared Rust consumer state machine with a software P-256 mock SEP. A test-only, random-token
control endpoint installs only the mock owner's public anchor and owner-signed directory snapshot;
all QR, proof, redemption, approval, delivery, composite persistence, ACK, broker authorization,
and directory-read transitions use production contracts.

## Full-flow acceptance chain

`mock_system_api_drives_qr_to_atomic_ack_full_flow` runs one continuous Rust-owned chain:

1. owner hardware key signs the exact pairing transcript;
2. consumer hardware key signs proof, confirmation, and terminal FirstPair request;
3. an opaque Relay batch is decoded and bound to the owner session;
4. QR proof redemption burns the QR and creates the owner-approval session;
5. mock local authentication approves the exact SAS;
6. owner hardware key signs acceptance, Grant, Activation, and delivery;
7. the consumer verifies delivery and creates the composite completion plan;
8. journal, FirstPair, Grant, and Activation bytes are atomically persisted and read back;
9. only after the durable `Committed` journal, the consumer hardware key signs an ACK bound to the
   exact session, read capability, delivery, Grant, completion ID and Committed generation;
10. the signed ACK is itself journaled and read back exactly before its stable HTTP request exists;
11. PairingTransport and the authenticated Grant broker independently verify the subject signature;
12. a request-bound Relay receipt advances the final journal to `added_idempotent` only after the
    final Acked bytes are read back exactly.

No earlier HTTP response, Relay phase, projection, or partial persistence makes the device visible.

## Scenario coverage

| Scenario | Expected Shared result | Primary automated coverage |
| --- | --- | --- |
| Happy path | `added_idempotent` only after final ACK journal readback | `mock_system_api_drives_qr_to_atomic_ack_full_flow` |
| Hardware signer unavailable | no signature output; caller may retry without state mutation | same full-flow test |
| Local authentication rejected | terminal burn, `local_authentication_rejected` | `mock_system_api_covers_authentication_expiry_mismatch_cancel_and_replay`; FFI mock callback test |
| Local authentication unavailable or invalid adapter result | terminal burn, `local_authentication_unavailable` | same Rust and FFI tests |
| QR deadline | proof redemption fails `Expired` at the exact QR boundary | `mock_system_api_enforces_qr_approval_delivery_and_ack_deadlines_independently` |
| Owner-approval deadline | opaque approval session expires independently of QR | same deadline test |
| Delivery/ACK deadline | unacked completion restores only as `repair_or_re_pair`; no pending action/trust | same deadline test |
| SAS mismatch | terminal `sas_mismatch`; replay is closed | authentication/mismatch/replay test |
| User cancellation | terminal `cancelled` | authentication/mismatch/replay test |
| Duplicate proof/request | closed or replay-classified; no second trust path | existing `relay_batches_are_bounded_deduplicated_and_e2e_verified` and scan tests |
| Candidate, SPKI, request, Grant, Activation, or delivery substitution | `Binding`/signature failure, no trust | existing PairingTransport and completion substitution tests |
| Persistence fails before write | same exact action remains pending | full-flow and every-boundary persistence tests |
| Persisted bytes fail exact readback | `Persistence`; same action remains pending | full-flow test |
| Failure at each of eight pre-ACK durable boundaries | exact idempotent retry reaches committed-but-unacked only | `mock_atomic_persistence_retries_every_completion_boundary_exactly` |
| Crash at each journal phase | safe roll-forward or `repair_or_re_pair`; never reconstructed trust | `completion_restore_covers_every_crash_boundary_and_quarantines_attacks` |
| ACK signer unavailable, wrong key, or signature substitution | no ACK HTTP request; durable Committed state remains recoverable | full-flow, signed-ACK and Harmony lifecycle tests |
| ACK response loss | the read-back `AckPrepared` journal reproduces byte-identical signed ACK request bytes | full-flow and completion journal tests |
| Duplicate ACK receipt | `Replay`; final journal is not applied twice | full-flow and completion journal tests |
| Forked/stale/tampered journal | `Binding` or quarantined repair state | completion restore test |
| Cancelled/revoked/expired Relay terminal stage | exact persistent `failure_stage` retained | Go Relay deadline test plus Rust Relay projection tests |
| Shared GrantV2 crosses the real authenticated broker | exact owner signature is verified; consumer-signed ACK binds the selected read-capability digest and is independently verified at claim and broker | `TestPairingVirtualPhoneMockSEPEndToEnd` |
| Virtual phone sees the paired Mac service | only an Acked Shared completion may create the directory session; signed revision 1 contains one AgentHalo service | same TLS E2E |
| Relay/server restarts or persisted broker state is malformed | existing store persistence and corruption tests remain authoritative; the TLS E2E uses temporary durable stores | owner-directory and PairingTransport store suites |

## Verification commands

```sh
cd src/rust
cargo test --locked -p muster-transport
cargo test --locked -p agenthalo-ffi

cd ../
GOCACHE="$PWD/../.gocache" go test ./internal/pairingtransport
make pairing-virtual-phone-e2e-test
make apple-macos-app-test
make harmonyos-app-test
```

The virtual-phone test proves the hermetic Shared/real-TLS-Relay/broker/directory network path. The
remaining suites prove source behavior, ABI linkage, and platform binding compatibility. None of
them proves real Secure Enclave/HUKS prompts, a deployed production Relay, signed installation, or
physical multi-device E2E; those remain separate acceptance gates.

The mandatory local aggregate is `make pairing-security-v2-hermetic-gate`. Release promotion and
the required `PASS`/`FAIL`/`NOT_RUN`/`BLOCKED` report contract are defined in
[`agenthalo-pairing-release-gates.md`](agenthalo-pairing-release-gates.md).
