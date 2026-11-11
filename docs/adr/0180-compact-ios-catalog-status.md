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

## Release evidence

The public download page offers macOS and registered-device iOS 1.1.35 (125) from main commit `89e18f07295f845a1e2fed05b8bbab3710b9f154`. The macOS daemon stamp, release ancestry, Apple notarization, stapled ticket, Gatekeeper and update-feed signature passed verification. The iOS archive passed strict signature verification and its IPA matched the package named in the public manifest.

| Artifact | SHA-256 |
| --- | --- |
| macOS portable | `e5b30e72304ca5a42d18bef759596c6d2c13ddbfe75923c4057ca41f9c9013d6` |
| macOS full update | `e4891f5fd354c47e442356665eda1e694625dd343a98f2c15b7519c58967723a` |
| iOS OTA IPA | `0ca83c0f7493c94a51c38ff222392107a83e6265bae548591a91af229143cab0` |

All 14 files matched the staged hashes over public HTTPS. The download and install links and cache policy were also verified. No physical iPhone was connected for verification of the updated build.
