# Architecture Decisions

Part of the [Muster architecture](../ARCHITECTURE.md). Section numbers are shared across all parts, so a reference such as §3.6.2 means the same section wherever it appears.

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
