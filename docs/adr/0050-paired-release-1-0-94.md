# ADR 0050: Land the outstanding branches and release them as 1.0.94 (85)

- Date: 2026-10-26
- Status: Accepted
- Related: [ADR 0049](0049-web-window-onto-one-origin.md);
  [ADR 0025](0025-ignore-the-scheduler-token-in-the-working-tree.md);
  [ADR 0047](0047-transcript-reading-anchor-parity.md);
  [ADR 0048](0048-admitted-send-acknowledgement.md);
  [desktop updates runbook](../DESKTOP_UPDATES.md)

## Context

Two reviewed branches were open against `main` and neither could merge on its
own: the ADR renumbering of PR #2 and the web window of PR #6. Both conflicted,
and PR #6 claimed request field `68` in `muster.proto`, which `main` had already
given to `agent_send_status` in a build that is installed on all three Macs.

At the same time the release ledger had fallen behind the devices.
`released-commits.txt` ends at 1.0.91 (82). 1.0.92 (83) was built and installed
but never published; the three Macs are in fact running two different 1.0.92
builds (`66a39fbf` on one, `93529e00` on the other two), which is exactly the
hazard the ledger exists to catch. 1.0.93 (84) was built, notarized and recorded
in ADR 0047 with exact hashes, but was never published and its retained
artifacts no longer exist. The public download page still offers macOS 1.0.91
and iOS build 82, and `releases.osx-arm64.json` returns 404, so in-app desktop
updates are currently unavailable fleet-wide.

The owner asked for the outstanding work to be merged and the latest version
packaged and delivered.

## Decision

Merge both branches into one release line and publish it as paired version
**1.0.94 (85)**.

Resolve the conflicts so that no shipped wire number moves: `agent_send_status`
keeps request field `68` and the web window's `forward_open` takes `69`. Its
response variant `forward_opened` keeps `68`, which was free in that oneof. The
checked-in Swift protobuf is regenerated from the resolved `.proto` rather than
hand-merged. The web window's record is renumbered from the taken 0026 to
[ADR 0049](0049-web-window-onto-one-origin.md), and every reference that belongs
to it moves with it; references to the unrelated ADR 0026 stay as they are.

Do not reuse 1.0.93. ADR 0047 records that number against exact ZIP and IPA
hashes, and this tree's content differs from it. A version number must keep
meaning what it meant when it was recorded.

Publish what can be published honestly. Velopack 1.2.158 cannot run here: `vpk`
is installed but no .NET runtime exists on any machine in the fleet, and the
owner declined installing one for this release. So this release publishes the
iOS OTA half only, and the macOS half stops at a locally built, signed and
notarized artifact. The unified download page therefore keeps advertising the
macOS version that is actually published, not this one.

## Alternatives

Merging only PR #2 would have left the web window unreleased and its conflict
to be resolved again later against a moved target. Renumbering
`agent_send_status` instead of `forward_open` would have changed a field number
that installed daemons already speak. Hand-resolving the generated protobuf
would have risked a file that no longer matches its source. Publishing a plain
notarized ZIP under the existing `Muster-osx-arm64-Portable.zip` name would have
broken the Velopack metadata that installed clients bootstrap from. Claiming
1.0.94 on the download page without publishing it would have made a public page
state something untrue.

## Consequences

`main` carries the web window and a unique ADR number per record again. The
desktop fleet stays on 1.0.92 and in-app desktop updates stay unavailable until
a .NET runtime is installed and a signed Velopack feed is published again; the
ledger does not gain a 1.0.94 desktop entry, because no desktop package was
published. The web window ships to devices without ever having had a dedicated
on-device acceptance pass of its own, which remains open on ADR 0049. The split
1.0.92 installations are recorded here but not repaired by this release.

## Verification

Source verification on 2026-10-26, from the merged tree before the version bump:

- `cargo check --workspace --all-targets` clean; `cargo test --workspace` passed
  564 tests with 0 failures and 7 ignored.
- `src/muster/scripts/check-layers.py`: 18 crates follow the layer map.
- `scripts/check-public-content.py`: 706 tracked text files, 0 findings.
- `scripts/check-adr.sh origin/main`: records present.
- `src/browser/agenthalo-extension/adapter.test.cjs`: 8 pass, 0 fail.
- The Swift protobuf was regenerated with `scripts/gen-protos.sh` (protoc with
  `protoc-gen-swift`), not edited by hand.

Build, signing, notarization, publication and installation are recorded below as
they happen. Nothing about them is claimed by this commit.
