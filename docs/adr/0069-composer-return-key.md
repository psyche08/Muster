# ADR 0069: Enter sends and Shift+Enter inserts a newline

- Date: 2026-10-30
- Status: Accepted
- Related: ADR 0068; Architecture §3.6 and §4

## Context

The owner requires Enter to send and Shift+Enter to insert a newline. The Mac
composer currently delegates Return behavior to a multiline SwiftUI TextField,
whose default editing/submission commands do not enforce that mapping.

## Decision

Adapt the Mac composer's native Return key event explicitly. Plain Return invokes
the same send callback as the send button; Shift+Return inserts a newline through
the active NSTextView's native editing command and consumes the event. Preserve
selection replacement, binding notifications and undo through AppKit rather than
reconstructing the text in SwiftUI. Marked-text composition stays with the input
method, so Return used to confirm Chinese input cannot dispatch a message.
Other modified keys retain native handling, including the existing Command+Return
send shortcut. Rust continues to own send validation, authorization and dispatch;
this change only adapts native keyboard input. iOS input behavior is unchanged.

## Alternatives

Relying on multiline TextField defaults leaves the two Return variants ambiguous.
A global unmodified Return shortcut could intercept IME confirmation or unrelated
controls. Manually appending a newline would ignore the selection and undo stack.

## Consequences

The mapping is scoped to the focused Mac composer. Empty/busy sends still use
the existing send guard. Native composition can consume Return before submission.

## Verification

Exercise AppKit selection replacement, ordinary Return dispatch, modified keys
and marked text with synthetic native editors. Compile the Mac shared UI. Record
source and build results separately from signed releases and live provider use.

### Source and native-event evidence

The Mac shared UI builds, and three AppKit tests pass: native selection/newline
editing, ordinary Return versus other modifiers, and marked-text composition.
`scripts/test-composer-return.sh` builds a separate synthetic native App from the
production key handler and posts events through the real application event queue.
It confirms Shift+Enter changes `draft` to `draft\n` with zero submissions, then
Enter submits exactly once without editing the text. No provider is connected.
The handler uses the event's window before the global key window, preserving the
active editor during key-window focus transitions. CLI XCTest windows do not
provide that full application event-loop/focus environment, so the complete
route is exercised in the standalone App instead.

These results were obtained on the current Mac. No signed product bundle,
notarization, publication, installation or live provider send was performed.
