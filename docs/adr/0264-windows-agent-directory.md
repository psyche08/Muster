# ADR 0264. Keep the Agents directory visible beside Windows session details

- Date: 2026-11-21
- Status: Accepted
- Related: [ADR 0241](0241-windows-agent-session-actions.md), [ADR 0249](0249-windows-desktop-chat-sidebar.md), architecture §3.7

## Context

The Mac keeps the Agents catalog beside a conversation and its files. On
Windows, opening a row removed the catalog. Opening another session from a
file detail also left that detail on the shared Back stack after its data was cleared.

## Decision

Use one Windows sidebar for the shared sections, search, device filter, create
actions and row menus across the Agents root and its chat, file and Git pages.
Open the projected session and device with the draft-bearing intent path.
Match mouse gestures and restore selection by action identity across refreshes.
When showing a chat, unwind any current file or Git detail pages in Shared Rust
before clearing their data and showing the new session.

## Alternatives

Returning to Agents to switch sessions reloads the catalog. Looking up a row by
title is ambiguous across devices. A Windows-only Back stack would disagree
with the navigation path projected by the core.

## Consequences

Switching sessions preserves an unsent draft through the existing shared cache.
The directory and the worktree file list occupy separate panes when both are visible.

## Verification

Exercise two rows with the same title, refresh during selection, and check the
device and session on the generated intents. Check Back after opening a
session from worktree detail. On an isolated Windows node switch between two
synthetic sessions and check that the original draft returns.
