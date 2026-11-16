# ADR 0219: Publish Apple 1.1.46 (136)

- Date: 2026-11-16
- Status: Accepted
- Related: [0211](0211-release-apple-1-1-45.md), [0216](0216-background-approval-window.md), [0218](0218-removed-device-readd.md)

## Context

The operator requested a paired release including the merged desktop, transcript, device management and session changes, and the pending device readmission PR. The last Apple build was 1.1.45 (135).

## Decision

Build macOS and iOS 1.1.46 (136) from the release merge on main. Include #210 and #235 through #239. Publish the signed and notarized Apple Silicon portable on GitHub, the signed desktop update and registered-device iOS OTA on the download page, and a separate App Store export for TestFlight. External beta submission follows the operator's choice for this build.

## Alternatives

Reusing build 135 would prevent an unambiguous update. Shipping the earlier Apple release would omit the requested changes.

## Consequences

Device readmission requires a fresh administrator decision. Updating both endpoints gives them matching roster rules. The executor update carries the background permission card fix.

## Verification

Verify merge ancestry, project versions, the embedded daemon revision, signatures, notarization, signed feed and public package hashes. Check the OTA manifest and App Store Connect processing and beta group state separately. The paired-daemon readmission test covers removal, a fresh invitation and approval; the background read acceptance is recorded in ADR 0216.
