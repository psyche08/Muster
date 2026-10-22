# iOS high-CPU diagnostics

The foreground performance monitor writes `performance.jsonl` every 10 seconds.
Settings → Performance monitor exports or sends the same bounded log to a Mac's
`/tmp/muster-reports`. Capture needs no attached debugger. Existing logs remain
readable; schema 2 adds `high_cpu_stacks` to selected records.

## Automatic native stacks

At >= 80% process CPU (100% = one core), rank native threads by their CPU-time
**delta in the current interval**, not lifetime totals. Select at most three
threads individually using >= 20% CPU. Sample each three times, 20 ms between
rounds. Allow one burst per 60 seconds of monotonic time. Foreground transitions
reset thread baselines, but do not reset the cooldown. New/unmeasurable threads
wait for a baseline; unavailable threads have explicit capture errors.

This captures executing machine stacks in **both Rust and Swift**, including
mixed FFI stacks. Rust Tokio workers do not need to reach a cooperative callback:
a spinning task can be sampled. Apple thread inspection is an OS adapter in
`CMusterDiagnostics`, not another implementation of Shared Rust business state.
It samples only this app's process; it does not sample a remote Mac's daemon.

The C sampler rejects its own thread, suspends one target at a time, obtains
registers and walks at most 64 frame records using `vm_read_overwrite`. It checks
monotonic frame addresses, alignment and an 8 MiB span from SP, and stops walking
after a 1 ms elapsed budget. This is a best-effort walk budget, not a hard deadline
for Mach calls. Every successful suspend has one balancing resume, including
register/read errors. No Swift, Rust, allocation, symbol resolution, dyld queries,
logging or mutex acquisition occurs while the target is suspended. ARM64 return
addresses have PAC stripped using XPACLRI. x86_64 simulators use RBP chains.

The log contains PCs, thread IDs/CPU, main-thread attribution, sample times,
capture/resume status, suspension duration and explicit walk stop reasons.
`invalid_frame`, `unreadable`, `time_limit`, `frame_limit` mean the walk is partial.
Even `end` does not guarantee every logical call: optimized/inlined/tail calls and
Swift/Rust async suspension boundaries may omit frames. These are execution
stacks, not reconstructed async task histories. The sampler preserves the
interrupted PC, but frame-chain walks can skip a caller during a prologue/epilogue.

Loaded image UUIDs, architectures, ASLR bases and executable ranges are copied in
dyld callbacks, then matched **after threads resume**. Only image basenames are
stored. A fixed 2048-image registry bounds memory; unmatched frames retain their
raw PCs. No stack memory contents, conversation text, keys or container paths are
exported. Log rotation remains 2 × 2 MiB, with an additional bounded export copy.
Sampling work is included in `monitor_previous_work_seconds`; main-queue probes
start after sampling, so profiler work is not misreported as a main-thread stall.

## Preserve matching symbols

Apple Cargo targets force frame pointers via the repository `.cargo/config.toml`.
`scripts/build-ffi.sh` also appends that flag when environment flags override Cargo
configuration. Custom build commands that set `RUSTFLAGS` or
`CARGO_ENCODED_RUSTFLAGS` must include `-C force-frame-pointers=yes` themselves.
The release profile already retains Rust line-table debug info (`debug = 1`).
Xcode uses `dwarf-with-dsym`. **Keep each shipped `.xcarchive`, including its
`dSYMs/Muster.app.dSYM`**, before deleting intermediate build objects. Symbols
from a later rebuild of the same version are not interchangeable.

Decode a report without connecting the phone:

```sh
python3 src/native/apple/MusterApp/scripts/symbolicate-performance.py \
  /tmp/muster-reports/iPhone-<timestamp>-performance.jsonl \
  --symbols /path/to/Muster.xcarchive/dSYMs > /tmp/symbolicated.jsonl
```

Only matching UUID **and** architecture are accepted. `atos` applies the recorded
ASLR base; return addresses are adjusted to the calling instruction. Rust symbols
are demangled with `llvm-cxxfilt`, and Swift symbols by `atos`. Missing system or
app dSYMs remain explicitly unresolved; raw frames are retained. Xcode tools are
required only on the machine doing offline symbolication.

## Verification

- `swift test --filter 'HighCPUStackTests|PerformanceLogTests'` covers thread CPU
  selection, lifecycle/cooldown, self/exited-thread rejection, actual Swift thread
  capture/resume and bounded log storage.
- The same stack tests run in `MusterMobileTests`; an iOS-only integration test
  burns CPU, waits for the monitor interval, and checks exported stack records.
- `sh scripts/tests/test-native-stacks.sh` builds optimized Rust and Swift worker
  threads, captures both without a debugger, resumes them and uses the generated
  matching dSYM to verify Rust and Swift function/line attribution.
- `python3 -m unittest discover -s scripts/tests -p '*performance*.py'` verifies
  imported report names, summaries, UUID rejection and address adjustment.

Simulator and macOS fixture results do not establish physical-iPhone acceptance,
App Store review, signing or distribution. Confirm a report from the shipped
build separately. This feature captures evidence; it does not itself fix the
underlying high CPU.
