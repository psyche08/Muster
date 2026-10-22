#!/bin/sh
# Regenerates entry/src/main/ets/proto/muster.{js,d.ts} from src/muster's schema
# (needs node with the pinned generator: npm i -g protobufjs-cli@1.3.3 protobufjs@7;
# protobufjs-cli 2.x emits a different layout and rewrites every line).
set -eu
cd "$(dirname "$0")/.."
PROTO=../../../muster/crates/muster-proto/proto
OUT=entry/src/main/ets/proto
pbjs -t static-module -w es6 --es6 --no-verify --no-convert --no-delimited --no-service \
  --dependency @ohos/protobufjs -o "$OUT/muster.js" "$PROTO/muster.proto" "$PROTO/app.proto"
pbts -o "$OUT/muster.d.ts" "$OUT/muster.js"
# @ohos/protobufjs exports the library as `index`, not as module members: a
# namespace import has no `roots`, and the module throws when the app loads.
sed -i.bak '2s|^import \* as \$protobuf from "@ohos/protobufjs";$|import { index as $protobuf } from "@ohos/protobufjs";|' "$OUT/muster.js"
sed -i.bak '1s|^import \* as \$protobuf from "protobufjs";$|import * as $protobuf from "@ohos/protobufjs";|' "$OUT/muster.d.ts"
rm -f "$OUT/muster.js.bak" "$OUT/muster.d.ts.bak"
grep -q '^import { index as \$protobuf } from "@ohos/protobufjs";$' "$OUT/muster.js"
