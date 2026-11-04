# ADR 0111: Native organization through AX with file confirmation

- Date: 2026-11-03
- Status: Accepted; implemented, native menu acceptance pending
- Related: [ADR 0099](0099-local-claude-identity-and-permission-cards.md), [ADR 0110](0110-native-groups-paging-and-session-entry.md), ARCHITECTURE.md §3.6.2
- Renumbered from 0106 when [ADR 0106](0106-deepseek-harness-session-adaptation.md) landed on `main` first;
  the decision and its evidence are unchanged.

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

### Disposable-session acceptance follow-up

The owner authorized creating a local Claude test conversation on the selected
Mac. Native AX creation returned a unique local UUID receipt. The first Pin
operation was rejected before mutation: the live menu uses AXGroup wrappers
around AXMenuItem nodes, not AXMenu. A row popup is additionally wrapped in its
own AXGroup beside the row button. Recognize menu-only AX subtrees outside the
menu bar, bind actions to a unique enabled menu item, and traverse bounded row
wrappers before the existing UUID verification. Continue refusing pre-existing
menus and ambiguous controls. Test nested/wrapped menu items, menu-bar exclusion,
incomplete snapshots and duplicate labels, then repeat native file-confirmed
operations on the disposable session before archiving it. This extends the same
AX-only decision; no provider API or store mutation is an alternative.

The corrected menu selector executed Pin, but its receipt stayed unknown because
Claude persists local pin membership as `isStarred` in the UUID-mapped registry
file. `pinnedOrder` alone is ordering data, not current membership. Read the
explicit registry boolean, including false. A missing field is not a pin;
ordering cannot establish membership or authorize a no-op. Invalid/unreadable
registry state must not confirm unpin.
The native UI and read-only registry confirmed the first Pin was applied, so it
must not be replayed. Verify unpin/pin transitions on the same test UUID after
fixing file interpretation, then restore its baseline before archive.

Section creation also exposed asynchronous AXValue publication: the immediate
read was empty, but a later AX snapshot contained exactly the requested name.
Wait for the expected value and enabled Create control under the existing UUID
and input-generation guards; reject any different nonempty value. The failed
attempt only filled a draft and did not create a section. Do not take over that
pre-existing dialog in a later request.

Native device follow-up confirms unpin, pin, restore-unpin and section creation
with both applied file receipts and catalog readback. Moving into the test section
stopped before mutation because AXPress on the submenu trigger did not expand it.
Use AXShowMenu when exposed, otherwise focus the exact verified menu item and send
application-directed AX Right Arrow only after exact item/window focus readback.
No global keyboard routing, pointer event or native-store write is introduced.
Submenu expansion and remaining membership/archive acceptance are still pending.

[ADR 0113](0113-pin-only-session-catalog.md) supersedes named-group presentation
and Section/Project UI controls at the owner's request. Native data is preserved;
Section acceptance is paused.

## Archive device evidence

On an unlocked authorized Mac, archiving a disposable local Claude session
through the organize operation returned a confirmed receipt for that session id;
the provider's own desktop session record then reported it archived and the
catalog no longer listed it. Pin and unpin were accepted earlier in the same way.

Archiving a session on a *locked* member Mac is not covered by this evidence.
Organize is not a read-only inspection, so it needs a Presence-signed Locked Use
operation, which the command line cannot issue; that case belongs to client
acceptance. Section and Project operations stay paused per
[ADR 0113](0113-pin-only-session-catalog.md).

## Target-scoped input generation

The organization guard read a generation of all local input. Merging
[ADR 0115](0115-target-scoped-local-input.md) narrowed that reading to the
input that could reach a given app, so the guard now reads it for the
controller's own process, as the chat and question paths already do. The guard
is unchanged in what it rejects: a generation that moved between the plan's
snapshot and a press still stops the operation. Input the owner directs at an
unrelated app no longer counts as that interference.

## Protocol field renumbering

The 1.1.20 (110) candidate built from this branch carried organization as
request field 74 and intent field 93. [ADR 0116](0116-failure-screenshot-analysis.md)
landed on `main` with those two numbers for its own vision request, so
organization takes the next free number in each oneof — 75 and 94 — and both
proto files say what the old numbers were and why they moved. Nothing addresses
these fields by number in Rust or Swift, and the signed Locked Use operation
keeps its own number, so no lease digest changes. Only the pre-release installs
on the authorized Macs and the OTA build of 110 speak the old numbers; the next
release replaces them, and until then an organize request from one of them is
not decodable as a vision request and is refused rather than misread.
