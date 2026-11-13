# ADR 0205: Locate desktop sessions in the background for every operation

- Date: 2026-11-13
- Status: Accepted
- Related: [0190](0190-background-desktop-admission.md), [0196](0196-foreground-waits-and-provider-queue.md), [0201](0201-background-claude-card-selection.md), [0203](0203-restore-native-navigation.md)

## Context

Three ordinary sends reached the executor but failed before entering the delivery ledger. The same hidden Claude Code session could be selected for reading an approval card. The selector chose between these routes by RPC type rather than by the capabilities of the running app.

## Decision

Use the already verified surface first, then the signed app's own session list when another app owns foreground. Use the same locator for sends, approval decisions, replies, interrupts and other session controls. Keep each provider's own identity evidence: Claude's unique desktop/CLI mapping and destination URL, Codex's catalog resolver, and CatDesk's runtime selection. A row is a locator, never permission to act. Recheck focus owner, process, input, console and permit at dispatch. An uncovered background selection cannot fall back to Launch Services. A remote inspect still defers navigation on recent local input. Restoration on a background provider must also use an in-app control, never an activating URL or window raise.

Use verified background creation forms even when model or effort was requested. Keep directory, form and option read-back checks. When a form or folder setup genuinely needs foreground, retain the existing admission boundary. Locked Use retains its independent authority and deadline.

## Alternatives

Opening a session URL with the background flag can still activate the provider. Retrying a send through a different route after uncertain navigation or submission can affect a different session or duplicate a message.

## Consequences

A missing or ambiguous row, changed foreground or unverified destination leaves the operation unsubmitted. A missing exact background return path skips cleanup. The provider's conversation can change within its own window during the operation.

## Verification

Exercise visible, hidden, unmapped, ambiguous and changed destinations, focus changes, active-user inspection, and the absence of activating fallback. Check the shared agent, macOS adapter and daemon; distinguish device acceptance from fixture coverage.

When Claude is on a positively identified Chat page, its own sidebar Mode/Code radio may expose the Code list; a Chat row with a matching title must not be pressed. An explicit provider unavailability alert is reported without pressing Retry. A covered executor with a different app in front tries the sidebar before using its existing exact-route authority. It may fall back only after a complete read without a match or expansion control and before dispatching any action.

Local verification: 628 agent, platform and daemon library tests passed (eight ignored); strict Clippy passed across all targets of these crates. One daemon late-receipt timing assertion failed on an intermediate run and passed in the final full run. Workspace formatting and the layer checker retain their existing unrelated baseline findings.

On an unlocked Mac, a send to a dedicated local test session reached background sidebar selection. The provider first exposed an empty Recents list and then explicitly reported that Claude was temporarily unavailable. The final candidate returned the provider error before the ledger in 365 ms. No message was submitted. Another Mac exposed a populated sidebar but did not have an eligible local test session for a write. Successful paired-device delivery, the return to the original conversation, and physical model/effort selection remain unverified. No production install or release was performed.
