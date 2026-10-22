# Built-in iOS performance monitoring

Muster automatically records a lightweight OS performance sample every ten
seconds while its scene is active. There are no launch arguments, countdowns,
chat-selection prerequisites or special diagnostic installs. Inactive/background
scenes pause the timer; returning to the foreground resets the CPU baseline so
suspended time is not counted as idle foreground time. Shared Rust remains the
source of the screen enum and message count; this OS telemetry adapter does not
change product, transport, session or authorization state.

In **Settings → 性能监控**, inspect the latest reading or prepare a frozen JSONL
snapshot and share it using the system share sheet. This screen refreshes only
on entry or on an explicit refresh, avoiding an additional UI refresh loop.

## Sending the log to a Mac

**Settings → 性能监控 → 上报日志** lists the network's Macs. Choosing one takes the same frozen snapshot and sends it as `DiagnosticsUpload` over Muster's own links.

The receiving daemon checks three things before saving:
- the sender is an active member;
- the kind is `performance`;
- the size is between 1 byte and 6 MB.

It saves the log as `/tmp/muster-reports/<sender>-<UTC time>-performance.jsonl`. The daemon names the file itself and keeps the newest 50 reports. Files are readable by its own user only, and the directory is cleared when that Mac restarts. A toast on the phone shows the saved path or why the upload failed.

## Direct extraction

From a Mac paired with the iPhone, run:

```sh
python3 src/native/apple/MusterApp/scripts/ios-performance.py extract \
  --device '<iPhone UDID or CoreDevice name>' \
  --output /private/tmp/muster-performance-unique-name
python3 src/native/apple/MusterApp/scripts/ios-performance.py summarize \
  /private/tmp/muster-performance-unique-name/MusterDiagnostics --last-minutes 15
```

Extraction reads existing app-container files without relaunching the app,
sending any messages or uploading data. iOS pairing/data-access rules still
apply and may require an interactive unlock. The script refuses to overwrite
an existing evidence directory. A concurrently written incomplete final line
is excluded and counted; malformed complete records fail the analysis.

## Storage and interpretation

Files in `Library/Caches/MusterDiagnostics`:

- `performance.jsonl`: current segment, at most 2 MiB.
- `performance.previous.jsonl`: previous segment, at most 2 MiB.
- `Export/Muster-performance.jsonl`: at most one frozen export, at most 4 MiB.

Logs survive app relaunch/update and rotate when full; caches can still be purged
by iOS. The collector recovers a partial final write before appending. Files are
private to the app and protected until the device's first unlock. They contain
no conversation text, user/device/session identifiers, credentials, network
payloads or clipboard data. A random per-process run ID distinguishes thread
counters between app launches. Nothing is uploaded automatically.

Samples include app version/build, interval CPU (100% = one core), physical
memory footprint, iOS thermal state (0 nominal, 1 fair, 2 serious, 3 critical),
low-power mode, brightness, battery level/charging state, main-queue probe
latency, and snapshot counts/bytes/decode time. Screen 5 is the session list;
screen 6 is a conversation. Thread CPU counters are read once per minute, or
when process CPU is at least 20%. UIKit context and queue latency describe the
previous probe; the first probe after foregrounding has no measured latency.
`monitor_previous_work_seconds` reports the preceding sampling callback's work.

Thermal state is pressure, not degrees. Thread values are cumulative and can
only be differenced within the same run and thread ID. Memory read failures are
omitted from logs. Low CPU alone does not exclude display, charging, radio or
other processes as heat sources. Keep brightness, charging and page comparable
when checking a proposed fix; use Instruments for function-level attribution.
