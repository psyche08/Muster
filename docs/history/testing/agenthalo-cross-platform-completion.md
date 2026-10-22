# AgentHalo Cross-Platform Development and Acceptance Checklist

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Updated: 2026-09-12. Retains the previous cross-platform baseline (including Windows); this round follows the latest request to advance HarmonyOS, iOS, and Mac in parallel, with results under “Three-Client Follow-up Development” below.
Scope is the AgentHalo platform Apps, not turning mobile clients into local Codex/Claude executors.

## Shared Constraints

- Shared Rust owns business state, authorization, protocols, session/operation semantics, and duplicate-send prevention; platforms wire native OS effects and UI only.
- Do not create authorization by removing unavailable gates, using JSON ready fields, empty directories, or successful HTTP responses.
- Reuse current opaque consumer target / certificate / client / mutation handles; do not add legacy raw-stream remote-authorization paths.
- Do not clear Owner, authorization, certificates, or operation history to conceal recovery gaps. This round does not perform release signing/notarization, installation, device operations, or remote-service deployment. Android Debug APK development signing is not release signing.
- Record compilation, software tests, signing/notarization, installation, native runtime, and actual cross-platform flows separately; do not claim all platforms are usable before all are complete.

## Evidence Baseline at the Start of the Previous Round

| Platform | Evidence on current branch | Main gaps |
| --- | --- | --- |
| macOS | Main workspace, modern authenticated remote client, and recovery exist; `936dd9f9` adds single-line input to existing iTerm2/Claude CLI from Terminal | Real-device validation of new original-client executors, Claude desktop assembly, Codex OS executor, and authenticated remote native-input routing incomplete |
| HarmonyOS | `HarmonyRemoteClient.ets` and production N-API wire Shared authenticated reads/writes/recovery/uploads; the historical README's “no connector” description is obsolete | Actual HUKS, matching signed installation, pairing/continuous interaction acceptance, and recovery/lifecycle gaps in the full existing checklist |
| iOS | Main workspace still uses legacy remote facade; security gate remains unready and current Apple opaque consumer wrapper is not yet in the full product chain | Post-ACK authenticated target/certificate/connection, production-workspace migration, modern durable writes/recovery, matching-device validation |
| Android | Kotlin/JNI, Keystore/pairing foundation; main entry starts with offline repository, modern remote-client JNI absent | Modern opaque JNI/Kotlin lifecycle, Keystore/TCP effects, post-ACK production repository, write recovery, physical-device validation |
| Windows | Current branch has no `src/native/windows`; old branch `ecab52c0` has only static-library/tests, not an executable product | Native App entry, current Shared DLL, OS protected storage/hardware identity, authenticated connections, Named Pipe/local backend, ConPTY, actual Windows validation |

The old Windows coordinator inferred business state from JSON booleans and violates current Shared-only rules; its entire branch cannot be merged back.
Historical OS-call experience can be reused, but new entry points must wire current ABI and native handles.

## Previous Round's Development Scope

1. Windows: executable thin Win32 shell, asynchronous Shared calls, restricted DLL loading, DPAPI in-memory byte-protection adapter, offline contracts, and cross-build validation of the Windows Shared DLL. Persistent storage, authenticated sources, and actual connections are outside this bounded increment.
2. Android: modern target/certificate/client/read/mutation JNI/Kotlin wrappers covering parent-child lifetimes, close/cancel, input boundaries; production entry remains closed until real ACK catalogs/OS callbacks are wired.
3. iOS: device selection after pairing, Shared target/certificate durable recovery, authenticated catalog reads; do not set the old mutation gate to true or claim all workspace writes migrated.
4. HarmonyOS: fix current ABI build gate/stale capability documentation, run existing production N-API/Shared regressions, avoid rewriting the existing connector.

## Previous Round's Verified Results

### Windows

- Generated unsigned x64 `AgentHalo.exe` and current ABI 1.86 `agenthalo_ffi.dll`, both PE32+ Windows artifacts. Packaged DLL and Rust artifact SHA256 match.
- MinGW cross-compiled App, DPAPI adapter, and tests with strict warnings; none were installed or executed on Windows.
- macOS local CTest **2/2**: portable lifecycle/boundary contract and real current Shared ABI. The root task independently reran in a separate build directory and passed. ASan/UBSan portable tests **1/1**.
- UI explicitly says “development preview / disconnected”; Shared projection of empty sources `[]` does not become devices/connections. DPAPI only diagnoses fixed nonsecret bytes in memory, not persistent storage, hardware keys, or Windows Hello acceptance.
- Unix-only transport imports now have platform guards; incremental Windows Shared DLL builds have no previous unused-import warnings. Isolated local UDS regressions **5/5**. Windows local backend/Named Pipe remain unimplemented.
- See [Windows README](../../../src/native/windows/README.md) for builds and remaining boundaries.

