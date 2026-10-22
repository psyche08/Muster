# ADR 0025: Ignore the scheduler token that sits in the working tree

- Date: 2026-10-22
- Status: Accepted
- Related: [ADR 0009](0009-task-contract-and-runtime-manifest.md) is the work
  that put it there; [ADR 0015](0015-offering-a-task-runtime-is-opt-in.md)
  governs whether this machine serves at all
- Numbering: written as 0016 in the repository this came from, where that number
  was free. It arrived here alongside
  [ADR 0016](0016-suspend-hosted-actions-budget.md), which already held it, so
  this record takes the next free number instead.

## Context

Connecting this machine to an external task scheduler needs that scheduler's
personal access token. During the ADR 0009 work one was saved as `.multica` at
the repository root — a 45-byte `mul_…` credential, untracked and matched by no
rule in `.gitignore`. The file was never committed, so there is no history to
rewrite; the exposure is prospective, and one `git add -A` is all it takes.

`.gitignore` already has a section for exactly this kind of material — `*.key`,
`*.token`, `*.p12`, `/keys` — so the omission was an oversight, not a position.

## Decision

Add `/.multica` to that section, anchored at the repository root, where it
covers both a file and a directory of that name.

This is not storage advice. The scheduler's CLI keeps its own configuration in
`~/.multica`, outside any checkout, and that remains the right home for a
credential. The rule exists because the working tree is where this one actually
is, and because the next person to follow the setup guide will put it in the
same place.

Renumbering also exposed a gap in the gate that is supposed to enforce these
records. `scripts/check-adr.sh` looked for added or modified numbered records
(`--diff-filter=AM`), and git reports a renumbering as a rename, so the gate
could not see this change at all. It now counts renames too (`AMR`).

## Alternatives

- **Move the file and add no rule.** Rejected on its own: it fixes this copy and
  not the next one, and the rule costs a line.
- **An unanchored pattern** such as `*.multica*`. Rejected: it would also hide
  files deeper in the tree, where a `.multica` is more likely to be something
  someone meant to commit.
- **A pre-commit secret scanner.** Deferred: worth having, much larger than this,
  and it would be a decision about everyone's workflow rather than one path.

## Consequences

- The token stops being one `git add -A` from publication.
- It also stops appearing in `git status`, which is the trade: an ignored
  credential is easier to forget than an untracked one. Nothing here encrypts
  it, scopes it, or expires it — it is still a live token any process running as
  this user can read, and rotating it is the only thing that undoes a leak.
- A `.multica` that someone genuinely wants committed now needs `git add -f`,
  which is the right amount of friction for a path named after a credential.
- The gate accepting renames means a change that only renumbers a record now
  satisfies it, which is correct, and that a rename of an unrelated record would
  also satisfy it, which is the same latitude it already gives a one-word edit.

## Verification

- `git check-ignore -v .multica` names the new rule (`.gitignore:48:/.multica`).
- `git ls-files --error-unmatch .multica` fails, confirming the file was never
  tracked: ignoring it is sufficient, and no history purge is needed.
- CI: this change touches only `.gitignore` and `docs/`, which the workflow's
  path filters do not match, so no run reported for the pull request. A verdict
  was asked for explicitly through `workflow_dispatch` — the case
  [ADR 0013](0013-ci-must-be-askable.md) added it for. That route is closed in
  this repository: hosted Actions are suspended under
  [ADR 0016](0016-suspend-hosted-actions-budget.md), so neither a push nor a
  dispatch answers here, and the verification above is the local evidence
  instead.
