# Muster Architecture: Desktop Applications → Agent Halo / Terminal Adapters → Networking + Hardware Security

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Status: **Target architecture, updated for the product-layering decision of 2026-09-28** (D12, see section 7). Section 2 and size figures retain the 2026-09-24 source baseline; section 8 distinguishes historical snapshots from this source review. Target capabilities do not imply implementation or successful device acceptance.

---

Decision records for subsequent changes are indexed in [docs/adr](adr/README.md).
Every repository change requires an ADR under [AGENTS.md](../AGENTS.md).

## 0. Overview

Product integration has three layers:

```text
┌──────────────────┬──────────────────┬──────────────────┬──────────────────┬────────────────┬────────────────┐
│ Codex Desktop    │ Claude Desktop   │ Catdesk Desktop  │ Chrome webpages  │ iTerm2         │ Ghostty        │
├──────────────────┴──────────────────┴──────────────────┴──────────────────┼────────────────┼────────────────┤
│ Agent Halo (Desktop AX / Browser AX + Locked Use)                         │ iTerm2 adapter │ Ghostty adapter│
├───────────────────────────────────────────────────────────────────────────┴────────────────┴────────────────┤
│ Muster Networking + Hardware Security                                                                       │
└─────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

- **Desktop application layer**: Existing desktop applications and Chrome pages are integration targets. Codex, Claude, and Catdesk desktop sessions belong to Agent Halo; iTerm2 and Ghostty terminal sessions each use their own adapter.
- **Integration layer**: Agent Halo owns Agent session viewing, Accessibility control, and the Locked Use operation lifecycle. The two terminal adapters sit alongside Agent Halo, map their terminals' own capabilities, and do not depend on Agent Halo's AX control path.
- **Independent CLI adapter**: Muster embeds the Orca-inspired CLI session manager as Shared Rust, alongside Agent Halo and terminal adapters. It owns native Codex app-server / Claude stream-json children, durable journals and prompt state. There is no Node/Electron runtime or separate Orca installation. The existing network, authorization and native UI remain shared; see [ORCA_ADAPTER.md](ORCA_ADAPTER.md).
- **Foundation**: Muster Networking + Hardware Security provides unified hardware identity, pairing and rosters, encrypted connections, reconnection, authorization, Presence verification, and revocation. Upper layers do not create separate connection stacks, trust models, or hardware-key management.

Chrome web accessibility integrates with Agent Halo through a JS extension + Rust native host. Remote calls reuse musterd RPC and independent browser scopes. Locked operations require Locked Control + Presence; CDP keeps the screen locked without temporary unlock. See [Browser capability](../src/browser/agenthalo-extension/README.md).

Muster's native macOS / iOS and other UIs are **clients** of these capabilities, not the controlled desktop applications in the diagram. UIs consume services through Shared Rust ViewModel / Intent / Effect interfaces. Phones do not offer Agent / Terminal services; they show the complete device roster only in network settings. Rust projects service-capable desktops into the service-device list, with connection state, capabilities, and authorization represented separately.

These are product responsibilities, not a requirement for one process or crate per layer; runtime and code locations are in §3.2 and §3.9. Agent Halo names a logical capability in the new architecture and does not revive the frozen `agenthalo-backend` / `agenthalo-ffi`.

The old implementation on 2026-09-24 distributed these responsibilities across **3 trust models × 5 transport paths × 4 languages × 7+N processes × 499 C ABI functions**. Four main causes drove complexity:

1. Versions were added without removing predecessors;
2. The threat model exceeded product needs;
3. Process and language boundaries were treated as security boundaries;
4. The Rust core did no I/O and had no runtime, forcing state loops into each platform.

**Recommendation: rebuild incrementally with the strangler pattern instead of continuing to patch.**

- **Networking**: Hardware keys + signed membership roster + one QUIC link carrying AgentHalo service streams, port-forwarding streams, and vnet IP packets.
- **Integration**: Agent Halo and iTerm2 / Ghostty adapters compose within Rust `musterd`; business state machines, protocols, and capability decisions stay in Shared Rust, while platforms execute OS APIs only.
- **Native layer**: Retain Views only; Rust `app-core` supplies all state and logic as ViewModels.
- Every phase must delete the legacy code it replaces.

Target metrics:

| Metric | Old implementation baseline, 2026-09-24 | Target |
|---|---|---|
| Persistent macOS processes | 5 persistent + 2 persistent child processes + one `muster connect` per peer + `product-pipe`, including 1 root process | **3** baseline runtime processes: App, `musterd`, root network helper; Locked Use additionally has a system authorization plugin and independent watchdog (see LOCKED_USE.md); do not merge security responsibilities merely to reduce process count |
| Trust models | 3 (Go SPKI ledger / Owner-Grant-Federation / Device Network distribution) | **1** (signed membership roster, including vnet addresses and port permissions) |
| Device-to-device transport | 5 (Go WSS+yamux+MST1, Rust WS+TLS+MST2+HTTP, Swift yamux+MST1, Go direct QUIC, vnet over yamux) | **1** QUIC link: services/forwarding use QUIC streams; vnet uses QUIC DATAGRAM; path is direct or Relay |
| Exported C ABI functions | 499 (3372-line header, ABI 1.102.0) | **≤ 15** (message-based ABI) |
| Platform state machines / retries / merge logic | Implemented separately on 4 platforms | **0** |
| Languages | Rust + Go + Swift (4 helpers) + platform UI languages | Rust + platform UI languages + small Apple OS shims (SEP, Apple Events, root network helper) |

---

## 1. Product Requirements (the New Architecture's Scope)

| # | Requirement | Acceptance criteria |
|---|---|---|
| R1 | **Hardware identity network**: Every device has one nonexportable hardware P-256 key (SEP / StrongBox / HUKS / TPM). Device traffic is end-to-end encrypted; Relay is untrusted. | Compromising Relay can cause at most DoS; device private keys cannot be exported; sessions disconnect within ≤ 2 seconds after revocation |
| R2 | **Enrollment**: Pair using QR or short codes. The creator only approves enrollment; enrolled members access one another as peers. Any device, including the creator, going offline leaves other members unaffected. | Remaining members can still access one another after creator shutdown, including after their own restarts |
| R3 | **vnet (desktop)**: Every desktop member has a stable virtual IP that **any third-party software**, including SSH and VNC (macOS Screen Sharing), can use directly. Authorize which device may access which ports on which device. **No phone VPN**: phones cover only in-App traffic; a built-in SSH client (Rust, hardware-key login) accesses the same ports through forwarding streams. No phone VNC. | Another Mac connects to a remote Mac's virtual IP using standard SSH / VNC clients; a phone opens the remote Mac's SSH session in-App; both direct and Relay paths work |
| R4 | **Remote Agent viewing and control**: Agent Halo views and controls Codex / Claude / Catdesk Desktop sessions, sends prompts, handles approvals, and interrupts; each application uses an independent profile. NativeApp starts locked and uses short-lived authorization to temporarily unlock, cover, operate, then relock. | Each Desktop application separately completes view → send → approve → observe result on a phone; unimplemented capabilities are explicitly unavailable. Displays are covered during temporary operations and Mac relock is confirmed after each completion; see LOCKED_USE.md |
| R5 | **Remote terminal work through independent adapters**: iTerm2 and Ghostty integrate separately with the same terminal service, offering discovery, creation, input, close, and output viewing according to their capabilities; prioritize a complete iTerm2 terminal experience. | iTerm2: LAN echo < 100 ms, pushed output, usable while locked. Ghostty: individually verify declared control capabilities; do not claim output reading without a source. Accept each independently |
| R6 | **Rust business logic, native UI**: Platform code does not reconstruct business state. | No state machines, retries, merging, or authorization decisions in platform code |

Out of scope (may be added later, but must not shape the core architecture):

- Phone VPN / Packet Tunnel (decided: third-party phone Apps do not use vnet);
- Phone VNC / remote display (decided);
- Exit nodes / subnet routing (vnet routes only the members' virtual address ranges and does not take over the default route);
- Multiple networks;
- Federation quorum;
- Defense against same-UID local attackers (decided);
- Browser access as a Muster web client (excluding Agent Halo's Chrome web accessibility adapter);
- A custom pixel-level remote-desktop protocol (VNC provides this).

---

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

---

## 3. Target Architecture

### 3.1 Design Principles

1. **One Rust runtime owns all state.** Desktop uses daemon `musterd`; mobile uses in-process `app-core`. Both use tokio and perform network I/O themselves.
2. **Platforms provide only three things**:
   - Render ViewModels and return Intents;
   - Execute OS Effects requested by Rust, such as hardware signing, biometrics, scanning, and notifications;
   - Provide OS-only capabilities such as desktop TUN, AX, Apple Events, and power assertions.
3. **One trust model**: Device keys + signed membership roster. Service access and which device may access which ports on which device depend only on the roster and target-local policy.
4. **One link, three uses**: One QUIC connection between devices. AgentHalo services use QUIC streams; port forwarding (the phone's in-App SSH client) also uses QUIC streams; desktop vnet IP packets use QUIC DATAGRAM (RFC 9221, unreliable, without head-of-line blocking, suitable for tunnels).
5. **Relay is dumb**: Rendezvous, forwarding, and mailbox only; no authorization semantics.
6. **Accept locked control separately by channel**: Verify Provider API, iTerm2 API, and native-App AX independently; do not assume AX windows remain visible while locked. Native-App Locked Use follows the 2026-09-26 requirements for short-lived signed authorization, system authorization plugin, temporary unlock, cover, and relock; see [LOCKED_USE.md](LOCKED_USE.md). Rust authorization/lifecycle is wired to daemon, platform adapters, and phone signing. Codex's normal path passed two consecutive sends and message-display acceptance on 2026-09-28; failure scenarios and other applications still need separate verification. Desktop pixel-level remote access uses VNC over vnet; phones do not view the display.
7. **No compatibility layer; clean cutover** (decided).
8. **Every phase deletes replaced code**, with deletion volume an acceptance item.

### 3.2 Product Responsibilities and Runtime Mapping

Section 0 shows product integration. The runtime implementation below composes Agent Halo and terminal adapters in `musterd` over a shared foundation; adapters do not belong in platform App business layers.

| Product responsibility | Shared Rust location | Thin OS-layer responsibility |
|---|---|---|
| Agent Halo: Chrome web accessibility | `muster-browser` request types, scheduling, deadlines, AX projections, and native host; local calls through `muster browser` | `src/browser/agenthalo-extension` only adapts Chrome tabs/debugger/nativeMessaging APIs; browser APIs do not decide authorization. musterd checks remote browser scopes and Locked Control + Presence; locked operations need no temporary unlock |
| Agent Halo: Codex / Claude / Catdesk Desktop | `muster-agent` sessions, viewer, NativeApp controller; authorization state and execution orchestration in `muster-core::locked_use` and `musterd::locked_use` | `muster-platform::macos` performs AX, lock observation, and input monitoring; system plugin, cover, and watchdog execute restricted Effects and cannot grant permission independently |
| iTerm2 adapter | `muster-terminal` API protocol, sessions, incremental output, and input semantics | Apple Events obtains cookies |
| Ghostty adapter | `muster-terminal` capability declarations, sessions, and operation semantics (pending migration) | `MusterMacShim` performs ScriptingBridge calls (pending migration) |
| Muster Networking + Hardware Security | `muster-core`, `muster-net`, `muster-rpc`, `muster-store`: identity, roster, policy, links, authorization proofs, operation deduplication | OS-only capabilities such as hardware-key creation/signing, user biometrics, and TUN |
| Muster clients | `muster-app-core` / `muster-ffi`: connections, subscriptions, projections, Intent / Effect | Native UI rendering and user input |

Agent Halo's Rust orchestration binds Locked Use operations/turns and relocks using foundational identity and authorization proofs. Network online state, device enrollment, or platform authentication-success booleans cannot replace operation authorization. Terminal adapters do not implicitly trigger temporary unlock for ordinary input; operations requiring it must explicitly declare that capability and pass through the same authorization chain.

```
┌──────────────────────────── Native UI (one per platform) ─────────────────────┐
│ SwiftUI (shared macOS+iOS)  Compose (Android)   ArkUI (HarmonyOS)   WinUI/Win32 │
│   render(ViewModel) · send(Intent) · perform(Effect) · terminal grid rendering │
└──────────────────────────────────┬─────────────────────────────────────────────┘
                                   │ Message-based C ABI (≤15 functions, protobuf messages)
┌──────────────────────────────────▼─────────────────────────────────────────────┐
│ muster-app-core (Rust, shared by all clients)                                 │
│   ViewModel projections (devices/sessions/details/terminal/pairing) · Intents · subscriptions │
│   Built-in client: SSH (russh, hardware-key signing) → peer via forwarding stream │
│   Desktop: UDS → local musterd (connects to peers)  Mobile: in-process PeerLink → peer musterd │
└──────────────────────────────────┬─────────────────────────────────────────────┘
                                   │ muster-rpc (same schema locally/remotely) + forwarding streams