### iOS

- Pairing completion/recovery pages wire device selection → ACK-gated catalog refresh → Shared opaque target → same certificate-plan recovery → Keychain exact CAS/readback → native TCP → Shared authenticated workspace reads.
- Provider aggregation, session identity, and archive filtering use existing Shared `.workspace`, without Swift aggregation or empty-argument calls to interfaces requiring Provider IDs. Read signing opens existing Secure Enclave keys only, noninteractively, without key creation or proactive Face ID.
- Background/navigation cancellation, late-result discard, original-plan retention after storage/exchange failure, exact recovery, actual workspace fields, and UI fixture **12/12**; independent root rerun passed. Host tests access no real Keychain/hardware/service and are not authenticated real connections.
- Debug **unsigned builds passed** for Simulator arm64 and generic iOS device SDK. Full iOS XCTest was not run; no simulator startup/iPhone installation/Secure Enclave or cross-platform pairing acceptance.
- Main workspace and remote writes were not yet migrated then; this round's read-only main-workspace migration is below and does not complete all iOS functionality. See [iOS README](../../../src/ios/README.md).

### Android

- Added Kotlin + JNI wrappers for modern opaque target / certificate / authenticated client / session read / mutation, forwarding current Shared ABI without legacy raw-stream authorization.
- Supports certificate rebinding/original-operation reconnection: retained evidence can be passed to Shared for new-target validation after old parent close; failure does not change parent or replace unknown-outcome operations. Worker leases/cancellation/deferred release cover concurrent close; R8 retains symbols required by literal JNI lookup.
- Kotlin lifecycle/exact recovery/boundary fixture **16/16**; actual host JNI → current Shared wrong-handle rejection, length limits, native-worker callbacks **5/5**, with `-Xcheck:jni`. No authenticated remote connection or actual Keystore device.
- Current Rust release + NDK strict-warning builds passed for **arm64-v8a / x86_64**, producing new `libagenthalo_android.so`.
- Existing App JVM regressions **114/114**; Debug AAR/APK builds passed, with both ABI JNI libraries and no host-test symbols. APK has Debug development signing, not release signing; no installation/device instrumentation execution.
- ACK-gated production catalog entry, concrete Android Keystore/raw TCP, and production repository are not fully wired; entry remains unavailable. The 21 low-level tests do not mean Android is complete. See [Android README](../../../src/native/android/README.md).

### HarmonyOS

- Host N-API ABI gate updated from 1.85 to **1.86**; macOS native AX framework dependencies added when linking current Shared.
- Host startup tests load the real debug-auth adapter and mock only non-debug OS bundle information, confirming ordinary startup authentication is not bypassed by test doubles.
- Production C++ N-API / Shared handle lifetimes, install markers/recovery, authenticated catalog, protected disk storage, and multi-Owner tests passed. These are macOS host/software-key tests, not actual HUKS authentication.
- `AGENTHALO_HAP_SIGNING=skip bash src/native/harmonyos/build.sh` passed: existing ArkTS/lifecycle/device layout/remote read-write/attachment/file/Git contracts and OHOS Rust + C++ + Hvigor succeeded.
- Artifact: `src/native/harmonyos/entry/build/default/outputs/default/entry-default-unsigned.hap`. Unsigned/uninstalled; no repeated real Mac → Relay → HarmonyOS pairing/session interaction.

## Remaining Gates

- [ ] Complete flows on each platform: first run, identity/local authorization, pairing ACK, authenticated catalog, actual session input/output.
- [ ] Windows current Shared DLL, native App startup on hardware, DLL identity/permissions, DPAPI/CNG/TPM, system authorization, Named Pipe, ConPTY.
- [ ] Android ACK catalog and Keystore/TCP bound to modern JNI; production workspace, operation journals/recovery, real device lifecycle.
- [ ] iOS modern authenticated writes, uploads, original-request recovery, actual Secure Enclave behavior; read-only main workspace wired this round.
- [ ] Shared authorization issuance, routing, independent reception observation for modern remote original-client input; ordinary provider send is not original-window input completion.
- [ ] Multiple devices/Owners, revocation, disconnection/reconnection, no resend of unknown results, abnormal exit, long observations.
- [ ] Matching-version signing/notarization/installation followed by real cross-platform unified acceptance.

The [original checklist](agenthalo-remote-completion-checklist.md) remains authoritative for all Mac/HarmonyOS gaps; these parallel increments do not replace it.

## Three-Client Follow-up Development (2026-09-12, Source and Builds Verified)

