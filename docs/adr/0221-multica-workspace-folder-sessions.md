# ADR 0221: Create Multica sessions in the run's workspace folder

- Date: 2026-11-16
- Status: Accepted; source and tests verified, device acceptance pending
- Related: amends [0184](0184-multica-task-authorized-codex-creation.md),
  [0192](0192-unlocked-multica-project-directory.md) and
  [0220](0220-background-only-native-creation.md);
  [0187](0187-trusted-multica-dispatch.md), [0191](0191-native-multica-receipts.md),
  [0199](0199-local-dispatch-grant-for-locked-use.md),
  [0222](0222-locked-use-dims-displays.md) (the locked Claude first turn)

## Context

Multica gives every run a new directory, `<root>/<workspace>/<run>/workdir`,
where `<run>` is the first eight characters of the run ID. It does not reuse
the directory. The runtime bridge created each native first turn in that
directory.

No app lists a directory that did not exist until the run started. Under
ADR 0220, a creation in a directory the app does not list fails. Every
unlocked Multica first turn therefore failed. On a locked Mac, the covered
Codex route instead added each run directory to the ChatGPT app as a new
project, one per run.

Claude first turns were direct `AgentNativeCreate` requests, not scheduler
operations. A Locked Use transaction that held the console refused them with
a conflict, and a locked Mac refused them outright.

A Claude side chat adds a second composer to the session's view. Creation and
sends both require exactly one composer. While a side chat was open, a
creation therefore waited without end, and a send failed because the
composer was ambiguous.

## Decision

1. A Multica first turn, for Codex or Claude, is created in the run's
   workspace folder: `<root>/<workspace>`, two levels above the run
   directory. The owner adds each workspace folder once to each app, as a
   ChatGPT project and as a Claude environment on this computer whose one
   folder is the workspace folder. Muster never adds either. A run directory
   that is not `<workspace>/<run[..8]>/workdir` fails with `Invalid`.
2. The submitted first turn is a brief that names the run directory, a blank
   line, and then the run's own first turn, unchanged. The brief tells the
   agent to change to the run directory, read its `AGENTS.md` first, and run
   every command and make every change there. Follow-up turns are sent
   unchanged.
3. The scheduler operation, the owner's grant and its contract still name the
   run directory and the unchanged text (ADR 0184). The scheduler derives the
   workspace folder and the brief only when it builds the native request, on
   the unlocked path and on the grant path alike. The bridge accepts a first
   user row in either of two cases. Either the row equals the expected first
   turn with the brief, or the row is bound to a digest and still matches the
   granted digest once the brief is removed.
4. Claude first turns use the same scheduler operation as Codex. They wait
   while Claude is the active app, and they are never refused because a
   Locked Use transaction holds the console. On a locked Mac they wait for the
   owner's grant (ADR 0184), as an unenrolled Codex run does. The trusted
   dispatch enrollment (ADR 0187, 0192) still names one Codex agent and
   applies only to Codex runs. A Locked Use transaction unlocks the Mac
   under cover for its own operation and locks it again when it ends. A run
   that arrives during that window waits for the transaction to end before
   it reads whether the Mac is locked. Before this, such a run took the
   unlocked path and failed when the Mac locked again.
5. If the ChatGPT app's own state does not list the workspace folder as a
   project, a Codex first turn fails with `Invalid` before anything is
   reserved, whether the Mac is locked or not. The covered route therefore
   never adds a project. Claude's folder is checked on its form (ADR 0220,
   rule 5), so while the Mac is locked it is not known before the covered
   creation starts.
6. An ADR 0192 project mapping still takes precedence on an unlocked Mac. Its
   directory and verified issue text are used without the brief.
7. Claude's new-session form names its folder through an environment picker.
   A named local environment is one folder and shows no folder picker; only
   "This computer" adds a folder picker. Muster reads the environments from
   the app's own `ccd-session-presets.json` and uses the one local
   environment whose only folder is the requested directory. It picks that
   environment by name from the picker's Local group, opening the folded
   rest of the list if needed. A name that another environment also uses,
   extra folders or a remote target do not identify a folder. Without such
   an environment, Muster chooses "This computer" and the exact folder from
   the app's recent list (ADR 0220). The form proof is either the named
   environment with no folder picker, or "This computer" with the exact
   folder.
8. A Claude side chat is not part of the session. It opens inside the
   session's primary pane as a landmark region with its own composer and
   send button. The Claude profile names that region (`auxiliary_regions`).
   A composer, send button or queued-message control inside that region is
   ignored, in creation and in sends alike. Background creation goes ahead
   while a side chat is open. It moves the window to the new-session form,
   as it would from any other open session (owner decision, 2026-11-16).
9. A background creation goes on while the front app changes among other
   apps. It stops only when the provider becomes the active app or no app
   owns the foreground, which is what ADR 0220, rule 1, asks for. Before
   this, any change of the front app stopped the creation, so switching from
   one unrelated app to another failed a Multica run.
