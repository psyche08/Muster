#!/bin/sh
# Apply only to the audited upstream release. Does not build, sign or install.
set -eu
if [ "$#" -ne 1 ]; then
  echo 'usage: apply-runtime-patch.sh UPSTREAM_CHECKOUT' >&2
  exit 2
fi
root=$(CDPATH= cd -- "$1" && pwd)
patch_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
expected=aa8d76b44a834eee3ebf6a0bc577e376f65f6361
[ "$(git -C "$root" rev-parse HEAD)" = "$expected" ] || {
  echo 'Expected the exact Apple container 1.6.1 release commit' >&2; exit 1;
}
git -C "$root" diff --quiet && git -C "$root" diff --cached --quiet || {
  echo 'Refusing to modify a checkout with tracked edits' >&2; exit 1;
}
git -C "$root" apply --check "$patch_dir/container-1.6.1-exec-lifecycle.patch"
git -C "$root" apply "$patch_dir/container-1.6.1-exec-lifecycle.patch"
echo 'Applied runtime lifecycle patch. Build and test before signing or installation.'