This round advances HarmonyOS, iOS, and Mac in parallel as requested. Serial mac-c/mac-b migration and actual pairing belong to another task; this round does not operate either remote machine, phones, installed clients, or the virtual network. ABI remains 1.86.

- iOS: wires existing ACK-gated authenticated reads into the main workspace; transcript previews, runtime, and pending items all use Shared typed reads. Production read-only wiring only, no fabricated legacy `health` success or unimplemented remote writes.
- HarmonyOS: fixes observation stopping after initial preview failure/sending, background reads, old replies/cleanup affecting new pages. Runtime uses an independent read channel; unchanged transcript signatures must not freeze approvals/questions/runtime information.
- Mac: fixes blocking main-thread Unix-socket readiness probes, adds startup cancellation/late-result isolation. Physical root causes of historical grey/stuck exit remain unreproduced; this source path alone does not solve every exit issue.

New Mac async-factory-during-stop regression actually triggered a stale-pointer crash in Shared `shutdown_inner`: directory/authorization federation retained the public peer handle's raw address, dereferenced again during late child cleanup after parent destruction. Scope therefore expanded to Shared internal parent-child leases/federation lifecycle; merely dropping UI replies or excluding these tests is insufficient. The crash came from isolated fixtures, not a reproduction of the installed App's historical crash.

The root task independently ran Shared `mobile_read::tests`, **9/9 passed**: unchanged transcript signatures still refresh pending items, read failure does not claim empty inbox, runtime/tool content, session identity, and authorization rejection retain contracts. No actual Providers/devices were connected.

- Shared `43aaf964`: internal network/read-only snapshot leases, mutual exclusion of stop/source updates, original-candidate retention when exiting during federation persistence. FFI all-target **104/104**, strict Clippy, plus **20 consecutive rounds** of persistence/exit race passed. The first full run was blocked only on temporary UDS bind by sandbox; authorized full rerun passed. ABI 1.86, no new C symbols/protocol changes.
- HarmonyOS `8a3e4f33`: root independently reran final page observation **13/13**, runtime lifecycle, remote-client effects successfully; full `build.sh` reran against Shared `43aaf964`, passing OHOS Rust, production N-API, ArkTS, unsigned HAP. New HAP SHA256 `684556c0ca5cebf0b1dfe8b794c8db9d33ad6541356a29e6429a36576369538f`, no signed installation.
- iOS `32c8bb41`: main-model/actual coordinator host tests **29/29**, 0 skipped, independently rerun by root with rebuilt macOS Shared. Covers no legacy facade from main entry, read-only capability, preview-independent runtime, pending items, coalesced refresh, foreground/background/Owner/selection generations, and scan/SAS/recovery exclusion. Initial host compilation hit sandbox Xcode macro-plugin restrictions; authorized rerun passed; no real Keychain/Relay. Latest Shared unsigned iPhoneOS/Simulator builds and full XCTest target `build-for-testing` passed; the latter compiled only, without executing real-Keychain tests or launching a simulator.
- Mac `d986c6ff`: after Shared XCFramework update, a fresh temporary SwiftPM directory rebuilt/ran **401/401** across 19 suites, without excluding the 5 original crash cases; root independently reran those 5 successfully. App/Migrator debug relinking passed. Also fixed reused UI event sequence numbers after restart incorrectly rejecting the new instance's first event; no durable state cleared.

All three Apple Rust release slices/XCFramework were rebuilt from `43aaf964`; macOS link smoke passed. Xcode packaging warned CoreSimulator service was inaccessible in sandbox, but XCFramework generation succeeded: build evidence, not simulator/physical runtime. None were release-signed, notarized, installed, or used for real cross-platform pairing.

Claude desktop executor still lacks independent observation, production assembly of shared input channels, and available OS/UI triggers; no fabricated reception cursors/`Accepted`, no bypass of native-tool access restrictions. Retaining stale failed-peer cards must reuse/extend Shared multisource state, not independently cache/restore authorization in ArkTS.

## Subsequent Production Wiring Order

1. Android: modern consumer-runtime/ACK-catalog JNI, Keystore and cancellable raw TCP, protected-storage CAS, then switch production repository to opaque handles.
2. iOS: main workspace reuses authenticated targets and wires Shared durable mutation/upload/watch/recovery; Swift must not regenerate send IDs or interpret unknown outcomes.
3. Windows: add CNG/Hello native effects, durable CAS, Shared pairing/authenticated remote sources to the buildable entry, then session UI; accept local-backend Named Pipe/ConPTY separately.
4. After these paths are wired, produce matching signed installation packages and jointly run real cross-platform multidevice/revocation/disconnection/abnormal-exit/duplicate-submission tests.
