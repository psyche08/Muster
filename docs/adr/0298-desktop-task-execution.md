# ADR 0298: Serialize desktop tasks and admit quiet foreground windows

- Date: 2026-11-23
- Status: Proposed
- Related: Supersedes the foreground and queue admission rules in [0196](0196-foreground-waits-and-provider-queue.md), [0220](0220-background-only-native-creation.md) and [0297](0297-codex-background-project-picker.md).

## Context

Two providers can contend for the same foreground. Independent provider queues, a busy scheduler refusal and queue timeouts can skip a request or consume its execution time before it reaches a window.

## Decision

Use one FIFO for desktop agent work on a host. Acquire it before reserving a scheduled unlock or starting UI preparation. Recheck authorization and console conditions at execution, retain the turn through outstanding OS work and release it before read-only reconciliation. Record the monotonic execution start after acquiring the FIFO. Pass it through blocking AX workers to bound readiness waits. A caller can cancel while queued. Recheck grant expiry and quotas at the head. The signed lease has its own deadline. A compatible signed provider queue can continue serving its requester while there are no other desktop waiters. Other work waits for protection to drain and takes its own turn.

Prefer the app's safe background controls. At a foreground window, allow work after three continuous seconds without physical input reaching that window. Before activating another app require the same quiet interval on the outgoing foreground window. Recheck window identity and input at dispatch. A background refusal caused by a draft, modal or ambiguous choice cannot authorize foreground navigation. The foreground fallback rechecks the single window, empty composer, project identity and available controls before activation. It verifies the same window after activation. An in-app control takes priority over the existing foreground directory route. The background picker route uses no activation, raise or URL. Under a lease all global input, revocation, cover and relock guards continue to apply.

## Alternatives

Keeping a per-provider queue alone allows two simultaneous foreground transitions. Global HID idle during all background work also delays a task when the user types in an unrelated application.

## Consequences

The head can spend its execution budget waiting for the user's window to become quiet. A blocked head delays later tasks until it completes, times out or is cancelled. Reads of stored data do not occupy the desktop.

## Verification

Exercise two providers and two hosts, FIFO order, cancellation, a lease queued behind another task, window-specific input at the quiet threshold, takeover, and protection of drafts. Physical validation needs background creation and foreground handoff evidence separately.


At `cef74bb6` the macOS agent, platform and daemon all-feature library suites passed with 449, 61 and 174 tests respectively. Eleven opt-in cases were skipped. The tests exercise FIFO cancellation and start time, outstanding blocking work, signed challenge admission, exact project resolution, input at 2.9 seconds and a focus change during handoff. The arm64 release daemon built with its Secure Enclave feature. Its signed test application passed strict nested signature verification. The two background device receipts in ADR 0297 came from an isolated instance of that daemon on an unlocked console. Automated tests provided the foreground handoff and Locked Use coverage.

### Locked Use device acceptance, 2026-11-24

The installed system `musterd` at `0818fceb` admitted a scheduled creation on a locked Mac. During OS authorization the watchdog detected a change in local input. The scheduler recorded `not started`, no prompt was submitted, and the temporary grant was revoked. The Mac remained locked.

After the input generation stayed stable, a new Manager request obtained a fresh scoped lease. The plugin authorized the operation under the display cover, and Codex submitted the prompt in the background. Once the provider queue and AX work drained, the daemon relocked the Mac under cover and then released the cover. The scheduler initially recorded `unknown`. After relock it reconciled the original submission against provider evidence and returned an applied receipt. The test grant was revoked. [ADR 0297](0297-codex-background-project-picker.md#locked-use-device-acceptance-2026-11-24) records the picker route and directory check.
