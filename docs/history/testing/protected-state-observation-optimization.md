# macOS Protected-State Observation Optimization

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

## Scope and Security Boundaries

This change addresses repeated decryption, serialized waiting, and observation-result installation races in backend state observation and the macOS Broker. Relay, the HarmonyOS protocol, and pairing data formats were not changed; no actual pairing data was cleared.

- Authorization freshness remains **2 seconds**, with a **250-millisecond** periodic fallback. Validity was not extended and Shared validation was not skipped.
- The Broker only adapts secure file reads, hardware decryption, and interprocess communication. It does not parse Shared business fields or return “trusted,” “paired,” or “ready” flags.
- The plaintext cache is not an authorization cache. Callers still submit raw bytes to Shared for validation; caching cannot restore revoked or expired permissions.

## Implementation

1. **Fixed two-slot read**: new `protected-read-listener-pair` reads only `owner-runtime` and `target-state`. One read before and one after Shared validation reduce each observation cycle from four XPC requests to two, with the same exact before/after readback requirement. Current certificate-state checks also reuse this fixed interface.
2. **Exact ciphertext cache**: every call still securely opens and fully reads the file, checking permissions, owner, link count, file size, and metadata before/after reading. The previous decryption result is reused only when newly read ciphertext, file metadata, and the digest of the securely read key-locator file all match exactly. Changes, missing data, tampering, or validation errors cannot fall back to cache. Cache payload is capped at 16 MiB; oversized entries are not cached. After an actual persistent write, ciphertext must be authenticated and read back again.
3. **Short state lock**: reads, Shared validation, and candidate-generation preparation run outside the target baseline lock; installation reacquires the lock and checks the baseline revision. A concurrent write superseding the old candidate returns `Superseded`, without installing or renewing it. Certificate OS persistence also runs outside the baseline lock, and observations cannot renew during a write; uncertain or timed-out writes terminate current state.
4. **Unified absolute deadline**: the observation's remaining validity covers the baseline lock, Broker serialization wait, request encoding, XPC wait, and result installation. The deadline is rechecked after queuing/encoding and before the backend worker starts sending. Late results cannot restore expired service permissions. System calls already executed cannot be forcibly undone; uncertain write results still follow fail-closed and Shared recovery boundaries.
5. **Coalesced wakeups**: certificate persistence completion in this process requests an immediate refresh; duplicate notifications coalesce instead of running concurrent observation cycles. External App writes and lost notifications still rely on the periodic fallback; a notification is not state evidence. Destroying the observer wakes waiting threads without waiting for a potentially stuck OS read.

## Diagnostics

Backend logs add:

- `protected pair`: serialization queue `queue_us`, request wait `rpc_us`.
- `protected observation`: total duration, baseline-lock wait, both reads, Shared validation and candidate preparation, final installation duration, and `Verified / Superseded / Rejected`.
- Existing expiry/observation-rejection logs are retained.

The Broker writes both stderr and macOS Unified Logging, subsystem `dev.linsheng.agenthalo.tls-broker`, category `protected-observation`:

- `protected-read`: fixed slot, byte count, cache hit, file-read time, key-context read time, and decryption/cache-return time.
- `protected-lock`: waits for in-process and file-coordination locks.

Slow paths are logged; normal reads are sampled once every 128 calls. No plaintext, ciphertext, keys, authentication material, or user-directory contents are logged. `verify_us` includes candidate-generation preparation and cannot be interpreted as pure cryptographic-algorithm time.

## Local Validation (2026-09-11)

- Full backend library suite: **406 passed, 0 failed, 14 ignored under the original configuration**. This includes real local TLS/socket software end-to-end tests, not phone physical-device acceptance.
- `cargo clippy -p agenthalo-backend --lib --tests --offline -- -D warnings`: passed.
- `cargo build -p agenthalo-backend --release --offline`: passed.
- Broker Swift compilation and software self-tests passed for cache invalidation, metadata changes, missing files, tampered ciphertext, fixed two-slot fields, invalid deadlines, and late decryption results; negative code-signing policy cases passed.
- Cache-count test: the first two-slot read decrypted twice, and 20 subsequent reads still totaled 2 decryptions. This measures software-path operation counts, not actual Secure Enclave latency.
- New concurrency tests verify that slow reads do not hold the certificate baseline lock, concurrent CAS supersedes old candidates, slow writes do not renew, lock waiting counts toward the deadline, late writes cannot resurrect state, wakeups coalesce, and exit wakes waiters.
- The default sandbox prohibits local sockets and caused existing noninteractive LAContext checks to fail; the full tests above were rerun outside it using only temporary files, software keys, and local test sockets.

## Release and Acceptance Not Performed at Initial Commit

This round **did not sign, notarize, replace an installation, or restart installed clients**, nor repeat real Mac—HarmonyOS directory/session access tests.

The next installation must update backend and Broker together: the old Broker lacks the new fixed two-slot operation, and a mixed installation must fail closed rather than downgrade by skipping validation. After installation, verify cache hits and phase timings, continuous directory access, certificate rotation, multidevice revocation, lock/unlock, and exit, recording physical-device results separately. Existing pairing data should be preserved; this optimization does not require scanning again.

