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

The paired-device trace reached the new window at 2.58 s, then spent the
remaining lease waiting for its initial composer. No directory navigation
was dispatched. For a covered Codex creation, send the directory-only route
once the uniquely new focused standard window exposes the application's
document. The first editor belongs to the as-yet *unrouted* shell and is
not a precondition for sending the route. Authorization to write still
requires that exact window, the routed form, the selected local project
and exact directory, empty composer and verified draft. The unlocked
scheduler and manual creation retain ADR 0154's composer wait. Do not
extend a lease or send a second route on timeout.
