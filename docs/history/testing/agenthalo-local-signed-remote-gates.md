# AgentHalo local signed-state integration checkpoint

Date: 2026-09-08. Parent revision: `cc95c542`.

This is an implementation/test checkpoint, **not** a release or remote-product
acceptance report. Production backend startup and the phone remote-client
factory remain unconnected. No installed application or Relay was replaced.

## Source implemented

- Shared local signed whole-authority journal with explicit no-independent-
  rollback policy, verified legacy migration and exact persistence readback.
- Shared whole-target snapshot retaining Grant, Interaction and both TLS
  histories, including contiguous expired same-key renewal and revocation.
- Shared owner certificate request validation, current Grant/key binding,
  quorum, durable-response gate and exact lost-response reconciliation.
- Verified directory-derived remote target and cancellation hooks; terminal
  cancellation preserves ambiguous mutation outcomes and credential deadlines.
- macOS authenticated owner/target broker roles, encrypted fixed storage slots,
  opaque App migration/transfer, and sealed backend startup composition.
- Harmony native TCP lifecycle and existing-only HUKS signing callbacks;
  Shared validates actual public key, exact message and returned signature.
- Shared ABI 1.51 and the thin native wrappers; read-only Muster identity
  locator, which never opens or creates the configured key.

## Verification for this checkpoint

| Gate | Result and scope |
| --- | --- |
| `cargo test --workspace` | 512 passed, 0 failed; host socket tests permitted |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| Owner certificate exchange | 3 fixture groups passed; actual signed cold-ACK state, canonical substitution rejection, quorum/readback gates, restart and expired history |
| Hardware signer FFI | 3 tests passed; invalid tables, ownership, key substitution, double hashing, bounds and malformed outputs |
| Apple core build | Latest macOS release static library built; actual C link smoke passed at ABI 1.51 |
| Apple CoreClient | 28 XCTest + 9 Swift Testing tests passed |
| Mac App | 242 tests passed in 15 suites, including local/legacy state paths |
| Mac broker | Host software-key/private-file/CAS tests and negative code-requirement checks passed; sandbox LAContext check was unavailable, so the same test was rerun on host |
| Harmony HUKS adapter | 9 fake-HUKS groups passed and OHOS aarch64 SDK library linked; not real hardware signatures |
| Harmony native TCP | 8 registry and 8 async NAPI groups passed, lifecycle ordering passed, OHOS SDK library linked |
| Production cross-adapter pairing contract | Fresh Mac production-coordinator software fixture → actual Harmony ETS/C++ NAPI → current Rust passed; no fallback Shared-only issuer fixture |
| Go identity locator | Focused race tests passed with temporary configurations; no real key read |

Current local logs are under
`/private/tmp/agenthalo-production-completion.grv3qF/resume-*`.
Native host-link warnings about the local macOS deployment target do not count
as a Harmony device build or installation result.

## Open production gates

1. Implement the Shared consumer certificate state: persist the exact signed
   request before I/O, replay the same request after lost responses, verify
   quorum and exact response binding, then hardware-sign and read back both
   complete TLS histories before exporting credentials.
2. Wire the authenticated production backend loader and certificate exchange
   ingress, durable admission updates, renewal and product-pipe lifecycle into
   `agenthalo-backend/main.rs`. It currently binds only local admin.
3. Expose the native remote-client factory from opaque paired/verified state
   and connect typed workspace/session/message operations. No projection,
   HTTP success, empty admission set or hard-coded endpoint may stand in for
   this constructor.
4. Build/sign and install the actual applications; validate existing Secure
   Enclave/HUKS identities across signed processes, ordinary startup without
   authorization prompts, physical workspace read, message/reply and reconnect.

The local policy deliberately allows an old authentic signed snapshot to be
replayed after restart. It does not claim independent rollback prevention.
Expired certificate history can reconcile a lost response but cannot authorize
a live connection. Relay remains unchanged and byte-forwarding only.
