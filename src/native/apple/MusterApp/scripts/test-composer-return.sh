#!/bin/bash
# Runs a synthetic native App to exercise SwiftUI's actual key event path.
set -euo pipefail
project=$(cd "$(dirname "$0")/.." && pwd)
scratch=$(mktemp -d -t muster-composer-return)
trap 'rm -rf "$scratch"' EXIT
app="$scratch/ComposerReturnHarness.app"
mkdir -p "$app/Contents/MacOS"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>dev.muster.keyboard-test</string><key>CFBundleName</key><string>Muster Keyboard Test</string><key>CFBundleExecutable</key><string>KeyHarness</string><key>CFBundlePackageType</key><string>APPL</string></dict></plist>
PLIST
swiftc -parse-as-library "$project/Sources/MusterUI/ComposerReturnKey.swift" \
    "$project/Tests/KeyboardHarness/ComposerReturnHarness.swift" \
    -o "$app/Contents/MacOS/KeyHarness"
python3 - "$app" "$scratch/result.json" <<'PY'
import json, subprocess, sys
subprocess.run(['open', '-W', '-n', sys.argv[1], '--args', '--result=' + sys.argv[2]], check=True, timeout=30)
with open(sys.argv[2]) as f:
    result = json.load(f)
assert result.get('shiftText') == 'draft\n' and result.get('shiftSends') == 0, result
assert result.get('enterText') == 'draft\n' and result.get('enterSends') == 1, result
print('Native composer event path passed: Shift+Enter newline; Enter one submission')
PY
