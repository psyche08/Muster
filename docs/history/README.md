# Historical Documentation Index

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

This directory contains the architecture, design specifications, tests, and field records written between 2026-08-13 and 2026-09-24. They describe **the implementation superseded by [../ARCHITECTURE.md](../ARCHITECTURE.md)** (three trust models, five transport paths, a 499-function ABI, and so on). They are retained to:

- Trace the evidence behind a decision at the time, such as why a join-only participant could not start a listener;
- Compare legacy behavior during migration, such as the iTerm2 adapter feature inventory, vnet anti-spoofing rules, and `suppress_broadcast` conventions;
- Record pitfalls already observed in testing, such as Fake-IP, SIGTERM deadlocks, and 304 directory synchronization.

They are **not** the target architecture or a backlog. Relative links between documents remain valid; source paths refer to the layout at the time of writing.

## architecture/ — Legacy Architecture and Protocols

| File | Title | Summary |
|---|---|---|
| `muster-security-protocol-v2.md` | Muster Security Protocol V2 | Superseded security protocol: owner / grant / federation / freshness / activation / MST2 |
| `agenthalo-cross-platform.md` | AgentHalo Cross-platform Architecture | Legacy Rust ↔ platform responsibilities, ABI evolution log (1.5 → 1.9x), and provider contracts |
| `agenthalo-network-authority.md` | Network Authorization and Member Services | Evidence that owner authority carried three responsibilities; the new architecture's roster scopes derive from this analysis |
| `device-network-trust.md` | Device Network Trust | Design and field results for the third trust model (Master distribution v3) |
| `muster-single-network.md` | Muster Single-network Constraint | Implementation constraints and silent-failure investigation for "a device may create or join only one network" |
| `muster-direct-paths.md` | Muster Direct Peer Paths | Go direct QUIC, STUN hole punching, and path migration; the logic will move to `muster-net` |
| `agenthalo-remote-terminal.md` | Remote Terminal | Four-part legacy remote terminal implementation; origins of full-screen polling and backoff |
| `agenthalo-terminal-native-iterm2.md` | AgentHalo Terminal Native iTerm2 Integration Architecture | Swift iTerm2 client protocol details, cookie retrieval, and TCC; reference for the Rust rewrite |
| `agenthalo-hardware-tls-v2.md` | AgentHalo Hardware TLS V2 | TLS broker and short-lived certificate lifecycle (removed concepts) |
| `agenthalo-remote-product-v2.md` | AgentHalo Remote Product Transport V2 | Rust WS + TLS + MST2 transport and `product-pipe` (removed paths) |
| `agenthalo-owner-registration.md` | Owner Registration and Pairing Readiness | Legacy owner registration flow |
| `agenthalo-owner-grant-revocation.md` | Owner Grant Revocation Transaction | Legacy grant revocation transaction |
| `agenthalo-consumer-enrollment-catalog.md` | Consumer Enrollment Catalog (ABI 1.80) | Legacy consumer enrollment and directory |
| `agenthalo-pairing-external-witness.md` | Production Pairing Rollback-witness Boundary | Anti-rollback witness (explicitly out of scope) |
| `agenthalo-pairing-recovery-diagnostics.md` | Pairing Successor Recovery and Diagnostics | Legacy pairing recovery |
| `agenthalo-pairing-release-gates.md` | AgentHalo Pairing Release Gates | Legacy pairing acceptance gates |
| `agenthalo-pairing-mock-test-matrix.md` | AgentHalo Pairing Mock System API Test Matrix | Legacy pairing mock test matrix |
| `agenthalo-client-joint-test-plan.md` | AgentHalo Unified Client Manual Test Plan | Legacy manual test plan |
| `agenthalo-device-page.md` | AgentHalo Devices Page: Networks, Devices, and Permissions | Devices-page information architecture; UI layout remains a useful reference |
| `agenthalo-macos-ui.md` | AgentHalo macOS UI | Legacy macOS page structure |
| `agenthalo-mobile-shell.md` | AgentHalo Unified Mobile Shell and Half-fold Layout | Mobile shell and foldable layout; UI portions remain a useful reference |
| `agenthalo-workspace-files.md` | Session Workspace File Reads | Workspace read-only file API boundaries; logic moves to `halo-agent` |
| `agenthalo-workspace-git.md` | Session-scoped Read-only Git History | Workspace read-only Git API boundaries; logic moves to `halo-agent` |