## Subsequent Signed Installation and On-site Validation (2026-09-11)

After the user authorized installation validation, both backend and Broker inside `/Applications/AgentHalo.app` were replaced, with both metadata records identifying source `0f23a7f4`. The UI retained original Debug build `893bc467`, and the package remained explicitly marked as a component update, not a complete new UI build. The old installation package was not backed up; pairing data was neither deleted nor migrated; Relay and the HarmonyOS package were unchanged.

- Developer ID: `the project publisher (89LGY6BD53)`; `codesign --verify --deep --strict` passed before and after installation; no notarization.
- Both installed components match the signed staging files byte for byte.
  - backend SHA-256: `2a64c862d91496447492ce7247abe36490a9e69f3f0dbe6b852a5d9f60a95ce5`
  - Broker SHA-256: `74cddbbcb7f786c717cc7c0e6aaa2498d5b62554dcfaf7dab0d058bcfdc556bd`
- The original main process did not respond to TERM. Under existing authorization, the exact PID was force-terminated before installation; the new App launched with `--debug-skip-local-auth`. The old UI's termination issue was not claimed fixed.
- The real signed App returned `read_completed` for `--check-owner-runtime-transfer`; this proves only OS transport, not Shared authorization acceptance.
- The actual Broker rejected a recompiled ordinary same-user XPC probe for caller-identity mismatch.
- Backend PID `19386` and Broker PID `19279` started; backend health returned `ok: true`, and logs confirmed the Shared listener started on loopback `8765` and connected to the product forwarding channel.
- On-site samples from 20:14:52 to 20:16:37 confirmed `cache_hit=true` for both slots. One cold decryption of the target slot took approximately `465029 us`; subsequent target-slot cache-return segments took `3–9 us`, plus separate file-read and key-context-check time. These segment numbers are not complete request latency.
- During this observation interval, backend slow cycles were `103–491 ms`, all `Verified`, with no new observation expired/rejected observed. Ordinary fast cycles are not fully logged, so this is neither a complete latency distribution nor a long-term stability conclusion.
- HarmonyOS device `5NC0225C10020352` was online, but both actual UI layouts showed `ScreenLockRootComponent` visible and focused. The user was asked to unlock it; phone directory/session access and multidevice revocation had not yet completed physical-device acceptance. No rescan was required.

In pre-installation revalidation, the first full run had 405 passed, 1 test-ledger lock failure, and 14 ignored. That individual rerun and a further full regression (406 passed, 14 ignored) passed. The lock failure has not been reproduced or located, so it cannot be claimed fixed. Broker software tests and code-policy checks for the newly signed backend passed.

## Phone Test After Unlock (2026-09-11 20:23–20:31)

**Conclusion: directory/session lists recovered; complete end-to-end acceptance still failed.**

- After the user unlocked the phone, the existing HarmonyOS App was operated through HDC. The Devices page showed authenticated workspace capability; the Agent page showed `mac-a`, `423 · healthy`, and `Show more · 5/423`, with actual sessions listed. Source `readWorkspace()` projects healthy only after real `client.workspace()` returns, so this gate is stronger than mere service advertisement.
- Opening the existing task-related session `AgentHalo 配对 - task 2` (“AgentHalo pairing - task 2”) produced `transport error: Unavailable: remote raw stream open failed` on the detail page, with no transcript. No message was sent, remote control recovered, or write operation performed.
- Returning to Devices showed `Loading remote device directory and workspace…`, with Refresh disabled as `Refreshing…`; this does not establish successful repeated refresh.
- To exclude residual connections from the old process, the phone App was force-stopped and restarted, retaining app data, Owner, and HUKS identity and continuing the authorized Debug authentication-bypass argument. The session list recovered again, but opening the same session returned the same connection error.
- The corresponding Mac backend remained PID `19386`, launchd `runs = 1`; this was not a main-backend crash. Observations included `TLS generation source released`, observation rejections of 22/27 ms, subsequent remote-listener rebuilds, and successful installation of a new TLS admission generation. No observation expired appeared during this interval; the remaining fault cannot continue to be attributed to the original two-second read timeout.
- Shared `remote_client_v2::open_raw_before_deadline` emits the error when `connector.open(remaining)` fails, before the request establishes the underlying stream or executes TLS/HTTP. The current wrapper discards original error text while retaining class `Unavailable`; Relay rejection, transport disconnection, and connection-lifecycle races cannot yet be distinguished. A causal relationship between Mac generation release and this error is also unproven.
- UI `selectSession` requests provider options, runtime status, and preview concurrently. Further investigation must cover this real concurrent access and reconnection after listener/product forwarding-channel reconstruction. Resetting pairing, relaxing authorization, or bypassing TLS must not conceal the failure.

Still not successfully verified: transcripts, file/Git reads, message sending, repeated stable refresh, multidevice revocation, and normal authentication mode. This round only records on-site results; business code was not modified or deployed again.
