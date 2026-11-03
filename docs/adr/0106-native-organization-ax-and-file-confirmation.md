# ADR 0106: Native organization through AX with file confirmation

- Date: 2026-11-03
- Status: Accepted; implemented, native menu acceptance pending
- Related: [ADR 0099](0099-local-claude-identity-and-permission-cards.md), [ADR 0105](0105-native-groups-paging-and-session-entry.md), ARCHITECTURE.md §3.6.2

## Context

The owner requests pin/unpin, native section creation/deletion, membership changes
and archive, through desktop context menus and mobile trailing swipe actions.
The owner explicitly requires AX plus files. Although the bundled Codex exposes
metadata APIs, those APIs are excluded from the selected control architecture.
Equal group names now aggregate presentation across multiple native owners.

## Decision

Extend the Shared Rust protocol and existing provider authorization, operation
ledger, serialized native controller and Locked Use binding. A mutation names one
verified local session UUID and, when needed, a provider-native section UUID.
Rust resolves current file metadata, account scope and available destinations.
Swift renders capabilities and submits an intent; it never writes provider stores.

Perform native controls only through AX in the exact verified session's window.
Use existing UUID navigation and surface verification. A sidebar candidate may be
opened to verify its UUID, but a label alone never authorizes a mutation. Ambiguous
rows, group labels, stale menus, changed input generations or account mismatches
fail closed. Never fall back to app-server, remote sessions, database writes,
CDP, injected JavaScript or coordinate clicks. Read native files after the final
AX action and report success only when the requested metadata change is observed.
An unknown outcome is not automatically retried or switched to another route.

Context menus and swipe menus expose the same Rust-owned actions. Aggregated
groups retain separate source targets; operations are directed to a named native
source rather than broadcasting by equal name. Removing a group must preserve its
sessions and project files; prompts that also delete files or discard work are not
automatically accepted. New-section names and existing destinations are validated
in Rust against the current provider/account catalog.

## Alternatives

Direct database writes can disagree with the running app. App-server metadata
mutations violate the owner's AX-only control decision. Treating menu opening or
AXPress success as persistence does not establish native synchronization. Moving
all equal-name groups together would conflate display aggregation with authority.

## Consequences

Native versions or languages with unrecognized AX controls report unavailable,
without modifying a different conversation. Locked operations keep existing
Presence, scope and lifecycle requirements. Empty groups need an existing local
session on the selected native source to bind the app/account context.

## Verification

Test request/digest binding, UUID and account validation, ambiguous selectors,
unsupported controls, file readback, unknown outcomes, and synthetic native AX
fixtures. Verify desktop context menus and mobile swipes through Apple builds.
Source tests, AX device acceptance, signing/notarization, Mac deployment and iOS
publication are separate evidence; no unperformed native mutation is claimed.

## Implementation evidence and limits

The protocol, provider control grant, Locked Use digest, operation ledger and
file-confirmed AX adapter are connected to macOS context menus and iOS trailing
swipes. Tests cover UUID mismatch, ambiguous native group names, unavailable
readback, stale legacy pin flags, create/archive confirmation and preservation of
sessions after group removal. Unknown outcomes are not replayed automatically.
The shared SwiftUI target compiles. Native menus were inspected read-only where
available; the local Mac subsequently locked and the UI inspection tool could no
longer read Claude. No real user sessions were moved, pinned, archived or deleted
for validation. Runtime versions exposing different or absent AX controls can
return unavailable and still require device acceptance. No app-server mutation,
provider-store write or coordinate input was introduced.

The final owner-selected distribution is signed macOS installation over SSH and
iOS OTA only. Do not push this turn to GitHub or upload to TestFlight. Apple
notarization is explicitly omitted for the Mac installation.

Final source checks: 580 affected Rust library tests passed (7 existing ignores),
one native-title entry/rescan integration test passed, Clippy with warnings denied
and the shared SwiftUI target passed. Real native mutations remain manual acceptance.

### Native AX selector follow-up

Observed Claude sidebar row buttons append directory and connection status to
the title, while the sibling popup has the exact `More options for <title>`
label. Resolve a unique popup within the sidebar and its single sibling row as
a navigation candidate, then verify the session UUID again before mutation.
Do not confuse it with the identically named conversation-header popup. Also
check actual native menu roles, submenu transitions and removal confirmation.
Use disposable native fixtures where the UI is available; do not archive user
work or bypass the protected Codex UI inspection restriction for acceptance.

Native follow-up inspection created and removed an empty disposable Claude section
through the native UI. The create button is disabled before the name is entered;
resolve it only after AXValue readback and a new snapshot. Section headings expose
a context menu, and the empty removal confirmation uses `Delete` plus `Nothing is
in it`. Match that button only inside the preserving dialog. Native menus expose
`Pin`, `Move to section`, and `Archive`; inspect these without changing work
sessions. Nested menus resolve only a unique leaf, and duplicate sidebar rows or
changed native account scope remain rejected. Group deletion still requires an
exposed AXShowMenu action when no labelled popup exists; no coordinate fallback.
These observations do not establish end-to-end Muster mutation acceptance.
