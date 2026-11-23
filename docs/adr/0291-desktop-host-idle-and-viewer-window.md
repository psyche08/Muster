# 0291. Retire idle desktop hosts and use an independent viewer window

- Status: Accepted
- Date: 2026-11-23
- Related: [Desktop session lifetime](0289-desktop-host-session-lifetime.md), partially superseded here

## Context

The host currently exits with each viewer. The Windows viewer is owned by the main form and stays above it. A passive viewer can be actively watching a stream for long periods without moving the pointer.

## Decision

Start the host on an admitted desktop request. On disconnect stop the media session and clear its native state. Shared Rust may retain a dormant host for 30 seconds from disconnect. A returning viewer undergoes normal admission and receives fresh session credentials. The private native process can re-enter its startup code after an image replacement that closes its sockets. The owner pipe and the daemon bound its lifetime. Failed startup or reset discards the host.

Show the viewer as an independent, resizable taskbar window in its normal state. Preserve explicit maximize, minimize, closing and main-application shutdown.

## Alternatives

Pointer inactivity would disconnect a passive viewer. Keeping an old stream or input key available across admissions would extend its authority. A topmost or owned window interferes with ordinary task switching.

## Consequences

During the idle interval the worker has no capture, input or network endpoint. A cold launch follows expiration. The Windows viewer and main form can be activated independently.

## Verification

Check cold admission, passive viewing beyond 30 seconds, reconnect during grace, and termination after the final disconnect. Verify normal window styles, task switching, rendering and shutdown separately.
