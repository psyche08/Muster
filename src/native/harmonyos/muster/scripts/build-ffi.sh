#!/bin/sh
# Builds libmuster_ffi.a for HarmonyOS with the OpenHarmony NDK's clang.
#   OHOS_NDK=$HOME/Developer/HarmonyOS/sdk/default/openharmony/native scripts/build-ffi.sh
set -eu
cd "$(dirname "$0")/../../../../muster"
: "${OHOS_NDK:?set OHOS_NDK to <sdk>/default/openharmony/native}"
test -x "$OHOS_NDK/llvm/bin/aarch64-unknown-linux-ohos-clang" || {
    echo "No HarmonyOS ARM64 compiler under $OHOS_NDK" >&2
    exit 1
}
rustup target add aarch64-unknown-linux-ohos
export CC_aarch64_unknown_linux_ohos="$OHOS_NDK/llvm/bin/aarch64-unknown-linux-ohos-clang"
export CXX_aarch64_unknown_linux_ohos="$OHOS_NDK/llvm/bin/aarch64-unknown-linux-ohos-clang++"
export AR_aarch64_unknown_linux_ohos="$OHOS_NDK/llvm/bin/llvm-ar"
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_OHOS_LINKER="$OHOS_NDK/llvm/bin/aarch64-unknown-linux-ohos-clang"
# Keep the output path in sync with CMake, even with a host Cargo override.
export CARGO_TARGET_DIR="$PWD/target"
# Cargo may otherwise find Homebrew's rustc before the rustup proxies.
export RUSTC="$(rustup which rustc)"
export RUSTDOC="$(rustup which rustdoc)"
"$(rustup which cargo)" build --locked --release -p muster-ffi --target aarch64-unknown-linux-ohos
ls -la target/aarch64-unknown-linux-ohos/release/libmuster_ffi.a
