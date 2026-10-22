#!/bin/sh
# Type-checks the Apple targets on a Linux box without an Apple SDK.
# `cargo check` never links, but build scripts (ring, sqlite, ...) still
# invoke a C compiler; this stand-in answers every compile with an empty
# object for the requested target. Real builds need a Mac.
#
#   scripts/check-apple.sh                # darwin: muster-platform, musterd, muster-app-core, muster-ffi
#   scripts/check-apple.sh <cargo check args...>
#
# Known limit: the aarch64-apple-ios check stops in ring's build script
# (llvm-ar refuses the ios64 objects); the iOS-relevant crates are the same
# ones the darwin pass covers, so that pass is the gate.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
fake="$here/fakecc"
export CC_aarch64_apple_darwin="$fake" CC_aarch64_apple_ios="$fake" CC_aarch64_apple_ios_sim="$fake"
ar=$(command -v llvm-ar || command -v ar)
export AR_aarch64_apple_darwin="$ar" AR_aarch64_apple_ios="$ar" AR_aarch64_apple_ios_sim="$ar"
rustup target add aarch64-apple-darwin aarch64-apple-ios >/dev/null 2>&1 || true
# cc-rs asks xcrun for the SDK unless SDKROOT is set; any directory will do for a check.
export SDKROOT="${SDKROOT:-$here}"
if [ $# -gt 0 ]; then
  exec cargo check "$@"
fi
# `sep` is on here: a check never links, so the SepSign library is not needed
# to keep the Secure Enclave code type-checked.
cargo check --target aarch64-apple-darwin --features muster-platform/sep \
  -p muster-platform -p musterd -p muster-app-core -p muster-ffi
