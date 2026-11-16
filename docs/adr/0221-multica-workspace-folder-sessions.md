# ADR 0221: Create Multica sessions in the run's workspace folder

- Date: 2026-11-16
- Status: Accepted; source and tests verified, device acceptance pending
- Related: amends [0184](0184-multica-task-authorized-codex-creation.md),
  [0192](0192-unlocked-multica-project-directory.md) and
  [0220](0220-background-only-native-creation.md);
  [0187](0187-trusted-multica-dispatch.md), [0191](0191-native-multica-receipts.md),
  [0199](0199-local-dispatch-grant-for-locked-use.md)

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

## Decision

1. A Multica first turn, for Codex or Claude, is created in the run's
   workspace folder: `<root>/<workspace>`, two levels above the run
   directory. The owner adds each workspace folder once to each app, as a
   ChatGPT project and as a Claude folder on "This computer". Muster never
   adds it. A run directory that is not `<workspace>/<run[..8]>/workdir`
   fails with `Invalid`.
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
   applies only to Codex runs.
5. If the ChatGPT app's own state does not list the workspace folder as a
   project, a Codex first turn fails with `Invalid` before anything is
   reserved, whether the Mac is locked or not. The covered route therefore
   never adds a project. Claude's folder is checked on its form (ADR 0220,
   rule 5), so while the Mac is locked it is not known before the covered
   creation starts.
6. An ADR 0192 project mapping still takes precedence on an unlocked Mac. Its
   directory and verified issue text are used without the brief.

## Alternatives

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

The owner must add each workspace folder to both apps once. Until that is
done, a Codex run there fails with `Invalid` before anything is reserved, and
a Claude run fails on its form.

In the app, the session's working directory is the workspace folder. The run
directory is inside it, so a sandbox that allows writes in the workspace
covers it. Only the brief keeps the agent in the run directory, and the agent
can see the directories of the workspace's other runs.

A locked Claude first turn now waits for the owner's grant for up to the
review window instead of failing at once. Follow-up turns go to the session
that was created, unchanged.

## Verification

- Shared tests: the workspace folder is derived only from an absolute
  `<workspace>/<run[..8]>/workdir` path without `..`. The brief is removed
  only for the same run directory, including from Codex's composer record.
  For both providers, a grant-path creation goes to the workspace folder with
  the brief, while the contract still refuses a changed directory. Other
  providers are not Multica first turns.
- Local checks: library tests of `muster-acp` (25), `muster-agent` (430),
  `muster-platform` (52) and `musterd` (161). Strict Clippy with `--no-deps`
  passes on those four crates. In a debug build, one Locked Use test
  (`a_dispatch_lease_unlocks_under_cover_runs_its_one_operation_and_relocks`)
  overflows the default 2 MiB worker stack. It does so on `origin/main`
  without this change too, so `musterd` was run with `RUST_MIN_STACK` set to
  8 MiB.
- Device acceptance (pending): Codex and Claude first turns of a Multica run,
  on a locked and on an unlocked Mac, in workspace folders the owner has
  added to both apps.
