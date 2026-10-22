#!/bin/sh
# Xcode Cloud calls this before resolving Swift packages and archiving iOS.
set -eu
app_root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
rust_root=$(CDPATH= cd -- "$app_root/../../../muster" && pwd)
# Keep the toolchain reproducible without changing the user's local default.
rust_toolchain=1.98.0
export PATH="$HOME/.cargo/bin:$PATH"
if ! command -v rustup >/dev/null 2>&1; then
  installer=$(mktemp)
  trap 'rm -f "$installer"' EXIT HUP INT TERM
  curl --fail --show-error --silent --location --proto '=https' --tlsv1.2 \
    https://sh.rustup.rs -o "$installer"
  sh "$installer" -y --profile minimal --default-toolchain none --no-modify-path
fi
rustup toolchain install "$rust_toolchain" --profile minimal
rustup target add --toolchain "$rust_toolchain" aarch64-apple-ios
# project.yml links this precise target directory, independently of CI defaults.
export CARGO_TARGET_DIR="$rust_root/target"
# Match the app deployment target for Rust and cc-rs (ring / SQLite). Without
# this, Xcode 27 can compile C objects for iOS 27 while the app targets iOS 17.
export IPHONEOS_DEPLOYMENT_TARGET=17.0
if [ "${CARGO_ENCODED_RUSTFLAGS+x}" = x ]; then
  CARGO_ENCODED_RUSTFLAGS="${CARGO_ENCODED_RUSTFLAGS}$(printf '\037')-C$(printf '\037')force-frame-pointers=yes"
  export CARGO_ENCODED_RUSTFLAGS
else
  RUSTFLAGS="${RUSTFLAGS:-} -C force-frame-pointers=yes"
  export RUSTFLAGS
fi
cargo +"$rust_toolchain" build --manifest-path "$rust_root/Cargo.toml" \
  --locked --release -p muster-ffi --target aarch64-apple-ios
test -s "$CARGO_TARGET_DIR/aarch64-apple-ios/release/libmuster_ffi.a"
