# Runtime, Native Boundary and Repository Layout

Part of the [Muster architecture](../ARCHITECTURE.md). Section numbers are shared across all parts, so a reference such as §3.6.2 means the same section wherever it appears.

## 3.2 Product Responsibilities and Runtime Mapping

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

## 3.3 Process Model

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
5. Install/replace the Locked Use system authorization plugin and its fixed authorization rules from the signed App. Rust daemon orchestrates this operation, with fixed audit-token caller verification and no arbitrary command/path interface; see [LOCKED_USE.md](../LOCKED_USE.md#6-installation-and-recovery).

The App-managed service is `dev.linsheng.muster.helper`, registered with `SMAppService.daemon` and built from `src/native/apple/NetHelper`. Retain the manually installed `dev.linsheng.muster.nethelper` until network migration finishes; its continued operation does not prove the new plugin-installation capability is ready. Its source left the tree on 2026-10-02 (§8.5).

It then passes the utun fd to `musterd` through XPC and **never reads or writes packets itself**. Caller verification reuses the existing implementation (audit token + SecRequirement for the fixed team ID and `musterd` identifier). `musterd` calls it directly through the libxpc C API (as the existing backend already calls tls-broker), eliminating `muster-net-bridge`.

**SEP signing**: CryptoKit SEP blobs are accessible only from Swift, so use an approximately 200-line Swift static library (`@_cdecl` exports `create` / `sign` / `public_key`) linked into `musterd` for in-process signing. Same-UID attackers are explicitly outside the threat model, so broker isolation is unnecessary.

## 3.7 Rust ↔ Native Boundary: Message-Based ABI + ViewModel

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

## 3.8 Storage

- `musterd`: One embedded database (SQLite or redb), encrypted by a Storage-Key-wrapped DEK, storing roster entries, peer cache, OperationLedger, session metadata, and local policy (exposed ports, AX allowlist). Transactions replace multiple `.protected`, `.sealed`, `.jsonl` files and CAS/readback callbacks.
- Phone clients: The same database with fewer contents (roster, unconfirmed op_ids, SSH host fingerprints, UI preferences).
- **No anti-rollback**: Same-UID attackers are outside the threat model (decided).

## 3.9 Repository Layout

The implementation lives in `src/muster/`. The frozen `src/rust` workspace and the rest of the legacy tree it replaced were removed on 2026-10-02 (§8.5). Current crate status is in [src/muster/README.md](../../src/muster/README.md).

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
  ARCHITECTURE.md (entry point)  architecture/ (its parts)  MODULES.md (layers, dependency rules, where state lives)  APP_PROFILES.md (the profile DSL)
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
