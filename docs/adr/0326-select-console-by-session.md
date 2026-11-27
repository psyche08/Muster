# ADR 0326: Select the physical console independently of its login account

- Date: 2026-11-26
- Status: Accepted for worker startup
- Related: Supersedes the supervisor account-selection rule in [ADR 0324](0324-resident-windows-stream-host.md). [ADR 0325](0325-ordinary-console-stream-worker.md) governs ordinary desktop leases.

## Context

The console selector runs as LocalService. A valid physical session can have empty or unavailable WTS account fields. Resolving the console user's name before requesting a worker made session selection depend on that metadata.

## Decision

Use the attached, nonzero physical console ID as the scheduling target. Pass the installer-configured SID explicitly to Task Scheduler with its session-ID and user-SID flags. Windows resolves the interactive token for that identity. Recheck the physical session immediately before dispatch and keep existing stop and retry behavior.

An ordinary worker may bind its authenticated account-local endpoint once it is in the selected session. Verify ownership of the ordinary display when an authenticated lease asks for frames. The operating system and the media worker retain their existing desktop access boundaries.

## Alternatives

Leaving the RunEx user unspecified asks Windows to run the task as the user currently in the target session. Looking up that user in advance depends on metadata that the selector does not need.

## Consequences

An empty login account no longer suppresses the service's launch request. A request can still fail when Windows has no suitable interactive token. Selecting a console cannot by itself produce a login-screen frame.

## Verification

Build the supervisor and worker for Windows x64. Check that the supervisor has no WTS username lookup and passes the fixed SID to RunEx. Inspect the installed service and task with RDP disconnected. Record viewer frames and target input separately from worker dispatch.

### Attended physical-console comparison, 2026-11-27

The Windows physical console stayed at the sign-in screen with RDP disconnected. A Mac controller used Moonlight 6.2.0 with stock Sunshine v2026.914.233613. The operator saw the Windows sign-in screen, clicked Accessibility to open its menu and pressed Escape to close it. This verifies a visible frame, a click and one keyboard key on that surface. The test did not enter credentials or exercise general text entry.

The temporary host ran in the physical console session. After the stock service, test firewall rules and pairing were removed, the same Mac requested video from the installed Muster 1.1.128 Windows host (release commit `45e088c`). The host returned `ordinary-console-unavailable` before delivering a frame. The Muster service was running in session zero and the account daemon was in the disconnected session. No ordinary stream worker was running in the physical console. Pointer and keyboard acceptance through Muster remain open.
