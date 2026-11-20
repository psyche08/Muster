# ADR 0254. Show conversation model and reasoning options in a Windows dialog

- Date: 2026-11-21
- Status: Accepted
- Related: architecture §§3.6.2 and 3.7, ADRs 0181, 0194 and 0249

## Context

Windows put every available model and effort in the conversation action bar.
The entry disappeared during a host read, option descriptions were omitted
and options or their errors appeared among the transcript lines. The Apple
desktop keeps the picker visible while the shared operation progresses.

## Decision

Open an owned model and effort dialog for the conversation whose shared view
offers the action. Show the provider's names, descriptions, current choices,
reported effort, notes, busy state and errors. Load through the existing intent
on opening, support refresh and send only the selected model or effort name.
Forward a changed directory provider selection from the native selection event,
including accessibility selections. Rendering the shared provider must issue no
navigation request. Relay Win32 WM_ACTIVATEAPP for application foreground
changes, including the observed state on core startup. Opening an owned form
keeps the application foreground. Switching applications must still cancel
pending protected chat calls according to Shared Rust lifecycle rules. Update from ChatView while open. Dismiss on loss of capability or a change of
conversation or host. Keep the shared draft-bearing intent path.

## Alternatives

Adding more text and buttons to the conversation action bar would further
reduce the space for history. Guessing choices or keeping a platform settings
cache would risk stale or unsupported requests.

## Consequences

Choice information has its own keyboard-accessible surface. Reading live
options can open a picker on the provider's device. The host and Shared Rust
continue to determine which operations succeed and what was actually selected.

## Verification

Exercise the real form with synthetic choices, selected values, identical
names across categories, a pending load, error/retry and a change of target.
Run the shared local-transport tests and compare tested assemblies with the
release. Separate physical provider changes from fixture acceptance.

The Windows form suite passed at 192 DPI, including category-specific apply
intents and retry. All nine shared desktop chat transport tests passed. In a
separate isolated GUI, UI Automation selected the fixture provider, opened a
listed session and read five model/effort rows and current reasoning. Opening
the owned picker originally cancelled loading. After the lifecycle change the
read succeeded, moving focus to a separate process cancelled a background
refresh, and returning focus and refreshing recovered the options. No live
provider setting was changed.
