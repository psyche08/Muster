# Legacy Implementation Baseline (2026-09-24)

Part of the [Muster architecture](../ARCHITECTURE.md). Section numbers are shared across all parts, so a reference such as §3.6.2 means the same section wherever it appears.

## 2. Old Implementation Diagnosis (2026-09-24 Baseline)

### 2.1 Size

| Part | Lines | Notes |
|---|---|---|
| Rust | About 180.7K | `muster-transport` 76K, `agenthalo-backend` 56K, `agenthalo-ffi` 35.5K, `agenthalo-client` 10K; about 67K test lines |
| Go | 25K (excluding tests) | `internal/agentd` 8.2K, `ownerdirectory` 3.2K, `pairingtransport` 1.6K, etc. |
| Swift helpers | About 4.1K | `muster-sep-helper`, `muster-tls-broker`, `muster-net-helper`, `muster-net-bridge` |
| macOS App | 43K + CoreClient 13K + TerminalAdapters about 6K (handwritten) | |
| iOS | 28K | About 1.6K implement yamux, TLS, and MST1 in Swift |
| Android | 30K | JNI C shim alone is 4.4K |
| HarmonyOS | 31K | `ets/domain` has 6.1K of TypeScript state machines; NAPI C++ is 6.4K |
| Windows | 0.6K | Preview |

Largest files:

| File | Lines |
|---|---|
| `muster-transport/src/pairing_transport_v2.rs` | 12,060 |
| `agenthalo-ffi/src/security_v2.rs` | 11,508 |
| `muster-transport/src/authority_pairing_issuance_v2.rs` | 10,037 |
| `freshness_v2.rs` | 4,977 |
| `authorization_v2.rs` | 3,372 |
| `activation_v2.rs` | 3,104 |

Of `muster-transport`'s 64 source files, 50 are `_v2`.

### 2.2 macOS Runtime Process Topology (Baseline)

```
AgentHalo.app (Swift UI, user)
 ├─ Process: muster connect --raw-unix …   × one per remote peer (Go, includes WSS+TLS+yamux)
 ├─ Process: muster peers / resolve-relay / product-identity-locator / pairing-request …
 ├─ Process: /usr/bin/caffeinate (privacy mode)
 ├─ NWListener loopback: AgentHaloLocalTerminalService (iTerm2 / Ghostty adapters in the App process)
 └─ XPC → muster-tls-broker

launchd (user)
 ├─ agenthalo-backend (Rust, UDS HTTP :backend.sock + TLS/MST2 :127.0.0.1:8765)
 │    ├─ Process: muster product-pipe (Go, moves Relay traffic to 8765)
 │    ├─ Process: muster peers / local-device / pairing-request
 │    ├─ Process: muster-agent authorize … --json
 │    ├─ XPC → muster-tls-broker (SEP TLS signing)
 │    ├─ loopback TCP → terminal service in the App
 │    └─ codex app-server / claude CLI / git
 ├─ muster-tls-broker (Swift, MachService)
 └─ muster-agent (Go, Relay registration, SPKI ledger, vnet, direct QUIC)
      ├─ muster-sep-helper (Swift, stdin/stdout signing)
      └─ muster-net-bridge (Swift) ──XPC──▶ muster-net-helper (Swift, **root** LaunchDaemon, creates utun)
```

Three paths independently open the same hardware identity:

- Go through `muster-sep-helper`;
- Rust through `muster-tls-broker`;
- App through CryptoKit.

### 2.3 Multiple Implementations of the Same Responsibility

**Trust models: 3 coexist**

| Model | Location | Purpose |
|---|---|---|
| Go SPKI ledger | `muster-agent authorize`, `internal/agentd/ledger.go` | authorized_keys model serving MST1 / vnet |
| Owner authority / GrantV2 / Federation / Freshness / Activation | Rust security V2 | Serves the MST2 product listener; also includes owner directory (Relay-side Go implements signature verification again) |
| Device Network / Master distribution v3 / network certificate / member enrollment | Rust, newest model | Addresses joining-only devices being unable to start listeners |

`remote_authorization` still branches between two principals:

```rust
Owner(p)   => p.has_scope(scope)
Network(_) => matches!(scope, Read | Write | Approval)
```

