# ADR 0057: Bind Codex creation to the selected project identity

- Date: 2026-10-27
- Status: Proposed
- Related: ADR 0055, ADR 0056, PR 52; supersedes name-only project resolution

## Context

Multica allocates independent task roots ending in `workdir`. Codex assigns
that basename to each project. Requiring a globally unique visible name rejects
later tasks even when their selected directory is correct. The installed
Codex desktop version also persists `selected-project` with a local project ID;
its new-thread directory route updates that selection before rendering the form.

## Decision

Shared Rust resolves the selected local project ID, then requires the exact
visible name and a single root equal to the authorized absolute directory.
A matching name elsewhere is never evidence. Missing, remote, stale, malformed
or wrong-root selections refuse submission; there is no name-only fallback.
Retain the selected identity when the owned new form becomes ready and require
the same identity before draft mutation and sending, rereading provider state.

The provider state corroborates the guarded new-window directory route; it is
not independent authorization to send into an arbitrary existing form. Preserve
signed process, console/input generation, owned window/page/editor identity,
empty draft, local Codex mode and actual provider receipt requirements. Other
project names and paths are not modified. Claude's exact folder proof is unchanged.

## Alternatives

Accepting any same-named project with a matching root cannot establish which
project is selected. Renaming projects or creating filesystem aliases changes
user state unnecessarily. UI project rows expose names and decorative selection
icons, not a stable accessible project ID; positional inference is rejected.

## Consequences

Independent same-basename task directories can pass exact directory checks.
Unsupported provider state fails closed. Existing-task cold-start exclusions
remain in effect. New private acceptance tasks and directories must be removed
once their receipts have been recorded.

## Verification

Test same-name selected roots, a wrong selected root despite another matching
project, missing/remote/stale selections, multiple roots and identity changes.
Run affected Rust tests, lint, ADR and public-content gates. Separately record
build/signing/notarization, M4Pro installation, two independent scheduler tasks
with the same basename, actual provider receipts and cleanup before landing.

The agent/platform library suites passed 278 tests with four environment cases
ignored. An initial sandboxed run could not create a temporary Unix socket;
the normal-permission rerun passed. Device acceptance is still pending.

Strict Clippy, public-content and ADR gates passed. Candidate `92517da` was
built and signed as 1.0.98 (89). Apple notarization
`16e0975d-3c9c-4775-aaba-fe23e409cc77` was Accepted; stapling and Gatekeeper
validation passed. It was installed on the designated desktop host with daemon
SHA-256 `e6f03946f7decb12df5d7b811d89d512458fbd2b787bab7c647b46e7193fbb82`.
Device identity, Accessibility, Relay and Vnet recovered. Container supervision
restored the existing runtime configuration. Public desktop/iOS feeds were not
changed. Repeated-directory task receipts are still pending a quiet console.

Two 45-second input observations failed to obtain 15 continuous quiet seconds,
so no new scheduler acceptance issue or prompt was submitted. Four empty test
forms/shells were closed with draft/busy/input guards. The two new calibration
directories and the temporary keep-awake process were cleaned. One leftover
REPL whose working directory was under the test-only root was stopped. The provider's
saved project metadata/history was not rewritten. PR 52 remains a draft until
actual same-basename task receipts pass; installation is not that acceptance.
