#!/bin/sh
# Regenerates Sources/MusterCore/Generated from src/muster/crates/muster-proto/proto.
# Needs `protoc` and `protoc-gen-swift` (brew install swift-protobuf, or the
# executable built from the swift-protobuf checkout: swift build -c release
# --product protoc-gen-swift). CI checks the output is up to date.
set -eu
cd "$(dirname "$0")/.."
PROTO=../../../muster/crates/muster-proto/proto
OUT=Sources/MusterCore/Generated
mkdir -p "$OUT"
protoc -I "$PROTO" \
  --swift_out="$OUT" --swift_opt=Visibility=Public --swift_opt=FileNaming=DropPath \
  "$PROTO/muster.proto" "$PROTO/app.proto"
echo "generated: $(ls "$OUT")"
