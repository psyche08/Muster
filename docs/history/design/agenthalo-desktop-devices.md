# AgentHalo Desktop Devices Page

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Status: **New candidate design, not implemented**, 2026-09-21. macOS / Windows share this specification; data, permissions, and recovery rules across all platforms follow the [device-page architecture](../architecture/agenthalo-device-page.md). The previous four-source badge layout is archived in the [historical record](history/agenthalo-device-page-2026-09-21-v1.md); its test results do not establish that the new design passes.

The [interactive preview](agenthalo-devices-preview.html) uses fixed sample data and supports switching among five platforms, roles, network states, and device details. It does not connect to real devices or perform pairing or permission changes.

## Page Structure

The page comprises network overview, needs attention, this device, network devices, and history/diagnostics. Everyday tasks are finding devices and using services; network policy is accessed through details, and the list is not organized around the local Owner authorization ledger.

```text
Devices                                                  Refresh  Add Device
My device network · Joined             Authorization valid · Last sync 17:20

Needs attention: mac-b is not ready to offer services               View reason

┌ Device list (about 1/3) ─────────────┬ Device details (remaining space) ───────────────────────┐
│ This device                          │ mac-c · Computer · Network member                      │
│ mac-a · Network member               │                                                         │
│                                      │ This device can use on the selected device              │
│ Network devices                      │ View sessions             Available                     │
│ [All] [Usable] [Attention]           │ Send messages             Available                     │
│                                      │ Connect terminal          Not wired in this version     │
│ mac-c · Network member ← selected   │                                                         │
│ Available: view sessions, send       │ Selected device access to other devices                 │
│                                      │ View / Send / Respond to approvals                      │
│ mac-b · Network member               │ Scope: network targets, not one computer                │
│ Not ready to offer services          │ Enrollment permissions · Independently verified locally │
│                                      │                                                         │
│ Phone · Network member               │ Connection and services                                 │
│ Accesses devices; no remote services │ Valid member · Service ready · Direct 3 ms              │
│                                      │                                                         │
│ ▸ History and compatibility          │ ▸ Technical details                                     │
│ ▸ Network diagnostics                │ Management actions (as allowed by Shared)               │
└──────────────────────────────────────┴─────────────────────────────────────────────────────────┘
```

Services and permissions above are examples. Availability comes from Shared; the UI must not combine membership, online, or scope booleans to derive it.

## Space and Actions

- Use list/detail columns at available widths of at least 960pt, with a 300–360pt list. At 680–959pt, use one column; tapping a device opens details, and Back preserves filters, selection, and scroll position. Thresholds are layout parameters and do not determine business capabilities.
- Keep only Refresh and one primary entry matching the local role in the header. Unenrolled computers show Create/Join; the creator shows Add Device; members show Invite Device only if they have invitation capability.
- List entries use at most two lines; long device names wrap, and full detail errors can be copied. Direct/relay and RTT belong in connection details, not every row's primary status.
- Needs Attention appears only when user action is required, with progress and original errors beside Continue/Recover actions. Ordinary offline devices are not universally red.
- Tapping a row opens details and does not directly invoke service operations with side effects. Entering a session/terminal is explicit; permission edits are not submitted immediately through list toggles.
- Diagnostics are collapsed by default, but current failures must remain visible. Old pairings and removed-device history identify their sources separately.

## Roles and Permissions

The creator has corresponding duties only during device enrollment authorization; the everyday list does not emphasize administrative ranks. All members can view network permissions and target capabilities; creator identity does not provide a permanent permission-management entry. Phones appear normally as members, and not offering services is not a failure.

Permission editing must always show the accessor and scope: "This member accesses targets in this network that pass membership verification." The current protocol cannot configure individual targets, so there is no "Allow access only to mac-c" checkbox. Future device-pair policies require protocol/ABI support before extension.

Enrollment authorization has five distinguishable stages: draft, diff confirmation, system authentication, signed save, and synchronization result. Governance for later permission changes is defined separately, without assuming permanent creator administration. Without a real write API, use read-only content and a clear reason instead of actionable toggles. Terminal, desktop control, and locked control are independent; session writes cannot substitute for them.

The add flow selects a computer or phone according to the user's goal; progress comes from transactions retained by Shared. Joining and service readiness are separate. An ordinary computer needs no extra Set as Owner step to be readable. After enrollment, Connect to Another Network is no longer a routine menu item.

## Presenting Offline State

When any device goes offline, including the creator, only that device's row becomes temporarily unreachable; other online devices retain normal available services and access entries. The creator is also an ordinary access target when online. The network overview does not turn red when the creator is offline; only incomplete enrollment requests may show Waiting for Enrollment Authorization. Enrolled members must not be told to wait for creator renewal before continuing access.

The preview has separate Creator Offline and Ordinary Member Offline scenarios, allowing inspection of both offline and still-available targets. Continued mutual access includes restarts and credential maintenance; the current code's dependence on Master renewal deadlines is an implementation gap, not a new product limitation.

## macOS

Use existing SwiftUI pages, native sheets, and system authentication callbacks. Retain the existing device-page authentication gate and retry on failure; refreshing does not repeatedly trigger the same already-satisfied page authentication. The new design does not authorize removing current security gates. OS adapters continue providing hardware keys and protected storage, while business state moves to Shared.

## Windows

Follow the same information architecture and wording. Support keyboard up/down selection, Enter for details, focus restoration on Back, and screen-reader output for name, role, state, and disabled reasons. Sensitive management actions use Windows Hello callbacks required by Shared; startup and ordinary refresh gain no unsupported authentication requirements.

This remains a development preview. Until CNG/TPM, Hello, protected persistence/exact readback, pairing, and device ABI are wired, show actual gaps; sample data must not enter production lists. See the Windows preview's README, removed on 2026-10-02 (`git show acf23be7:src/native/windows/README.md`).

## Acceptance

Shared acceptance follows the [architecture checklist](../architecture/agenthalo-device-page.md#10-implementation-and-acceptance). Desktop checks additionally cover 680/1024/1440 widths, long names, keyboard and screen readers, detail-focus restoration, window closure during permission changes, and late callbacks.

This round validated only documents and the illustrative preview. No Swift/C++ clients changed, and no product builds, signing, notarization, installation, or real cross-device E2E were run.

Static checks passed for local documentation links, HTML ID/DOM references, JavaScript syntax, and Git whitespace. Browser URL safety policy rejected opening local HTML and was not bypassed; browser visual and interactive acceptance of the preview remains outstanding.
