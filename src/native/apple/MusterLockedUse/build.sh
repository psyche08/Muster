#!/bin/sh
set -eu
cd "$(dirname "$0")"
out="${1:-build/MusterLockedUse.bundle}"
mkdir -p "$out/Contents/MacOS"
xcrun clang -mmacosx-version-min=14.0 -fobjc-arc -O2 -Wall -Wextra -Werror -bundle Plugin.m -framework Foundation -framework Security -o "$out/Contents/MacOS/MusterLockedUse"
cat > "$out/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>dev.linsheng.muster.locked-use</string>
<key>CFBundleExecutable</key><string>MusterLockedUse</string>
<key>CFBundlePackageType</key><string>BNDL</string>
<key>CFBundleVersion</key><string>1</string>
</dict></plist>
PLIST
if [ -n "${SIGN_IDENTITY:-}" ]; then
  codesign --force --options runtime --timestamp --sign "$SIGN_IDENTITY" "$out"
  codesign --verify --strict "$out"
fi
printf 'Built %s\n' "$out"