## design/ — AgentHalo UI Specifications

Moved here on 2026-10-02 with the AgentHalo apps they specified (architecture §8.5). The active shells under `src/native/` do not read `mobile-style-tokens.json` or run `check-mobile-style.mjs`.

| File | Title |
|---|---|
| `agenthalo-mobile-style.md` | AgentHalo Mobile Visual Specification (with `mobile-style-tokens.json` and `check-mobile-style.mjs`) |
| `agenthalo-desktop-devices.md` | AgentHalo desktop devices page specification (with `agenthalo-devices-preview.html`) |
| `history/agenthalo-device-page-2026-09-21-v1.md` | First devices-page implementation, frozen 2026-09-21 |

## superpowers/ — Earliest Design Specifications and Plans

| File | Title | Summary |
|---|---|---|
| `specs/2026-08-13-private-tunnel-hardware-identity.md` | private-tunnel Hardware-rooted Identity Migration · Design Specification | Project origin: from private-tunnel to hardware identity; §12 records the shift to an untrusted rendezvous relay |
| `plans/2026-08-13-private-tunnel-hardware-identity.md` | Same Project · Phased Implementation Plan | M1–M3 plan (M1 and M3 retired) |
| `specs/2026-08-21-muster-ios-client.md` | Muster iOS Client · Design Specification | Origin of the iOS Swift yamux / TLS / MST1 implementation |

## testing/ — Acceptance Checklists and Field Records

| File | Title |
|---|---|
| `agenthalo-remote-completion-checklist.md` | macOS / HarmonyOS Complete Gap Inventory and Unified Acceptance Checklist |
| `agenthalo-remote-completion-history.en.md` | macOS / HarmonyOS historical implementation and verification records |
| `agenthalo-cross-platform-completion.md` | AgentHalo All-platform Development and Acceptance Checklist |
| `agenthalo-local-signed-remote-gates.md` | AgentHalo local signed-state integration checkpoint |
| `native-client-computer-use-input.md` | Original-client Computer Use Input and Independent Output Observation (origin of the AX executor; includes 2026-09-12 smoke results) |
| `mac-b-mac-c-deployment-20260911.md` | mac-b / mac-c Signed Deployment and Virtual Network Verification |
| `mac-c-migration-20260912.md` | mac-c Migration Field Record |
| `vnet-local-packet-read-recovery.md` | Local Fragmented Packets Disconnecting the Virtual Network: Fix and Verification |
| `relay-product-route-recovery.md` | Relay Product Route Recovery (2026-09-11) |
| `owner-directory-304-recovery.md` | Directory Synchronization 304 Fix After Pairing (2026-09-11) |
| `server-tls-preemptive-renewal.md` | Server TLS pre-expiry renewal and overlapping generations |
| `protected-state-observation-optimization.md` | macOS Protected-state Observation Optimization |
| `provider-discovery-async-sessions.md` | Provider Automatic Recovery and Asynchronous Session Loading |
| `macos-sigterm-termination-recovery.md` | macOS SIGTERM Shutdown Deadlock |
| `debug-local-authentication.md` | Debug Local Authentication Toggle |
| `agenthalo-mobile-layout.md` | Mobile Layout Unification Verification Record |
| `agenthalo-mobile-shell-flex.md` | Unified Mobile Shell and Half-fold Layout Verification Record |

## Other Documents

| File | Title |
|---|---|
| `desktop-session-loading.md` | Desktop session demand |
