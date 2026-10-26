#!/bin/bash
set -euo pipefail
# VPK is a pinned 1.2.158 executable/wrapper, not a shell command string.
: "${VPK:?Set VPK to Velopack 1.2.158 executable}"
: "${SIGN_IDENTITY:?Set Developer ID Application identity}"
: "${MUSTER_RELEASE_KEY:?Set the offline P-256 feed signing key path}"
if [ "$#" -ne 3 ]; then echo 'usage: package-macos.sh Muster.app 1.0.9 output-directory' >&2; exit 2; fi
app=$1
version=$2
output=$3
case "$(uname -m)" in arm64) channel=osx-arm64;; x86_64) channel=osx-x64;; *) exit 2;; esac
"$VPK" --help | head -8 | /usr/bin/grep -q '1.2.158' || { echo 'Velopack 1.2.158 required' >&2; exit 2; }
# Never package a daemon that drops an earlier release's source or fixes.
"$(dirname "$0")/verify-release.sh" "$app"
# Preserve Xcode-expanded keychain groups; the source plist has build variables.
# vpk 1.2.158 refuses a --signEntitlements path whose name does not end in
# `.entitlements`, which a bare mktemp name does not.
entitlements_dir=$(mktemp -d)
entitlements="$entitlements_dir/exported.entitlements"
trap 'rm -rf "$entitlements_dir"' EXIT
swift "$(dirname "$0")/check-presence-signing.swift" "$app" --export-entitlements "$entitlements"
args=(pack --packId Muster --packTitle Muster --packVersion "$version" --packDir "$app" --mainExe Muster --runtime "$channel" --channel "$channel" --delta None --noInst --outputDir "$output" --signAppIdentity "$SIGN_IDENTITY" --signDisableDeep --signEntitlements "$entitlements")
if [ -n "${MUSTER_NOTARY_PROFILE:-}" ]; then args+=(--notaryProfile "$MUSTER_NOTARY_PROFILE"); fi
"$VPK" "${args[@]}"
python3 "$(dirname "$0")/sign-feed.py" "$output/releases.$channel.json" --key "$MUSTER_RELEASE_KEY"