vnet's mapping from SPKI to virtual address is a **manually written static table** in each machine's `agent.yaml` (`internal/agentd/vnetconfig.go`). This is a fourth identity-related dataset requiring manual consistency.

**Device-to-device transports: 5**

1. Go: WSS / QUIC-h3 → yamux → MST1 → inner TLS 1.3 → `pipe` / `vnet` / `path`. vnet IP packets have 4-byte framing in yamux streams; Relay paths are **TCP over TCP**.
2. Rust: platform raw TCP callbacks → handwritten RFC 6455 WebSocket → rustls → MST2 → HTTP/1.1 (one request per connection); the server relies on Go `product-pipe` forwarding to 8765.
3. iOS：Swift Network.framework TLS + Swift yamux + MST1。
4. Go direct QUIC (`quicpeer`) + STUN hole punching.
5. vnet: utun + root helper + bridge + per-packet source-address validation.

Android and HarmonyOS peer transports remain unimplemented (fail closed); none of the three mobile platforms has vnet.

**Client API: 3 paths**

- `agenthalo_local_*`（UDS）；
- `agenthalo_muster_*`（MST1）；
- `remote_consumer_v2` / `remote_mutation_v2` / `remote_upload_v2`（MST2）。

**Mutation-deduplication journals: 5**

- backend `local_mutation.rs`；
- client `local_mutation.rs`；
- client `mobile_mutation/local.rs`；
- ffi `local_mutation.rs`；
- ffi `remote_mutation_v2.rs`。

**Codecs duplicated in Go and Rust**: MST1 headers (two Go copies: `innerchan/codec.go`, `agentd/forwarder.go`; another in Swift), owner-directory signatures, GrantV2, invitation proofs, and pairing signed acknowledgments.

### 2.4 ABI Growth

- **499** `extern "C"` functions. Every feature adds an entry; ABI reached 1.102.0, with 80 "ABI 1.x" change comments in the header.
- `AgentHaloSecurityV2Handle` is an enum with **101 variants** containing cryptographic objects, sessions, ceremonies, remote clients, and device networks; 176 functions return through it.
- Much cross-FFI state uses JSON strings (`sources_json`, `event_json`); backend has 573 occurrences of `json!(`.

### 2.5 Business Logic Leaks into Platforms (Violating AGENTS.md)

Four platforms each implement the following:

| Concept | macOS | iOS | Android | HarmonyOS |
|---|---|---|---|---|
| Session `displayState` derivation (the same expression is identical in three places) | `AgentModels.swift:154` | `AgentHaloModels.swift:252` | `AgentHaloModels.kt:131` | `.ets` |
| Multidevice session merging / archive filtering / pagination | `WorkspaceModel` (2,833) | `WorkspaceModel` | `MultiDeviceAgentHaloRepository` | `SessionCatalog.ts` |
| Device-page list (merge 4 sources, compute needsAttention) | `MusterDeviceDirectory` | `MobileDeviceDirectory` | `MobileDeviceDirectory.kt` | `MobileDeviceDirectory.ts` |
| Peer state → reachability + probing 8765 | `MusterPeerManager` | `WorkspaceModel` + `RemotePeerLifecycle` | `MusterServiceDirectory` | `MusterPeerStateAdapter` |
| Pairing state machine + retry backoff | `LiveMobileAuthorizationService` (1,670, 4 retries) | `PairingConsumerRuntime` | `SecurityV2PairingTransport` (1,931) | `SecurityV2PairingTransportLifecycle.ts` (1,523, 10 phases, 4 retries) |
| Single-network guard | `AgentHaloNetworkRole.swift` | — | — | — |
| Terminal polling backoff | `terminalPollInterval` | — | — | — |

Platform engineers are not the root cause. The Rust core has no runtime and does no I/O (`muster-transport` has no `TcpStream::connect`, tokio, or async). For retry timing, polling, and merging sources into one list, Rust can only tell platforms what to do next; platforms must write the loops.

### 2.6 Product Goals and Implementation Diverge

