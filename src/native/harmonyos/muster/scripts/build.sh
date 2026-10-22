#!/bin/sh
# Build the Rust core, native bridge and ArkUI shell with SDK 26.
#   sh scripts/build.sh          everything
#   sh scripts/build.sh --ui     ArkTS and the bridge only (the Rust archive exists)
set -eu
cd "$(dirname "$0")/.."
. scripts/env.sh
node -e '
const fs = require("fs");
const sdk = JSON.parse(fs.readFileSync(process.env.DEVECO_SDK_HOME + "/default/sdk-pkg.json", "utf8"));
if (sdk.data.apiVersion !== "26") throw Error("This shell requires HarmonyOS SDK 26");
'
# DevEco writes signing material into build-profile.json5, so only the
# unsigned example is tracked.
test -f build-profile.json5 || cp build-profile.example.json5 build-profile.json5
if [ "${1:-}" != --ui ]; then
  sh scripts/build-ffi.sh
  ohpm install --all
fi
hvigorw --mode module -p product=default -p module=entry@default -p buildMode=debug assembleHap --no-daemon
