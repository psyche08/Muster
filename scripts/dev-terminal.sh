#!/usr/bin/env bash
# Build/run an isolated local app; never install, notarize or publish.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
case "${1:-}" in
  ""|--build-only) ;;
  *) echo "usage: scripts/dev-terminal.sh [--build-only]" >&2; exit 2 ;;
esac
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-"$root/src/muster/target"}
(cd "$root/src/muster" && cargo build -p musterd -p muster-cli -p muster-ffi)
export MUSTER_FFI_LIB="$CARGO_TARGET_DIR/debug/libmuster_ffi.a"
package="$root/src/native/apple/MusterApp"
(cd "$package" && swift build --product MusterDev --disable-sandbox)
bin=$(cd "$package" && swift build --show-bin-path)
bundle="$root/build/dev/MusterDev.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "$bin/MusterDev" "$bundle/Contents/MacOS/MusterDev"
cp "$CARGO_TARGET_DIR/debug/musterd" "$bundle/Contents/MacOS/musterd"
cp "$CARGO_TARGET_DIR/debug/muster" "$bundle/Contents/MacOS/muster"
for resource in "$bin"/*.bundle; do
  [ -d "$resource" ] || continue
  cp -R "$resource" "$bundle/Contents/Resources/"
done
cat > "$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>dev.linsheng.muster.local-dev</string>
<key>CFBundleName</key><string>MusterDev</string>
<key>CFBundleDisplayName</key><string>Muster Dev</string>
<key>CFBundleExecutable</key><string>MusterDev</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>1</string>
<key>LSMinimumSystemVersion</key><string>14.0</string>
<key>NSAppleEventsUsageDescription</key><string>Muster Dev connects to terminal windows you select in iTerm2 and Ghostty.</string>
</dict></plist>
PLIST
# Ad-hoc signing uses no signing identity or Keychain access.
codesign --force --sign - "$bundle/Contents/MacOS/musterd"
codesign --force --sign - "$bundle/Contents/MacOS/muster"
codesign --force --sign - "$bundle"
printf 'Dev bundle: %s\n' "$bundle"
if [ "${1:-}" != --build-only ]; then open "$bundle"; fi
