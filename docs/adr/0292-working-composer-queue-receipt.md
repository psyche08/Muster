# ADR 0292: Recognize a working composer's return to Stop as queue acceptance

- Date: 2026-11-23
- Status: Accepted
- Related: [provider queue receipts](0063-queue-receipts-use-provider-text-matching.md), [explicit send resolution](0061-send-receipt-format-and-manual-resolution.md), [queued actions](0158-queued-message-actions-and-codex-paginated-runtime.md)

## Context

Codex and Claude use different records for queued prompts. A person can see the working composer's submit button return to Stop before Muster finds the new queue or transcript record. The original operation then remains unconfirmed and blocks further sends.

## Decision

Extend ADR 0063's evidence boundary for a normal prompt sent to a working native Codex or Claude session. Observe a unique enabled Stop in the selected composer before drafting, its disappearance when the exact draft and unique submit control are verified, one successful press, and the return of a unique enabled Stop with a readable empty composer. Bind all observations to the same session view, target process, uninterrupted input generation and operation. Use the existing receipt deadline and ledger confirmation callback. The observation only acknowledges the provider accepting the submitted input. Provider record receipts keep their existing behavior.

## Alternatives

A Stop sampled only after the press could belong to a previous turn. An empty editor alone could reflect local editing. Retrying a press could enqueue the same message twice.

## Consequences

A working session can acknowledge queue acceptance before writing a matching record. An idle send, question answer, ambiguous controls, changed view, or unreadable editor needs the existing provider evidence. A later Stop sampled outside the operation cannot resolve an outstanding write.

## Verification

Exercise the composer transitions, ambiguous observations, exact-operation matching and persistence failure in Rust. Build and test the macOS adapter separately. The Windows Codex send adapter supports idle sessions under [ADR 0281](0281-windows-codex-desktop-send.md). Live provider and packaged-device acceptance require each desktop app to be running a turn.

On macOS, 443 agent library tests, 55 platform library tests and two daemon provider-queue RPC tests passed. Seven opt-in library tests were ignored. The new cases covered both provider profiles, an unchanged or ambiguous control, an unreadable or nonempty draft, a stale operation and a failed ledger write followed by reconciliation. Signing, installation and live working-turn acceptance on both providers remain pending.
