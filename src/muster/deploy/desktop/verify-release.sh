#!/bin/bash
# Refuses a Muster.app whose daemon was not built from this clean source
# tree, lacks a fix an earlier release shipped, or omits a commit an earlier
# release was built from. Desktop 1.0.20 (2026-10-01) was built from a line
# that never received 1.0.16-1.0.18 and silently brought back fixed Locked
# Use failures; a higher version number proved nothing.
set -euo pipefail
if [ "$#" -ne 1 ]; then echo 'usage: verify-release.sh Muster.app' >&2; exit 2; fi
here=$(cd "$(dirname "$0")" && pwd)
daemon="$1/Contents/MacOS/musterd"
[ -f "$daemon" ] || { echo "no daemon at $daemon" >&2; exit 1; }
swift "$here/check-presence-signing.swift" "$1"
text=$(mktemp)
trap 'rm -f "$text"' EXIT
/usr/bin/strings -a "$daemon" > "$text"

stamps=$(/usr/bin/grep -Eo 'MUSTER-BUILD-COMMIT:([0-9a-f]{40}(-dirty)?|unknown)' "$text" | sort -u || true)
if [ "$(printf '%s' "$stamps" | grep -c '^')" -ne 1 ]; then
  echo "expected one build stamp in the daemon, found: ${stamps:-none}" >&2
  exit 1
fi
commit=${stamps#MUSTER-BUILD-COMMIT:}
case "$commit" in
  *-dirty|unknown) echo "daemon was built from an unclean or unknown tree: $commit" >&2; exit 1;;
esac
head=$(git -C "$here" rev-parse HEAD)
[ "$commit" = "$head" ] || { echo "daemon commit $commit is not this source tree's HEAD $head" >&2; exit 1; }
[ -z "$(git -C "$here" status --porcelain --untracked-files=no)" ] \
  || { echo "this source tree has uncommitted changes" >&2; exit 1; }

status=0
# Historical snapshot releases are accepted only by the bound root ledger;
# releases after the import still require normal Git ancestry.
if ! python3 "$here/verify-release-ancestry.py" "$commit" "$here/released-commits.txt"; then
  status=1
fi
# release-fingerprints.txt: <exact string in musterd> | <what it proves>
while IFS= read -r line; do
  case "$line" in ''|'#'*) continue;; esac
  fingerprint=${line%% | *}
  if ! /usr/bin/grep -qF -- "$fingerprint" "$text"; then
    echo "the daemon lacks: $line" >&2
    status=1
  fi
done < "$here/release-fingerprints.txt"
[ "$status" -eq 0 ] && echo "verified $1: daemon built from $commit"
exit "$status"
