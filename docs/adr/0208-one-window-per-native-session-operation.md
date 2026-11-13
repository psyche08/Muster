# ADR 0208: Bind existing desktop session operations to one window

- Date: 2026-11-13
- Status: Accepted
- Related: [0205](0205-background-native-tasks.md), [0203](0203-restore-native-navigation.md)

## Context

A Codex process exposed two session sidebars during a covered inspection. Collecting their rows across the application produced two matching controls. Resolving duplicate titles after collection would still allow a later controller refresh to operate in another window.

## Decision

For existing Codex and Claude sessions, select one eligible provider window before reading its content. Prefer the provider's focused window, then its main window, then the frontmost eligible window in its AXWindows list. Exclude minimized windows and non-window handles. Bind the window by its AX object identity for the operation, including blocking workers. Resolve the session, sidebar and controls inside that window. Recheck ownership before actions and stop if the selected window changes. Use in-window navigation for both ordinary and covered selection. A retained controller must use the operation's window without replacing its delivery guard. Session restoration is limited to that window.

## Alternatives

Combining entries across windows or choosing the first duplicate permits an unrelated window to affect a request. Dispatching an application-wide URL cannot guarantee which window receives it.

## Consequences

A matching row in another window cannot rescue a missing row in the selected window. Duplicates inside the selected window remain ambiguous. New native project creation retains its separate admission and window-allocation rules.

## Verification

Exercise duplicate rows across two windows, same-window ambiguity, missing targets, window changes and absence of a usable window. Check scoped navigation, controller verification, cleanup and library regressions. Record physical-device evidence separately.

On an unlocked test Mac, a read-only probe using the macOS adapter found two listed Codex windows. It selected one, scanned one sidebar and found exactly one matching control for the session that previously returned two. Claude listed one window and the same probe found one matching control in one sidebar. Both complete scans checked that the control belonged to the selected window. Neither probe pressed a row. Covered end-to-end delivery and restoration with this build remain unverified.

Three Rust library suites passed 630 tests (nine ignored in the default run), and strict Clippy passed across all targets of those crates. The workspace layer checker continues to report its existing muster-acp to muster-agent and muster-ffi to muster-paths edges.
