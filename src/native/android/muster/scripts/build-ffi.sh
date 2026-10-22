#!/bin/sh
# Builds libmuster_ffi.a for the Android ABIs the app ships (needs the NDK
# and `cargo install cargo-ndk`).
#   ANDROID_NDK_HOME=... scripts/build-ffi.sh
set -eu
cd "$(dirname "$0")/../../../../muster"
for target in aarch64-linux-android x86_64-linux-android; do
  rustup target add "$target" >/dev/null 2>&1 || true
done
cargo ndk -t arm64-v8a -t x86_64 build --release -p muster-ffi
ls -la target/aarch64-linux-android/release/libmuster_ffi.a target/x86_64-linux-android/release/libmuster_ffi.a