┌──────────────────────────────────▼─────────────────────────────────────────────┐
│ musterd (Rust daemon, desktop/server)                                        │
│  Integration: Agent Halo (AX + Locked Use) · iTerm2 adapter · Ghostty adapter  │
│  Shared services: agent · terminal · forward · device · network               │
│  Authorization: roster scope × target-local policy × high-risk Presence signature │
│  OS adapters: AX · iTerm2 API · Apple Events · power assertions · lock state · SEP │
└──────────────────────────────────┬─────────────────────────────────────────────┘
┌──────────────────────────────────▼─────────────────────────────────────────────┐
│ muster-core / muster-net (Rust, shared by clients and daemons)                │
│   Identity (hardware-signing trait) · Roster (signed membership) · Pairing     │
│   PeerLink(QUIC): streams → services / forwarding / gossip   DATAGRAM → vnet  │
│   vnet (desktop only): TUN I/O · source validation · port filtering · name resolution │
└──────────────┬───────────────────────────────────┬─────────────────────────────┘
               │ utun fd (via XPC)                  │ Direct UDP (LAN/hole punching) or Relay
      ┌────────▼─────────┐               ┌─────────▼─────────┐
      │ muster-nethelper │               │ muster-relay (dumb) │ Rendezvous · datagram forwarding · mailbox · STUN
      │ root, ≈500 lines │               └─────────────────────┘
      └──────────────────┘
  Desktop: SSH / VNC / any software ──TCP/UDP──▶ virtual IP ──▶ utun ──▶ musterd ──▶ PeerLink
  Phone: in-App SSH client ──forwarding stream──▶ peer musterd ──▶ 127.0.0.1:22
```

### 3.3 Process Model

| Platform | Processes | Notes |
|---|---|---|
| macOS | `Muster.app` (UI + app-core, bundle id `dev.linsheng.muster`, same name/id on iOS) + `musterd` (user LaunchAgent in the Aqua session) + `muster-nethelper` (root LaunchDaemon) | Register both daemons from the App bundle through `SMAppService` (`.agent` / `.daemon`), with one-time user approval in System Settings. `musterd` must run in the user's GUI session to call AX on its Apps, use iTerm2 API, and keep running while locked. **No bridge, SEP helper, TLS broker, or NetworkExtension (decided).** |
| iOS / Android / HarmonyOS | App only, with in-process `app-core` | **No VPN extension (decided).** AgentHalo uses in-process PeerLink; the built-in SSH client reaches peer `musterd` through forwarding streams. Third-party phone Apps cannot use vnet. Phones dial out only and offer no services, needing neither a listener nor the passive side of hole punching. |
| Windows | UI + `musterd` (user session) + network service (SYSTEM, wintun) | UI Automation is the Windows AX equivalent; TPM uses NCrypt |
| Linux (server) | `musterd` (systemd `--user`) | An administrator precreates a persistent TUN owned by the user (`ip tuntap add mode tun user <u>`); `musterd` needs no root |
| Relay | One `muster-relay` process | See 3.5 |

**The macOS root helper performs only these privileged OS operations**:

1. Create utun;
2. Configure addresses and MTU;
3. Add routes only for the vnet address range;
4. Write `/etc/resolver/<domain>` (optional, for name resolution);
5. Install/replace the Locked Use system authorization plugin and its fixed authorization rules from the signed App. Rust daemon orchestrates this operation, with fixed audit-token caller verification and no arbitrary command/path interface; see [LOCKED_USE.md](LOCKED_USE.md#6-installation-and-recovery).

The App-managed service is `dev.linsheng.muster.helper`, registered with `SMAppService.daemon` and built from `src/native/apple/NetHelper`. Retain the manually installed `dev.linsheng.muster.nethelper` until network migration finishes; its continued operation does not prove the new plugin-installation capability is ready. Its source left the tree on 2026-10-02 (§8.5).

It then passes the utun fd to `musterd` through XPC and **never reads or writes packets itself**. Caller verification reuses the existing implementation (audit token + SecRequirement for the fixed team ID and `musterd` identifier). `musterd` calls it directly through the libxpc C API (as the existing backend already calls tls-broker), eliminating `muster-net-bridge`.

**SEP signing**: CryptoKit SEP blobs are accessible only from Swift, so use an approximately 200-line Swift static library (`@_cdecl` exports `create` / `sign` / `public_key`) linked into `musterd` for in-process signing. Same-UID attackers are explicitly outside the threat model, so broker isolation is unnecessary.

### 3.4 Identity and Trust: One Model

**Keys** (all nonexportable hardware P-256):

| Key | Purpose | Platform implementation |
|---|---|---|
| Device Key | Mutual QUIC / TLS handshake authentication; admin roster-entry signing; directly used as the phone SSH client key (`ecdsa-sha2-nistp256`, decided) | SEP / StrongBox (fallback to TEE) / HUKS / TPM |
| Presence Key (optional, one per device) | Biometric ACL; signs high-risk operation intents: locked control, adding/removing members | As above, with `.biometryCurrentSet` / `setUserAuthenticationRequired` |
| Storage Key | ECIES wrapping of the local database DEK (reusing the existing sealbox approach) | SEP key agreement / Keystore |

`DeviceId = SHA-256(SPKI)`; display uses a short base32 fingerprint (reusing `enroll.Fingerprint`).

Presence Key signatures replace `LAContext` booleans. Targets verify cryptographic proof rather than platform-supplied `authenticated=true`. This implements AGENTS.md's prohibition on substituting native authentication booleans.

**Network = signed membership roster (Roster)**

```
NetworkId = SHA-256(Genesis entry)

Entry {
  network_id, parents: [EntryHash], lamport: u64,
  body: Genesis{name, vnet_cidr, creator: DeviceRecord}
      | Add{DeviceRecord}
      | Update{device_id, name?, role?, scopes?}
      | Remove{device_id}
  signer: DeviceId, sig: ECDSA-P256(domain="MUSTER-ROSTER-V1", canonical(entry))
}
DeviceRecord { device_id, spki, presence_spki?, name, kind: desktop|phone|server,
               role: admin|member, vip: IPv4?, scopes: Set<Scope> }
```

Roster rules:

- **The entry set only grows (2P-set)**. Roster state is a deterministic fold of valid entries:
  - Remove wins permanently; rejoining requires a new key;
  - Update selects the maximum `(lamport, hash)`;
  - An entry is valid only if its signer is an admin in its causal past;
  - Entries signed by a removed admin are invalid unless they are in the Remove entry's causal past, preventing stolen admin keys from backfilling entries.
- **Virtual addresses**: Genesis defines `vnet_cidr`, default `192.168.13.0/24`. An admin can sign `Readdress` for an existing network, atomically updating the subnet and all desktop/server addresses while preserving network identity, member keys, roles, and permissions. Restart desktop daemons afterward to apply the interface. The admin signing Add assigns desktop/server `vip`; phones have no vnet and an empty `vip`. If two admins concurrently assign the same address, the lower entry hash wins, and a subsequent Update readdresses the other device.
  - This eliminates manually maintained mappings in each machine's `agent.yaml`.
- **No expiry or renewal**, so creator unavailability affects no one (R2).
- **Admin is a role, not a single point of failure.** The creator is an admin in Genesis and may promote other desktops. Post-enrollment permission governance allows any admin to sign Update / Remove.
- **Distribution**:
  - Exchange roster heads on every PeerLink establishment and fetch missing entries;
  - Any member can upload entries to the Relay mailbox (key = NetworkId), fetched on cold start;
  - Relay may withhold entries (DoS), but cannot forge them.
- **Revocation**: On Remove, immediately close all links, service streams, and forwarding streams for that device and clear its vnet connection-tracking entries. For inbound connections, synchronize with the Relay mailbox first if the roster was last synchronized more than N seconds ago.

Desktop Presence may also be approved after pairing using an admin-signed
binding anchored to an empty DeviceUpdate epoch. This retains the v1 roster
wire format and existing identity/address; targets verify the binding against
the accepted epoch in Shared Rust. See [ADR 0006](adr/0006-desktop-presence-and-console-routing.md)
and [Presence enrollment](PRESENCE_ENROLLMENT.md).

**Scopes** (roster member-level permissions: what this device may do to other devices as an accessor; since 2026-09-29 admins configure each device under Chats / Agents / Terminals):

| Module | Scope | Capability |
|---|---|---|
| Chats | `chats` | Device-level switch; when off, all providers below are ineffective, but selected providers remain stored |
| | `chats:<provider>` | Ordinary desktop-App chats (`chatgpt`, `claude`, not their coding Agents). Service not yet implemented; authorization comes first |
| Agents | `agents` | Device-level switch, as above |
| | `agents:<provider>` | Agent sessions for that provider: viewing, preview, files, git, sending, approval, interruption (`codex`, `claude`, `catdesk`) |
| | `locked.control` | Device-level switch for control while locked; requires **both** target-local policy and a Presence signature. Also covers locked Chrome operations and cover |
| Terminals | `terminals` | Device-level switch: view/input all terminal sessions, create, close, resize |
| Other | `browser.view` / `browser.control` | Read/control Chrome pages |
| | `net:<proto>/<port>` | Target ports accessible to the member, e.g. `net:tcp/22` (SSH), `net:tcp/5900` (VNC / Screen Sharing), `net:*` (all). **Desktop vnet filtering and phone forwarding streams share this permission** |
| | `network.admin` | Sign roster entries (equivalent to role=admin) |

- Providers are stored as text (`[a-z0-9-]{1,32}`). Unknown providers still parse and sort consistently, but match no sessions; adding providers does not require upgrading the whole network first.
- Agent requests are evaluated against their session's provider; lists and full push updates are provider-filtered, and narrowed authorization ends existing push streams on the next event. Locked Use continuously checks the provider during challenge and execution.
- Legacy scopes `agent.view`, `agent.control`, `terminal.view`, and `terminal.control` remain parseable and retain their original all-provider meaning, but are no longer offered. Management lists them as "Earlier grants" and can remove them when saving new settings.
- `muster-app-core` projects groups, labels, summaries, and enrollment defaults (all three modules and providers on; locked control and other items off) into `AccessView`; platforms only render it.

**Target-local policy** (local configuration, not part of the network):

- Services offered locally;
- Ports exposed to the network (e.g. only 22 and 5900), filtering both inbound vnet packets and forwarding streams;
- Whether locked control is permitted;
- AX-controllable App allowlist: Codex, Claude, and Catdesk only after its independent profile and identity verification are complete.

One authorization function is used by service streams, forwarding streams, and vnet packet filtering:

```
allowed = roster.active(subject)
        && roster.scopes(subject) grant permission   // module providers require their switch
        && local_policy.allows(permission, target_state)
        && (!permission.high_risk || presence_proof.verify(subject, op))
```

`net:` authorization establishes only which device is connecting and whether it can reach that port. Application authentication, such as SSH keys and VNC passwords, still applies normally.

**Pairing**:

- **QR (primary)**: An admin displays `{network_id, admin_device_id, admin_spki, relay_url, invite_secret(128bit), expiry}`.
  1. The new device generates hardware keys, connects to the admin through Relay, and pins the peer using `admin_spki` from the QR.
  2. It proves possession of the invitation with `HMAC(invite_secret, TLS exporter)`.
  3. The admin UI shows Add 'xxx's iPhone' (phone), permissions: … (also assigned virtual IP for desktops); the user confirms with the Presence Key, then signs Add.
  4. The new device receives the complete roster; pairing finishes.
  
  The camera is itself a trusted out-of-band channel; **no SAS comparison is needed**.
- **No camera (desktop-to-desktop)**: Six-digit short code + PAKE (CPace / SPAKE2), also without SAS.

**Concepts entirely replaced by this model**:

- owner authority / owner registration / owner directory；
- GrantV2 / grant revocation；
- federation / authority orchestrator / authority pairing issuance；
- activation / freshness / sealed head / runtime readiness / protected state；
- first pair bootstrap / consumer enrollment / member enrollment / member listener bootstrap；
- device network distribution / enrollment / management / mobile join；
- network certificate / remote certificate (checkpoint, consumer, exchange, owner driver) / short certificate / TLS identity lifecycle / TLS broker;
- interaction binding ledger (replaced by operation ledger + Presence signatures);
- Go SPKI ledger and vnet mappings in `agent.yaml`;
- Single-network guard (one roster per device; simultaneous owner and consumer does not exist conceptually).

### 3.5 Network Transport: One Link, Three Uses

**PeerLink = one QUIC connection** (quinn + rustls TLS 1.3):

- **Inner device identity**: Mutual PeerLink certificate authentication. Device certificates are only self-signed hardware-public-key containers, freshly signed with Device Key on each startup; verify against roster SPKI / pairing pins, not WebPKI hostnames or certificate-lifecycle semantics. This does not apply to public Relay server certificates below.
- Custom verifier: the peer SPKI must be active in the roster. Signing uses the platform hardware-signing trait.
- **Paths**:
  - The same QUIC connection can migrate between direct UDP and Relay forwarding (QUIC connection migration);
  - Direct paths come from LAN candidates or STUN hole punching (port existing Go `quicpeer`, `stun`, and `DirectEngine` logic to Rust);
  - Relay forwards inner QUIC datagrams by DeviceId. Public access uses two secure transports at `muster.linsheng.dev:9443`: **UDP / QUIC DATAGRAM** and **TCP / TLS 1.3 length-prefixed frames**. Both maintain availability concurrently; Shared Rust selects using health and RTT with hysteresis, optionally preferring UDP when latency is similar, never ignoring health based only on protocol name. Automatic fallback to raw UDP, `ws://`, or legacy WSS is forbidden.
  - One end-to-end device identity chain and one revocation point; outer Relay TLS / QUIC authenticates only the public server, granting neither device membership nor business permissions. This replaces WSS + yamux + MST1 + MST2 + HTTP/1.1.
