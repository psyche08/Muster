# Muster Architecture: Desktop Applications → Agent Halo / Terminal Adapters → Networking + Hardware Security

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Status: **Target architecture, updated for the product-layering decision of 2026-09-28** (D12, see [§7](architecture/decisions.md)). The [2026-09-24 baseline](architecture/legacy-baseline.md) keeps the old implementation's size figures; [§8](architecture/status.md) separates historical snapshots from later source reviews. Target capabilities do not imply implementation or successful device acceptance.

This page is the entry point: what Muster is made of, what it must do, and
the principles every change follows. Each topic's detail lives in one part
under [architecture/](architecture/). Read this page first, then only the parts
your change touches. Section numbers (§0–§9) are shared across all parts, so a
reference such as §3.6.2 in an ADR or a code comment names the same section
wherever it appears.

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

These are product responsibilities, not a requirement for one process or crate per layer; runtime and code locations are in [§3.2 and §3.9](architecture/runtime.md). Agent Halo names a logical capability in the new architecture and does not revive the frozen `agenthalo-backend` / `agenthalo-ffi`.

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


### 3.2 – 3.9 and the Rest: Where Each Topic Lives

| § | Topic | Part | Read it when |
|---|---|---|---|
| 3.2, 3.3 | Product responsibilities mapped to crates; processes per platform; the root helper and SEP signing | [runtime.md](architecture/runtime.md) | Adding a process, a crate or an OS capability |
| 3.4 | Keys, the signed roster, scopes, target-local policy, the one authorization function, pairing | [identity-and-trust.md](architecture/identity-and-trust.md) | Touching membership, permissions, Presence or pairing |
| 3.5 | PeerLink, relay carriers, presence, recovery, DNS, vnet, `muster-relay` | [network.md](architecture/network.md) | Touching links, the relay, vnet or forwarding |
| 3.6, 3.6.1, 3.6.3 | `muster-rpc` and the operation ledger; iTerm2, Ghostty, managed tmux and phone SSH terminals | [services.md](architecture/services.md) | Adding an RPC or changing a terminal adapter |
| 3.6.2, 3.6.4 | Agent Halo viewers and controllers, cloud and plugin-driven sessions, locked operation, app profiles | [agent-halo.md](architecture/agent-halo.md) | Changing how an agent app is read or driven, or adding an app |
| 3.6.5 | Multica container management and the native runtime bridge | [multica.md](architecture/multica.md) | Touching Multica |
| 3.7 – 3.9 | Message-based ABI and ViewModels; storage; repository layout | [runtime.md](architecture/runtime.md) | Changing the Rust ↔ native boundary or moving code |
| 2, Appendix | The 2026-09-24 implementation that this architecture replaced, and why | [legacy-baseline.md](architecture/legacy-baseline.md) | Asking why a rule exists |
| 4 – 6 | Keep / migrate / delete table, phases P0–P6 with exit criteria, alignment with AGENTS.md | [migration.md](architecture/migration.md) | Planning or accepting a migration phase |
| 7 | Decided (D1–D14) and pending decisions | [decisions.md](architecture/decisions.md) | Proposing something a decision may already settle |
| 8, 9 | Implementation status snapshots (8.1–8.12); risks and open validation questions | [status.md](architecture/status.md) | Checking what is built and what is still unaccepted |

Related references outside this tree: [MODULES.md](MODULES.md) (crate layers
and dependency rules), [APP_PROFILES.md](APP_PROFILES.md) (the profile DSL),
[PROTOCOL.md](PROTOCOL.md) (wire formats), [SECURITY.md](SECURITY.md) (threat
model) and [LOCKED_USE.md](LOCKED_USE.md).

## Maintaining This Architecture

- Change the part that owns a topic; keep its section number. Add a new
  numbered section to the part where it belongs and a row to the table above.
- Keep this page to the overview, requirements, principles and the map. Detail
  that only one kind of change needs goes in a part.
- Record decisions in [§7](architecture/decisions.md) and dated status in
  [§8](architecture/status.md).
