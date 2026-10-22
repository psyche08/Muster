# mac-b / mac-c Signed Deployment and Virtual-Network Validation

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

## Conclusion

On 2026-09-11 at 20:51–20:58 (Beijing time), both Macs installed the complete Release AgentHalo; App, Muster user service, and independent backend started. All 60 final pings across six directions between three Macs returned, and original virtual-network SSH addresses allowed login. Latency varied; this is not long-term stability or phone end-to-end acceptance.

AgentHalo's phone remote listeners remain unready: both machines lack current Owner signing-identity files. No new Owners, copied local identities, pairing resets, or relaxed authorization.

## Build and Signing

- Source: `0fbe1f8f5a11e1c1ff6c5874c385360ca21e13aa`, clean worktree during build.
- Completely rebuilt Rust Apple XCFramework, backend, Muster CLI/agent, SEP helper, Broker, and Release UI; no old UI mixed in.
- Developer ID Team: `89LGY6BD53`; installed `codesign --verify --deep --strict` passed on both ends. No notarization.
- Actual signed App Shared report matches the current Rust reference linker program: `abi=1.85.0 authority_runtime_bytes=33357824 activation_bytes=163840`.
- Broker software self-tests passed, including caller paths, existing identities, protected state, fixed two-slot reads, cache, and deadlines. Full product regressions were not rerun.

| Artifact | SHA-256 |
| --- | --- |
| Installation zip | `3f79e443fdd219b79f3810eb9ce3173884aad96a281716d69661882face5ea1c` |
| App executable | `df0adf43948fa0a25bb59493213d2d9f5015252c96398939efff63c8b7360452` |
| backend | `5352e7f3af54b92ee4a6df89f60d03f1cb8e3e0fc010dbbe809702bc0c17cc1e` |
| Broker | `c95d30cf0468dc3d3287b97dabaffa6f8e38791fb69ab28c989bb6c14fa11c41` |

## Deployment Scope and Preserved State

- Original SSH aliases use virtual addresses; deployment used Thunderbolt physical addresses with verified host keys, without disabling host-key checks.
- mac-b: management `192.0.2.3`, virtual `192.168.13.12`.
- mac-c: management `192.0.2.2`, virtual `192.168.13.13`.
- Local machine: virtual `192.168.13.11`.
- Both retained their own canonical configuration/identity. Signed migrators reported `migrated=false`; new agent configuration validation passed.
- Updated `/Applications/AgentHalo.app` and two user LaunchAgents; privileged nethelper, Relay, locally running AgentHalo, and other proxy software unchanged.
- Old Apps retained at mac-b `/Applications/AgentHalo.app.backup-20260911-205139` and mac-c `/Applications/AgentHalo.app.backup-20260911-205302`. Service installer also retained old LaunchAgents; no user data deleted.
- To satisfy existing security constraints, tightened both `~/Library/Application Support/AgentHalo/State` directories from `755` to `700`, and mac-c `Muster/State/devices/mac-c.sepblob` from `644` to `600`. Verified first that these were user-owned actual directories/single-link regular files; key contents neither read nor changed.

## On-site Validation

| Item | mac-b | mac-c |
| --- | --- | --- |
| App PID | 28762 | 11332 |
| Muster PID | 28133 | 11167 |
| backend PID | 28244 | 11265 |
| Broker PID | 28780 | 11337 |
| backend health endpoint | `ok: true` | `ok: true` |
| Virtual route interface | `utun6` | `utun0` |
| Signed App→Broker read | `read_completed` | `read_completed` |
| Ordinary same-user process | Rejected: `backend identifier mismatch` | Same |

Signed reads initially failed on both machines and passed after tightening state-directory permissions; this proves OS transport only, not Shared authorization/remote listening.

Final samples: 10 pings per direction, all zero loss:

| Direction | Average / maximum latency (ms) |
| --- | --- |
| Local → mac-b | 144 / 231 |
| Local → mac-c | 90 / 147 |
| mac-b → local | 206 / 465 |
| mac-b → mac-c | 18 / 21 |
| mac-c → local | 174 / 580 |
| mac-c → mac-b | 80 / 123 |

An approximately 1692 ms late ICMP reply occurred early in reconnection but ultimately arrived, not lost. Finally both original virtual-network SSH aliases were used again to confirm services/health remained available. These short samples are not low-latency or long-term stability guarantees.

## Gates Not Passed

- Both backends ultimately reported `owner broker: existing hardware identity file is unavailable`.
- Following current backend fixed-location rules, read-only inspection confirmed both lacked `SecurityKeys/example-identity-fingerprint.sepblob` (Owner signing identity). Neither was automatically created or copied from another machine.
- mac-c's previous `existing hardware identity file metadata is invalid` advanced to this missing Owner-file boundary after its local Muster identity permissions were tightened to `600`.
- Initial startup also logged one `resolve TLS broker from XPC reply failed OSStatus=-67071`; later actual signed App reads/backend progress show it no longer blocked there, but its cause was not independently reproduced.
- No Owner initialization, phone pairing, transcript/file access, or normal-authentication E2E on either Mac. Earlier local—HarmonyOS raw-stream failure remains documented in `protected-state-observation-optimization.md`; this deployment neither fixed nor validated it.
