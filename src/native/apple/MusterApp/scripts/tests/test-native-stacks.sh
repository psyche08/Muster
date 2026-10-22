#!/bin/sh
# Optimized Rust + Swift integration: capture both live threads, then symbolize
# the exported JSON with the exact dSYM, without a debugger or a phone.
set -eu
APP=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
OUT=$(mktemp -d "${TMPDIR:-/tmp}/muster-native-stacks.XXXXXX")
echo "Stack fixture artifacts: $OUT" >&2
RUSTC="${RUSTUP_HOME:-$HOME/.rustup}/toolchains/stable-aarch64-apple-darwin/bin/rustc"
[ -x "$RUSTC" ] || RUSTC=rustc
"$RUSTC" --edition=2024 --crate-type staticlib --crate-name stack_fixture -O -g \
  -C force-frame-pointers=yes "$APP/scripts/tests/stack-fixture/hot.rs" -o "$OUT/libfixture.a"
xcrun clang -O2 -g -fno-omit-frame-pointer -Wall -Wextra -Werror \
  -I "$APP/Sources/CMusterDiagnostics/include" \
  -c "$APP/Sources/CMusterDiagnostics/MusterDiagnostics.c" -o "$OUT/sampler.o"
cat > "$OUT/module.modulemap" <<MAP
module CMusterDiagnostics {
  header "$APP/Sources/CMusterDiagnostics/include/MusterDiagnostics.h"
  export *
}
MAP
xcrun swiftc -O -g -whole-module-optimization -emit-object -I "$OUT" \
  "$APP/Sources/MusterCore/HighCPUStacks.swift" "$APP/scripts/tests/stack-fixture/main.swift" -o "$OUT/main.o"
xcrun swiftc "$OUT/main.o" "$OUT/sampler.o" "$OUT/libfixture.a" -o "$OUT/fixture"
xcrun dsymutil "$OUT/fixture"
"$OUT/fixture" > "$OUT/performance.jsonl"
python3 "$APP/scripts/symbolicate-performance.py" "$OUT/performance.jsonl" \
  --symbols "$OUT/fixture.dSYM" > "$OUT/symbolicated.jsonl"
python3 - "$OUT/symbolicated.jsonl" <<'PY'
import json, sys
row = json.loads(open(sys.argv[1]).read())['high_cpu_stacks']
assert len(row['samples']) == 6
for sample in row['samples']:
    assert sample['capture_status'] == 0 and sample['resume_status'] == 0, sample
symbols = [f.get('symbol', '') for s in row['samples'] for f in s['frames']]
assert any('rust_hot_inner' in s for s in symbols), symbols
assert any('swiftHotInner' in s for s in symbols), symbols
print('PASS: optimized Rust and Swift hot stacks captured, resumed and UUID-symbolicated')
print('Maximum suspension (ns):', max(s['suspend_duration_ns'] for s in row['samples']))
for symbol in sorted(set(s for s in symbols if 'rust_hot' in s or 'swiftHot' in s)):
    print(symbol)
PY
