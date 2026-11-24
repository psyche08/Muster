# 0305. Switch iOS workspaces from a navigation menu

- Date: 2026-11-24
- Status: Accepted
- Related: [0299](0299-apple-remote-workspace.md), [0296](0296-embedded-remote-workspace.md)

## Context

The leading navigation item shared space with workspace actions while trying to display three or four segment titles. The labels competed for width on compact iOS layouts.

## Decision

On iOS and iPadOS, display the current workspace in a single native menu button. Use a compact symbol when the title cannot fit or accessibility text sizes are active. The system picker identifies the current selection and lists the available workspaces in their existing order. Keep visibility and navigation driven by the shared view model and intents.

## Alternatives

A bottom tab bar would consume space used by the terminal, remote desktop and keyboard. Keeping all segment titles visible would still depend on the available navigation width.

## Consequences

Switching workspaces takes one tap to open the menu and one to choose a destination. VoiceOver can announce the action and current workspace even when only the symbol is visible. The macOS toolbar retains its segmented picker.

## Verification

Build the Apple shells and check the collapsed control, menu choices, current selection and workspace switching on compact and regular layouts, including landscape and accessibility text sizes.
