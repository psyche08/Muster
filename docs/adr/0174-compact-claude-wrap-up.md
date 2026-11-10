# ADR 0174: Show Claude's optional wrap-up as inline actions on Apple

- Date: 2026-11-10
- Status: Accepted
- Related: [Claude Desktop cards](../CLAUDE_CARDS.md), [native ViewModels](../architecture/runtime.md#37-rust--native-boundary-message-based-abi--viewmodel)

## Context

Claude Desktop displays the recap, next question and one or two action buttons
together at the end of a turn. On Apple, Muster displayed the same interaction
inside a collapsed questionnaire. Expanding it repeated the optional heading
and option text and required a separate submission.

## Decision

Shared Rust identifies actionable optional Claude wrap-ups in the chat ViewModel
using the existing card classifier. Apple renders them open with a subdued recap,
the question, and the provider's actions in order. Pressing a button returns the
existing structured question and option IDs. The close control folds this local
view and allows reopening. When the question accepts custom text, the card
context menu opens the answer field. The composer retains its Command-Return
binding.

## Alternatives

Restyling every optional question would also change multi-page forms and task
proposals. Matching localized question headers in Swift would duplicate provider
classification.

## Consequences

One click answers the wrap-up. The existing submission, failure and uncertain
outcome states apply. Required questions and clients using the ordinary form
continue to consume the same pending interaction.

## Verification

Exercise the projection with optional and required cards and another provider.
Render synthetic cards at desktop and phone widths, compile the Apple UI, and
run the public-content and ADR checks. Installed-device layout and live provider
delivery require separate acceptance.

The projection regression passed. The macOS and iOS Simulator UI targets built.
Both Apple render tests passed on macOS, and the 320 pt and 736 pt dark-mode
images were inspected. The reference shortcut hint is omitted because
Command-Return sends a composer draft. Installed-device testing and live
Claude reply delivery remain pending.
