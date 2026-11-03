# ADR 0108: Show only Pin and the ordinary session list

- Date: 2026-11-03
- Status: Accepted; verification pending
- Related: [ADR 0105](0105-native-groups-paging-and-session-entry.md), [ADR 0106](0106-native-organization-ax-and-file-confirmation.md), [ADR 0107](0107-silent-native-catalog-read-failures.md)

## Context

The owner finds Section and Project grouping unclear and requests disabling both,
keeping only Pin. This supersedes ADR 0105's named-group presentation and ADR
0106's Section/Project user controls, without deleting native data.

## Decision

Shared Rust partitions the catalog into Pin and Recent. Every unpinned session,
including members of native sections/projects, appears in Recent. Normalize old
peer group descriptors into the same two display categories while retaining their
original source cursor keys for continuation requests. Keep ten main sessions per
page and include Claude children without consuming slots. Search and UUID identity
remain unchanged. Hidden section renames must not invalidate ordinary pagination.

Remove native group labels, group management menus/dialogs and Section swipe
controls. Shared Rust rejects stale Section/Project UI intents. Pin/unpin and the
existing Archive action remain available. Preserve provider groups and native
metadata internally; this change never deletes or moves native sessions/groups.
Keep read failures silent as specified in ADR 0107. Pause native Section acceptance.

## Alternatives

Hiding only the group header would leave fragmented pages and confusing controls.
Deleting native groups would change the user's original applications. Filtering
out grouped sessions would hide conversations. None matches the request.

## Consequences

Native sections/projects remain in the provider but do not organize Muster's UI.
Old peers may supply several source pages for the single Recent display group.
The existing per-device/provider/UUID mutation guard remains in force.

## Verification

Test mixed native sections/projects, pinned membership, old-peer group cursors,
search, ten-root paging and child inclusion. Verify that no group control targets
are projected and stale group intents cannot dispatch. Run Rust checks and Apple
builds. Report SSH installation, iOS OTA publication and physical acceptance as
separate evidence; no GitHub publication, TestFlight or notarization.

Source checks: 24 catalog/search/paging tests passed (one existing ignore), and
strict Clippy passed. Earlier native-device acceptance confirmed Pin/unpin and
new-section creation. Move did not produce a mutation; Section acceptance is now
paused by the owner. Archive acceptance is still unperformed. Candidate 1.1.17
(107) requires Apple builds, SSH installation and OTA publication below the
source-verification boundary.
