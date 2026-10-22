#!/bin/sh
# Builds Muster.app for macOS signed with the Developer ID identity and
# installs it into /Applications together with the Locked Use system plugin.
# TCC and launchd keep a stable identity for the bundled daemon. The plugin
# installation is requested by musterd through its SMAppService root helper.
#
#   scripts/install-mac.sh                       # Developer ID Application: <team 89LGY6BD53>
#   TEAM=XXXXXXXXXX scripts/install-mac.sh
#
# Prerequisites: the Rust core (scripts/build-ffi.sh) and the daemon
# (cd src/muster && cargo build --release -p musterd --features sep);
# the Developer ID certificate in the login keychain (unlock it yourself
# if signing fails with errSecInternalComponent).
set -eu
cd "$(dirname "$0")/.."
install_uid=$(id -u)
[ "$install_uid" -gt 0 ] || { echo "Run this script as the console user; SMAppService manages privileged helper approval." >&2; exit 1; }
team="${TEAM:-89LGY6BD53}"
identity=$(security find-identity -v -p codesigning | grep "Developer ID Application" | grep "($team)" | head -1 | sed -E 's/.*"(.*)"/\1/')
[ -n "$identity" ] || { echo "no Developer ID Application identity for team $team in the keychain" >&2; exit 1; }
xcodegen generate
xcodebuild -project Muster.xcodeproj -scheme MusterMac -configuration Release \
  -derivedDataPath build/DerivedData CODE_SIGN_STYLE=Manual "DEVELOPMENT_TEAM=$team" \
  "CODE_SIGN_IDENTITY=$identity" OTHER_CODE_SIGN_FLAGS=--timestamp build
app=build/DerivedData/Build/Products/Release/Muster.app
codesign --verify --deep --strict "$app"
plugin="$app/Contents/PlugIns/MusterLockedUse.bundle"
installer="$app/Contents/Resources/LockedUse/install.py"
socket="$HOME/Library/Application Support/Muster/locked-use.sock"
# Validate policy and signature before replacing the app.
/usr/bin/python3 "$installer" "$plugin" "$socket" "$install_uid"
# Stop the daemon that the previous copy registered, then replace the app.
pkill -x Muster >/dev/null 2>&1 || true
launchctl bootout "gui/$(id -u)/dev.linsheng.musterd" >/dev/null 2>&1 || true
rm -rf /Applications/Muster.app
ditto "$app" /Applications/Muster.app
echo "Locked Use installation will be verified by musterd through the root helper after launch."
echo "installed /Applications/Muster.app ($(codesign -dv /Applications/Muster.app 2>&1 | grep Authority= | head -1))"
# LaunchServices needs a moment after the bundle is replaced (-609 otherwise).
for _ in 1 2 3 4 5; do
  sleep 2
  # A CLI/developer MUSTER_DIR override means "manually managed daemon" to
  # the GUI. This installer targets the production bundle; do not leak it.
  env -u MUSTER_DIR open /Applications/Muster.app 2>/dev/null && break
done
