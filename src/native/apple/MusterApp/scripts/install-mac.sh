#!/bin/sh
# Builds Muster.app for macOS signed with the Developer ID identity and
# installs it into /Applications together with the Locked Use system plugin.
# TCC and launchd keep a stable identity for the bundled daemon. The plugin
# installation is requested by musterd through its SMAppService root helper.
#
#   scripts/install-mac.sh                       # Developer ID Application: <team 89LGY6BD53>
#   TEAM=XXXXXXXXXX scripts/install-mac.sh
#
# Prerequisites, all of them (the build phase fails on a missing one):
#   scripts/build-ffi.sh                                       # the Rust core
#   cd src/muster && cargo build --release -p musterd --features sep
#   cd src/muster && cargo build --release -p muster-cli       # Helpers/muster
#   cd src/muster && cargo build --release -p muster-browser   # browser host
#   cd src/muster && cargo build --release -p muster-platform --bin muster-panel
#                                                              # Helpers/muster-panel
#   cd addons/l4forwarder && cargo build --release --locked
# and MUSTER_MAC_PROFILE set to the installed provisioning profile's name, or
# xcodebuild refuses the Presence entitlement ("requires a provisioning
# profile"); see docs/PRESENCE_ENROLLMENT.md. The Developer ID certificate has
# to be in the login keychain -- unlock it yourself if signing fails with
# errSecInternalComponent.
set -eu
cd "$(dirname "$0")/.."
install_uid=$(id -u)
[ "$install_uid" -gt 0 ] || { echo "Run this script as the console user; SMAppService manages privileged helper approval." >&2; exit 1; }
team="${TEAM:-89LGY6BD53}"
identity=$(security find-identity -v -p codesigning | grep "Developer ID Application" | grep "($team)" | head -1 | sed -E 's/.*"(.*)"/\1/')
[ -n "$identity" ] || { echo "no Developer ID Application identity for team $team in the keychain" >&2; exit 1; }
xcodegen generate
# project.pbxproj is generated from project.yml on every build, so a version or
# setting edited in the generated project is reverted right here, silently.
# That is how 1.1.20 (110) came back as 1.1.15 (105); say so instead (ADR 0121).
if git rev-parse --is-inside-work-tree >/dev/null 2>&1 &&
   ! git diff --quiet -- Muster.xcodeproj/project.pbxproj; then
  echo "Muster.xcodeproj/project.pbxproj no longer matches project.yml:" >&2
  git --no-pager diff --stat -- Muster.xcodeproj/project.pbxproj >&2
  echo "move the edit into project.yml, or commit the regenerated project." >&2
  exit 1
fi
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
# pkill only asks; a SwiftUI app takes a moment to go. Deleting the bundle
# while it is still running strands it: Launch Services loses the bundle, the
# process keeps its event loop but no window-server connection, and the Dock
# shows it as not responding forever (ADR 0122). So wait for it to leave,
# escalating once, and refuse to delete the bundle while anything holds it.
pkill -x Muster >/dev/null 2>&1 || true
waited=0
while pgrep -x Muster >/dev/null 2>&1; do
  waited=$((waited + 1))
  if [ "$waited" -eq 10 ]; then
    pkill -KILL -x Muster >/dev/null 2>&1 || true
  fi
  if [ "$waited" -gt 20 ]; then
    echo "Muster is still running after SIGTERM and SIGKILL; not replacing the bundle underneath it." >&2
    exit 1
  fi
  sleep 0.5
done
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
