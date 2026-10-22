# muster single-network constraint

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Status: 2026-09-20 (see “Progress” at the end for the later update). muster currently **supports only one network per device**: either create one (become owner) or join one (become consumer), but not both, and not multiple networks. This is a constraint of the current implementation, not the design goal; see [device-network-trust.md](device-network-trust.md) for the multi-device network goal.

## Constraint

A device's identity state has only two valid forms:

- **Create**: hold owner authority (`state/remote-owner-registration-v2.protected`, `remote-owner-runtime-v2.protected`), publish signed snapshots to the relay owner directory, and let other devices join.
- **Join**: hold consumer enrollment (`state/ConsumerEnrollment/`, `owners/<ownerID>/`) and read service lists from an owner's published directory.

They **must not coexist**. Holding both breaks the authority pairing issuance invariant, inside local protected state, with no repair entry point: `AgentHaloConsumerEnrollmentError` has only the enum cases `repair` and `terminalRepairOrRePair`, with no corresponding implementation. The only way out is to clear state and pair again.

## This is not theoretical

It happened on mac-a on 2026-09-20. The machine held both:

```
state/remote-owner-runtime-v2.protected          890KB   (create: owner runtime)
state/remote-owner-registration-v2.protected     2.1KB   (create: owner registration)
state/ConsumerEnrollment/owners/6dd5beda…                (join: consumer enrollment)
```

The symptom was a completely empty remote-session list. Investigation chain (each step supported by logs or probes):

| Stage | Result |
|---|---|
| Pairing ACK | `phase=acked`, complete |
| relay owner-directory / pairing-transport | Both services enabled |
| Client → relay pairing request | Reachable after the port-validation fix (below) |
| Owner registration | Reached the relay and wrote a record |
| **Owner publishes signed snapshot** | **Failed: `CorruptOrUnsupportedProtectedState: issuance_invariant`** |
| Directory projection → peer devices → remote sessions | Empty |

The relay returned **404 rather than 401** for snapshot reads. This proves the client's read credentials were valid and the owner record existed; only `rec.Envelope` was missing, meaning publication had never succeeded.

A separate defect fixed during investigation: `buildPairingRequest` compared relay origins by host:port. After a relay port change, every directory read/write by paired clients was rejected locally, including `/v2/owner-directory/publish`, leaving no self-recovery path. It now compares by hostname.

## Why it was hard to detect

Every layer failed silently, directly causing the lengthy investigation. Related fixes have landed:

1. The App had no logs (stdout/stderr pointed at `/dev/null`, with no os_log). It now writes `~/Library/Logs/AgentHalo/agenthalo-app-YYYY-MM-DD.log`, rotated daily and retained for seven days.
2. Directory-refresh failures did not appear in the UI: the entire status line was inside `if let display`, although “no display” is precisely the state most in need of explanation. There is now a separate `musterOwnerDirectoryError`.
3. Owner-preparation failures appeared only in the phone-invitation section, far from the triggering button, making a click appear to do nothing. They now appear beneath the button and in logs.
4. The FFI boundary discarded diagnostics: `Display` wrote only `kind: message`, while `corrupt_state()` had over 20 call sites. Messages now include `source_line` and byte counts.
5. Directory refresh did not retry after failure (`next_refresh_ms` was empty). **This remains unchanged**: the loop explicitly assigns delay policy to native Shared, with `nil stops the loop pending explicit action`; adding Swift-side backoff would violate that boundary.

## What to do

Until multiple networks are supported:

- Each device **does only one thing**: create a network for others to join, or join someone else's network.
- A device that has created a network must not join another, and vice versa.
- To change roles, clear the previous state before pairing; there is no in-place migration path.

The UI and code should reject a second network **before it is established**, rather than failing an irreparable invariant during publication.


## Progress: 2026-09-20

### Completed

**The relay** is working again and standardized on 7443: musterd is now a systemd system unit running as the dedicated non-root user `musterd`. systemd reads certificates as root into tmpfs using `LoadCredential=` (no on-disk copy). TCP and UDP both use 7443, and nginx is out of the muster path. Both owner directory and pairing transport are confirmed enabled.

**Owner directory recovered.** Previously, three owner records (`85604f11…`, `9783e9fd…`, `bbf32b54…`) conflicted, and none had ever published a signed snapshot. After clearing them, mac-a registered cleanly as `bbf32b54…` and published successfully: `is_configured: true`, the first successful run since investigation began.

**mac-a local state** was cleaned: consumer enrollment and the stale owner runtime (890KB, belonging to an old identity) were removed and backed up. The invariant failure at `local_authority_runtime_v2.rs:289` came from the old runtime's trust anchor differing from the current owner anchor—not corruption, but an identity change with the old runtime left behind.

**The single-network guard** (`AgentHaloNetworkRole`) rejects a second network **before** it is created. Both entry points are connected, and checks on both sides are injectable rather than reading each other's storage.

**UI consolidation**: device interconnection (the base layer required on every device) and AgentHalo service access (the service layer) are now two layers of one network. Service-layer state and failures have moved from the collapsed-by-default “Diagnostics” section to the top level.

**CLI pairing**: `--muster-pair-host` / `--muster-pair-join` can drive desktop pairing from a shell. A real handshake succeeded, with matching SAS and owner key on both ends.

### Incomplete

**The final pairing step requires user presence.** `confirm()` invokes LocalAuthentication; noninteractive SSH sessions return `Authentication canceled.` (mac-c returns `errSecInteractionNotAllowed`). This is the SAS-confirmation control working, not a defect: it must run inside the target machine's own login session. mac-b and mac-c have not joined the network, so remote sessions remain empty.

**Per-device permissions are not implemented.** The protocol has read/write/approval, but the UI member model has no capability field, and device-network-trust.md cited at the start records that network authorization has “no production exchange/persistence driver yet.” A control that appears to authorize but does not is worse than none. Implementation requires: capability fields in the member model → ABI configuration entry point → persisted distribution records → capability enforcement by the backend.

### Defects fixed in this round

Silent failure at every layer directly caused the lengthy investigation. These issues are fixed:

| Defect | Fix |
|---|---|
| App had no logs (stdout/stderr → `/dev/null`, no os_log) | Daily `agenthalo-app-YYYY-MM-DD.log`, retained for seven days |
| Directory-refresh failure absent from UI (entire status line inside `if let display`) | Separate `musterOwnerDirectoryError`, pure-function decision + tests |
| Owner-preparation failure only in the phone-invitation section, far from the button | Display directly below the button and log it |
| FFI discarded diagnostics (`Display` wrote only `kind: message`) | Include `source_line` and byte counts |
| Role report used `directoryEndpoint` to infer “network created” | Use owner-registration state; this field is always empty on the coordinator path |
| Role report labeled storage-path components as owner ID | Rename to `joined_storage_components` |
| `buildPairingRequest` compared relay by host:port | Compare by hostname, so port migration no longer locks out paired clients |
| Pairing listener held in a local variable, released on return | Keep alive with a static reference; peers previously received ECONNREFUSED |

One item **remains unchanged**: no retry after directory-refresh failure. The loop explicitly assigns delay policy to native Shared, with `nil stops the loop pending explicit action`; Swift-side backoff would violate that boundary.
