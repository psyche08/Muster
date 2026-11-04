# ADR 0118: Sweep past a worktree that cannot answer for itself

- Date: 2026-11-04
- Status: Accepted; source and tests verified
- Related: [AGENTS.md](../../AGENTS.md) (worktree-per-session),
  [ADR 0117](0117-bounded-view-cache-and-empty-pin.md) (the session whose
  landing found this)

## Context

Each session develops in its own linked worktree, and `scripts/finish-session.sh`
ends by running `scripts/worktree-gc.sh`, which removes the worktrees whose work
has landed. The sweep asks each registered worktree whether it is dirty with
`git -C <path> status --porcelain`.

A registered path can outlive its administrative files: the directory and the
registration stay while `.git` is gone, which is what is left when a worktree's
directory is copied, restored or partly deleted by hand. `git` then fails there
with `fatal: not a git repository`. Under `set -euo pipefail` that failure is
the status of the pipeline the sweep assigns from, so the whole sweep ends at
the first such path — and because the sweep runs last, `finish-session.sh`
reported a nonzero exit after a merge that had already succeeded. Four such
paths were registered on the owner's Mac, and three landed worktrees behind
them had never been swept.

Had the sweep instead continued, the empty answer would have read as a clean
tree, and a path whose state cannot be read would have been a candidate for
removal.

## Decision

Before judging a registered worktree, ask it for its git directory. A path that
cannot answer is kept, with `registered, but not a readable worktree` as the
reason, and the sweep goes on to the rest. Keeping it is both halves of the
fix: the sweep no longer ends early, and a path whose state is unknown is never
taken for a clean one. A path whose directory is gone entirely is still left to
`git worktree prune`, which frees its branch.

The sweep's own judgement is unchanged: locked, dirty, unlanded, in-use and
recently active worktrees are kept as before, and nothing new is removed.

## Alternatives

Removing the unreadable path would discard whatever is in it on the strength of
an error. Treating the failure as "dirty" would keep it, but under a reason that
is not true. Dropping `pipefail` around the status read would hide this class of
failure everywhere else in the sweep. Pruning the registration automatically
would decide for the owner that the directory is rubbish.

## Consequences

`finish-session.sh` exits zero after a successful merge, and a stale
registration is reported rather than acted on — the owner can remove the
directory and run `git worktree prune`, or restore it. The sweep now reaches
every worktree behind such a path.

## Verification

Cover a registered path whose `.git` is gone: the sweep exits zero, keeps that
path with its reason, and still removes a landed worktree listed after it. Cover
that an unreadable path holding uncommitted work is never a removal candidate.
Run the repository's script tests.

## Implementation verification

`python3 -m unittest discover -s scripts/tests -p 'test_*.py'` passes, including
the two new cases. A dry sweep on the owner's Mac, which has four such
registrations, completes and lists three landed worktrees it would remove where
it previously ended at the first unreadable path. No Apple build, installation
or device acceptance is involved in this change.