- **Service streams**: Every bidirectional QUIC stream begins with `ServiceHello{service, version}`, then length-prefixed protobuf messages (request/response + server push). Carries muster-rpc, roster gossip, the build heartbeat, and path negotiation. Check service authorization on stream opening and again for high-risk method calls.
- **Build heartbeat**: Each member link exchanges `{version, commit}` on the `heartbeat` service when it is adopted and every 60 seconds. `version` is the installed release (Velopack `sq.version` on desktops; the bundle version a phone app passes in its config); `commit` is `git rev-parse --short HEAD` compiled into musterd (`MUSTER_GIT_COMMIT` overrides). `MemberView` and `DeviceInfo` project them for Settings and device details. A peer's report is display only, never an authorization or compatibility input. A peer that does not know the service is not asked again on that link.
- **Forwarding streams**: Raw bytes follow `ServiceHello{service: forward, proto: tcp, port}`. Target `musterd` checks requester `net:tcp/<port>` and locally exposed ports, connects to its own `127.0.0.1:<port>`, and copies bidirectionally. This is the only path from the phone's in-App SSH client to desktop ports and replaces `muster connect -L`. Targets are restricted to the peer's own ports; no jump-host forwarding.
- **Packet plane (vnet, desktop only)**: IP packets go directly into QUIC DATAGRAM frames without retransmission or ordering; inner TCP handles those. This removes yamux-stream-over-TCP nesting on Relay paths. TCP / TLS Relay still has TCP head-of-line blocking, so routing must reflect actual health and RTT.
- **Domain resolution**: Shared Rust first uses RFC 8484 DoH for all Muster external hostnames, including TCP / TLS and UDP / QUIC Relay, with fixed bootstrap IPs and normal HTTPS certificate verification, without reading proxy environment variables. If all DoH requests fail or return no addresses, fall back to system-default DNS on the physical network interface. Apple uses `DNSServiceGetAddrInfo` with a specified `enN` / `pdp_ipN` interface index, never interface 0 or tunnel interfaces. Other platforms without physical-interface adapters still return resolution failure. Shared Rust owns fallback decisions, timeouts, and result validation; platforms only adapt system resolver APIs. System DNS may be plaintext, but Relay TLS hostname/certificate and device-identity verification remain unchanged, and synthetic `198.18.0.0/15` addresses are rejected. A/AAAA and CNAME use standard DNS codecs; DoH caching respects TTL, and fallback results are not separately cached. Network changes and registration failures re-resolve, and changed addresses trigger registration recovery. Numeric IPs and local Unix sockets need no DNS. Business data still uses QUIC/Relay; DNS carries resolution only.
- **Presence (2026-10-10)**: whether a member is online comes from evidence in Shared Rust (`muster_net::presence`), not from the instant state of one link. `MemberView.connected` and `presence` project it; Apps only render them.
  - A link coming up, traffic from the member, or the relay's `PRESENT` is online evidence.
  - A lost link stays shown online (`reconnecting`) while at most two retries run after the first reconnect; only when all fail is the member `unreachable`.
  - The relay's `ABSENT` (the member has not been registered for longer than its 10 s re-registration grace) makes it `offline` at once, unless the member's own traffic within the last 2 s is fresher. The dead link is closed and the relay is asked (`WATCH`) for one `PRESENT`.
  - An offline member is not dialed or polled, and every concurrent request fails at once with a definite "not sent". It is tried again only on new evidence (`PRESENT`, its traffic, a link it opens) or a user refresh (`NetworkRefresh`). An unreachable one is also probed once a minute, and again after a network change or a return to the foreground.
  - This device's own outage is not evidence about anyone: while the relay is registering or unreachable, or the app is in the background, failed dials do not count, and lists show `local_offline` instead of members going offline.
  - A member this device never dials (a phone, seen from a desktop) is unreachable 30 s after losing its link.
  - Offline and unreachable members' sessions are hidden from every list, including cached and aggregated ones. Their cached data stays on disk and is listed again after a fresh load once they are back.
  - Member links use a 15 s QUIC idle timeout with 5 s keepalives on each side. The old 3 s timeout turned every relay re-registration or radio pause on a phone into a disconnection, and the old 1 s keepalive woke a phone's radio for nothing.
  - A second connection from a member whose current link is older than the 3 s connect window replaces it: the member dials only when it has lost its link. Within the window, two connections are a simultaneous dial and the lower device id's wins.
- **Recovery**: Shared Rust maintains member connections; connection establishment and service admission together have a 3-second maximum, then enter `failed`. Retry automatically with 1, 2, 4, 8-second capped backoff. Network changes or foreground restoration clear stale RPC channels, re-register, and probe. Simultaneous dialing selects the surviving connection by DeviceId. Restore transport only, never automatically replay business writes. Relay registration also has a 3-second deadline; unanswered heartbeats trigger re-registration. Native UI only presents Rust connection state.
- **Mobile**: Rust directly uses sockets (tokio supports iOS, Android, and OpenHarmony). Platforms no longer supply raw TCP, WebSocket, or yamux callbacks; they only notify core to pause/resume on background/foreground changes.

Service streams, forwarding streams, and vnet datagrams share one link, one roster authorization model, and one revocation point, so this is not a second system. Service and forwarding streams directly receive the link-authenticated DeviceId without indirectly mapping source IPs back to devices.

**Public Relay security and routing (2026-09-28 decision; delivery status accepted separately)**:

- TCP 9443 and UDP 9443 use the same **Let's Encrypt fullchain and private key** covering `muster.linsheng.dev`. Clients use WebPKI to verify hostname, validity, and trusted chain, retaining hostname SNI even when connecting to a numeric DoH-resolved IP. Never reuse the device SPKI verifier, skip verification, or default production to a private CA.
- Both transports independently connect, register, maintain keepalives, and observe RTT. Connection, registration, and the first full-frame probe share a **3-second** total deadline; expiry marks failure and a healthy standby immediately takes over. Failed transports reconnect with background backoff. After recovery, collect health/RTT samples before switching with hysteresis, avoiding per-sample oscillation. If both fail, show unavailable truthfully instead of remaining connecting indefinitely.
- Health probes use `PROBE` / `PROBE_ACK` with u64 nonce, both 1400-byte zero-padded frames. Health requires the first full-frame probe after registration; probe again 5 seconds after each success, with a 3-second matching-response deadline. ACTIVATE REFLECT does not update RTT. Reject incorrect lengths or nonzero padding; small registration packets do not establish usable data MTU.
- Shared Rust owns routing, deadlines, and lifecycle; Apps only display state. Transport switching neither regenerates nor replays writes; ambiguous operations remain unknown in the ledger. Rebuilding registration and keepalive is not resending Agent input.
- Both transports authenticate the same DeviceId, but maintain separate registration routes; standby registration/keepalive cannot overwrite the active return path. When selected `(carrier, generation)` changes, send `ACTIVATE{id}` (`0x0a`); only authenticated routes may assert it, and the server confirms with REFLECT. Selection ticks every 100 ms; a full send queue retries on the next tick. Repeat the idempotent assertion every 1 second against QUIC control-packet loss. It selects only the return transport, granting no membership or business permission. The server forwards on one path and never duplicates business packets.
- **Phones (2026-10-04, after the iPhone heat investigation)** keep a lighter profile; desktops keep everything above.
  - No direct paths: relay only. There are no candidate exchanges or hole-punching PINGs.
  - One carrier at a time: QUIC first, and TLS only while QUIC cannot connect. A lost connection starts over with QUIC. A network change (every iOS path update, foreground return and silent push) verifies a live QUIC carrier with one probe and keeps it if answered; it restarts only a TLS carrier or a dead one. Resetting both carriers on each path update left no relay route for a second or more each time (2026-10-10).
  - No periodic PROBE: a carrier is live while its connection is. ACTIVATE is repeated only until the relay acknowledges it.
  - The relay QUIC connection uses 10 s keepalives and a 30 s idle timeout. The relay's own transport is sparse too. A desktop still gets its 3 s detection, because each connection uses the smaller idle timeout of its two sides and the desktop sends its own 1 s keepalives.
  - The trade-off is slower carrier failover on phones. The relay's change takes effect when the relay is redeployed.
- **Presence frames (2026-10-10)**. A carrier opts in with `FEATURES{bits}` (`0x0c`, bit 1). An older relay drops the frame and carries on. A newer relay sends the new frames only to carriers that asked, because an older client drops its carrier on a frame kind it does not know.
  - A FWD to a device without a fresh carrier for longer than `--absent-grace-secs` (10 s; the relay start counts as everyone's departure) is answered with `ABSENT{id}` (`0x84`), at most once per sender and destination every 2 s.
  - `WATCH{id}` (`0x0d`) asks for one `PRESENT{id}` (`0x88`) when the device registers, or at once if it already is. Watches last 10 minutes, 64 per client; clients renew theirs every minute.
  - Clients ignore relay frame kinds they do not know. Without these frames (an older relay) members become unreachable after their retries instead of offline.
- With an independent outer QUIC socket, Relay REFLECT describes only that socket and cannot be published as the inner PeerLink's public hole-punching address. Keep LAN direct candidates; WAN NAT hole punching requires same-socket probing or verified correct binding and cannot currently be claimed from REFLECT alone.
- After renewal, atomically update matching fullchain/key files and reload or restart both endpoints together; failures never fall back to plaintext. Only public 9443 is open, so issuance/renewal uses DNS-01 or existing centralized issuance/distribution without assuming 80 / 443 reachability. Record certificates, installation, public handshakes for both endpoints, switching, and real RPC separately; historical raw-UDP probes do not establish new-protocol acceptance.

**vnet (desktop/server only)**:

| Aspect | Design |
|---|---|
| Addresses | One roster `vip` per desktop (default `192.168.13.0/24`); **route only this range**, without default-route takeover or exit nodes |
| MTU | 1150 (`VNET_MTU`): the largest packet one DATAGRAM carries at QUIC's 1200-byte minimum path MTU, after a worst-case 50 bytes of 1-RTT and frame overhead. When quinn detects a black hole it falls back to that minimum. The former 1280 then lost full-sized packets silently, and TCP with Don't Fragment stalled. A packet that still does not fit earns its local sender an ICMP "fragmentation needed" with the size that fits |
| Outbound | `musterd` reads TUN packets, resolves destination `vip` to DeviceId through the roster, and sends via that device's PeerLink DATAGRAM; connects on demand if no link exists |
| Inbound | Verify source address equals the sender DeviceId's `vip` (existing per-packet anti-spoofing); then apply **port-level filtering**: new `(proto, dst_port)` must be granted by sender `net:*` scopes and local policy; established return packets use connection tracking |
| Names | `<device-name>.<network-domain>` resolves to `vip`. Optionally on macOS, `nethelper` writes `/etc/resolver/<domain>` pointing to `musterd`'s local DNS port (`resolver(5)` supports `port`) |
| Revocation | Effective Remove closes the link and clears that device's connection-tracking entries; active SSH / VNC sessions stop within 2 seconds |
| Phones | **No vnet**. All phone access to desktop ports uses forwarding streams; desktops need not access phone ports because phones offer no services |

**muster-relay (dumb, Rust, decided)**:

| Endpoint | Purpose |
|---|---|
| `register` | Device signs a challenge proving possession of its DeviceId private key. Relay may accept only configured NetworkIds by verifying the Genesis → Add signature chain itself; **no token needed** |
| `datagram` | Forward inner QUIC datagrams by DeviceId over public UDP 9443 QUIC DATAGRAM or TCP 9443 TLS length-prefixed frames |
| `mailbox` | Put/get opaque signed blobs, keyed by NetworkId or `H(invite_secret)`, for roster cold start and pairing rendezvous |
| `stun` | Return this probe socket's public address; an independent outer QUIC socket's reflected address is not an inner PeerLink hole-punching candidate |

Relay and clients share `muster-core` roster and codecs, so Relay can verify signature chains but cannot forge entries. It sees metadata (participants and traffic sizes), not content, and makes no authorization decisions. Existing special exceptions—owner-directory signature verification and GrantV2 binding—disappear entirely. Remove Go from the repository.

### 3.6 Service Layer

#### 3.6.1 muster-rpc: Identical Local and Remote Interfaces

App connections to local `musterd` (UDS) and remote `musterd` (PeerLink streams) use **the same schema and client code**. The local principal is the local owner, verified through the UDS peer uid; the remote principal is a roster DeviceId.

Desktop Apps also ask local `musterd` to open PeerLink streams to remote devices. Each desktop thus has one network endpoint and shared links for vnet, forwarding, and services.

```proto
service Agent {
  rpc ListSessions(ListReq) returns (SessionPage);
  rpc Watch(WatchReq) returns (stream SessionEvent);          // Push replaces preview polling
  rpc Send(SendReq) returns (OpResult);                       // op_id idempotency
  rpc Respond(RespondReq) returns (OpResult);                 // Approvals / structured questions
  rpc Interrupt(InterruptReq) returns (OpResult);
  rpc Files(FilesReq) returns (FilesPage); rpc Git(GitReq) returns (GitPage);
}
service Terminal {
  rpc Sessions(Empty) returns (stream TermSessionList);       // Push iTerm2 window/tab/session changes
  rpc Attach(AttachReq{session, since_rev}) returns (stream TermFrame); // Snapshot followed by row deltas
  rpc History(HistoryReq{session, line_range}) returns (TermLines);     // Fetch scrollback on demand
  rpc Input(InputReq{op_id, session, text}) returns (Ack);
  rpc Create(CreateReq{profile?, window?}) returns (TermSession);
  rpc Resize(ResizeReq) returns (Ack); rpc Close(CloseReq) returns (Ack);
}
service Device  { rpc Info(Empty) returns (DeviceInfo); rpc WatchState(Empty) returns (stream DeviceState); } // Lock state, online, capabilities, vnet state, exposed ports
service Network { /* Local or network.admin only: roster, invitations, approval, removal, scope changes, vnet switch, locally exposed ports */ }

// forward is a stream-level protocol, not RPC: raw bytes follow ServiceHello{forward, tcp, port} (see 3.5)
```

**One idempotency implementation**: `OperationLedger` (retaining existing JSONL ledger semantics).

- The client generates `op_id`; the server persists pending before any side effect;
- The same `op_id` returns the persisted result; the same `op_id` with a different body is rejected;
- After a crash, the state is Unknown, with no automatic replay.

The five journals become this one in `musterd`; clients store only unconfirmed op_ids.

#### 3.6.2 Agent Halo：Accessibility + Locked Use（R4）

The three Desktop targets share Agent Halo's service contract, with independent process-signature identity, session location, AX surface, and approval-control profiles per application. Catdesk is not treated as Codex or Claude merely because it supports a model protocol; its session-reading source, stable IDs, controls, and operations require separate implementation and validation. Read sources may be local transcripts or trusted application interfaces; AX writability does not imply readable history.

**2026-09-28 revision: Codex / Claude session operations are Accessibility-only.** Remove Owned provider CLI, app-server, stdio process launchers, and iTerm2 Agent CLI bridges; no automatic takeover, recovery, or fallback. Historical transcripts remain read-only observations; sessions without an AX controller are unwritable. CLI session-creation RPC / App Intent entries were removed with field numbers reserved, and all platforms remove their corresponding UI entries; sessions must first be opened in the desktop application. The independent terminal service retains terminal capabilities under terminal scopes and is not projected as an Agent session controller.

**2026-10-04 revision: cloud sessions beside the desktop ones.** Claude Code on the web and Codex cloud tasks are the same account's sessions running in the provider's cloud. The agent service lists, follows and (Claude) writes them through the provider's own API with the credentials its CLI keeps on the Mac (`muster_agent::cloud`, the profile's `[cloud]` table in [APP_PROFILES.md](APP_PROFILES.md)), so a phone sees them under the Mac next to its local sessions. They are sessions of their own kind (`mode = "cloud"`, `controller = "cloud"`): no process, window or working tree on the Mac, hence no desktop lock, Accessibility or Locked Use; a prompt is an ordinary ledgered `AgentSend` under the same `agents:<provider>` scope, and approvals, interrupts, question cards and files are refused by name and stay with the web app. This does not reopen the Accessibility-only rule above for desktop sessions: nothing here drives a desktop app, and a session bridged from the Mac itself is listed once, as the local session it is.

