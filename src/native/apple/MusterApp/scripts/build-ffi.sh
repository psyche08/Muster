#!/bin/sh
# Builds libmuster_ffi.a for the Apple targets the apps link (run on a Mac).
#   scripts/build-ffi.sh            # macOS arm64, iOS arm64, iOS simulator arm64
#   scripts/build-ffi.sh --debug    # unoptimized
# Output: src/muster/target/<triple>/{release,debug}/libmuster_ffi.a, which
# project.yml points OTHER_LDFLAGS at per SDK.
#
# The iOS targets need rustup's toolchain (rust-std for iOS); a Homebrew
# `cargo` earlier on PATH fails with "can't find crate for `core`". The
# rustup toolchain's bin goes first on PATH so cargo also picks its rustc.
set -eu
cd "$(dirname "$0")/../../../../muster"
profile=release
flag=--release
if [ "${1:-}" = "--debug" ]; then profile=debug; flag=""; fi
toolchain="${RUSTUP_HOME:-$HOME/.rustup}/toolchains/stable-$(uname -m | sed 's/arm64/aarch64/')-apple-darwin"
if [ -x "$toolchain/bin/cargo" ]; then
  export PATH="$toolchain/bin:$PATH"
fi
# Environment flags override Cargo's target flags; preserve the caller's flags
# while guaranteeing walkable optimized Rust frames for the shipped archive.
if [ "${CARGO_ENCODED_RUSTFLAGS+x}" = x ]; then
  CARGO_ENCODED_RUSTFLAGS="${CARGO_ENCODED_RUSTFLAGS}$(printf '\037')-C$(printf '\037')force-frame-pointers=yes"
  export CARGO_ENCODED_RUSTFLAGS
else
  RUSTFLAGS="${RUSTFLAGS:-} -C force-frame-pointers=yes"
  export RUSTFLAGS
fi
for target in aarch64-apple-darwin aarch64-apple-ios aarch64-apple-ios-sim; do
  rustup target add "$target" >/dev/null 2>&1 || true
  # shellcheck disable=SC2086
  cargo build $flag -p muster-ffi --target "$target"
  ls -la "target/$target/$profile/libmuster_ffi.a"
done
