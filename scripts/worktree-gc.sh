#!/usr/bin/env bash
# Remove linked worktrees and local branches whose work has landed on the
# default branch (origin/HEAD, else main). Runs against the repository of the
# current directory, so it also works from other repositories.
#
# A linked worktree is removed only when all of these hold:
#   - it is not locked and has no modified or untracked files;
#   - its HEAD is contained in the default branch, or is the head of a merged PR;
#   - no running process has its working directory inside it;
#   - git has not touched it for --min-idle minutes (default 120), so a
#     session that has just started or just merged keeps its worktree.
# The worktree's branch is deleted with it, and so is the remote branch when
# that has landed too. Local branches that are checked out nowhere and have
# landed are deleted the same way. Every worktree that is kept is listed with
# the reason.
#
# Usage: scripts/worktree-gc.sh [--dry-run] [--quiet] [--min-idle MINUTES]
set -euo pipefail

dry_run=0
quiet=0
min_idle=120
while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) dry_run=1 ;;
    --quiet) quiet=1 ;;
    --min-idle) min_idle=${2:?--min-idle needs a value}; shift ;;
    -h | --help) sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

# The worktree this runs from belongs to a live session; never remove it.
self=$(cd "$(git rev-parse --show-toplevel)" && pwd -P)

if ! git fetch --quiet --prune origin; then
  echo "warning: fetch failed; judging against cached remote branches" >&2
fi
trunk=$(git symbolic-ref --quiet --short refs/remotes/origin/HEAD 2>/dev/null || echo origin/main)
trunk=${trunk#origin/}
base=$(git rev-parse --verify --quiet "origin/$trunk") || { echo "origin/$trunk not found" >&2; exit 1; }
now=$(date +%s)
busy=$(lsof -nP -d cwd -Fn 2>/dev/null | sed -n 's/^n//p' | sort -u || true)
removed=0
kept=0
verb=removed
[ "$dry_run" = 0 ] || verb="would remove"

say() { [ "$quiet" = 1 ] || echo "$@"; }

# landed SHA [BRANCH]: the commit is on origin/$trunk, or is the head of a merged
# PR from BRANCH (which covers squash and rebase merges).
landed() {
  git merge-base --is-ancestor "$1" "$base" 2>/dev/null && return 0
  [ -n "${2:-}" ] || return 1
  gh pr list --head "$2" --state merged --json headRefOid --jq '.[].headRefOid' 2>/dev/null | grep -qx "$1"
}

in_use() {
  local real
  real=$(cd "$1" && pwd -P)
  [ "$real" = "$self" ] && return 0
  printf '%s\n' "$busy" | grep -qxF "$real" && return 0
  printf '%s\n' "$busy" | grep -qF "$real/"
}

# Minutes since git last wrote this worktree's HEAD, reflog or index.
idle_minutes() {
  local gitdir newest=0 t f
  gitdir=$(git -C "$1" rev-parse --path-format=absolute --git-dir)
  for f in "$gitdir/HEAD" "$gitdir/index" "$gitdir/logs/HEAD"; do
    [ -e "$f" ] || continue
    t=$(date -r "$f" +%s)
    [ "$t" -gt "$newest" ] && newest=$t
  done
  echo $(((now - newest) / 60))
}

drop_branch() {
  local branch=$1 sha=$2 remote
  [ -n "$branch" ] && [ "$branch" != "$trunk" ] || return 0
  if [ "$dry_run" = 0 ]; then
    git branch -D --quiet "$branch" || return 0
  fi
  remote=$(git rev-parse --verify --quiet "refs/remotes/origin/$branch") || return 0
  if [ "$remote" = "$sha" ] || landed "$remote" "$branch"; then
    if [ "$dry_run" = 0 ]; then
      git push --quiet origin --delete "$branch" || echo "warning: could not delete origin/$branch" >&2
    fi
  fi
}

# Codex puts each worktree in a directory of its own beside an empty
# .codex-worktree-name marker; drop that directory once nothing else is left.
remove_codex_shell() {
  case "$1" in */.codex/worktrees/*) ;; *) return 0 ;; esac
  [ -z "$(ls -A "$1" | grep -vxE '\.codex-worktree-name|\.DS_Store')" ] || return 0
  rm -f "$1/.codex-worktree-name" "$1/.DS_Store"
  rmdir "$1" 2>/dev/null || true
}

keep() {
  kept=$((kept + 1))
  echo "kept     $1  ($2)"
}

primary=1
checked_out=""
wt="" head="" branch="" locked=0
flush() {
  [ -n "$wt" ] || return 0
  local path=$wt sha=$head br=$branch is_locked=$locked dirty idle
  wt="" head="" branch="" locked=0
  if [ "$primary" = 1 ]; then
    primary=0
    checked_out=" $br "
    [ "$br" = "$trunk" ] || echo "note     primary checkout $path is on ${br:-a detached HEAD}, not $trunk"
    return 0
  fi
  if [ ! -d "$path" ]; then
    return 0 # left for `git worktree prune`, which frees its branch
  fi
  [ -n "$br" ] && checked_out="$checked_out $br "
  if [ "$is_locked" = 1 ]; then keep "$path" "locked"; return 0; fi
  # --no-optional-locks keeps status from rewriting the index, which would
  # reset the idle clock below.
  dirty=$(git --no-optional-locks -C "$path" status --porcelain | wc -l | tr -d ' ')
  if [ "$dirty" != 0 ]; then keep "$path" "$dirty uncommitted files on ${br:-detached HEAD}"; return 0; fi
  if ! landed "$sha" "$br"; then
    keep "$path" "$(git rev-list --count "$base..$sha") commits on ${br:-detached HEAD} not on $trunk"
    return 0
  fi
  if in_use "$path"; then keep "$path" "in use by a running process"; return 0; fi
  idle=$(idle_minutes "$path")
  if [ "$idle" -lt "$min_idle" ]; then keep "$path" "landed, but active ${idle}m ago"; return 0; fi

  if [ "$dry_run" = 0 ]; then
    git worktree remove "$path" || { keep "$path" "git worktree remove failed"; return 0; }
    remove_codex_shell "$(dirname "$path")"
  fi
  drop_branch "$br" "$sha"
  removed=$((removed + 1))
  say "$verb  $path  (${br:-detached HEAD}, landed)"
}

while IFS= read -r line; do
  case "$line" in
    "worktree "*) flush; wt=${line#worktree } ;;
    "HEAD "*) head=${line#HEAD } ;;
    "branch refs/heads/"*) branch=${line#branch refs/heads/} ;;
    locked*) locked=1 ;;
  esac
done < <(git worktree list --porcelain)
flush
[ "$dry_run" = 1 ] || git worktree prune

# Branches without a worktree.
while IFS=' ' read -r br sha; do
  case "$checked_out" in *" $br "*) continue ;; esac
  [ "$br" != "$trunk" ] && landed "$sha" "$br" || continue
  drop_branch "$br" "$sha"
  say "$verb  branch $br (landed)"
done < <(git for-each-ref refs/heads --format='%(refname:short) %(objectname)')

if [ "$dry_run" = 1 ]; then
  echo "dry run: $removed worktrees would be removed, $kept kept"
else
  echo "worktree-gc: removed $removed worktrees, kept $kept"
fi