10. On a covered console (Locked Use), a creation uses the in-app route of
    ADR 0220, rule 2, when a window offers it. Either a window already shows
    the requested empty new-session form, or it has the Codex project's
    "Start new chat in" button or Claude's "New" button. The directory route
    (`codex://new`, `claude://code/new`) is used only when no window offers
    one. Muster looks for one after it has activated the app and its windows
    have settled, at the point where it would choose a window for the
    directory route. Before this, a covered Codex creation reused a window
    that already showed the requested project. It verified that form before
    the directory route reloaded it, and the draft write then failed against
    the page being replaced.

## Alternatives

- Only "This computer" and its folder list: that list holds only the
  most recent folders, so a workspace that has not been used for a while
  drops out of it, and a folder Muster cannot pick fails.
- One fixed folder for every workspace: runs of different workspaces would
  share one project, and their sessions would no longer be grouped by
  workspace in the apps' sidebars.
- Adding each run directory as a project: the apps' project lists grow by one
  for every run, against ADR 0220.
- A reused Multica working directory: concurrent runs of one workspace would
  share one checkout.
- Rewriting the directory in the bridge: the grant, the trusted verification
  and the owner's review identify a run by its own directory.

## Consequences

The owner must add each workspace folder to both apps once: a ChatGPT
project and a Claude environment. Until that is done, a Codex run there fails
with `Invalid` before anything is reserved, and a Claude run fails on its
form unless the folder is in Claude's recent list.

Choosing a Claude environment changes the form's selected environment, as a
person's choice would. Renaming an environment or giving it another folder
changes which runs it serves.

In the app, the session's working directory is the workspace folder. The run
directory is inside it, so a sandbox that allows writes in the workspace
covers it. Only the brief keeps the agent in the run directory, and the agent
can see the directories of the workspace's other runs.

A locked Claude first turn now waits for the owner's grant for up to the
review window instead of failing at once. Follow-up turns go to the session
that was created, unchanged.

A granted Claude first turn on a locked Mac still does not start. Under
cover, Claude's new-session form never appeared in its accessibility tree
within the Locked Use window: not after the "New" button (decision 10), and,
on 2026-11-13, not after the directory route either. The run fails before
anything is submitted. [ADR 0222](0222-locked-use-dims-displays.md) dims the displays instead of covering them,
so the Claude window is no longer occluded; whether its form then appears
under cover is not yet tested on the device.

Under cover, a creation now navigates a window that is already open in the
same way as in the background. A Claude creation there picks its folder by
environment (decision 7). Only an app without a usable window still gets the
directory route.

A send to a Claude session reaches the session's own composer while a side
chat is open. A creation leaves the session that had the side chat as a
person pressing New would.

## Verification

- Shared tests: the workspace folder is derived only from an absolute
  `<workspace>/<run[..8]>/workdir` path without `..`. The brief is removed
  only for the same run directory, including from Codex's composer record.
  For both providers, a grant-path creation goes to the workspace folder with
  the brief, while the contract still refuses a changed directory. Other
  providers are not Multica first turns. A Claude environment identifies a
  folder only as the one local environment with that name and that folder
  alone; a shared name, a second name for the folder, extra folders, a
  remote target, the "This computer" name and unreadable state identify
  none. In a Claude view with a side chat, only the main composer and its
  send button count. Only a landmark region with the declared name, and
  only in Claude's profile, is a side chat. A background creation continues
  under any other front app and stops when the provider or nobody is in
  front.
- Local checks: library tests of `muster-acp` (25), `muster-agent` (433),
  `muster-platform` (60) and `musterd` (162). Strict Clippy with `--no-deps`
  passes on those four crates. In a debug build, one Locked Use test
  (`a_dispatch_lease_unlocks_under_cover_runs_its_one_operation_and_relocks`)
  overflows the default 2 MiB worker stack. It does so on `origin/main`
  without this change too, so `musterd` was run with `RUST_MIN_STACK` set to
  8 MiB.
- Device, locked Mac, 2026-11-16, with the workspace folder added to both
  apps as a ChatGPT project and a Claude environment:
  - Codex, under the trusted dispatch enrollment: the front window already
    showed the workspace project's empty form. Under cover, Muster used that
    form without a route, verified it in 0.1 s and submitted 3.4 s after
    admission. The session's working directory was the workspace folder and
    its first turn began with the brief. The receipt was reconciled from the
    transcript four seconds after the Locked Use window closed, as covered
    creations are. The agent ran its commands in the run directory and the
    run completed.
  - Codex, before decision 10, in the same state: four runs reused that
    window, sent the directory route and verified the form about 0.2 s
    later. Each then failed before submission: the draft write was refused,
    the form no longer verified, or the form had been replaced.
  - Claude: the owner granted the one pending run over the local socket,
    after its provider, run directory and contract hash were checked. Locked
    Use admitted it, and Muster pressed "New" under cover. The form did not
    appear within the Locked Use window, which ended with nothing submitted.
- Device, unlocked Mac (pending with this build).
