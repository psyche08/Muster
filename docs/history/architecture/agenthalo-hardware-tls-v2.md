# AgentHalo hardware TLS V2

This component is a prerequisite for the secure Remote Listener. It does not
publish a service, set the `REMOTE_LISTENER` capability, or restore the removed
legacy `8765` forward.

## Ownership boundary

- Shared Rust creates the X.509 `TBSCertificate`, Muster authorization signing
  bytes, hashes, certificate generation, predecessor relation and protected
  state transaction.
- The macOS broker opens only the existing Muster P-256 Secure Enclave identity
  selected by an explicit OS wrapped-blob locator. It does not create a separate
  TLS key, import a software private key, change an ACL, or repair permissions.
  The locator and enclave-wrapped bytes are protected OS inputs, not authority
  or proof of the target identity. Shared bootstrap must independently compare
  the returned actual SPKI with its verified runtime target before activation.
- Blob reads are bounded to 64 KiB, reject symlinks in every path component, and
  require an owned private parent directory (0700) and single-link regular file
  (0600). Missing, changed, unsafe, or inaccessible input fails closed. The broker
  neither logs nor returns the locator or blob.
- CryptoKit opens the existing `SecureEnclave.P256.Signing.PrivateKey` with
  `LAContext.interactionNotAllowed = true`; background restoration/signing cannot
  request interactive authorization. If the OS does not confirm that setting on
  readback, opening the key is rejected before any Secure Enclave call. The
  broker signs exact 32-byte SHA-256
  digests using CryptoKit's `Digest` overload, without hashing the digest again.
- rustls implements `SigningKey` and `Signer` over the authenticated broker;
  no PEM, environment variable or private-key file is accepted.

## Authenticated broker

`dev.linsheng.agenthalo.tls-broker` is a user-domain launchd Mach service
bundled in `AgentHalo.app`. Each XPC connection is accepted only when all of
these checks pass:

1. audit-token resolution produces a live `SecCode`;
2. effective UID matches the broker;
3. identifier matches its compiled role: `agenthalo-backend` for hardware
   signing, or `dev.linsheng.agenthalo` for the bounded owner-state transfer;
4. Team ID is exactly `89LGY6BD53` and the compiled designated requirement
   validates;
5. the executable has the required bundle location and a non-empty CDHash;
6. the first request binds an exact 32-byte backend boot nonce for the lifetime
   of that XPC connection.

The opening request also locks the existing identity locator for that session.
Repeated opens reuse the same opened hardware key; another locator is rejected,
including after an earlier open failed. The Rust locator-free `connect` entry
point returns unavailable without contacting XPC. Only
`connect_existing_identity(path, nonce)` attempts the authenticated open, and it
does not itself authorize a remote listener.

The App transfer role can read/write only the encrypted `owner-runtime` slot;
it cannot sign through this broker or open backend target/TLS slots. The backend
may read owner state and persist target/TLS state. Slots have fixed paths, a
64 MiB bound, private no-symlink files, serialized compare-and-swap, fsync, and
exact readback. Existing Secure Enclave encryption material seals these files;
the broker never creates a key as a repair for missing state.

The owner signing identity uses a separate immutable XPC connection with the
explicit `owner-authority-raw` role. Its existing `SecurityKeys` CryptoKit blob
is raw and bounded to 4096 bytes; the original `muster-target-base64` role keeps
its base64 encoding. There is no encoding autodetection or role/locator change,
and an owner connection cannot activate, revoke, or sign a TLS handshake. Both
roles are open-existing-only with the same private-file, no-symlink and
noninteractive checks. The returned actual public key is OS input, not an
authority claim: Shared must match it to the durable owner or target context.

Software regression tests exercise path/file rejection, immutable session
binding, the exact digest-signing overload, noninteractive context settings, and
the compiled code requirement against signed code. They do **not** open a real
Muster blob, prove Secure Enclave access across signed processes, or establish
successful hardware signing. Those remain explicit physical-device gates; the
production protected-state loader and remote bootstrap are not enabled here.

An XPC disconnect, broker restart, backend restart, nonce mismatch, code-sign
failure, Keychain failure or Secure Enclave signing failure is terminal for the
TLS identity. Same-UID Unix sockets are not an authentication substitute.

## Short certificate lifecycle

The DER/TBS codec and two exact signing effects live in Shared transport.
Completing the TBS signature and the separate subject/predecessor possession
proof returns only an unsigned authorization draft; it cannot mint quorum
approval, TLS admission, or runtime readiness. The backend merely sends the
Shared-selected SHA-256 digests to its authenticated hardware broker.

