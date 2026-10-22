# ADR 0016: Suspend hosted Actions when the owner exhausts the budget

- Date: 2026-10-22
- Status: Accepted
- Related: Supersedes the enabled-setting decision in [ADR 0013](0013-ci-must-be-askable.md); AGENTS.md landing rules

## Context

The owner explicitly requested disabling GitHub Actions because the allowance
was exhausted. This is a repository setting, independent of workflow triggers.

## Decision

Disable Actions for `psyche08/Muster` through the repository permissions API.
Check for queued/in-progress runs and cancel any still consuming the allowance.
Do not automatically re-enable it or dispatch runs while this decision applies.
Continue proportional local checks and the ADR/PR landing workflow. Report
hosted CI as disabled, never as passed. This supersedes only ADR 0013's decision
to keep Actions enabled; its distinction between missing checks and passing
checks still applies.

## Alternatives

Removing workflow files loses useful checks and does not represent the actual
repository switch. Leaving Actions enabled would keep consuming the allowance.

## Consequences

PRs have no hosted CI coverage until the owner authorizes re-enabling it.
Local tests do not establish Linux, Windows, signing, installation or device
acceptance unless those checks are actually run and reported separately.

## Verification

On October 22, GET after PUT returned `enabled: false`. Separate queries for
`in_progress` and `queued` returned no runs, so no cancellation was necessary.
No build, signing, publication or installation is part of this setting change.
