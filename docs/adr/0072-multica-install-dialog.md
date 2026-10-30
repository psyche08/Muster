# ADR 0072: Guided Multica installation with live logs

- Date: 2026-10-30
- Status: Accepted; device acceptance pending
- Related: ADR 0051, ADR 0054, ARCHITECTURE §3.7

## Context

Settings starts a lengthy Multica installation without a guided dialog or command
output. A single final status does not explain prerequisites, current work,
failures or the separate terminal login required after installation.

## Decision

Open a native sheet before installation with editable configuration, prerequisites
and an explicit start action. Show Rust-owned stages and bounded live stdout/stderr
from installation commands, then the outcome and terminal login instructions.
Keep the sheet open through completion; closing it does not cancel installation.

Shared Rust owns progress, exclusivity, output limits and outcomes. Add an additive
local-owner progress query to the existing typed ABI. App-core polls only during
an outstanding installation and recovers progress when Settings checks a running
installation. The platform renders this projection without running commands or
inferring success from logs. No automatic retry follows an uncertain RPC outcome.

Retain only the latest 64 KiB of logs in daemon memory, bound individual output
lines, remove control sequences, and suppress credential-bearing lines and URL
values. Do not log command arguments, configuration reads, environment or login
output. Do not change migration, authorization or runtime execution semantics.

## Alternatives

A UI-only spinner cannot report backend work. Sending shell execution to Swift
would duplicate lifecycle logic. Unbounded or persisted raw logs unnecessarily
retain installer output. A new transport is unnecessary for one local operation.

## Consequences

Operators can inspect progress and errors without leaving Settings. Apple
container and a trusted installer remain prerequisites; login is still an explicit
terminal operation. Logs are diagnostic and may be truncated or redacted. Daemon
restart loses this transient history; installation is never replayed automatically.

## Verification

- Source: 20 focused Rust unit tests passed (one existing operator-only test
  ignored), covering output before process exit, stdout/stderr, exit failures,
  timeout, dropped requests, UTF-8 bounds, redaction, exclusive mutation and
  interrupted app-core state. Two real local-RPC integration tests passed for
  live progress and reconnect recovery, preserving configuration and stopping
  polling after completion without replaying installation.
- Authorization: the existing real-link access integration test passed with the
  new progress action included in remote administration refusals.
- Build/checks: macOS MusterUI target compiled; affected Rust libraries/tests
  passed Clippy and focused formatting checks. Swift and HarmonyOS protobuf
  bindings were regenerated. Public-content, whitespace and ADR gates passed.
  Initial sandbox socket restrictions and a full system disk interrupted test
  attempts; reruns passed after local-socket access and deletion of this task's
  generated incremental cache. Sandbox Xcode cache warnings are not application
  failures.
- Acceptance limits: no real installer, container creation, terminal login or
  visual/device acceptance was performed. No signing, notarization, application
  installation or macOS/iOS release publication occurred. iOS and HarmonyOS were
  not built; the installation entry remains local desktop only.
