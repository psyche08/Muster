# ADR 0272. Confirm ending a side chat on Windows

- Date: 2026-11-21
- Status: Accepted
- Related: [ADR 0215](0215-side-questions.md), architecture §3.7

## Context

End permanently closes a side chat to new messages. The Apple composer asks for confirmation and explains that history remains readable. The Windows composer sent End immediately. A modal dialog also lets queued host observations change the active conversation before its answer arrives.

## Decision

Show an owned Windows confirmation with Cancel as the default, retain the composer's Rust-issued conversation key at opening, and dispatch the captured End only on explicit approval. Recheck the current projection and validate the wrapped End against the active conversation key in Shared Rust. Keep admission, journaling and history on the existing Rust path.

## Alternatives

A direct button makes an accidental click permanent. Checking the window alone misses queued Rust navigation. The existing composer snapshot already carries the scoped key.

## Consequences

Cancelling leaves the child open. Switching hosts or sessions while confirming cannot redirect the operation. Opening a side chat remains a single action.

## Verification

Check cancellation and approval through the Windows form, stale key and screen rejection in Rust, and continued history readability in the daemon's side-chat tests. Use synthetic sessions and report installed GUI and real provider acceptance separately.

The Windows x64 Release FFI and WinForms app built locally. The scoped confirmation regression covered a changed key, absent snapshot and a change of page. An isolated process exercised the real WinForms dialog and Shared Rust against a software-identity daemon. Cancel was the default button. Approval after changing children and replaying the first child's snapshot did not end the second child. Restarting the daemon preserved the confirmed child's read-only state and addressable history, and the other child remained open. This test created empty synthetic side chats without sending provider questions.
