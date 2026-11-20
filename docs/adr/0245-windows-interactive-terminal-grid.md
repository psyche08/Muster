# ADR 0245. Render and operate the shared terminal grid on Windows

- Date: 2026-11-20
- Status: Accepted
- Related: [ADR 0035](0035-native-terminal-dev.md), [ADR 0043](0043-ordered-terminal-input.md), [ADR 0244](0244-windows-terminal-catalog.md), architecture §3.6.3

## Context

The Windows terminal showed plain lines and required Send line to submit the
text followed by Return. A user could not navigate an interactive program one
key at a time or see the projected cursor, cell styles and viewport.

## Decision

Use a focusable WinForms terminal surface. Render the shared rows by index,
styles and cursor in terminal cells. Offer selection and copy. Report measured
dimensions only for a resizable live session. Expose fixed native sessions
through a scrollable viewport. Forward committed Unicode text, key names and
explicit clipboard pastes through the existing typed intents. Keep the system
IME composition UI and Windows shortcuts for changing focus and closing the
window. Clear transient selection and partial characters on a change of target.

## Alternatives

A line editor cannot operate prompts and full-screen programs. A local VT
emulator would parse terminal state twice. Shared Rust already owns the grid,
key-to-byte mapping, paste policy, ordered input and resize authority.

## Consequences

Windows renders a remote program's own echo. A session can receive Return,
Escape, arrows, control combinations and function keys immediately. Clipboard
access remains user initiated. A click or keyboard focus returns input to the
terminal. Ctrl+Tab moves focus out of the terminal for keyboard navigation.

## Verification

Exercise committed Unicode, named keys and paste intents, ended and switched
sessions, styled cells, cursor position, copy selection, and resizable versus
fixed dimensions. Verify input without an appended Return and read back
computed output through a fresh local ConPTY session. Keep cross-device
authorization and physical IME testing distinct from controlled input tests.

The Windows fixture exercised styled output, a supplementary character,
cursor and selection at 192 DPI. The live GUI created a private test shell
through the normal terminal catalog, showed the command before Return,
inserted a digit with the Left key and displayed the computed result.
Changing the client width changed the columns reported by the daemon.
The test then closed its own shell. Physical candidate selection and a
terminal on a peer granting the required scope remain separate checks.
