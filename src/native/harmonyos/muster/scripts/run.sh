#!/bin/sh
# Install the newest HAP on a device or emulator and start it.
#   sh scripts/run.sh [hdc-target]     (hdc list targets; default: the only one)
# A phone needs the signed HAP; an emulator also accepts the unsigned one.
set -eu
cd "$(dirname "$0")/.."
. scripts/env.sh
OUT=entry/build/default/outputs/default
HAP="$OUT/entry-default-signed.hap"
if [ ! -f "$HAP" ] || [ "$OUT/entry-default-unsigned.hap" -nt "$HAP" ]; then HAP="$OUT/entry-default-unsigned.hap"; fi
test -f "$HAP" || { echo "No HAP under $OUT: run scripts/build.sh" >&2; exit 1; }
set -- ${1:+-t "$1"}
echo "Installing $HAP"
"$HDC" "$@" shell aa force-stop dev.linsheng.muster >/dev/null 2>&1 || true
"$HDC" "$@" install -r "$HAP"
"$HDC" "$@" shell aa start -a EntryAbility -b dev.linsheng.muster
