#!/usr/bin/env bash
# Cross-build an x64 Windows portable package with the .NET SDK and mingw-w64.
set -euo pipefail
: "${CARGO_TARGET_DIR:?use the shell Cargo build directory}"
version=${1:?usage: build-portable.sh VERSION OUTPUT-DIRECTORY}
mkdir -p "${2:?output directory required}"
out=$(cd "$2" && pwd)
root=$(git rev-parse --show-toplevel)
source_dir="$root/src/native/windows"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
cd "$root"
export MUSTER_WINDOWS_VERSION="$version"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
cargo build --manifest-path src/muster/Cargo.toml --release --locked --target x86_64-pc-windows-gnu -p musterd -p muster-cli -p muster-ffi -p muster-browser -p muster-vnet
dotnet publish "$source_dir/Muster.csproj" -c Release -r win-x64 -p:Version="$version" -o "$stage/gui"
mkdir -p "$stage/cli" "$stage/AgentHaloBrowser"
cp "$stage/gui/Muster.exe" "$stage/Muster.exe"
bin="$CARGO_TARGET_DIR/x86_64-pc-windows-gnu/release"
for file in muster-tun.exe musterd.exe muster-vnet.exe muster_ffi.dll muster-browser-host.exe; do
  x86_64-w64-mingw32-strip -o "$stage/$file" "$bin/$file"
done
x86_64-w64-mingw32-strip -o "$stage/cli/muster.exe" "$bin/muster.exe"
if [[ -n "${WINTUN_ARCHIVE:-}" ]]; then cp "$WINTUN_ARCHIVE" "$stage/driver.zip"; else
  curl --fail --location https://www.wintun.net/builds/wintun-0.14.1.zip --output "$stage/driver.zip"
fi
cd "$stage"
echo '07c256185d6ee3652e09fa55c0b673e2624b565e02c4b9091c79ca7d2f24ef51  driver.zip' | shasum -a 256 -c -
unzip -p driver.zip wintun/bin/amd64/wintun.dll > wintun.dll
unzip -p driver.zip wintun/LICENSE.txt > Wintun-LICENSE.txt
cp "$root/LICENSE" LICENSE.txt
python3 "$source_dir/installer/runtime-notices.py" "$source_dir/obj/project.assets.json" Runtime-Notices.txt
cp "$source_dir/installer/dev.linsheng.muster.browser.json" .
cp "$root/src/browser/agenthalo-extension/"{manifest.json,sw.js,jumper.js} AgentHaloBrowser/
python3 "$source_dir/installer/portable-zip.py" "$stage" "$version" "$out"