| Goal | Baseline state |
|---|---|
| R3 vnet | Works only between Macs; manually configured addresses; no port-level authorization (authorized peers can access every port); Relay uses TCP over TCP; phones have no equivalent (no forwarding streams or in-App SSH / VNC clients). |
| R4 Locked operation | `macos_claude_native_input.rs:200` rejects `!unlocked \|\| !focused`. The existing AX executor **works only unlocked with the input field focused**, contrary to the goal. `/native_ui/*` routes are installed only in tests. Computer Use / Locked Use are only authorization vocabulary on each platform, with no executor. |
| R4 Viewing | Polls preview (5 seconds / 2 seconds). No push. |
| R5 Terminal | iTerm2 adapter runs in the **App process** (Swift + SwiftNIO). Backend loops back to App: App A → Rust → muster → peer backend → loopback → peer App → iTerm2. **Without the App running, remote terminal is unavailable.** Snapshots use **full-screen base64 polling** (250 ms, backoff up to 16×), without iTerm2's screen-update notifications. Network-member scopes lack `TerminalConnect`, so remote terminals still cannot be authorized end to end. |
| R2 Creator offline has no effect | Distribution expires in 90 days and needs Master renewal. Member service depended on owner broker (`load_existing_state`). From September 21–24, member enrollment, runtime lease, and publication were still being wired incrementally. |
| R2 Pairing | One device, one network is an implementation constraint rather than a design: holding owner and consumer state simultaneously triggers an unrecoverable `issuance_invariant`. |
| Diagnosability | `docs/history/architecture/muster-single-network.md` records silent failures at every layer: no App logs, FFI drops diagnostics, and no retry after directory-refresh failure. |

### 2.7 Root Causes

1. **The threat model exceeds product needs.** Security protocol V2 already acknowledges same-UID malicious code / endpoint root as non-goals. Yet considerable code implements local anti-rollback: freshness witness quorum, sealed heads, two-phase activation, protected state, anti-rollback, and broker processes existing only to restrict same-UID access. A personal network with at most 10 devices does not need federation quorum.
2. **Add without deleting.** v1 and v2 coexist (`pairing.rs` / `pairing_v2.rs`, `remote.rs` / `mst2_v2.rs`, `authorization.rs` / `authorization_v2.rs`), and ABI only grows. Every fix adds a concept; members failing to start created a third trust model.
3. **Process and language boundaries are treated as security boundaries.**
   - SEP signing goes through helper or broker processes;
   - TUN has a bridge in addition to the necessary root helper, solely because Go cannot directly use XPC;
   - Relay traffic needs Go `product-pipe` forwarding to the Rust listener;
   - Terminals require backend loopback to the App.
   
   Every boundary brings its own lifecycle, IPC protocol, signature verification, and failure modes.
4. **The Rust core is a pure-function library plus callbacks, not a state-owning runtime.** Platforms must drive connections, retries, polling, and merging, so business state naturally leaks outward.
5. **Driven by acceptance gates instead of product slices.** Much documentation records failed gates. The shortest UI-to-peer-execution feature path never became complete end to end, while new gates kept accumulating.

## Appendix: Baseline Data Sources

Paths below refer to the legacy tree at commit `acf23be7` (§8.5).

- Process topology: `native/apple/AgentHaloMacApp/build.sh`, `Resources/*.plist`, `AgentHaloCLIMusterPeerTransportOwner.swift:65`, `backend_product_pipe_v2.rs:93`, `muster_network.rs:213`, `apple_tls_broker_v2.rs:50`, `deploy/dev.linsheng.muster.nethelper.plist`
- vnet baseline: `internal/vnet/{bridge,frame,ctlproto}.go`, `internal/agentd/{vnet,vnetconfig,vnetpeer,forwarder}.go` (`192.168.14.0/24`, MTU 1280, manual SPKI → address mapping)
- ABI：`rust/include/agenthalo_core.h`、`agenthalo-ffi/src/lib.rs:83`、`security_v2.rs:421-529`
- Lock rejection: `agenthalo-backend/src/macos_claude_native_input.rs:200,400-412`
- Authorization branches: `agenthalo-backend/src/remote_authorization.rs`; `docs/history/architecture/agenthalo-network-authority.md`
- Terminal path: `docs/history/architecture/agenthalo-remote-terminal.md`, `AgentHaloLocalTerminalService.swift`, `terminal_bridge.rs`, `native/apple/AgentHaloTerminalAdapters/Sources/AgentHaloIterm2*`; unused `ios/Deps/MusterDeps` (swift-nio-ssh, SwiftTerm)
- Duplicate platform logic: files in the table in section 2.5