Runtime state and historical content are separate: `process_state` uses only the bound desktop process PID / kernel start time; missing binding or failed reads mean unknown, while confirmed exit / PID reuse means ended. `state` describes current-turn activity observed through AX; losing the AX surface means unknown, never restored from old transcript running events. A shared application process existing does not prove every historical session is executing.

Each session has **1 viewer and at most 1 controller**. Only NativeApp AX controllers are allowed, and ViewModels show the active channel to users:

| Layer | Implementation | While locked |
|---|---|---|
| **Viewer** (application-dependent) | Codex rollout / Claude transcript file observation (reusing incremental parsing in `codex.rs`), with file watching pushing `SessionEvent`; Catdesk reads the app's own SQLite store (`catpaw-memory.db`, read-only) and its local command socket for live status, project paths and the current session | Implemented for Codex / Claude; Catdesk implemented in source, device acceptance pending |
| **Controller: NativeApp (AX)** | Codex / Claude / Catdesk Desktop sessions opened in the GUI, controlled through AX: `AXValue` (Codex and Catdesk: selection replacement) enters text, `AXPress` clicks Send, approval, or interrupt | Codex normal path accepted; independent Claude acceptance and Catdesk device acceptance remain (below) |

**Principles and boundaries for locked operation**:

**2026-09-26 revision**: The early assumption that AX remains available while locked cannot serve as acceptance evidence.
The Claude chat "Mac 和 iOS 开发" (Mac and iOS development) recorded mac-b exposing menus but no thread window while locked, and becoming operable after unlock.
The user then explicitly required bounded temporary unlock through AuthorizationPlugin,
rather than treating a privacy cover as Locked Use. On 2026-09-28, Codex passed normal-path consecutive-send
and message-display acceptance; this does not generalize to all applications or failure scenarios. Implementation and outstanding boundaries follow
[LOCKED_USE.md](LOCKED_USE.md); ordinary `DeviceSetCover` does not supply this authorization chain.

