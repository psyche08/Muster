# ADR 0249. Keep the desktop chat directory beside the Windows conversation

- Date: 2026-11-20
- Status: Accepted
- Related: [Desktop chat catalog](../CHAT_CATALOG.md), [ADR 0246](0246-desktop-chat-history-availability.md), architecture §3.6.2

## Context

macOS keeps the desktop Chats directory beside the current conversation.
Windows removes its list when a row opens. Returning to Chats just to inspect
another conversation causes another catalog load. Windows also leaves cached
rows interactive after the chosen host disappears.

## Decision

Show the Rust-projected provider picker, selected host, open state, directory
and its errors in one WinForms sidebar on the desktop Chats root and its chat
detail screen. Use the listed opaque ID for mouse and keyboard selection and
the existing draft-bearing intent path for navigation. Keep the generic refresh
action. Retain cached rows for reading while disabling opening when the core
has no host or an open is in progress. Display an observed empty directory
separately from loading and failure. Normal updates preserve list focus and
scroll position. Size the conversation heading for the actual font and DPI. Hide the sidebar outside this workspace.

## Alternatives

Duplicating the directory in a platform cache would make stale or archived rows
look openable. Resolving by title would be ambiguous. The provider owns archive
membership. Muster's typed local directory and its open validation remain the
authority for the available desktop chat list.

## Consequences

A user can switch between listed chats without leaving the transcript. If an
existing chat is missing, the provider's archived list can be checked by exact
ID independently of local sync and message history. Changing archive membership
is an account action and separate from read-only acceptance.

## Verification

Exercise the real WinForms sidebar with two identically titled chats, a busy
open, a missing host, catalog refresh, provider switch, and a stale selection.
Check the desktop chat directory-to-history flow in Shared Rust. Verify package,
ordinary-user upgrade and live network separately.

All nine existing desktop chat integration tests passed using the production
local transport on Windows. At 192 DPI the control tests verified row identity
and navigation gates. A separate real WinForms session used the Rust FFI and a
synthetic RPC host with two identically titled rows. It read distinct histories,
restored the unsent draft on return and showed the sidebar beside the transcript.
The first screenshot exposed a clipped heading. A second screenshot and a
font-height assertion verified the correction. A live provider read independently
confirmed that the original acceptance conversation was archived and its two
messages survived. No provider message was sent during this check.

On Windows x64, the 1.1.99 WinForms assembly and packaged Rust FFI reopened
a synthetic ChatGPT catalog across a GUI and RPC host restart. The second
GUI used the same client data directory and a fresh host connection. It
opened the other identically titled row first, then reopened the original
by its listed ID. Each displayed the expected text and excluded the other
row's text. The separate daemon RPC regression also passed for temporary
receipts and stable IDs before and after observation. The GUI fixture exercised
catalog navigation and transcript rendering. It used no provider account.
