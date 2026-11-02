# Migration: Keep / Migrate / Delete and the Roadmap

Part of the [Muster architecture](../ARCHITECTURE.md). Section numbers are shared across all parts, so a reference such as §3.6.2 means the same section wherever it appears.

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