**2026-10-10 manual mode**: The explicit signed device lease described in
[LOCKED_USE.md](LOCKED_USE.md#manual-device-cover-2026-10-10) can remain active
under the host watchdog independently of Accessibility and phone foreground
state. It preserves per-operation grants and lock-before-uncover cleanup.

- Agent sessions operate only through NativeApp AX; AX IPC alone does not imply locked writability. Accept locked usability of the independent iTerm2 terminal API separately.
- NativeApp temporary-unlock windows first require Rust capability verification and system-plugin authorization, then actual console, cover, and watchdog receipts to admit a bound operation. Cancellation, local input, revocation, disconnect, timeout, and task completion all enter relocking.
- Retain target-App signature identity, startup generation, empty-draft, and user-input-generation checks. Temporary unlock does not relax them or let other ordinary Agent requests borrow the window.
- If plugin/platform capabilities are unavailable, reject this control path. There is no CLI fallback, and the frozen old `macos_claude_native_input.rs` is not changed.
- For display viewing, use VNC over vnet from another desktop; phones do not offer display viewing.
- Power: use `IOPMAssertionCreateWithName(PreventUserIdleSystemSleep)` to keep the system awake while allowing display sleep, replacing `caffeinate -dimsu`.
- Ordinary privacy covers suit the independent unlocked-but-covered scenario. Locked Use additionally requires all-display readiness, continuous observation, nonce binding, and locking before uncovering; ordinary `covered` state is insufficient. Cover behavior combined with VNC still requires verification.

Approvals: Controllers normalize pending interactions to `PendingInteraction{id, kind, options}`; the UI renders only `options`, retaining the rule to display only provider-declared decisions.

#### 3.6.3 Terminal Service: Independent iTerm2 / Ghostty Adapters (R5)

The two adapters integrate as peers with `Terminal`, independently declaring discovery, creation, input, close, resize, output subscription, and history capabilities. Rust validates operations and projects UI using actual adapter capabilities. Unsupported actions cannot appear available, and successful input receipts cannot fabricate output. Each session binds an adapter and native session ID; do not guess or switch sessions across adapters by title. Both share foundational authorization, OperationLedger, and remote subscriptions without another App-local loopback service.

**Ghostty adapter** (2026-10-04, `muster_terminal::ghostty` over `muster_platform::macos::ghostty`): Ghostty 1.3 has no API like iTerm2's (no socket, no screen notifications). Each of its parts uses a different channel:

- **Discovery, create and close**: its AppleScript dictionary (`osascript`).
- **Input**: the `text:` binding action through `perform action`. This writes bytes to the program as typed, not as a paste.
- **Output**: the terminal view's accessibility text (`AXNumberOfCharacters`, `AXStringForRange`), polled every 300 ms into line deltas. Ghostty refreshes that text at most every 500 ms.
- **Resize**: none; the window keeps its Mac size.
- **Identifying a terminal's view**: accessibility shows the view only as the focused one. Attaching brings the terminal to the front and checks Ghostty's focused-terminal id before and after taking the view.
- **Permission checks**: Apple Events need the Automation permission for `musterd` to control Ghostty. It is checked without a prompt before every use; only the Mac's setup action may ask.

Accessibility shows no windows while the console is locked, so on a locked Mac a phone reads and types only inside a Locked Use terminal session (see [LOCKED_USE.md](LOCKED_USE.md)). `--terminal auto` picks iTerm2 when it is installed, else Ghostty when it is, else iTerm2.


**Move the iTerm2 client into `musterd`, implemented in Rust**:

- Connect over UDS to `~/Library/Application Support/iTerm2/private/socket`, upgrade to WebSocket, and exchange protobuf. Generate protobuf with prost from the vendored `api.proto` (iTerm2 3.7.0beta8).
- Match the existing Swift version (SwiftNIO, about 3.9K handwritten + 16.7K generated lines), with an estimated 2K lines.
- Result: remote terminals no longer depend on the App process running; delete the entire backend → App loopback terminal-service path.

**Event-driven, no polling**:

- Subscribe to iTerm2 notifications: `NOTIFY_ON_SCREEN_UPDATE`, `NOTIFY_ON_NEW_SESSION`, `NOTIFY_ON_TERMINATE_SESSION`, `NOTIFY_ON_LAYOUT_CHANGE`; optionally `NOTIFY_ON_PROMPT`, which can push command completion to phones with shell integration.
- On screen updates, use `GetBuffer` for the visible region with styles; `musterd` compares rows against the previous contents and pushes only changed rows and cursor.
- Multiple clients attached to one session share each fetch and maintain individual revisions.
- Fetch scrollback on demand through `History`, not with pushed frames.
- This replaces full-screen base64 + `if_revision` + backoff + eight echo polls and resolves the earlier document's remaining full-frame transfer during screen changes.

**Client rendering**: app-core maintains styled row grids, applies deltas, and emits ViewModels; native code only draws the grid and collects keys. Rust maps special keys (arrows, Ctrl-C, etc.) to byte sequences; platforms only report the pressed key. iTerm2 sessions provide styled rows, not a byte stream, so a full terminal emulator such as SwiftTerm is unsuitable; use a custom grid control, shared with SSH sessions below.

**Input, create, close, resize**: Map to `SendText` (explicit `suppress_broadcast`, retaining existing rules), `CreateTab`, `Close`, and `SetProperty(grid_size)` respectively. Input carries `op_id` for `OperationLedger` deduplication.

**Authentication (decided: Automation permission)**: iTerm2 API requires a cookie by default, requested by `musterd` through Apple Events (`request cookie and key for app named …`). Grant `musterd` Automation permission once; do not change iTerm2's global allow-all-Apps setting. Apple Events calls live in the Apple shim static library alongside SEP.

**Lock screen**: iTerm2 API is local socket IPC and works normally while locked.

**Phone SSH (in-App over forwarding streams; decided: Rust, hardware keys)**:

- `app-core` embeds an SSH client (`russh`), using forwarding streams directly as transport without local ports.
- The client key is the phone's Device Key (P-256, `ecdsa-sha2-nistp256`); private keys remain in SEP / StrongBox / HUKS, with signing through `hw_sign`. Depending on local policy, `musterd` can write roster public keys to an `authorized_keys` include file (optional, P6); otherwise users add them manually once.
- `muster-terminal`'s VT state machine parses SSH bytes into the same row grid, reusing iTerm2 terminal ViewModels and the custom grid. To users, iTerm2 and SSH sessions are simply two sources in the terminal list.
- This replaces unused swift-nio-ssh and SwiftTerm in `ios/Deps/MusterDeps`; Rust implements SSH once for all four platforms.

**Managed conversation terminals** (2026-10-23, [ADR 0033](adr/0033-managed-terminal-compatibility.md)):

The existing conversation workspace's dedicated tmux backend complements both
native adapters. Rust persists a generation-specific handle in the tmux session
and lazily reconstructs its PTY bridge after musterd restarts. Stale handles fail
instead of recreating or retargeting a shell. A view detaches; explicit End or
Restart destroys the session. Full frames carry shell-quoted commands for a
Ghostty/iTerm2 terminal or iTerm2 control mode on the host. Native windows are not
automatically merged by title/cwd. Desktop commands use `ignore-size`, retaining
Muster's viewport sizing in this phase; desktop-first ownership and native pane
mapping are not implemented by this slice. PTY output coalesces over 8 ms and
subscriber snapshots preserve other subscribers' pending updates.

**Other terminals**:

- **PTY**: Later-phase fallback only for Linux / Windows (ConPTY) and environments without iTerm2.
- **Desktop SSH**: Available directly through vnet (`ssh <device-name>`), requiring no dedicated development.

#### 3.6.4 Desktop App Profiles (Proposed, Pending Decision 6)

Codex, Claude and Catdesk differ mostly in data: where sessions are read from, what controls look like, what the app can do. On 2026-10-02, before the Catdesk slice, that knowledge was about 190 provider-specific branches across 21 files (`match provider`, `ChatApp::…`, `"claude" | "codex"` aliases, bundle ids repeated in `handler.rs` and `muster-platform`), and the same controls were matched twice: by closures over the live AX element (`native_app.rs`) and by functions over the platform-neutral `AxTree` snapshot (`desktop_chat.rs`). Adding an app means touching every one of them.

The proposal: describe each app once, as a declarative **profile** compiled into the binary, and keep one engine.

| Profile part | Describes | Examples from the three apps |
|---|---|---|
| Identity | Provider id, display labels, aliases, bundle id, signing requirement, executable, main-process rule, scopes `agents:<id>` / `chats:<id>` | Catdesk's helper processes run the same binary; its GUI pid comes from its command socket |
| Session source | Where history is read and how records become messages: rules matching record fields, emitting role, kind, text, tool name, call id, time; named Rust hooks for what rules cannot say (usage, queued prompts, receipts) | Codex rollouts and Claude projects (JSONL), Catdesk's SQLite store, desktop Chat mode (AX) |
| Runtime source | Live status, project path, current session | none (Codex, Claude); Catdesk's owner-only command socket |
| Surface | Selectors: role, label sets per UI language, subrole, URL pattern, ancestor and uniqueness constraints, for the composer, send, stop, approvals, session area and session key, sidebar rows, mode switch, new chat, obstructions | Catdesk: "发送"/"Send" and "停止"/"Stop" share one slot; popouts carry `sessionId` in the URL, the main window does not |
| Strategies | A closed set implemented once in the engine: draft by `AXValue` or by selection replacement; send button always present, only after text, or shared with Stop; session key from URL, title or the runtime's current session | Catdesk's Lexical composer needs selection replacement |
| Capabilities | send, respond, interrupt, questions, queued send, chat mode, history, model options. Undeclared means rejected by Rust and shown unavailable | Catdesk has no queued send |

What a profile can never change, because it lives only in the engine: process signature and start-time binding, the user-input generation check, empty-draft and exact-echo checks, `control_guard`, a single press per delivery attempt, no replay after an ambiguous write, receipts confirming sends, the Locked Use authorization chain and relock, and the target's local AX allowlist. A profile can only narrow what the engine allows.

Form: data compiled into the binary and never read from disk at runtime, so the signed binary stays the only source. The pilot wrote it as Rust `static` values in `muster_agent::profile`; since 2026-10-04 each app is one TOML file in `muster-agent/profiles/` (schema: [APP_PROFILES.md](APP_PROFILES.md)) that the crate's `build.rs` validates and compiles into those same statics, plus one `App` aggregate per app (identity, process, session-key and source strategies, lock name, capabilities, controls, Chat mode) and the `APPS` list the daemon builds its scan, queues, locks and resolvers from. One selector interpreter runs over an `AxQuery` trait implemented by both the `AxTree` snapshot and the live macOS element, so selectors are tested on Linux against AX trees recorded with `muster debug ax-dump`.

Migration, each step behavior-preserving and started after the Catdesk slice landed: (1) one app registry for identity, replacing the string branches (done 2026-10-03: `muster_core::apps` holds each app's provider, labels, Chat mode, bundle id and signing team; signing requirements, Locked Use provider checks, the default AX allowlist and access labels read it); (2) selectors as data, proven against recorded AX fixtures (2026-10-03: `muster_agent::profile` describes, as data matched by one `Selector` over live elements and recorded trees, each app's agent-conversation composer, send, stop and approval controls, and the Claude and ChatGPT Chat mode's send, stop, new chat, dialog close and settings exit; the profiles also choose strategies (drafting, session list, Chat-mode page, conversation URL, mode switch, history rows) and declare the queue and question cards, so muster-platform and `desktop_chat` no longer branch on the provider for them; fixtures recorded on Mac-B are in `muster-agent/tests/fixtures/ax`, redacted by `scripts/redact-ax-dump.py`. Done 2026-10-04: the profiles moved to TOML compiled by `build.rs`, and the daemon's per-app enums (`DesktopApp`, three queue fields, the resolver branch, the three `AppProfile` statics in muster-platform) became lookups over `profile::APPS` keyed by the profile's `session_key`, `gui` and `lock`); (3) session-source rules plus hooks (done 2026-10-04 for the two JSONL sources: `[source]` in `claude.toml` and `codex.toml` is the record grammar, which records are messages, tool calls, tool results and turn boundaries and where their fields are, compiled into `profile::Records`; one engine, `muster_agent::jsonl`, reads both transcripts over it and replaces `CodexViewer` and `ClaudeViewer`, and receipts and recency classify records through the same grammar; the named hooks per source are text cleaning, title records, turn usage and Claude's prompt queue. Still code: discovery and head metadata, the CatDesk store reader, question-card protocols, Chat mode's model and effort pickers, the two resolver implementations); (4) declared capabilities (done 2026-10-04: `[capabilities]` in each profile; `musterd::handler` rejects an undeclared send, respond, interrupt, question-card or queued-message operation before any lock or ledger entry, and `AgentChatOptions` without `model_options`; `muster-app-core` projects `can_interrupt`, `can_choose_model` and `can_browse_files` from them, keeping a provider this build has no profile for visible); (5) the next app as a profile only. Any step that changes how a control is found is re-accepted on devices per [LOCKED_USE.md](LOCKED_USE.md); a profile does not replace investigating a new app's data sources and controls.

### 3.7 Rust ↔ Native Boundary: Message-Based ABI + ViewModel

Follow Crux (Red Badger)'s core/shell pattern. Platforms are shells doing only three things:

```c
// Complete ABI (illustrative)
MusterCore* muster_core_new(const uint8_t* config, size_t len, const MusterPlatform* platform);
void      muster_core_intent(MusterCore*, const uint8_t* intent, size_t len);        // User actions
void      muster_core_effect_result(MusterCore*, uint64_t effect_id, const uint8_t*, size_t);
void      muster_core_lifecycle(MusterCore*, uint32_t event);                         // Foreground/background/network changes
void      muster_core_free(MusterCore*);
void      muster_bytes_free(uint8_t*, size_t);

typedef struct MusterPlatform {   // Rust → platform
  void* ctx;
  void (*on_view)(void* ctx, const uint8_t* view_model, size_t len);       // Screen ViewModel update
  void (*on_effect)(void* ctx, uint64_t id, const uint8_t* effect, size_t len); // Platform execution required
  void (*on_stream)(void* ctx, uint64_t stream, const uint8_t* bytes, size_t len); // High-frequency data such as terminal deltas
  int  (*hw_sign)(void* ctx, uint32_t key, const uint8_t* msg, size_t len, uint8_t* sig, size_t* sig_len);
} MusterPlatform;
```

- This ABI is only for App shells. `musterd` is a pure Rust executable obtaining OS capabilities (utun fd, SEP, Apple Events) in-process through linked shim libraries or XPC, not this ABI.
- **Intent**: `OpenDevice(id)`, `SendPrompt{session, text}`, `Approve{interaction, option}`, `ScanResult(qr)`, `TerminalKey{…}`, `OpenSsh{device}`, `SetVnetEnabled(bool)` (desktop), etc.
- **Effect** (OS-only capabilities):
  - `CreateHardwareKey`, `PromptBiometric{reason}` (returns a Presence signature, not a boolean);
  - `ScanQR`, `Notify`, `OpenURL`;
  - `SecureStore{get,put}` (only when platform keychains are required).
- **ViewModel**: One per screen, containing all display text, states, available buttons, and their Intents. Platforms only lay it out.
  - Available / Needs Attention classification, `displayState`, multidevice merging, pagination, online state, and vnet connection state all belong in Rust;
  - Platforms no longer contain `*Coordinator`, `*Lifecycle`, `*StateStore`, retry constants, or polling timers.
- **Schema**: protobuf generates Swift, Kotlin, and ArkTS (protobufjs / ohpm) types. All four platforms share one ABI; HarmonyOS needs no separate NAPI business layer, only an approximately 300-line NAPI shim.
- **Compatibility**: ABI functions stay fixed; capabilities evolve through schema fields, without individually appended 1.x versions.

Desktop Apps are also `app-core` clients, connecting locally to `musterd` over UDS. macOS and iOS share one SwiftUI package, differing only in layout.

### 3.8 Storage

- `musterd`: One embedded database (SQLite or redb), encrypted by a Storage-Key-wrapped DEK, storing roster entries, peer cache, OperationLedger, session metadata, and local policy (exposed ports, AX allowlist). Transactions replace multiple `.protected`, `.sealed`, `.jsonl` files and CAS/readback callbacks.
- Phone clients: The same database with fewer contents (roster, unconfirmed op_ids, SSH host fingerprints, UI preferences).
- **No anti-rollback**: Same-UID attackers are outside the threat model (decided).

### 3.9 Repository Layout

The implementation lives in `src/muster/`. The frozen `src/rust` workspace and the rest of the legacy tree it replaced were removed on 2026-10-02 (§8.5). Current crate status is in [src/muster/README.md](../src/muster/README.md).

```
src/
  muster/crates/
    muster-core/      Identity traits, roster (entries/folding/validation/address assignment), pairing (QR/PAKE), scopes,
                      local policy, Locked Use proofs (locked_use), desktop app registry (apps: provider, labels, bundle id, team)
    muster-net/       PeerLink (quinn), path management (direct/hole punching/Relay), roster gossip, forwarding streams
                      vnet/: TUN I/O, source validation, port filtering/connection tracking, DNS responses (desktop only)
    muster-relay/     bin: register / datagram / mailbox / stun
    muster-apns/      bin: APNs provider behind Relay push hooks (HTTP/2 :2197 silent wake; separate process holds keys)
    muster-proto/     protobuf schemas and generated code (rpc + ViewModel + Intent + Effect)
    muster-rpc/       Envelopes over any byte stream: calls, events, cancel, presence proofs, Envelope.target
    muster-store/     SQLite: identity, roster, policy, kv, OperationLedger
    muster-paths/     Per-user state and log locations; leaf crate shared by musterd and muster-cli
    muster-agent/     Agent Halo: viewers (codex/claude/catdesk), desktop Chat mode (desktop_chat), controllers (NativeApp AX only),
                      app profiles (profiles/*.toml compiled by build.rs into profile, §3.6.4, docs/APP_PROFILES.md);
                      tests/fixtures/ax: redacted recorded AX trees
    muster-browser/   Agent Halo Browser: Rust native host / typed local client / AX projection
    muster-terminal/  iTerm2 API client (prost + WebSocket over UDS), row deltas, VT state/grid,
                      SSH client (russh), independent Ghostty adapter (pending migration); later PTY/ConPTY
    muster-platform/  OS adapter traits + cfg implementations: macos (AX, NativeApp app bindings, IOPM, lock state,
                      XPC nethelper, Apple shims), windows, linux
    musterd/          bin: composes the above; UDS + PeerLink server + forward + vnet;
                      locked_use/: owner API (mod.rs) and the actor's state machine (actor.rs)
    muster-app-core/  Client runtime: ViewModel projections, Intents, subscriptions, local/remote connections, in-App SSH;
                      update/: one module per feature (network, operations, agents, desktop chats, files, terminals, shell)
    muster-ffi/       ≤15 C functions + header (nine today)
    muster-cli/       bin: debugging/operations (pairing, roster, vnet state, diagnostics, debug ax-dump)
  muster/scripts/     check-apple.sh, check-layers.py (the crate dependency map of docs/MODULES.md, run by CI), vnet-netns-test.sh,
                      redact-ax-dump.py (redacts AX recordings before they become fixtures)
  browser/agenthalo-extension/  Chrome MV3 JavaScript API adapter (no Python host)
  native/
    apple/MusterApp/        SwiftUI (shared macOS + iOS package), including custom terminal grid
    apple/MusterMacShim/    Swift/ObjC static library: SEP, Apple Events (iTerm2 cookie), Ghostty ScriptingBridge
    apple/MusterLockedUse/  system authorization plugin and its installer (Locked Use temporary unlock, D11)
    apple/NetHelper/        root LaunchDaemon: utun + addresses + routes + resolver, fd over XPC; Locked Use plugin installation
    android/                Compose + Keystore signer (JNI shim)
    harmonyos/              ArkUI + HUKS signer (NAPI shim)
    windows/                UI + wintun (P4; not present yet)
docs/
  ARCHITECTURE.md  MODULES.md (layers, dependency rules, where state lives)  APP_PROFILES.md (the profile DSL)
  PROTOCOL.md (roster/pairing/links/forwarding/vnet/rpc)  SECURITY.md (threat model)  history/ (archived documents)
```

Rough size estimates:

| Part | Estimated lines |
|---|---|
| Networking (including vnet, forwarding, Relay) | About 20–25K |
| Services (mostly migrated Provider / AX code, plus Rust iTerm2 and SSH clients) | About 28K |
| app-core | About 8K |
| Each platform UI | About 8–12K |

Total: approximately one-third of the current size.

---

## 4. Keep / Migrate / Delete

2026-10-02 authorization update: foreground phone agent operations use their
enrolled hardware device key for the operation-bound Locked Use signature,
without per-message biometrics. Administrative/browser operations and desktop
requesters retain Presence. Shared Rust owns this distinction and all existing
scope, nonce, TTL, revocation, cover and relock checks; see LOCKED_USE.md.

Agent Halo names a product responsibility; migration/deletion decisions for old crates below remain binding. Reusing the name does not restore the old business layer.

The legacy code named in this table left the tree on 2026-10-02 (§8.5). **Delete** rows are done. **Migrate**, **Port** and **Rewrite** rows that are not yet finished, such as the Ghostty adapter, name sources that are now read from Git history: `git show acf23be7:<path>`.

| Existing code | Treatment |
|---|---|
| `agenthalo-backend`: `codex.rs` (incremental rollout parsing), `codex_app_server.rs`, `claude.rs`, `claude_sdk*`, `claude_control*`, `operation_ledger*`, `attachment_store`, `workspace_files`, `workspace_git`, `session_pagination` | **Migrate** into `muster-agent` / `muster-store`; replace outer HTTP routes with muster-rpc |
| `macos_claude_native_input.rs`, `claude_native_input.rs`, `native_ui_input.rs` (AX traversal, process signature/generation checks, draft checks, input generation) | **Migrate** into `muster-platform::macos::ax` and `muster-agent` NativeApp controller; change lock policy to 3.6.2 |
| `claude_iterm_input.rs`, `claude_iterm_process_macos.rs` (Claude CLI editor readback, bracketed paste, process-signature binding) | **Migrate** into `muster-agent`'s iTerm2 controller; replace `ClaudeItermNativePort` FFI callbacks to Swift with direct in-process calls |
| `AgentHaloTerminalAdapters`: `Iterm2Adapter`, `Iterm2Client`, `Iterm2Transport` (SwiftNIO), `Iterm2Authorization` | **Rewrite in Rust** within `muster-terminal`, matching existing Swift implementation/tests; retain vendored `api.proto`; move cookie acquisition to `MusterMacShim` |
| `GhosttyAdapter` / `GhosttyAdapterLifecycle` (Swift), `GhosttyScripting` (ObjC) | **Migrate** session, capability, and operation semantics to `muster-terminal`; **migrate** only ScriptingBridge OS calls to `MusterMacShim`; delete old business implementation after P3 |
| Chrome web accessibility (new integration) | **Add in the new Rust architecture**: port the JS extension from the mac-b Claude chat and replace Python bridging with `muster-browser`; local/remote CLI and App setup are wired; accept locked-browser Presence separately from desktop temporary unlock |
| Catdesk Desktop (new integration) | **Add in the new Rust architecture**: `muster-agent` viewer/session semantics + `muster-platform` AX profile; reuse Agent Halo Locked Use, without adding capabilities to the frozen tree |
| `agenthalo-domain` (`session_projection`, etc.) | **Migrate** to `muster-app-core` projections |
| Go `vnet` (frame validation, source anti-spoofing), `agentd/vnet*.go` (routing, on-demand links), `quicpeer`, `stun`, `DirectEngine` (hole punching, candidates, network-change rebinding), `physdial` (physical-interface binding to bypass proxy utun), `doh` (Fake-IP bypass) | **Port** to `muster-net`, retaining logic in Rust and replacing framing with QUIC DATAGRAM |
| Go `agentd/forwarder.go` (MST1 `{"port":N}` forwards to local ports) | **Port** to forwarding streams with `net:` scope authorization |
| Swift `muster-net-helper` (utun creation, routes, caller signature verification) | **Retain and trim** as `NetHelper`, change caller to `musterd`, add resolver writing |
| Go relay: `rendezvous`, `pairingtransport` mailbox, `stun.Reflector` | **Rewrite in Rust** as `muster-relay` (decided); retain hole-punching signaling and STUN reflection logic |
| Platform hardware identity: `SecureEnclaveBlobKeyStore`, iOS `MusterIdentity`, `AndroidHardwareIdentity`, HUKS C++ | **Retain**, adapting to `hw_sign` callbacks / Effects; macOS parts move into `MusterMacShim` |
| Platform Views (SwiftUI, Compose, ArkUI pages) | **Retain layout**, binding to ViewModels |
| Principles: fail closed, bounded parsing, no replay of ambiguous mutations, immediate disconnection on revocation, only provider-declared decisions, `suppress_broadcast` | **Retain** in the new `SECURITY.md` |
| Go `agentd` (V2Runner, Ledger, ProductPipe, manual `vnetconfig` mappings), `innerchan`, `tunnel`, `sealbox`, `roster`, `keysource`, `enroll`, `ownerdirectory`, `pairingack`; `cmd/muster`, `muster-agent`, `musterctl`, `musterd` (after replacement); `go.mod` | **Delete** (portable logic listed above) |
| Swift `muster-sep-helper`, `muster-tls-broker`, `muster-net-bridge`; multiple LaunchAgent installation scripts in `deploy/` | **Delete** |
| All `muster-transport` modules except small codec utilities (concepts in 3.4, MST1 / MST2 / `http1` / WebSocket / pairing v1 / v2) | **Delete** |
| 499 `agenthalo-ffi` functions, `agenthalo-client`, `agenthalo-transport-local` | **Delete**, replaced by `muster-ffi`, `muster-app-core` |
| App-local `AgentHaloLocalTerminalService`, `AgentHaloRemoteTerminalAdapter`, backend `terminal_bridge.rs` | **Delete** (terminal becomes direct `musterd` → iTerm2) |
| Platform `*Coordinator`, `*Lifecycle`, `*StateStore`, `*Enrollment*`, `*Federation*`, `*OwnerDirectory*`, Swift `Yamux` / `MusterTLS` / `MST1`, 4.4K Android JNI lines, HarmonyOS `ets/domain` | **Delete** |
| `ios/Deps/MusterDeps` (swift-nio-ssh, SwiftTerm, unused by App) | **Delete**; implement SSH client and terminal grid in Rust |
| `terminal-core`, `terminal-adapter-fake` (no dependents) | **Delete**, replaced by `muster-terminal` |
| `spike/` | **Delete** or archive |

---

## 5. Migration Roadmap (Strangler Pattern, Delete Code Every Phase)

The old system keeps running during migration, with **new features frozen**. Old and new systems use different Relay endpoints, LaunchAgent labels, and vnet ranges (or run at separate times), without interference. Devices re-pair at cutover; no old state migrates.

| Phase | Work | Exit criteria | Deletions |
|---|---|---|---|
| **P0 Validation (1–2 weeks)** | Four device POCs:<br>① Locked AX read/write and button clicks on Electron Claude / Codex desktop Apps from a LaunchAgent;<br>② quinn + rustls hardware-signed handshakes (SEP static library / Android Keystore / HUKS), including a physical iPhone;<br>③ Relay datagram forwarding + hole punching + path migration, with IP over DATAGRAM running SSH and VNC;<br>④ Rust `musterd` (LaunchAgent) connects to iTerm2 API: Automation cookies, screen-update notifications, locked I/O | Each reports pass/fail and a fallback plan | — |
| **P1 Network Core + Desktop vnet** | `muster-core`, `muster-net` (vnet/forwarding), `muster-relay` (Rust), trimmed `NetHelper`, `muster-cli`; roster, QR / PAKE pairing, gossip, revocation, port filtering | Two Macs use SSH/VNC over virtual IP on direct and Relay paths; public TCP/UDP 9443 both pass WebPKI/registration; connection/registration or keepalive response over 3 seconds fails, with healthy standby restoring RPC; recovery reselects with hysteresis; both endpoints show renewed certificates; members communicate with creator powered off; Remove interrupts SSH within ≤ 2 seconds; unauthorized ports drop; malicious Relay tests cover tampering, replay, withholding | Old vnet bridge; `agent.yaml` vnet mappings; Go relay |
| **P2 Agent Halo + macOS / iOS App** | Providers in `musterd`; muster-rpc; `muster-app-core`; SwiftUI ViewModels; separate Codex / Claude / Catdesk viewers and AX profiles sharing Locked Use | Each Desktop App completes iPhone view → send → approve → observe result and relock **starting with Mac locked**; cover session-location failure and local-input cancellation. Missing capabilities reject explicitly; no CLI / terminal Agent-controller fallback | Old backend HTTP routes, `agenthalo-client`, Mac/iOS Coordinators/Lifecycles/Stores, Swift yamux / TLS / MST1 |
| **P3 iTerm2 / Ghostty Adapters + Phone SSH** | `muster-terminal` (Rust iTerm2 client, row deltas, VT grid); Ghostty control-only adapter migration to Rust + OS shim; Claude CLI iTerm2 controller; in-App SSH over forwarding streams; custom terminal grid | Phone views/operates the Mac's active iTerm2 sessions: LAN echo < 100 ms, zero idle traffic, locked usability; hardware-key SSH login to Mac in-App; separately verify Ghostty discovery/create/input/close, lock behavior, and rejection of unsupported actions, without extrapolating iTerm2 acceptance | Swift `AgentHaloTerminalAdapters` iTerm2/Ghostty business code, App-local terminal service, `terminal_bridge.rs`, polling backoff, `ios/Deps` |
| **P4 Android / HarmonyOS / Windows** | Platform shells/signers; Windows `musterd` (ConPTY / UIA / NCrypt / wintun) | Each platform completes P2/P3 acceptance scenarios | Android JNI business layer, HarmonyOS `ets/domain` and NAPI business layer, old Windows preview |
| **P5 Cleanup** | Retire old Relay endpoints, LaunchAgents, root-helper configuration; archive documents | No `_v2` security modules, Go code, excess helpers, or 499-function ABI remain | Remaining Delete items from section 4 |
| **P6 (as needed)** | Roster-generated `authorized_keys`; Linux/Windows PTY fallback; enhanced vnet name resolution; privacy cover + VNC combination | Defined as needed | — |

---

## 6. Alignment with AGENTS.md

| AGENTS.md rule | New architecture implementation |
|---|---|
| Business logic in Shared Rust | Structural guarantee: state exists only in `musterd` or `app-core`; platforms receive display-only ViewModels and can send only Intents individually validated by Rust. vnet routing, anti-spoofing, packet filtering, and SSH protocol also live in Rust; root helper only configures OS facilities. |
| Platforms only handle UI, lifecycle, and OS-only APIs | Effects form a closed set (3.7); TUN, AX, Apple Events are OS-only capabilities in shims or `muster-platform`, isolated by traits |
| Never infer trust state from JSON/booleans | ViewModels are display-only; local authentication produces a Presence signature verified by the target |
| Extend Rust first when ABI lacks a capability | Extend schema fields, keeping ABI functions unchanged |

---

## 7. Decisions

### 7.1 Decided (Beginning 2026-09-24; Later Decisions Dated)

| # | Decision | Architectural effect |
|---|---|---|
| D1 | **vnet is required** for SSH, VNC, and other software | Networking includes TUN, roster-assigned virtual IPs, and port authorization; QUIC DATAGRAM vnet shares service links; VNC provides pixel-level remote desktop, without a custom Screen service |
| D2 | **Clean cutover** | No old-state migration; all devices re-pair; no v1/v2/v3 compatibility code |
| D3 | **Malicious programs under the same user are outside the threat model** | Delete local anti-rollback, freshness quorum, sealed heads, activation, TLS broker; SEP signing occurs inside `musterd` |
| D4 | **iTerm2 is the primary terminal** | Embedded Rust iTerm2 API client in `musterd`, notification-driven row deltas; remote terminals independent of App process; PTY only a Linux/Windows fallback |
| D5 | **No NetworkExtension for macOS virtual interfaces** | Retain a trimmed root helper (`SMAppService.daemon`) creating utun/routes and passing fd to `musterd` through XPC; delete `muster-net-bridge` |
| D6 | **iTerm2 uses Automation permission** | `musterd` obtains cookies through Apple Events with one-time approval; do not enable iTerm2's allow-all-Apps setting |
| D7 | **Rust Relay** | `muster-relay` shares `muster-core` roster/codecs with clients; remove Go repository-wide |
| D8 | **No phone VPN; in-App traffic only** | No phone vnet/VPN extension; built-in clients reach desktop ports through PeerLink forwarding; share `net:` scopes and exposed-port policy with vnet; third-party phone Apps cannot use vnet |
| D9 | **No phone VNC** | No phone display viewing; pixel-level remote desktop only between desktops over vnet; no in-App RFB client |
| D10 | **Rust phone SSH with hardware-key login** | `app-core` embeds `russh` for all four platforms; Device Key is the client key, private keys stay in hardware, signing uses `hw_sign`; SSH reuses iTerm2 row grids and terminal UI |
| D11 (2026-09-26) | **NativeApp Locked Use uses short-lived signed capability + AuthorizationPlugin temporary unlock** | Bind turn/devices, 5–15-second TTL, one-time nonce, local-input monitoring, covered execution then relock; Rust owns decisions. Ordinary privacy covers do not replace this flow; see LOCKED_USE.md for implementation boundaries |
| D12 (2026-09-28) | **Three product layers: Codex / Claude / Catdesk Desktop → Agent Halo (Accessibility + Locked Use); iTerm2 / Ghostty → respective adapters; all over Muster Networking + Hardware Security** | Agent Halo and terminal adapters sit alongside one another, sharing hardware identity, authorization, and networking; UI is a client, business logic stays in Shared Rust. Retain D4's iTerm2 priority, with Ghostty independently integrated. New Catdesk profile and Ghostty migration belong to P2/P3 acceptance respectively; see §3.2, §3.6, §4, §5. |
| D13 (2026-09-28) | **Public port 9443 only, combined TCP+UDP routing, Let's Encrypt on both endpoints** | Hot standby between TCP/TLS 1.3 length-prefixed frames and UDP/QUIC DATAGRAM; WebPKI hostname/validity/chain checks; 3-second connect/register deadline; health/RTT routing with hysteresis and background recovery; unchanged inner hardware identity; no automatic raw UDP/ws fallback. Renew both endpoints together; see §3.5. |
| D14 (2026-10-04) | **Cloud sessions through the providers' own APIs, with the CLIs' credentials, read-mostly** | Claude Code web sessions and Codex cloud tasks join the Mac's agent list as `mode = "cloud"` sessions; the daemon speaks the APIs the `claude` and `codex` CLIs speak, with the tokens they keep (the file, or the macOS Keychain item through the system's consent prompt), never refreshing or storing them; a prompt goes to Claude only and nothing else is written; no desktop app, Accessibility or Locked Use is involved, and the Accessibility-only rule for desktop sessions stands |

### 7.2 Pending Decisions

| # | Decision | Recommendation | Reason |
|---|---|---|---|
| 1 | Post-enrollment governance: admin is a roster role, multiple devices may hold it, and any admin may sign Add / Update / Remove? | **Yes** | Meets creator-only enrollment approval and offline independence without quorum |
| 3 | vnet address range | **Default `192.168.13.0/24`**, changeable in Genesis or signed Readdress | Preserve existing member identities/pairing; physical LAN must not overlap the virtual subnet |
| 4 | Message-based protobuf ABI or UniFFI? | **Message-based** | UniFFI does not support ArkTS; messages give all four platforms the same interface and permanently fixed ABI |
| 5 | Platform order | macOS + iOS → Android → HarmonyOS → Windows | Consistent with the existing plan |
| 6 | Describe Codex / Claude / Catdesk as declarative app profiles with one engine (§3.6.4)? | **Yes, after the Catdesk slice lands** | Most per-app differences are data; safety invariants stay in one engine; a new app becomes a profile plus fixtures |

---

## 8. Implementation Status

### 8.1 Historical Snapshot (2026-09-25)

The following preserves that day's progress, including its not-yet-implemented/verified conclusions. Later updates are in §8.2; this is not the current overall status.

`src/muster/` contains every Rust crate listed in 3.9 and passes Linux tests:

- Networking: roster, pairing, gossip, service/forwarding streams, direct/Relay paths, revocation ≤ 2 seconds, vnet filtering (TUN requires root; test marked ignored).
- Services: Codex/Claude session viewing and change push; Owned controllers (codex app-server, Claude CLI stream-json); iTerm2 controller (editor readback, bracketed paste, screen approval prompts); iTerm2 API client (row deltas, screenshots, protected sends); VT grid; hardware-key SSH client using russh.
- `musterd`: UDS and `muster` service streams share one handler; remote calls use roster scopes and local policy, with Presence signatures for high-risk scopes; mutations with `op_id` use OperationLedger; `Envelope.target` lets local Apps reach other members through musterd.
- `muster-app-core` / `muster-ffi`: Intent → Effect / ViewModel runtime (embedded mobile links, desktop through musterd), nine C functions; session-worktree files, individual-file, git log/diff views; `SetPushToken` supplies platform push tokens to Relay; network changes/foreground restoration re-register Relay, reprobe paths, and restore session watching. Independent C ABI tests: `muster-ffi/tests/abi.rs`.
- `muster-agent`: Filesystem watching (notify) wakes transcript-directory polling; polling is fallback only, with subsequent rescans covering files in new directories.
- `muster-platform`: macOS lock state, input counter, code signatures, AX controller, SEP signer, and NetHelper XPC client are written but compiled only on macOS, **not yet device-verified**, corresponding to P0 items in section 9.

- `muster-relay`: Registration caps, per-device/source rate limits, mailbox limits, offline-device push hooks (`PUSH_REG` + `--push-command`), systemd/Caddy deployment files (`deploy/relay/`). Mailbox roster cold start: store on `Node` publication, reconcile after registration and every 10 minutes (`tests/mailbox.rs`).
- Documents: [PROTOCOL.md](PROTOCOL.md) (wire formats), [SECURITY.md](SECURITY.md) (threat model/invariants); CI in `.github/workflows/muster.yml`.
- Native shells (`src/native/`, see README): One Apple SwiftUI package built and run with Xcode 27 on macOS and iOS Simulator. Mac App creates networks, invites, and approves over musterd socket; iPhone Simulator pastes invitations, joins, directly connects to Mac through local Relay, lists/follows Claude/Codex sessions live, and views active iTerm2 sessions (draws/scales/scrolls the peer grid without resizing its window; core determines `TerminalView.resizable`). Simulator SEP device keys work; Presence keys, scanning, and push await real devices.
- macOS `musterd`: `cargo build -p musterd --features sep` uses `muster-platform/build.rs` and swiftc to build/static-link SepSign and Swift runtime; started locally and connected to iTerm2 API (cookies via osascript, handshake headers via tungstenite), listing/pushing real sessions. Files follow macOS conventions: daemon bundled in `Muster.app` (`Contents/MacOS/musterd` + `Contents/Library/LaunchAgents/dev.linsheng.musterd.plist`, registered through SMAppService on first launch), state in `~/Library/Application Support/Muster`, logs in `~/Library/Logs/Muster` (`musterd::paths`); Relay is configured/persisted in App Settings or `muster relay` (`DeviceSetRelay`).
- Relay deployment: `muster.linsheng.dev:9443` (UDP) and `wss://muster.linsheng.dev` (nginx to loopback WebSocket), `deploy/relay/install.sh`; verified registration by Mac and physical iPhone, followed by LAN direct connectivity. Rescanning enrolled devices has `pair::serve` resend the roster without approval. `MusterMacShim` SepSign static library; Android JNI shim + Kotlin wrapper + Keystore + Compose (not SDK-built); HarmonyOS NAPI shim + ArkTS wrapper + HUKS + ArkUI (not DevEco-built). Windows remains P4 per §5.

Not yet done: local storage encryption (D3), Ghostty / PTY fallback (Linux PTY exists), Windows shell, Codex desktop AX adapter, Android FCM registration (iOS APNs registration exists in the shell; HarmonyOS push unwired).

---

### 8.2 Three-Layer Architecture Source Review (2026-09-28)

| Target component | Current evidence | Remaining work |
|---|---|---|
| Agent Halo / Codex, Claude Desktop | New Rust tree has viewers, NativeApp profiles, and Locked Use orchestration; `muster-platform/src/macos/native_app.rs` defines two profiles | Accept Claude scenarios independently; Codex passing does not establish every operation passes |
| Agent Halo / Catdesk Desktop | 2026-10-02: `muster-agent::catdesk` reads the app's SQLite store read-only (ui-sdk and legacy conversations, stable `message_id`s, receipts from new user rows); `catdesk_app` reads its owner-only command socket for the GUI pid, live status, project paths and current session; `catdesk_ui` holds labels/routes; `muster-platform` has the `com.catpaw.cowork` / `BHWTW6L8X6` profile with selection-replacement drafting. Unit tests and a read-only parse of a real store copy pass; see [history record](history/2026-10-02-catdesk-adaptation.md) | Install on Mac-B, allowlist `com.catpaw.cowork`, collect covered AX evidence (sidebar rows, composer settability), then iPhone view → send → observe → relock |
| Agent Halo / Locked Use | [LOCKED_USE.md](LOCKED_USE.md) records mac-b + iPhone passing two consecutive user-authenticated sends, relock, uncover, and live message display | Independently verify cancellation, local-input interruption, disconnection, crashes, multiple-display failures |
| iTerm2 adapter | Rust API client exists in `muster-terminal`; daemon connects directly to local terminal | P3 device acceptance of output latency, locked behavior, and complete operation capabilities |
| Ghostty adapter | Control-only implementation in frozen `src/native/apple/AgentHaloTerminalAdapters` (removed 2026-10-02, §8.5); not integrated in new Rust tree | Migrate business logic/OS shim per §4 and complete P3; old implementation existence does not make it usable in the new architecture |
| Muster clients | [AGENT_INTERFACE.md](AGENT_INTERFACE.md) records device tabs, full Agent list, collapsed tool activity, Markdown, turn usage, file previews | New-session file preview still needs iPhone tap acceptance; mobile devices appear only in network-settings roster |
| Networking + Hardware Security | New Rust tree includes identity, roster, networking, authorization, SEP implementation, shared by all integrations | Judge platform/link acceptance separately from existing records; this documentation update adds no network/hardware device verification |

This reviews source and recorded acceptance only; this update changes architecture documentation without implementing Catdesk, migrating Ghostty, or deploying a new version.

---

### 8.3 Dual Secure Transport Source Update (2026-09-28, Pending Deployment)

`muster-net::relay_tls`, `secure_relay`, and `muster-relay` implement TCP/TLS and UDP/QUIC endpoints, WebPKI, 3-second connect/register/first full-frame probe, health/RTT routing, ACTIVATE return-path assertions, and nonce-matched PROBE/PROBE_ACK. Source status does not establish installation; signed artifacts, public TCP/UDP handshakes, production-device switching, and renewal acceptance require separate records. See [NETWORK_DIAGNOSTICS.md](NETWORK_DIAGNOSTICS.md).

### 8.4 Session Controller Consolidation (2026-09-28)

The active Rust tree removed Codex app-server, Claude CLI stream-json, iTerm2 Agent bridging, and related process configuration/test fixtures. Section 8.1's Owned description is historical and no longer applies. CLI session-creation requests and platform entries are removed; missing desktop AX controllers explicitly reject writes. Read-only transcripts, independent terminal/browser services, and the Locked Use authorization chain remain.


### 8.5 Legacy Tree Removal (2026-10-02)

The frozen implementation left the tree in one change, because nothing in the active build, CI, packaging or runtime read it:

- Go: `src/cmd`, `src/internal`, `src/test`, `src/go.mod`, `src/Makefile`, and a committed 10 MB `musterd` Go binary.
- Rust: `src/rust` (`agenthalo-*`, `muster-transport`, `terminal-core`, `terminal-adapter-fake`, the 499-function header).
- Apps: `src/native/apple/AgentHalo{MacApp,CoreClient,TerminalAdapters}`, `src/ios`, the AgentHalo Android project (`android/app`, `android/agenthalo-core`), the AgentHalo HarmonyOS project (`harmonyos/entry`, `tests`, `native-smoke`), and the Windows preview.
- Tooling and records: `src/deploy`, `src/spike`, `src/poc` (the P0 tools; see [P0-validation.md](P0-validation.md)), and the AgentHalo UI specifications, now under `docs/history/design/`.

The one live dependency was moved, not deleted: the bundled root helper (`dev.linsheng.muster.helper`) was built by `MusterApp/project.yml` from `src/cmd/muster-net-helper`. It now lives in `src/native/apple/NetHelper`, keeping only its bundled configuration (musterd caller, `192.168.13.0/24`).

Deleting source stops nothing that is already installed. The Go relay `musterd.service` on the relay host and the manually installed `dev.linsheng.muster.nethelper` on the Macs keep running until retired separately ([relay deployment](../src/muster/deploy/relay/README.md), `src/muster/deploy/desktop/retire-agenthalo.py`). Their sources, installers and configurations are recoverable with `git show acf23be7:<path>`; the paths in §2 and the appendix refer to that commit.

### 8.6 Shell Decisions Moved to Rust (2026-10-02)

An audit of the Apple, HarmonyOS and Android shells found decisions §3.7 assigns to Rust. The ViewModel now carries them; the Apple shell renders them all and the HarmonyOS shell those it has UI for (it has no cover or Mac setup): `DeviceActions` (cover, in-app SSH, access summary, edit access, remove), `ChatView.can_interrupt` / `can_choose_model` / `can_browse_files` / `provider_label`, `TerminalsView.can_create`, `DesktopSetupView.ready`, `DesktopChatsView.choices`, and the display strings `JoinView.message`, `MemberView.link` and `DeviceInfo.key_label`. Queued messages are listed only in `ChatView.queue`. Shells no longer pair `can_go_back` with their own root-screen lists, and pass any Ctrl-letter key to `muster-terminal`.

Schema leftovers left the protocol with reserved numbers: the never-emitted `Log` / `KeepAwake` / `Haptic` effects, the never-sent `Init`, `PresenceGranted` and `ChatDraft` intents (a presence answer is the `Authenticate` effect's result), `ViewModel.presence`, and three `FileView` fields copied from `DesktopSetupView`. `ChatView` also stopped repeating the transcript as a flat `messages` list beside `timeline`, which doubled every chat view's size; `message_count` carries the count. The Android shell, already stale (src/native/README.md), was brought up to this contract on 2026-10-04 (§8.9).

### 8.7 Code Organization and App Profiles (2026-10-02 – 2026-10-03)

Behavior-preserving restructuring after §8.5 and §8.6:

- `muster-app-core`'s 4,400-line `update.rs` became `update/`, one module per feature (network, operations, agents, desktop chats, files, terminals, shell). The dispatchers in `update/mod.rs` name every intent and message variant, so a new variant cannot fall through a wildcard.
- `musterd`'s `locked_use.rs` became `locked_use/`: the owner API (`mod.rs`), the actor's state machine (`actor.rs`) and its tests. The code moved without logic changes; [LOCKED_USE.md](LOCKED_USE.md) stays the acceptance record, and a later change to the actor is accepted on devices again.
- CatDesk landed (PR #52; §8.2 row): store viewer, socket runtime, `muster-platform` profile.
- App profiles (§3.6.4, pending decision 6). Step 1: `muster_core::apps` is the one registry of each app's provider, labels, Chat mode, bundle id and signing team; signing requirements, Locked Use provider checks, the default AX allowlist and access labels read it. Step 2 started: `muster_agent::profile` holds controls as data, matched by one `Selector` over the live macOS element and over recorded trees: each app's agent-conversation composer, send, stop and approval options, and Chat mode's send, stop, new chat, dialog close and settings exit (ChatGPT only, which removed a provider branch). Chat mode matches exactly as before. In agent conversations, Stop and approvals are now named the way Chat mode names controls (title, else description; an empty title counts as none), and an interrupt presses whatever the running-state check reports as Stop. Before, a present but empty AXTitle hid the description, and an interrupt pressed only a control named exactly "Stop". CatDesk's Stop is its own two labels ("停止", "Stop"). The remaining provider branches then became profile data without changing behavior: the agent conversation's queue, question cards (labels and Claude's paged, renderer-refresh and button-choice flags), session list and drafting; Chat mode's page, conversation URL, mode switch and history rows. A probe over the recorded trees printed every Chat-mode reading for both apps before and after: identical.
- AX fixtures: idle agent conversations of the three apps were recorded on Mac-B with `muster debug ax-dump`, redacted by `scripts/redact-ax-dump.py` (it keeps only labels Muster's product code names) and stored in `muster-agent/tests/fixtures/ax`. Raw recordings hold conversation text and never enter the repository. Running Claude Code and Codex turns, a Claude Code question card and both apps' Chat mode followed on Mac-C ([AGENT_INTERFACE.md](AGENT_INTERFACE.md#ax-recordings-on-mac-c-2026-10-03)). They showed that Claude's Stop button has an empty AXTitle, so through 1.0.32 a running Claude turn was never seen; the profiles' name rule (title, else description) fixes that. The redactor now also keeps vocabulary prefixes and replaces links outside the apps' own pages.

Deployment: 1.0.33 (`5614785c`, PRs #50–#57) is installed on Mac-B only and not published ([DESKTOP_UPDATES.md](DESKTOP_UPDATES.md)); Mac-A and Mac-C stay on 1.0.31. CatDesk is not allowlisted on Mac-B, so its device acceptance (§8.2) has not started. 1.0.33 carries the step-2 selectors including the Stop change; a locked send and an interrupt through each app are their device acceptance, not yet done.

### 8.8 Module Map, Layer Check and the Profile DSL (2026-10-04)

Behavior-preserving restructuring of the upper layer, on Linux with the macOS code type-checked (`scripts/check-apple.sh`); no device acceptance changed:

- [MODULES.md](MODULES.md) maps the six crate layers, the dependency rules, where each product state machine lives and which product knowledge is data. `scripts/check-layers.py` enforces the dependency map in CI, so the direction of a new edge is a decision recorded in both places.
- App profiles (§3.6.4) are now the TOML DSL of [APP_PROFILES.md](APP_PROFILES.md): `muster-agent/profiles/{vocabulary,claude,codex,catdesk}.toml`, compiled by the crate's `build.rs` into the same `profile` statics the engine already read, plus the `App` aggregate and `APPS`. The generated data was compared against the previous hand-written statics; the fixture tests over the recorded AX trees pass unchanged, and new tests check the profiles against `muster_core::apps`, the capability tables and the lock names.
- The daemon no longer enumerates apps: `agent_bridge` builds provider queues, desktop locks (keyed by the profile's `lock`), the background scan and each app's resolver (the profile's `session_key` strategy) from `APPS`; muster-platform's `AppProfile` is the shared `App`. Adding an app is a registry entry, a TOML file and fixtures (MODULES.md §5).
- Capabilities (step 4): each profile declares what the app offers; the daemon rejects an undeclared agent operation before any queue, lock, unlock or ledger entry, and `ChatView` projects the declared set. All three apps declare what they did before, so no operation changed hands.
- `muster-agent`'s 2,200-line `desktop_chat.rs` became `desktop_chat/`, one submodule per interpreter (`tree`, `app`, `mode`, `history`, `pickers`, `conversation`, `transcript`) with `mod.rs` re-exporting them and holding the tests; every item kept its name and path.
- Step 3 of §3.6.4, same day: the Codex and Claude transcripts are read by one engine (`muster_agent::jsonl`) over the record grammar each profile declares (`[source]`: message envelope and roles, text and media blocks, block numbering, tool calls and results, turn start and end, bootstrap, skipped records) plus four named hooks per source (text cleaning, title records, usage, Claude's prompt queue). `receipt.rs` and `recency.rs` classify records through the same grammar. Message ids, kinds, texts, phases and states are the readers' previous ones; the one visible difference is that a Codex bootstrap prompt no longer names a session whose title is still a placeholder, which matches the head reader. The viewer, service, receipt, recency and question tests pass unchanged.

### 8.9 Mobile Shells Aligned with iOS (2026-10-04)

The HarmonyOS and Android shells were brought level with the iOS shell against the current `app.proto`; neither was compiled or run (the environment had no DevEco Studio or Android SDK), and each README's verification status says so.

- HarmonyOS (`src/native/harmonyos/muster`) gained the Chrome extension screen (`Screen.BROWSER`, from a device's details and from Settings on a device with services), Cover / Uncover on a device's details while `DeviceActions.can_cover` holds, the composer draft riding along with every intent (`ComposerAction`, restored from `ChatView.draft`, cleared only for the revision a send receipt names), the unconfirmed-send card (`ChatView.unconfirmed`), a terminal Paste control (`TerminalPaste` while `TerminalView.can_paste`), and answers to `ActivateNetwork` (one TCP connect to the core's endpoint), `LocalAuth` (the system user-authentication prompt) and `Cover` (refused on a phone), which it had refused as unknown. Names were checked against the generated `muster.d.ts`; the host tests pass.
- Android (`src/native/android/muster`) was rewritten screen by screen after the iOS shell: the workspace switcher and device tabs, the chat timeline (Markdown, Working groups, tool groups with paired output, turn changes, usage footers), the composer with attachments, drafts and the unconfirmed-send card, interaction cards with questions, queued messages, the Locked Use status, desktop chats and their options sheet, the browser screen, device actions, the resizable / fixed terminal grid with paste, diff previews, the relay editor, and the three effect answers above; the removed `Log` / `KeepAwake` / `Haptic` effects and `PresenceGranted` intent are gone. Every getter and DSL builder it uses was checked against the Kotlin and Java `protoc` generates for the schema.
- Neither shell ports the iOS performance monitor (the only `UploadDiagnostics` sender).

### 8.10 Half-Fold Layouts and the Muster Keyboard on iOS and Android (2026-10-04)

The two half-fold workspaces and the in-app keyboard the HarmonyOS shell already had (`src/native/harmonyos/muster/README.md`, "Keyboard" and "Half-fold workspace") were ported to the iOS and Android shells, so every mobile shell offers Top and bottom, Side by side, Automatic and Single pane, and can type without the system keyboard. Both ports keep the HarmonyOS structure file for file, and all of it is display state: no intent, effect or ViewModel field changed, and `app.proto` is untouched.

- Shared logic, pure on each platform and covered by ports of the HarmonyOS host tests (32 each): the workspace geometry (`FlexPreference`, `WorkspaceArea`, `FoldGeometry`, `workspaceLayout`: stacked above a horizontal crease or an equal split, side by side across a vertical crease plus 8 units on each side or at the middle, 280 units per pane, 56 / 220 for the controls), the pinyin engine (syllable segmentation, frequency-ranked whole words, sentence composition, partial words, learned picks, 80 candidates), the MacBook layout data (14.5 units per row, the function row, Chinese punctuation) and the UTF-16 caret arithmetic (character, word, line, line ends). Swift: `MusterCore/Display/WorkspaceLayout.swift`, `MusterCore/Input/{PinyinEngine,MacLayout,TextEdits}.swift`, `swift test` on Linux. Kotlin: `app/.../display/WorkspaceLayout.kt`, `app/.../input/{PinyinEngine,MacLayout,TextEdits}.kt`, JUnit 4. The 55,320-word AOSP PinyinIME dictionary ships as a package resource on Apple and an asset on Android.
- iOS (`MusterUI/Display/FoldDisplay.swift`, `MusterUI/Keyboard/{KeyboardPrefs,MusterKeyboard,KeyboardFields}.swift`): the preference lives in `UserDefaults`, `MusterRootView` resolves the split for the window and hands it down as an environment value; the chat and terminal screens stack above the hinge with a separately scrolling lower pane, and Side by side keeps the workspace list beside the open row. The keyboard is a SwiftUI view inside a `UIInputView`, attached as the input view of UIKit-backed fields (`MusterTextField`, `MusterTextEditor`, the composer's editor) and of the terminal's key catcher; arrows and modifiers edit through `UITextInput`, terminal keys go to the core by name. iOS exposes no fold geometry, so Automatic is one pane. In text fields it is off by default (Settings → Keyboard) until verified on a device; in a terminal it is on by default, with its own switch (`terminalSystemKeyboard`, the same on all three shells), since the system keyboard has no Esc, F-keys, Ctrl or Alt. The UI parts are not yet compiled (no Xcode here).
- Android (`app/.../display/FoldDisplay.kt`, `app/.../input/{Keyboard,KeyboardFields}.kt`): the preference lives in `muster-display` SharedPreferences; Automatic follows Jetpack WindowManager's half-opened `FoldingFeature` (`androidx.window:window:1.3.0`); `MusterRoot` resolves the layout and provides it as `LocalWorkspaceLayout`, draws the keyboard as the scaffold's bottom bar for whichever field or terminal registered a `KeyboardTarget`, and the IME is suppressed with `InterceptPlatformTextInput` so a field and the keyboard never both show. The whole `app` and `core` Kotlin was compiled on Linux with kotlinc 2.1.0 and the Compose compiler plugin against the declared BOM's jars (no errors or warnings); that check also caught nine `ListItem` calls from §8.9 that passed the modifier positionally, now fixed. Not built with Gradle and never run.
- HarmonyOS gained only the terminal's own keyboard switch, on by default as its one switch already was; not compiled or run.

### 8.11 Agents Cloud Mode (2026-10-04)

The agent service gained the providers' cloud sessions (§3.6.2 revision, decision D14): Claude Code on the web and Codex cloud tasks are listed under the Mac beside its local sessions, followed, and (Claude) written to, through the APIs the `claude` and `codex` CLIs speak, with the credentials those CLIs keep on the Mac. No protobuf field changed: a cloud session is `mode = "cloud"` with `controller = "cloud"`, shows its repository or environment label where a local session shows its directory, and carries the profile's label (`Claude Cloud`, `Codex Cloud`) as its provider label, so the three shells show it without a change.

- The profile DSL gained `[cloud]` (APP_PROFILES.md): the strategy (`claude-code`, `codex-tasks`), origin, label, poll cadence, where the CLI keeps its credentials (file paths, the token's expiry, the account id, the macOS Keychain item) and what a cloud session offers (`send`). `build.rs` validates it into `profile::Cloud`; `claude.toml` and `codex.toml` declare theirs.
- `muster_agent::cloud` is the engine's two strategies over an `Http` trait the daemon implements (`musterd::cloud_http`: reqwest, WebPKI roots, no redirects) and tests fake. Claude: the session list (sessions bridged from a machine, archived and pooled ones left out), its events read incrementally by sequence number through the one JSONL engine, whose record reader was separated from the file tail for that (`jsonl::Reader`), and a prompt posted as a user record. Codex: the task list and the task details (prompt, replies, the latest diff as a `diff` tool pair, a failed turn's error), re-read whole and replaced when changed. Credentials are read per request and never refreshed: an expired or refused token reads as signed out, logged once, and the listing stays empty until the CLI signs in again.
- `AgentService` keeps cloud sessions in their own catalog, lists each source at the profile's cadence while watched (a quarter as often otherwise), reads the sessions someone opened, sends through the source and refuses interrupts, answers, question cards and files for them; `musterd::handler` routes a cloud session's operations past the desktop locks, Accessibility and Locked Use to the source, still under the roster's `agents:<provider>` scope, the profile's cloud capabilities and the idempotency ledger; `muster-app-core` projects the cloud label and withholds `can_interrupt` and `can_browse_files`. `musterd --no-cloud` (`DaemonConfig::cloud`) keeps a device to local sessions; phones, which embed the daemon without the agent service, never speak to the clouds.
- Verified on Linux: the profile, cloud, handler and render unit tests and a service test over a fake transport (list, read, send, refusals), with `cargo fmt`, `check-layers.py`, clippy and the workspace tests. The request and reply shapes were taken from the open-source Codex CLI (`codex-rs/cloud-tasks-client`, `backend-client`) and from the calls the Claude Code CLI makes; neither API was called from here (no credentials, no device), so the first run against the live services is the next acceptance step.

## 9. Risks and Open Validation Questions (P0 Must Answer)

**Locked Use update (2026-09-28)**: Acceptance records are in [LOCKED_USE.md](LOCKED_USE.md).
Codex passed two normal-path consecutive sends and message display; all-display failures, local input, cancellation,
disconnection, and crashes still require independent verification. One successful path does not establish all P0 gates passed.

1. **NativeApp temporary-unlock chain**: Validate system-plugin authorization, actual unlock, all-display cover, trusted terminal state, and relock per D11, plus local input/crash/disconnect/late receipts. Missing links reject this path without CLI fallback; accept independent terminal channels separately.
2. **LaunchAgent TCC ownership**: `musterd` needs Accessibility and Automation (iTerm2 cookie, Ghostty). Confirm permissions bind to its code identity and survive upgrades after one approval through stable signing identity.
3. **iTerm2 API in `musterd`**: Can LaunchAgent cookie requests display authorization? Do screen-update notifications arrive while locked? What coalescing/fetch throttling handles high-frequency output?
4. **SEP access in LaunchAgent**: Reuse CryptoKit blobs in the in-process static library; verify locked signing (Device Key must not require unlock ACL; Presence Key should).
5. **vnet performance and MTU**: QUIC DATAGRAM carries 1150-byte inner packets at any working path MTU; confirm acceptable throughput and Relay VNC bandwidth/latency with small outer MTUs (PPPoE, proxy tunnels).
6. **Coexistence with local proxies**: macOS Surge/Clash utun versus vnet route priority; reuse `physdial` physical-interface binding and `doh` Fake-IP bypass experience. Phones no longer have this issue because there is no VPN.
7. **`russh` interoperability with macOS sshd**: `ecdsa-sha2-nistp256` public-key authentication with hardware signing callbacks; first-trust host-fingerprint UI; long-session keepalives and reconnection after forwarding-stream loss.
8. **iOS backgrounding**: Service links/forwarding streams disconnect in background, interrupting SSH; approval notifications require APNs. Relay may send content-free notifications by DeviceId, as post-P2 work.
9. **Catdesk and Ghostty integration**: Catdesk reading, unique location, and AX profile exist in source but have no device acceptance (its window is not exposed to AX while locked, and its Lexical composer needs selection replacement rather than `AXValue`); Ghostty awaits migration, with no promise of output reading or locked operation. Verify each application's signature identity, post-upgrade control/interface changes, and unsupported-capability rejection independently.
10. **Rust sockets and tokio on HarmonyOS**: Validate with the OpenHarmony NDK. If sockets are unavailable, HarmonyOS is the only platform that must retain raw socket callbacks.

---

## Appendix: Baseline Data Sources

Paths below refer to the legacy tree at commit `acf23be7` (§8.5).

- Process topology: `native/apple/AgentHaloMacApp/build.sh`, `Resources/*.plist`, `AgentHaloCLIMusterPeerTransportOwner.swift:65`, `backend_product_pipe_v2.rs:93`, `muster_network.rs:213`, `apple_tls_broker_v2.rs:50`, `deploy/dev.linsheng.muster.nethelper.plist`
- vnet baseline: `internal/vnet/{bridge,frame,ctlproto}.go`, `internal/agentd/{vnet,vnetconfig,vnetpeer,forwarder}.go` (`192.168.14.0/24`, MTU 1280, manual SPKI → address mapping)
- ABI：`rust/include/agenthalo_core.h`、`agenthalo-ffi/src/lib.rs:83`、`security_v2.rs:421-529`
- Lock rejection: `agenthalo-backend/src/macos_claude_native_input.rs:200,400-412`
- Authorization branches: `agenthalo-backend/src/remote_authorization.rs`; `docs/history/architecture/agenthalo-network-authority.md`
- Terminal path: `docs/history/architecture/agenthalo-remote-terminal.md`, `AgentHaloLocalTerminalService.swift`, `terminal_bridge.rs`, `native/apple/AgentHaloTerminalAdapters/Sources/AgentHaloIterm2*`; unused `ios/Deps/MusterDeps` (swift-nio-ssh, SwiftTerm)
- Duplicate platform logic: files in the table in section 2.5
