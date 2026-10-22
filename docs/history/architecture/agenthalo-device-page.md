# AgentHalo Devices page: network, devices, and permissions

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Status: 2026-09-21 **Candidate design, not implemented or accepted as a product**. Covers macOS, Windows, iOS, Android, and HarmonyOS. This supersedes the previous Devices-page specification centered on the local Owner authorization ledger. The [previous implementation and validation record](../design/history/agenthalo-device-page-2026-09-21-v1.md) is historical only and is not evidence that this proposal is implemented.

This carries forward the Claude network-decoupling decisions: upper-level Agent / Terminal declare required capabilities, and Muster Network decides centrally; the network authority issues member server-certificate authorization together with the grant during pairing. Basis: [network authorization and member services](agenthalo-network-authority.md), [single-network constraint](muster-single-network.md), [device-network mutual trust](device-network-trust.md). Layout: [desktop specification](../design/agenthalo-desktop-devices.md), [mobile specification](../design/agenthalo-mobile-style.md#devices-page).

## Confirmed principle: the creator only authorizes enrollment

On 2026-09-21, the user explicitly confirmed that the creator only authorizes device enrollment. Once joined, members access each other as peers through Muster. Any device going offline, including the creator, does not affect access between other online members. When online, the creator is also an ordinary target, not a mandatory gateway, network-wide liveness criterion, or ongoing authorization center.

Offline status affects only that device's reachability. Pending enrollment requests may wait for the authorizing device, but enrolled members do not wait for the creator to recover. The Devices page must not show “Network unavailable,” “Waiting for creator renewal,” or a network-wide warning merely because the creator is offline. Peer access still follows established service permissions and target capabilities; membership does not automatically grant every service scope.

The previous proposal to make the creator a permanent permission administrator is withdrawn. Post-enrollment permission changes/revocation governance must be defined separately. Management power cannot be inferred from the “creator” label, and this round does not invent a new governance mechanism.

## 1. Boundary between implementation and target

The source baseline is `1ba39750`, not the installed application's version. Review covers producers, FFI, consumers, and UI; a type's existence does not establish a complete production flow.

| Capability | Current source facts | Constraint on the new page |
| --- | --- | --- |
| One network per device | Creator and joiner roles cannot coexist | After joining, no normal entry point for creating a second network or switching Owner spaces |
| Members independently serve | `00a9793d` only adds a broker `member-enrollment` slot and read interface | Pairing-material production and member-listener bootstrap are still missing; paired/enrolled does not mean service-ready |
| Member permissions | `58c10fff` validates each member's scope set in v3; backend consumes the requesting member's set | Current permissions are requester-member-level, not per-target policy; do not offer an editable device-pair matrix |
| Policy production and editing | `1ba39750` adds scoped drafts, C ABI, and Swift bridging; the old automatic draft still produces v2 | Low-level entry points do not prove complete permission UI, signed persistence, and propagation on all platforms, nor define permanent creator management power |
| Remote terminal | `POST /terminal` declares `TerminalConnect`, but the network branch of `resolve_verified_mst2_authorization` still maps only three AgentHalo operations | Show terminal availability only after distribution, operation resolution, and actual execution all connect |
| Continued access among existing members | Current distributions still expire after 90 days, with Master-signed renewal | This is an implementation gap against the above target; credential maintenance must cease depending on an always-online creator, without skipping expiry/signature checks |
| Existing Devices page | macOS and the three mobile platforms still merge multiple sources independently | Shared supplies identity merging, roles, permissions, and action eligibility; platforms only lay out the new page |
| Windows | Developer preview only, lacking a full Devices page and identity/pairing flow | This document defines a target, not existing Windows production capability |


Parallel commit `1ba39750` already includes `distribution_draft_scoped`, `agenthalo_device_network_distribution_begin_scoped`, and Swift bridging. This round did not modify or validate that runtime code. The commit also contains the enrollment-role/offline-independence principles written in this round. New low-level capabilities neither change the confirmed product principles above nor prove availability on all platforms.

Source anchors: `src/rust/crates/muster-transport/src/device_network_distribution_v2.rs`, `src/rust/crates/agenthalo-ffi/src/device_network.rs`, `src/rust/crates/agenthalo-backend/src/remote_authorization.rs`, `src/rust/crates/agenthalo-backend/src/lib.rs`. Recheck the baseline before implementation.

## 2. User model

The page answers three questions: **Which network am I in, what can each device do, and what needs my attention?**

- The creator only authorizes enrollment; after enrollment it is an ordinary member that can also provide or access Agent / Terminal services. Service capability is not determined by whether the local device is Owner.
- A device can both access others and serve them. Permissions must state direction: “This device can use on this target” and “This device can access other devices.”
- Membership, network permissions, target service capabilities, and connection results are independent facts. Joining is not service readiness, a direct connection is not authorization, and cached sessions do not mean online.
- Any member going offline affects only itself, including the creator. Other online members discover, verify, and access each other independently through Muster without traversing it. Long-running operation and restarts must not make the creator required again.
- Agent / Terminal must not build separate authorization tables. The UI displays network permissions; enrollment authorization is confirmed in its corresponding flow, while Shared and the target validate each execution.

## 3. Information architecture

All platforms share one structure. Desktop shows list and detail side by side; phones navigate from list to detail:

```text
Devices                                      [Refresh] [Add device]
Network: My device network · Joined
Authorization valid · Last synced 17:20        [Network details]

Needs attention (only when items exist)
  mac-b joined; serving not yet ready           [View reason]
  New phone awaiting code comparison           [Continue]

This device
  mac-a · Network member · This device
  Offers to network: Agent sessions, terminal  [View this device]

Network devices                               [All / Usable / Needs attention]
  mac-c · Computer · Network member
  Available: View sessions, send messages      [Details]
  mac-b · Computer · Network member
  Serving not yet ready                       [Details]
  Phone · Phone · Network member
  Access device; no remote services offered    [Details]

▸ History and compatibility records
▸ Diagnostics
```

This example is a target design, not live device state. “My device network” is a display name, not identity; without a verified name, show “Device network · short identifier.” If no network exists or only legacy pairing has recovered, show the corresponding empty state rather than constructing a network from legacy authority lists.

The local device appears once in its own section; list counts include only other active members. Removed devices belong in history. Legacy pairings without confirmed membership are labeled separately and excluded from member counts. Members must remain visible without service announcements. Unknown device types display “Device,” not a default “Computer.”

Each row has at most two lines: name/type/role, then a Shared-supplied capability summary or blocker. Links, RTT, full permissions, session counts, and fingerprints belong in details, avoiding four protocol badges crammed into one row. Names may wrap, and statuses use text as well as color.

“Usable” and “Needs attention” classification/counts come from the same Shared snapshot. Ordinary offline status, lack of offered services, and historical removal do not automatically count as failures. Required authentication continuation, protected-state recovery, or conflict resolution count as attention items. Text search, selected rows, and collapse preferences are platform presentation state.

## 4. Device details

Details follow the same order:

1. **Identity**: device name, computer/phone/unknown, local-device marker, network membership. Explain the creator's enrollment-authority role only in enrollment/verification details. Verification short fingerprints come from Shared; never merge by name.
2. **What this device can use on the target**: each item shows available, unauthorized, not offered by target, checking, or temporarily unavailable with a reason. Selecting a device changes navigation only, not access rights; revalidate handles on execution.
3. **What this device can access on others**: requester permissions, coverage, expiry, and sync state. All members use the same permission presentation; creator identity does not imply permanent management power. Future editing needs separately defined governance and Shared action authorization; no fake entry point now.
4. **Connections and services**: show membership record, authorization validity, service readiness, and current connection observations separately; timestamp the last successful observation. Missing state means “Not yet confirmed”; missing service announcement means “Service information not yet received,” without inferring offline or unauthorized.
5. **Management**: Shared-provided invite, remove, resume recovery, and other actions. Destructive actions separately confirm affected subjects and propagation boundaries.
6. **Technical details**: redacted identifiers, distribution version, service errors, and connection diagnostics. Do not collapse current blockers or recovery buttons.

Phones are network members too. Not offering remote desktop services is neutral information; their access permissions remain inspectable. Show session counts only when explicitly returned by the directory. Missing records do not mean zero; historical counts do not determine online status.

## 5. Permission direction, scope, and effect

**Current code expresses requester-member-level permissions.** Permission details and enrollment confirmation must first make this clear:

> Requester: phone. Coverage: target devices in this network that pass membership verification, not one specific computer. Actual use also depends on services offered by each target.

Coverage includes targets that join later and pass verification. “The current two computers” must not imply a fixed set. Independent `(subject, target)` configuration needs a new signed format, migration, ABI, and server enforcement; adding a target picker to the current page is insufficient.

| Product label | scope | Constraint |
| --- | --- | --- |
| View sessions | `agenthalo.session.read` | Explicitly included in legacy v2 |
| Send messages and operate sessions | `agenthalo.session.write` | Not equivalent to terminal keyboard input |
| Respond to approvals | `agenthalo.approval.respond` | Not inferred from read access |
| Connect terminal | `terminal.connect` | Explicit grant; show unsupported until the full production path connects |
| Control desktop | `computer-use.execute` | Separate capability, also requiring target-platform support and OS authorization |
| Lock-screen control | `locked-use.execute` | Not enabled together with desktop control |

Scopes match `MusterScope::as_str`. Service scopes are separate from enrollment-authority identity. “Creator” does not imply daily management privileges, nor do “all service permissions” imply creator status. The UI does not automatically select dependencies; Shared returns validation errors or an explicit change plan for protocol dependencies.

New authorization candidates default to View sessions only; confirm all others explicitly during enrollment authorization. This is a product recommendation for the new flow, **not a reinterpretation of legacy v2**. Legacy v2 retains read/write/approval and displays “Existing network permissions retained.” Migration must show differences, obtain explicit confirmation, and actually distribute changes; unchecked UI boxes do not establish revoked permissions.

Enrollment permission persistence: edit draft → Shared validates and reports impact → user confirmation and OS authentication → hardware signature → exact persistence/readback → propagation and target acceptance. Show “Draft,” “Saved locally,” “Waiting for target sync,” and “Accepted by target” separately. Without receipts, show “Published; target effect unknown,” never infer target acceptance from successful send, timeout, or online lists.

## 6. Add, join, and remove

Without a network: computers offer “Create network” and “Join network”; phones offer “Scan to join network.” Missing hardware identity or ABI yields a reason, not a fake entry point. Existing roles cannot create a second network; conflicts enter recovery without automatically deleting protected state.

With a network:

- The creator uses one “Add device” entry point to choose computer (discovery and code) or phone (QR code); permissions are confirmed in the same user-presence ceremony.
- Ordinary members show “Invite device” only if Shared exposes invitation capability, labeled “Waiting for creator issuance.” Inviting does not confer certificate-issuance or policy-editing authority; disconnected paths are explicitly unavailable.
- Once enrolled, phones emphasize network details/device inspection rather than repeatedly prompting “Add to another computer.” Invitation forwarding also requires explicit Shared support.
- Continue/cancel legacy directed pairing in compatibility records; in-progress transactions still rise to “Needs attention.” Redesign must not hide recovery entry points.

Target ceremony progress: discover/scan → compare on both ends → OS authentication → issue and persist → synchronize authorization → device joined. Computers additionally show “Preparing service/Ready/Failed.” This presents the target protocol; **sequential UI calls to legacy pairing and enrollment cannot establish atomic completion**. Shared must retain transaction management for certificates, grants, member records, and failure recovery.

Previously paired computers missing service material show “Joined; service setup incomplete.” Offer “Continue setup” only if a recovery API exists; otherwise explain the current version's gap without automatically re-pairing, clearing identity, or creating another Owner.

“Remove from network,” “Revoke legacy directed authorization,” and “Remove local display record” are distinct actions and must not all be called Delete device. Network removal shows local persistence, confirmed propagation, and offline pending sync separately, without promising immediate enforcement on offline targets. Do not offer execution entry points for leaving/dissolving a network or future permission governance without explicit rules and a complete Shared flow. This design does not authorize data cleanup.

## 7. State and recovery

| Scenario | Main text | Actions and boundaries |
| --- | --- | --- |
| Initial load | Reading network state | Preserve the current list; mark restored snapshots pending refresh |
| No network / legacy pairing only | Not joined / Network membership unconfirmed | Create/join or view compatibility records; do not count as members |
| Enrollment awaiting issuance | Waiting for network creator | Preserve transaction; Shared supplies retry/cancel |
| Valid member, service not ready | Joined; serving not yet ready | View reason/continue setup; do not prompt Owner creation |
| Directory expired or load failed | Device information not yet updated | Preserve verified members, show timestamp and retry |
| No service capability | This device does not offer this service | Neutral explanation, not a permission failure |
| Explicitly missing scope | Unauthorized | View network permissions; do not reauthorize separately in each app |
| Any device offline (including creator) | Temporarily unreachable on that device's row only | Other online members retain normal access; no network-wide warning without pending enrollment approvals |
| Expired / revoked / identity conflict | Authorization expired / Removed / Identity needs verification | Stop actions, without falling back to old Grants or automatically trusting again |
| Persistence outcome unknown | Persistence outcome awaiting confirmation | Read back/recover the original transaction; do not start another |
| Revocation propagation incomplete | Revocation saved; some devices awaiting sync | Show unknown coverage, not network-wide completion |
| Platform lacks feature | Unsupported in this version | Explain the specific gap, never render success |

Refresh only asks Shared to refresh; platforms must not add independent retry loops. Offline members, restarts, window closure, and late callbacks follow Shared lifecycle. Creator online status is not a precondition for other members' recovery.

## 8. Shared/platform contract (to be added)

The following are candidate interface shapes, **not existing ABI**:

```text
DevicePageSnapshot
  generation, network(identity, role, distributionState, managementState)
  localDevice, devices[], compatibilityRecords[], attentionItems[]
DeviceEntry
  stableIdentity, displayName, deviceClass, membership
  offeredServices[], permissions(scope, direction, coverage, source, validity)[]
  usability[], observationTime?, blockers[], actions[]
DevicePageAction
  opaqueHandle, kind, label, availability, disabledReason?, generation
```

Shared owns identity namespaces/merging, local-device identification, network roles, freshness, permission interpretation, service usability, counts, and action eligibility. Lowercasing and stripping prefixes from IDs in multiple JSON objects does not establish identity equality. `masterDeviceID != nil` does not prove local enrollment, nor does `readyCount > 0` establish member permissions.

Display snapshots contain no credentials and do not authorize. Actions bind device, direction, and generation through opaque handles and revalidate before execution. Platforms own layout, text search, QR/camera, authentication, hardware signatures, protected storage, raw transport, and lifecycle. Add capabilities in this order: Shared → C ABI → Swift/JNI/NAPI/Windows binding → native UI.

## 9. Platform integration points

| Platform | Page and interactions | Current entry point and prerequisites |
| --- | --- | --- |
| macOS | List + right-hand detail; push detail in narrow windows; native authentication and invitation sheet | `MusterNetworkView.swift`; replace business projections in `MusterDeviceDirectory.swift`, retaining existing authentication gates until separately changed |
| Windows | Same desktop structure; keyboard selection and focus restoration | `src/native/windows/src/main.cpp`; first implement hardware identity, Hello, protected storage, pairing, and device ABI; do not copy Swift inference |
| iOS | Devices Tab → detail → permissions; scan to join; two columns on wide iPad | `AgentHaloRootView.swift` / `AgentHaloMobileDeviceDirectory.swift`; use member-level projections, not authority counts as network identity |
| Android | Same page order; system Back restores list position; tablet two columns | `AgentHaloApp.kt` / `MobileDeviceDirectory.kt`; JNI consumes the unified snapshot; protocol cards move to diagnostics |
| HarmonyOS | Same order; foldables change columns by available width; retain scanning and recovery | `Index.ets` / `domain/MobileDeviceDirectory.ts`; integrate the Master card without rebuilding business state from five fields |

“All platforms” means these five product clients. Linux currently covers network/backend and future client adaptation; the repository has no standalone Devices GUI, so no sixth implemented page is claimed. A future GUI follows the desktop contract. CLI/backend output uses the same facts without a separate authorization model.

## 10. Implementation and acceptance

1. Remove creator-online dependencies from existing-member discovery, credential maintenance, and restart recovery while preserving signature, expiry, and revocation checks. Complete member service-certificate issuance/persistence, listener bootstrap/recovery, policy production/editing/distribution, and scope resolution throughout.
2. Shared provides unified projections/actions, with common golden fixtures covering roles, direction, coverage, unknown states, and invalidation; retire platform business projections.
3. Wire native pages in the four existing clients; Windows uses the same interface once prerequisites are complete. Report each platform's completion independently.
4. Preserve identity, pairing, signed distributions, and in-progress transactions. Label legacy authorization sources without silently promoting them to membership or changing permissions.
5. Separately accept source/build, signing/notarization (where applicable), installation, actual execution, and cross-device E2E.

Key scenarios: reading a joiner-only device; message sending allowed but terminal still denied without its grant; insufficient target capability; take creator/ordinary members offline in turn and verify bidirectional access among remaining members (including the creator as target when online); creator offline across renewal windows and remaining-member restart recovery; offline catch-up, revocation of old connections/queued operations, expiry, restart, and unknown persistence outcome; same names with different keys, unknown types, duplicated local device, phones without services, legacy pairing; late snapshots cannot revive revoked actions.

Layout acceptance: desktop logical widths 680/1024/1440; phone widths 320/360/390/430, large text, long Chinese names/errors, screen readers, keyboard, focus restoration, landscape/foldables. Touch areas at least 44pt on iOS and 48dp on Android/HarmonyOS; critical errors/actions do not depend on hover or color.

**This round delivers design documents and illustrative previews only.** Document checks are not product tests, and previous build records do not validate this proposal.
