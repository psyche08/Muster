# AgentHalo Devices Page: First Implementation Record (Historical)

> This document freezes the first implementation and its verification at the time; it is not the current target specification. See the new [device-page architecture](../../architecture/agenthalo-device-page.md). States, tests, and follow-up items below are historical records.

**Status:** On 2026-09-21, macOS and iOS / Android / HarmonyOS implemented the same structure and passed their respective build and test gates; Windows did not yet have a devices page and was to follow the same specification. No visual acceptance in a real multidevice environment occurred. Visual/information architecture is in [Desktop Devices Page](../agenthalo-desktop-devices.md) and [Mobile Visual Specification · Devices Page](../agenthalo-mobile-style.md#devices-page); trust rules are in [Device Network Trust](../../architecture/device-network-trust.md). Verification records: [macOS](#verification-gates) and [mobile round three](../../testing/agenthalo-mobile-shell-flex.md#third-round-2026-09-21-devices-page-reworked-to-match-desktop-structure).

## Problem to Solve

The devices pages on four platforms grew layer by layer with the protocol: each new verified source (signed member records, outbound directory, local authorization ledger, path snapshots, live device status, session directory) added a section displaying it verbatim. The result:

- macOS had eight sections, four listing the same computers from different perspectives; seven Add buttons were scattered across four places, and the page needed a paragraph explaining Add Computer versus Invite Computer to Network.
- Phones put protocol-state cards (Security V2, authority set / epoch / revision) at the top, above the operator's actual concern—which computers were reachable. Each device printed every directory field (Relay, SPKI, mTLS, Tunnel, Health, capability list) line by line.
- Owner, Grant, revision, SPKI, epoch, quorum, and tombstone appeared directly in the body, mixing Chinese and English.

The trust-layer constraint was clear: [Device Network Trust](../../architecture/device-network-trust.md) explicitly requires that existing authorization records not be interpreted as network membership credentials merely because the UI merges their presentation. The issue was treating each source as a section; the intended approach groups by device while retaining source-specific badges.

## Design Decisions

**One row per device, one source per badge.** The page body has one list. Desktop rows have at most four badges (network member / whether this device can read it / whether it can read this device / link); phone rows have at most two (online state / session count). Each badge reads only its own source. If that source has no record, **omit** the badge rather than showing a negative value: no ledger record does not mean unauthorized, and absence from a path snapshot does not mean offline.

**Grouping is not inference.** Combining records from multiple sources into one row by device identity is presentation grouping; Network Member and Authorized to Access This Device appearing together does not imply either derives from the other. Actual access still uses target-side certificates and Shared's opaque authorization handles. This directly implements the rule above and explains why projection functions are pure, read no models, and accept only primitive types.

**Keep unmatched records.** A phone found only in the authorization ledger or a peer found only in a path snapshot gets its own row; unnamed devices show Device · last six characters. Prefer an extra row over guessed names or incorrect merges.

**Three levels instead of eight sections.** This device (identity, role, state, single add entry) → Devices (list) → Advanced (collapsed: all records, raw fields, diagnostics). The redesign removes no information or actions; it collapses them. In-progress pairing/enrollment expands at the list top because that is what the operator is waiting for.

**Name actions by purpose and keep them together.** Desktop has one top-right Add Device menu grouped into Allow Access to This Device / Access Other Computers / Device Network; phones have only Add to Computer · Scan Code. Flows themselves (scan, SAS, system authentication, signing, persistence) remain unchanged; only entry positions and names change.

## Projections

Each of four platforms has pure functions of the same shape: `project(input) -> [Entry]` and `summarize([Entry]) -> Summary`. Inputs are primitive-type slices from each source, mapped by views from their respective models; outputs are ordered rows.

### Desktop (`MusterDeviceDirectory`, `src/native/apple/AgentHaloMacApp/Sources/AgentHaloMacCore/MusterDeviceDirectory.swift`)

```text
input:  localName, masterDeviceID?, localIsMaster,
        members[deviceID, active]            ← signed member records
        ledgerNames[fingerprint, name]       ← local ledger, used only to fill names
        remotes[id, name, reachable, label]  ← outbound directory: whether this device can read it
        grants[subjectDeviceID, name, platform, ready, waiting, attention]  ← local ledger: whether it can read this device
        peers[fingerprint, spki, name, active, rtt]                          ← path snapshot
entry:  id, name, kind(computer|phone), isLocal,
        membership?(master|member|removed), access?(reachable|unreachable(label)),
        authorization?(ready|waiting|attention), link?(direct(rtt)|relay|none)
```

- Identity normalization: remove `spki:`, lowercase, and trim. Member `spki:<hash>`, ledger `<hash>`, and path `fingerprint` are the same hash in practice; directory `id` and grant `subjectDeviceID` in different namespaces do not match and naturally form separate rows.
- Local row: when `localIsMaster`, use `masterDeviceID` as the key and merge with the Master member record; otherwise use fixed key `local`, labeled Member. No local row is produced without a network record.
- Kind: only the authorization ledger knows the platform; `iOS / Android / HarmonyOS / phone` means phone, otherwise default to computer. If any source in a row says phone, it is a phone.
- Order: local → Master → active → removed, preserving source order within groups; refresh does not reorder.
- Attention: authorization failure or an active device that cannot be read; removed devices are history and excluded. Summaries exclude local and removed devices.

### Phones (`AgentHaloMobileDeviceDirectory` / `MobileDeviceDirectory.kt` / `domain/MobileDeviceDirectory.ts`)

```text
input:  services[id, name, relayPeer, ownerID]   ← cached directory (iOS)
        statuses[id, name, state(reachable|checking|offline), label, detail]  ← live device status
        sessions[id, name, count]                ← session directory
entry:  id, name, access?(reachable|checking|unreachable(label)), detail?, sessionCount?, ownerID?, relayPeer?, isSelected
```

- Order follows sources: directory → status → session directory, preserving original order within each.
- Android inputs are directly `DeviceStatus` and `SessionGroup` (session counts sum across providers), with `transportPhase.wireName · service_ready/unavailable` and capability versions retained for Advanced. HarmonyOS `reachable` uses the directory's existing five-field `serviceReachable()` predicate without inventing another standard.
- Empty detail produces no row; `healthState` is badge text only when unreachable.

The three mobile projections use the same cases: multiple sources merge into one device row, unmatched records stay separate, no invented state, name fallback without invented state, and summaries count only actionable items.

## Platform Wiring

### macOS（`MusterNetworkView.swift`）

- `body` remains `NavigationStack` + authentication gate (`MusterNetworkPresentationState` unchanged); toolbar is only Close / Refresh / Add Device ▾. All sheets (add phone / add computer / connect computer / manual SPKI / approve knock) and three delete/revoke alerts remain, without separate sections for each.
- `devicePage`: error banner → `List { local card; devices section; advanced DisclosureGroup }`. The devices section starts with ongoing enrollment (`ComputerNetworkEnrollmentView`, changed from `private` to module-visible for this purpose) and pairing progress (shown when `MusterMobileAccessPresentation` is not `readyToCreateInvitation`).
- `directoryInput` maps `deviceNetwork.display`, `musterDevices`, `musterRemoteDevices`, `mobileAuthorization.snapshot.grants`, and `musterPaths` to projection inputs. Each device gets a `MusterDeviceDirectoryRow`; four badges are colored independently (ok / muted / warning), with absent badges omitted.
- `advanced` contains the former full page: `MusterDeviceNetworkSection` (member records/enrollment), `MusterGrantManagementView` (individual revocation), `MusterPathSection` (link candidates/RTT), connectable computers (original lifecycle wording), and diagnostics (directory, Owner registration, pending knocks, authorization roster, revocation records, manual SPKI).
- The three Add Device menu groups open existing sheets or call `deviceNetwork.showInvite / showMobileInvite / showJoin / create`; Set This Device as Master retains its confirmation alert.

### iOS（`AgentHaloNetworkView` in `AgentHaloRootView.swift`）

- `AgentHaloMobileForm` has three Sections: This Device (`UIDevice.current.name`, Paired with n Authorities / m Pending Sync counted from `authorityFederationStatus`, connection-state dot, and `AgentHaloMobilePairingView` NavigationLink) → Devices (tapping calls `connection.selectService`, selection gets a checkmark; footer appears only for identity conflicts) → Advanced (authority set / epoch / each authority's membership and revision / original conflict text / Provider / each device's id, relay, Authority).
- Developer Connection Records and Security and Permissions moved to Settings in the previous round and no longer appear here.

### Android（`NetworkScreen` in `AgentHaloApp.kt`）

- `LazyColumn` (`network-list`): title → `SecurityV2QrPairingCard` (local identity, pairing state, and scan entry; `mobile-authorization-card` tag unchanged) → Devices heading + summary → one `MobileDeviceRow` card per device (`device-<id>`) → `network-advanced` toggle → when expanded: `SecurityV2StatusCard` (former top protocol card, `security-v2-status` tag unchanged), original transport/capability text per device, DEBUG development simulation card, and Architecture explanation.
- Instrumentation cases now scroll to and click `network-advanced` before asserting `transport_ready · service_ready`; the main list asserts only `在线` (Online) / `offline`. These tests were still compiled only, not executed.

### HarmonyOS（`deviceNetwork()` in `Index.ets`）

- The root remains one Column in `Scroll#devices-scroll`: `devices-header` (two Text lines: title and summary) → `this.mobileEnrollmentCard()` (local identity, pairing, scanning, Owner space) → `this.masterNetworkCard()` (phone network membership/enrollment) → `this.deviceDirectoryStatus()` (loading / directory error / workspace error / empty / refresh, retaining semantics of four ids and Refresh) → identity-conflict card → one card per device (● name Online/reason; n sessions; detail; Select as New Session Target) → `devices-advanced` button → `if (this.devicesAdvancedOpen) this.deviceAdvancedDetails()` → `devices-security-note`.
- `deviceAdvancedDetails()` receives the old cards' `devices-provenance`, Service directory / Relay / Authorization / mTLS / Tunnel / Health / Device ID / SPKI, and CAPABILITIES list.
- The collapsed area contains **only** those fields because `DevicesLayout.test.cjs` visibility fixtures evaluate `if` guards individually for the four directory-state ids, Refresh, and pairing controls. Placing them under `devicesAdvancedOpen` would mark them invisible because fixtures lack that field. With this arrangement, the existing gate passed without changing a test line.

## Invariants

- **Each badge comes only from its own source.** Omit absent records; never infer from other badges, hostnames, history lists, or HTTP success.
- **Merged presentation is not a credential.** Projections read no models and touch no handles; views only render results, and actions still call original model methods.
- **Collapse, do not delete.** Every previously visible item and action remains in Advanced; ongoing pairing/enrollment stays expanded above the list.
- **Flows remain unchanged.** Scanning, SAS comparison, system authentication, signing, persistence, revocation, and directory-refresh entries retain their semantics; only location and names change.
- **Failures stay visible.** Directory/enrollment errors, authorization failures, and identity conflicts appear in the main list or local card with original text and recovery actions.

## Verification Gates

| Platform | Command | Coverage |
| --- | --- | --- |
| macOS | `cd src/native/apple/AgentHaloMacApp && swift build --scratch-path .build && swift test --scratch-path .build --no-parallel` | `MusterDeviceDirectoryTests` (6), existing `MusterNetworkPresentationState` / path / pairing presentation tests; 1 existing failure among 451 tests (`WorkspaceUploadTests…PreservesNewDraftAndOtherSessionFiles`, reproducible on a clean tree and unrelated to this page) |
| iOS | `bash src/ios/CoreTestHarness/run-mobile-shell-host-tests.sh` | `AgentHaloMobileDeviceDirectoryTests` (3) + shell/layout tests, 22 total; complete source Simulator target type-checking |
| Android | `cd src/native/android && ./gradlew :app:testDebugUnitTest :app:assembleDebug :app:assembleDebugAndroidTest` | `MobileDeviceDirectoryTest` (2), 132 total; two debug APKs |
| HarmonyOS | `bash src/native/harmonyos/build.sh` | `MobileDeviceDirectory.test.ts`, `DevicesLayout` AST gate (13 regression probes), `MobileStyle` gate, ArkTS compilation, and HAP packaging |

## Known Boundaries and Follow-up

- None of the four platforms had visual checks in a real multidevice environment. Desktop four-badge rows at 680pt, mobile long names/errors, and scroll position after expanding Advanced still required device inspection.
- Desktop identity matching merges directory `id` / grant `subjectDeviceID` with SPKI hashes only when they share a namespace. Actual records using different identifiers yield two rows for one computer: intentionally conservative, but visually repetitive, and requiring confirmation against real directory data.
- Phone rows do not show network membership: HarmonyOS's Master network member list is not yet projected per device to the phone, which has only its local membership-state card.
- Windows has only a development-preview window; implementing its devices page under the [desktop specification](../agenthalo-desktop-devices.md) should reuse the same projection semantics (the `MusterDeviceDirectory` rules and cases can be ported directly).
- macOS Advanced no longer calls `MusterMobileGrantRow`; it remains for cleanup.
