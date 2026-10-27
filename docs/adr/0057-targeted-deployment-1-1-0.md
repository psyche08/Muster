# ADR 0057: Targeted deployment of 1.1.0 (90) and device acceptance

- Date: 2026-10-27
- Status: Accepted
- Related: [0055](0055-clear-every-input-before-a-submission.md),
  [0056](0056-bounded-scans-skip-transcripts-and-retry-vanished-nodes.md),
  [0052](0052-targeted-multica-deployment.md) (the guarded targeted process),
  [deployment runbook](../DESKTOP_UPDATES.md)

## Context

ADRs 0055 and 0056 fixed the send failures found in the three Macs' daemon
logs: drafts merged instead of replaced, refusals over a draft in a question
field, `CoverLost` stops on merely-stale cover reports, a destroyed submit
control counted as ambiguous, a session scan that walked every transcript into
the node ceiling, and a control scan that gave up on one vanished node. Both
are source changes with unit coverage only. The owner asked for the version to
become 1.1.0 and for the change to be deployed and accepted on the devices.

The fleet is not uniform. The Mac Mini and M1Max run the published 1.0.94 (85).
M4Pro runs 1.0.98 (89), built from the unmerged branch
`codex/native-runtime-integration` (daemon stamp `92517da9`) — another line of
work under test there.

## Decision

Build 1.1.0 (90) from clean committed source containing ADRs 0055 and 0056,
with the guarded, Developer ID signed and notarized portable-bundle process of
ADR 0052, and install it on the owner's Macs. Build number 90 stays above every
build in the fleet, including M4Pro's 89, so no installation is a downgrade.

This is a targeted installation, not a publication: the public desktop feed and
the iOS OTA pair stay unchanged, and `released-commits.txt` records only
published versions. M4Pro is replaced only with the owner's explicit consent,
since that would remove the unmerged branch build under test there.

Device acceptance exercises what the fixes change, through real provider
conversations on an installed build: a send into a composer that already holds
a draft, a question answered while its field or the composer holds text, and
a send with several conversation views open.

## Alternatives

- **Reuse 1.0.97–1.0.99.** 1.0.97 and 1.0.98 are already taken by builds from
  another branch, and reusing a number for different source obscures
  provenance.
- **Publish the update feed.** Exceeds the request, and each macOS publication
  needs a paired iOS build.
- **Install over M4Pro unasked.** Would silently replace another branch's build.

## Consequences

The targeted Macs advance to 1.1.0 independently of the published 1.0.94 pair.
In-app updates on them keep offering nothing newer until a release is
published. No old application bundle is retained.

## Verification

Pending: release build, provenance and Presence gates, notarization, guarded
installation, unchanged device identity, service recovery and the device
acceptance described above.
