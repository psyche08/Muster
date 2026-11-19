# ADR 0230: Pick a Claude environment by its main folder, extra folders included

- Date: 2026-11-19
- Status: Accepted; source and tests verified, device acceptance pending
- Related: supersedes decision 7 of
  [0221](0221-multica-workspace-folder-sessions.md); keeps the fallback of
  [0220](0220-background-only-native-creation.md)

## Context

ADR 0221, decision 7, lets a native Claude creation pick a named local
environment from the new-session form only when that environment's only
folder is the requested directory. An environment with extra folders
(`additionalFolders` in the app's `ccd-session-presets.json`) identified no
folder, so the creation fell back to "This computer" and the app's recent
folder list.

Owners keep repositories in multi-folder environments: one main folder with
the related checkouts attached as extra folders. A caller that wants a session
over such a group asks for the environment's main folder. Because of the
extra folders, Muster chose "This computer" instead, and failed with
"is not a folder Claude lists" once the main folder had dropped out of the
recent list. A multi-folder environment could not be reached at all.

A session started from a named environment runs in the environment's main
folder: the session record's `cwd` and the transcript's first `cwd` are the
main folder, and the transcript is stored under that folder's project
directory. The extra folders are attached to the session beside it. This was
checked on the owner's Mac against a session created from a two-folder
environment (2026-11-19).

## Decision

1. A local Claude environment is identified by its main folder,
   `target.cwd`, whatever its `additionalFolders` hold. A creation for
   directory `D` picks, by name, the one environment with `target.kind`
   `local` and `target.cwd` equal to `D`. The session then runs in `D`, with
   the environment's extra folders attached.
2. The other rules of ADR 0221, decision 7, stand. An empty name or
   "This computer" names no environment. A name that another, non-matching
   environment also uses names none. Two differently named environments with
   the same main folder name none. A remote target, unreadable state or a
   relative directory name none. An extra folder never names the environment:
   asking for it is not asking for the environment.
3. The form proof is unchanged in shape: the named environment selected and
   no folder picker on the page, or "This computer" with the exact folder.
   The named branch now accepts a multi-folder environment through rule 1.
4. Receipt discovery is unchanged. It matches a new session by the requested
   directory and its first prompt, and the session's directory is the
   environment's main folder (Context).
5. Without such an environment, Muster still chooses "This computer" and the
   exact folder from the app's recent list (ADR 0220), and never adds a
   folder or an environment.

## Alternatives

- Keep requiring a single-folder environment and have the owner keep a
  second, single-folder copy of each multi-folder environment: two
  environments for one folder would then name none (rule 2), and the copy
  would lose the extra folders the session needs.
- Accept a request for an extra folder and pick its environment: the session
  would run in the main folder, not the requested one, and receipt discovery
  by directory would never find it.
- Match the directory against every folder of every environment and pick by
  the most specific: the form names environments, not folders, so this adds
  guessing without adding a case the owner needs.

## Consequences

A multi-folder environment is reachable by asking for its main folder, and
the new session has its extra folders attached, as when a person picks it.
Callers that want a session over an extra folder's repository ask for the
main folder and tell the agent which attached folder holds the repository.

Adding an extra folder to an environment no longer moves its main folder's
creations from the environment to "This computer". Renaming an environment,
or changing its main folder, still changes which creations it serves.

## Verification

- Source and tests: `muster-agent` test
  `a_claude_environment_names_its_main_folder` covers a multi-folder
  environment, an extra folder, empty extras, a remote target, missing and
  reserved names, a reused name and two names for one folder. `muster-agent`
  and `muster-platform` tests and clippy pass.
- Runtime evidence: on the owner's Mac a session created from a two-folder
  environment records the main folder as `cwd` in both the app's session
  record and the transcript (Context).
- Not verified here: a live native creation through Muster into a
  multi-folder environment, including the form proof (no folder picker for a
  multi-folder environment). That needs a released build on the Mac that
  runs the creation.
