# ADR 0155: Remove the finish-session landing script

- Date: 2026-11-06
- Status: Accepted
- Related: [ADR 0001](0001-require-adr-for-changes.md) (ADR check),
  [ADR 0013](0013-ci-must-be-askable.md) (CI must be askable),
  [ADR 0118](0118-sweep-past-an-unreadable-worktree.md) (worktree sweep),
  [AGENTS.md](../../AGENTS.md#branches-worktrees-and-landing-work)

## Context

`scripts/finish-session.sh` landed a session's branch in one command:
1. it ran the ADR check and pushed the branch;
2. it opened a pull request if none was open, and waited for CI checks;
3. it merged with a merge commit and deleted the remote branch;
4. it ran `scripts/worktree-gc.sh`.

AGENTS.md required every session to land through it.

The operator asked to delete the script and stop using it.

On 2026-11-06, while landing PR #164, the script's first `git fetch` failed on
an intermittent SSH connection to GitHub in six of seven runs. Each failed run
stopped before it reached the pull request.

## Decision

- Delete `scripts/finish-session.sh`.
- AGENTS.md now lists the landing steps the script performed, as direct
  commands run from the session's worktree:
  1. run `scripts/check-adr.sh origin/main`;
  2. push the branch and open a pull request with an English title and
     description;
  3. wait for CI checks with `gh pr checks <number> --watch`;
  4. merge with a merge commit using `gh pr merge <number> --merge`;
  5. delete the remote branch;
  6. run `scripts/worktree-gc.sh`.
- The landing policy is unchanged:
  - land in the same turn;
  - merge with a merge commit;
  - never remove worktrees or branches by hand.
- ADR 0013's rule now sits in the AGENTS.md step itself: "No checks reported"
  is reported as CI not answering, not as a pass.
- `scripts/check-adr.sh` and `scripts/worktree-gc.sh` are kept.
- Earlier ADRs that mention the script stay as written, as history.

## Alternatives

- **Keep the script and add network retries.** This keeps one command, but the
  operator chose to stop using the script.
- **Stop merging in the session.** Leaving pull requests for the operator to
  merge would change the landing policy itself. The operator asked only to
  remove the script.

## Consequences

- Landing takes several commands instead of one. A failed step is visible, and
  can be retried on its own without rerunning the rest.
- Nothing runs the ADR check automatically before a merge. AGENTS.md requires
  it before opening and before merging the pull request.
- `gh pr merge` is not given `--delete-branch`, because that option tries to
  check out the default branch locally, and the primary checkout already holds
  `main`. The remote branch is deleted with `git push origin --delete`. The
  local branch goes with its worktree when `scripts/worktree-gc.sh` removes it.

## Verification

- `git grep finish-session` finds no references outside earlier ADRs and the
  dated validation record.
- `scripts/check-adr.sh origin/main` passes.
- `scripts/check-public-content.py` reports no findings.
- This change is landed by following the new AGENTS.md steps.
