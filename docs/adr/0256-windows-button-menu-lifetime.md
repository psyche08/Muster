# ADR 0256. Bind Windows action menus to their anchor control

- Date: 2026-11-21
- Status: Accepted
- Related: architecture §3.6.3, ADRs 0245, 0247 and 0255

## Context

Closing an isolated Windows GUI with the conversation terminal action menu open
raised InvalidOperationException when its Closed handler called BeginInvoke on
a control with no window handle. The device filter and three transcript menus
used the same disposal pattern.

## Decision

Tie each one-shot button menu to the lifetime of the control that shows it.
Dispose it when the anchor handle is destroyed. On ordinary closure, release
the menu after the current input event so its selected item can finish its
click handler. Unsubscribe when the menu is disposed. Keep option content and
selection guards unchanged.

## Alternatives

Leaving closed menus attached to the form would retain obsolete choices and
their captured view state. Disposing during every ordinary close could
interfere with the input event that selected the item.

## Consequences

Menu release works during normal navigation and application shutdown. The
change is limited to the Windows window lifecycle.

## Verification

Exercise item selection and anchor destruction while the real menu is visible.
Repeat the normal GUI path on an isolated managed shell and close the window
with the menu open. Verify the installed GUI resumes and the production daemon
keeps its process and identity.

At 192 DPI, the Windows terminal tests selected a real menu item with Enter,
observed exactly one callback, and verified disposal after both selection and
window closure. In a separate GUI process, opening a synthetic conversation
through Agents and selecting Terminal resumed its shell. UI Automation found
the enabled copy command in the visible menu. Closing the main window with
that menu open exited with code zero. The installed GUI reopened through its
fixed shortcut and the production daemon retained its PID.

The same GUI path also created the first shell on a fresh node and produced
the expected output. Starting again after ending the last shell separately
exposed a tmux new-session timeout. An explicit Connect retry reached the
connected state, but a later has-session query timed out. That host behavior
remains open.
