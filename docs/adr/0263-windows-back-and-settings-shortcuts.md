# ADR 0263. Use shared Back availability and desktop navigation shortcuts on Windows

- Date: 2026-11-21
- Status: Accepted
- Related: [ADR 0252](0252-windows-network-member-details.md), architecture §3.7

## Context

On the installed Windows desktop the Back button was active at the Agents
root. Shared Rust already reports whether a page has a parent. The Mac
toolbar uses that decision and its Settings command has a keyboard shortcut.

## Decision

Display Back when the core allows returning. Submit a one-step navigation
intent with the displayed path so a delayed second click cannot pop an
unseen parent. Use Alt+Left for Back and Ctrl+comma for Settings in the
Windows main window. Route button and keyboard actions through the existing
draft-preserving navigation path. Leave input inside a terminal grid to
the terminal.

## Alternatives

A second navigation stack in WinForms would have to mirror Rust on every
page transition. Global hotkeys would capture input outside the window.

## Consequences

The root toolbar has no Back action. Settings and nested detail pages can
be reached and left with the keyboard. The core owns the page transition
and the effects of leaving a conversation.

## Verification

Check Back at the Agents root and on a nested Settings or device page.
Exercise both shortcuts on the installed window against an isolated node,
including returning to the prior page, and run the shared stale-path test.
