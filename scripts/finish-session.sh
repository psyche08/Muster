#!/usr/bin/env bash
# Land the current session's branch on main, then sweep finished worktrees.
#
# Run from the session's worktree after the final commit. It
#   1. requires a named branch other than main and no uncommitted changes;
#   2. pushes the branch and opens a pull request unless one is open
#      (open it yourself first to choose the English title and description);
#   3. waits for CI checks, merges with a merge commit and deletes the remote
#      branch;
#   4. runs scripts/worktree-gc.sh. This worktree is in use, so a later sweep
#      removes it once it has been idle.
#
# Usage: scripts/finish-session.sh [--no-wait]
#   --no-wait  merge only if checks have already finished; exit 3 if pending.
set -euo pipefail

wait=1
case "${1:-}" in
  "") ;;
  --no-wait) wait=0 ;;
  -h | --help) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
  *) echo "unknown option: $1" >&2; exit 2 ;;
esac

die() { echo "finish-session: $*" >&2; exit 1; }

branch=$(git symbolic-ref --short -q HEAD) || die "HEAD is detached; create a branch with \`git switch -c <name>\` first"
[ "$branch" != main ] || die "on main; session work belongs on its own branch"
[ -z "$(git status --porcelain)" ] || die "uncommitted changes; commit the turn's work first"
git fetch --quiet origin main
[ -n "$(git rev-list origin/main..HEAD)" ] || die "no commits beyond origin/main; nothing to land"

"$(dirname "$0")/check-adr.sh" origin/main

git push --quiet -u origin HEAD
pr=$(gh pr list --head "$branch" --state open --json number --jq '.[0].number // empty')
if [ -z "$pr" ]; then
  gh pr create --base main --head "$branch" --fill
  pr=$(gh pr list --head "$branch" --state open --json number --jq '.[0].number')
fi
echo "finish-session: PR #$pr"

# A push takes a few seconds to register its checks. gh capitalizes
# "No checks reported" on some paths, so match either case.
checks=""
for _ in 1 2 3 4 5 6; do
  if checks=$(gh pr checks "$pr" 2>&1); then break; fi
  case "$checks" in *[Nn]"o checks reported"*) sleep 10 ;; *) break ;; esac
done
case "$checks" in
  *[Nn]"o checks reported"*) echo "finish-session: no CI checks on this PR" ;;
  *)
    if [ "$wait" = 1 ]; then
      gh pr checks "$pr" --watch --fail-fast --interval 30 || die "CI failed on PR #$pr; fix it and rerun"
    else
      rc=0
      gh pr checks "$pr" >/dev/null 2>&1 || rc=$?
      [ "$rc" != 8 ] || { echo "finish-session: checks still running on PR #$pr" >&2; exit 3; }
      [ "$rc" = 0 ] || die "CI failed on PR #$pr; fix it and rerun"
    fi
    ;;
esac

gh pr merge "$pr" --merge ||
  die "PR #$pr could not be merged; bring the branch up to date with origin/main, resolve conflicts, commit and rerun"
git push --quiet origin --delete "$branch" 2>/dev/null || true
echo "finish-session: merged PR #$pr"

"$(dirname "$0")/worktree-gc.sh"
