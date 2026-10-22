#!/bin/sh
# Builds musterd with the Secure Enclave signer (run on a Mac with Xcode).
# muster-platform's build.rs compiles Sources/SepSign with swiftc and links
# the Swift runtime; nothing here needs SwiftPM.
set -eu
cd "$(dirname "$0")/../../../muster"
toolchain="${RUSTUP_HOME:-$HOME/.rustup}/toolchains/stable-$(uname -m | sed 's/arm64/aarch64/')-apple-darwin"
if [ -x "$toolchain/bin/cargo" ]; then
  export PATH="$toolchain/bin:$PATH"
fi
cargo build --release -p musterd --features sep
echo "musterd with SEP: target/release/musterd"