Certificates are self-issued key containers, valid for no more than five
minutes. Their SAN bindings include service, device SPKI, key ID, authority
set/epoch/head, certificate generation and predecessor. Current authority
quorum separately signs the exact certificate hash and those same fields.

Issue requires a hardware-key proof and current authority quorum. Rotation
requires two independent proofs over the exact authorization bytes: one from
the certificate key and one from the predecessor key, plus current authority
quorum. Rotation is contiguous, permits at most 30 seconds of old/new
certificate overlap, and becomes live only after exact protected-state hash
readback. Broker activation is certificate-hash and generation scoped, so
activating the replacement makes an old rustls signer fail immediately.

The runtime controller additionally binds the exact opaque Shared online
readiness handle, its protected-state generations, the short-certificate
generation, and the authenticated backend boot nonce used by the XPC broker.
Readiness expiry/change, certificate rotation/revocation/expiry, hardware-key
loss, or broker failure marks every existing session lease evicted; all later
stream reads and writes fail closed.

Revocation is quorum-only so loss of the hardware key cannot block it. Shared
first commits the terminal protected state; the verified revocation is then
applied to the broker, which clears the active certificate and rejects further
TLS signatures. Protected restore retains and re-verifies the latest authority
authorization and hardware SPKI and retains revocation history. Within a live
history it rejects predecessor forks and cannot revive a revoked certificate.
Cross-restart rollback guarantees depend on the explicit policy below.

Client admission requires both current Shared grant membership for the exact
leaf SPKI and a current quorum-verified short certificate hash. The TLS 1.3
handshake accepts only ECDSA P-256/SHA-256, one leaf certificate, no early data
and no session tickets.

## Listener integration gate

The listener may consume these opaque identities only after authenticated
backend-state exchange, the selected protection policy, verified grants,
interaction ledger and Relay publication are complete. The runtime session
leases implement immediate stream eviction on expiry, rotation, revocation,
broker failure, or freshness loss. The sealed startup composition can consume
these handles, but production `agenthalo-backend/main.rs` still binds only the
local admin socket; it does not invoke this composition.

Certificate renewal with the same Secure Enclave key is implemented. A full
hardware-key replacement remains unavailable unless the predecessor key can
produce its proof or a separate quorum-authorized recovery protocol commits a
cross-key protected-state transition. There is no serialized, manual, or
single-local-authority fallback, and no Remote Listener capability is exposed.

## Explicit local signed-state policy

`LocalSignedNoIndependentRollback` is a separate Shared policy, not a fabricated
witness or freshness success. The owner signs the complete authority journal;
the target signs the complete Grant, Interaction and server/client TLS history
snapshot. Both require cryptographic verification and exact durable readback.
Legacy migration is available only from an already verified durable runtime.
An invalid existing local envelope does not fall back to an older witness file.

This policy intentionally does **not** prevent replay of an older, valid signed
snapshot across process restarts. It retains identity checks, current Grant and
quorum requirements, bounded process leases, tamper rejection, and in-process
history/CAS checks. It does not add an independent counter, remote witness or
rollback recovery claim. Ordinary startup uses noninteractive key access;
unavailable keys fail closed without requesting authentication or making keys.

## Certificate exchange and remaining production wiring

The Shared owner plan accepts only a bounded, canonical, hardware-signed
request for the exact current paired Grant, target and certificate key. It
verifies certificate possession, predecessor history and authority quorum.
The response is available only after that exact certificate is present in the
hardware-signed, durably read-back target snapshot. An exact replay may recover
already installed expired client evidence, but that evidence is history, never
live TLS admission; new issuance of an expired proposal remains rejected.

The request uses a domain-separated SHA-256 commitment to keep the original
hardware signing message bounded. The native HUKS callback signs that original
message with SHA-256 exactly once. The Shared callback adapter independently
verifies P-256 SPKI and every returned signature. The native adapter opens only
the existing paired identity alias, has no JavaScript/UI reentry, and retains
its context until the last Shared owner releases it.

| Boundary | Current source state |
| --- | --- |
| Local owner journal and target histories | Signed whole envelopes, exact readback and legacy migration implemented |
| Owner certificate exchange | Shared validation, quorum and durable-response gates implemented and tested |
| OS ports | macOS authenticated broker and Harmony raw TCP/HUKS callback adapters implemented |
| Consumer certificate exchange | Pending: signed request checkpoint, exact retry, response verification and durable TLS histories |
| Production startup and connector | Pending: backend loader/main wiring, certificate exchange ingress, renewal and native remote-client factory |
| Product acceptance | Pending: signed deployment and physical workspace read, message/reply and reconnect |

Passing local protocol tests, SDK linking or a successful pairing does not
clear the pending rows. Relay remains a byte forwarder; neither an HTTP success
nor directory metadata can create a verified remote client.
