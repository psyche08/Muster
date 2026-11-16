# ADR 0220: Create native sessions only while the provider is in the background

- Date: 2026-11-16
- Status: Accepted; source and tests verified, device acceptance pending
- Related: amends [0190](0190-background-desktop-admission.md),
  [0196](0196-foreground-waits-and-provider-queue.md) and
  [0205](0205-background-native-tasks.md); [0189](0189-reuse-native-create-windows.md),
  [0203](0203-restore-native-navigation.md), [0115](0115-target-scoped-local-input.md)

## Context

An uncovered creation that found no ready form in the background waited for
its owner to bring the provider to the front. It then navigated the window
the owner was using: Codex through its directory route and Claude through its
`code/new` URL. The owner wants the opposite. While Codex or Claude is not the
active app, Muster should start the session there without bringing it
forward. While it is the active app, Muster should wait.

Three facts measured on the owner's Mac shape the change:

- Codex lists a "Start new chat in" button for every project in its sidebar.
  The button was present and pressable with another app in front, even while
  the window showed an older chat. ADR 0190 used it only in a window Muster
  had allocated or on an empty home page.
- A hidden app (Hide, Command-H) does not render. After that button was
  pressed, the window title changed, but its AX tree still showed the old
  chat. Setting the application's `AXHidden` to false showed the window
  behind the front app. The front app stayed in front, and the new form
  appeared in the AX tree within a few seconds.
- With the screen locked, Claude's AX tree is degenerate: `AXWindows` returns
  the application element. Locked Use keeps its own covered route.

Every Multica project already exists as a project in the ChatGPT app and as a
folder in Claude. Muster never needs to add one.

## Decision

1. An uncovered creation acts only while another app owns the foreground.
   `NativeConsole::may_create` admits `Background` and `Covered` only.
   While the provider is the active app, a managed creation keeps waiting
   outside the ledger and provider queue. A held request returns a conflict
   that names the reason. The foreground directory route is now reachable
   only from a covered console. Locked Use is unchanged.
2. In the background, each provider window can be navigated by its own
   in-app control, never by activation, raising or a URL:
   - Codex: the project's "Start new chat in" button, in any window.
   - Claude: the sidebar "New" button, in a window that shows Code (a session
     or the empty new-session form), never a Chat page.

   The window must have no sheet or in-app dialog. It must have exactly one
   composer, and that composer must be empty, so no draft is navigated away.
   An already verified empty form for the directory is still used first.
   Muster's allocated Codex window is still preferred. A running session in
   the window keeps running in the app after the window navigates.
3. On Claude's new-session form, Muster chooses "This computer" and the exact
   folder from the form's own Environment and Folder pickers. A menu item is
   chosen only if one of its labels is the full path or its exact `~/`
   abbreviation, as in the existing form proof. The directory, form and
   composer checks of the existing submit path then run unchanged.
4. A hidden provider is shown without being activated for the creation, and
   hidden again when it ends. It stays shown if its user brings it to the
   front meanwhile. A minimized window is not restored.
5. A directory that the app does not list fails with `Invalid`, and nothing
   is submitted. For Codex, this means no local project of the app has that
   root, which can be read before any action. For Claude, it means no item in
   the folder picker matches. Muster never opens a folder sheet or adds a
   project. Tusk treats `Invalid` as a refusal, so the requester gets a
   failure instead of a retry. A Multica first turn fails the same way
   instead of waiting.
6. The fallback that pressed "New session" in the window of the previous
   submission is removed. Claude now names that button "New", and rule 2
   covers both providers.

## Alternatives

- Activating the provider and restoring the previous app afterwards leaves an
  interval in which the owner's keystrokes go to the provider (ADR 0190).
- Opening a route with the background flag can still activate the provider
  (ADR 0205).
- Navigating Codex's folder sheet ("select project root") works without
  activation, but posting the Go-to-folder key had no effect, and adding
  projects is out of scope.
- Waiting indefinitely for a project that does not exist hides a setup error
  from the requester.
- Restoring a minimized window would animate it out of the Dock in front of
  the owner.

## Consequences

The owner's provider window can show the new session afterwards. ADR 0203
still returns it to the original session when its conditions hold. A
provider that was hidden is hidden again, so it is not seen until its owner
opens it. While the provider is in front, creations queue (managed) or are
retried by their caller (Tusk). A Claude window left on a Chat page, a
minimized-only window, an open dialog, or a draft keeps the creation waiting
or retried rather than navigated.

## Verification

- Shared tests: `may_create` admits only `Background` and `Covered`; a held
  wait refuses for the active provider and names it; `codex_lists_directory`
  separates a listed root, including a duplicate project name, from an
  unlisted one and from unreadable state.
- Local checks: library tests of `muster-agent`, `muster-platform` and
  `musterd`, and strict Clippy with `--no-deps` on those crates. Clippy
  without `--no-deps` stops in `muster-core` on a pre-existing
  `collapsible_match` finding from the installed toolchain, in a file this
  change does not touch.
- Device, with a signed build of this branch installed on the owner's Mac:
  - Locked: a Codex Multica first turn ran through Locked Use and created its
    session. The covered route added the run's own directory to the ChatGPT
    app as a new project. ADR 0221 now refuses that case before anything is
    reserved.
  - Unlocked, with another app in front: a Claude Multica first turn for a
    run directory left Claude in the background. Muster opened and closed the
    folder picker without finding a match and failed with `Invalid`. Nothing
    was submitted.
  - A probe script for these checks acted during a Locked Use lease, while
    the Mac was briefly unlocked under the cover. It left Claude's window on
    its New page. The earlier session was reopened by pressing its sidebar
    row in the background. Probes that wait for an unlocked Mac must also
    wait for Locked Use leases to end.
  - Not yet observed: a successful background creation in a listed directory,
    for either provider. Since ADR 0221, Multica runs are created in their
    workspace folders, and that acceptance is recorded there.
