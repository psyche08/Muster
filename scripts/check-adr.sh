#!/usr/bin/env bash
# Require a numbered decision record in the committed branch diff.
set -euo pipefail
base=${1:-origin/main}
git rev-parse --verify "$base^{commit}" >/dev/null
# R counts too: renumbering a record is a change to it, and git reports that
# as a rename, which an AM-only filter cannot see.
records=$(git diff --name-only --diff-filter=AMR "$base"...HEAD -- 'docs/adr/[0-9][0-9][0-9][0-9]-*.md')
if [ -z "$records" ]; then
  echo "check-adr: add or update a numbered docs/adr/NNNN-*.md record for this change" >&2
  exit 1
fi
printf 'check-adr: decision records present\n%s\n' "$records"
