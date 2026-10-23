# ADR 0038: Export content-free client send diagnostics

- Date: 2026-10-23
- Status: Accepted
- Related: Architecture §3.7; ADR 0027

## Context

An uploaded iPhone report contained only periodic OS performance samples. It
could not identify a failed send, its operation ID, the failed client phase or
its structured RPC error. Target-side logs alone cannot explain failures before
the operation reaches the target.

## Decision

Shared Rust emits bounded structured diagnostic records for send and answer
operations through a new fire-and-forget Effect. The Apple adapter only appends
opaque JSONL to the existing serial, bounded diagnostic file and exports it
alongside OS samples. No platform code infers delivery from snapshots or errors.

Record operation ID, client phase, per-attempt monotonic elapsed time, wall-clock
time, structured error category/code and server operation status when available.
Keep the same operation ID through Locked Use preparation and authenticated
execution; each execution attempt has its own timing origin. Authentication and
cancellation are explicit phases. Client RPC acknowledgement does not prove a
model reply, and timeout/unknown remains unknown without replay.

Never serialize requests, message text, answers, attachments, raw error messages,
authentication challenges, signatures, network addresses or device/session names.
Target references are one-way digest prefixes. Operation IDs are bounded ASCII
identifiers. Reuse the existing two-file 4 MiB rotation and user-triggered export;
diagnostics never gate a send and storage failures never trigger retries.

## Alternatives

Logging Swift snapshots loses short-lived phases and duplicates Rust semantics.
Uploading unrestricted RPC messages or native logs leaks content. A separate
unbounded log or new polling loop increases storage and performance cost.

## Consequences

Reports contain mixed performance and send-diagnostic JSONL records, distinguished
by type/schema. Old reports cannot be retroactively enriched. Phone diagnostics
cover client routing, capability checks, dispatch and authentication; native AX
stages and exact provider receipts still require correlation with target logs.

## Verification

Test privacy allowlisting, exact error codes, unknown versus rejection, operation
identity across authentication, failure phases, bounded storage/rotation and
combined export. Run Rust and Apple regressions, generated-code, public-content,
architecture and ADR gates. Keep build/signing/publication/installation/physical
acceptance separate. GitHub Actions stays disabled.

### Source verification (2026-10-23)

- Shared Rust app-core: 118 tests passed, one pre-existing ignored test;
  Clippy with warnings denied passed. Duplex RPC regression preserves remote
  codes and unknown outcomes and observes exactly one dispatch per test case.
- Apple Core/storage: 10 tests passed, including the new Effect wire round trip,
  mixed-record export, partial-write recovery and bounded rotation.
- iOS simulator: 28 tests passed, including immediate diagnostic persistence
  while periodic sampling is paused, oversized-record rejection and reopening.
- Generated Swift protobuf output is reproducible; layer and public-content
  checks passed. ADR gate is required before landing.
- Apple tests linked an existing local Rust FFI archive; current Rust changes
  were tested separately. This is source/adapter verification, not a release
  build or an end-to-end send through the newly packaged core on a phone.
- No signing, notarization, OTA publication, installation or physical-device
  acceptance is claimed. GitHub Actions remains disabled; there is no CI result.


### Paired release follow-up

This change shipped in 1.0.88 (79), built with current Rust FFI archives for
both Apple platforms. The notarized macOS release is installed on three Macs;
iOS OTA was independently downloaded and verified. See
[ADR 0039](0039-exact-session-navigation.md#published-and-installed-1088-79)
for source provenance, artifact hashes and runtime checks. Publication does not
extend the earlier physical-device acceptance or establish new provider delivery.
