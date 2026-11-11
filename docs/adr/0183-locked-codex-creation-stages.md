# ADR 0183: Identify the blocking stage of a locked native creation

- Date: 2026-11-11
- Status: Proposed; device acceptance pending
- Related: [0150](0150-remote-desktop-session-creation.md), [0154](0154-wait-for-slow-codex-windows.md), [0181](0181-native-create-model-and-effort.md)

## Context

On two Macs running 1.1.36, both member-initiated and scheduler-initiated Codex creation opened a new window but ended before first submission. The application window becoming visible did not prove that its composer or the requested directory was ready. Debugger sampling also perturbed the display watchdog and is unsuitable for measuring this deadline.

## Decision

Measure transitions on the guarded native creation path using elapsed monotonic time and fixed stage names. Do not log prompt text, directories, window titles, session identifiers or accessibility values. Compare the target's provider log, the submission ledger and the confirmed screen state against those stages.

## Alternatives

Extending the protected unlock or ignoring an empty composer would change the protection contract without showing where the time went. A UI screenshot alone cannot identify whether the directory route was dispatched.

## Consequences

A refusal before submission can be localized without stopping the daemon to sample it. These observations confer no authority and do not change delivery semantics.

## Verification

Check the macOS adapter and guards. On paired Macs test a new remote operation and a separate scheduler grant, starting locked. Verify the exact first prompt, unique session, assistant answer and automatic relock separately for each. Revoke the scheduler test grant after use.
