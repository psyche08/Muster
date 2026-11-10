# ADR 0180: Keep status on the title line of iPhone catalog rows

- Date: 2026-11-10
- Status: Accepted
- Related: [0178](0178-consistent-session-list-rows.md), [0179](0179-publish-unified-apple-catalog.md)

## Context

An iPhone running the unified catalog shows a large gap between the title and metadata of working sessions. Idle rows in the same list have ordinary two-line heights. The existing width check uses macOS and compares providers in the same state.

## Decision

Give the catalog status its own intrinsic single-line presentation on iOS. Keep the status meaning accessible, respect Dynamic Type, and let search excerpts and the process hint grow independently. Distribute the correction as Apple 1.1.35 (125).

## Alternatives

A fixed height for the entire row would clip legitimate additional content. Provider-specific layouts would reintroduce the inconsistency addressed by ADR 0178.

## Verification

Host actual catalog rows inside an iPhone List. Compare the running, waiting and idle states, including a family disclosure and a nested child. Check the resulting row heights and rendered screen. Record simulator, physical device and delivery results separately.

At both 320 and 390 points, the iOS 27 simulator measured the original content at 44.7 points for idle, 139.7 for running and 155.7 for approval. Setting the catalog badge's label style to title-and-icon overrides the List's automatic presentation. The same test passed at both widths with a three-point tolerance against the idle content. The rendered screen showed both active state captions beside their icons. The supplied phone image documents the original defect; physical acceptance of the corrected build is pending.
