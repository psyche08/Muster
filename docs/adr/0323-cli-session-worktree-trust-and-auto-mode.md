# ADR 0323: Open a CLI session in a trusted worktree, with Claude in auto mode

- Date: 2026-11-26
- Status: Accepted, installed-device acceptance pending
- Related: [ADR 0173](0173-direct-native-cli-protocols.md), [ADR 0322](0322-native-cli-provider-identities.md), [native CLI sessions](../NATIVE_CLI.md)

## Context

An external scheduler can open native CLI sessions through `orca_create`.
One such scheduler handles code-review work: each pull request gets one
Claude session that reviews it, answers comments, and fixes what reviewers
raise. Three things are missing for that work to run unattended:

- **A checkout of its own.** A session works in the directory it names. Work on
  one pull request must not disturb the person's checkout or another pull
  request's session, so it needs its own Git worktree. The scheduler cannot
  create one: it reaches the device only through Muster.
- **Folder trust.** `claude --print` never shows the trust dialog. In a folder
  nobody has trusted, Claude ignores the project's `permissions.allow` and
  `additionalDirectories`, and reports this on stderr, which Muster discards.
  Claude records trust in its global config as
  `projects[<dir>].hasTrustDialogAccepted`. It has no flag or variable that
  sets it. A new worktree has never been opened interactively, so it is
  never trusted.
- **A permission mode.** Claude sessions always start with
  `--permission-mode default`, so every tool use waits for a person. Claude's
  desktop sessions for the same work run in `auto` mode. In that mode Claude's
  classifier approves what it judges safe and asks for the rest.

## Decision

`OrcaCreateRequest` gains three fields. A create that leaves them empty behaves
exactly as before.

- `permission_mode` (5): `""`/`default`, or `auto` for Claude. Every other mode
  is refused, including `bypassPermissions`, `dontAsk`, `acceptEdits` and
  `plan`, and so is any mode for Codex. Muster never launches a provider with
  approvals switched off. Auto mode still sends what its classifier will not
  approve to Muster's ordinary approval cards. The mode is stored with the
  session. A child started again after a restart passes the same
  `--permission-mode`.
- `worktree` (6): a name, 1-64 of `[A-Za-z0-9._-]`, not starting with `.` or
  `-`. Muster finds the main checkout of the repository that holds
  `directory` from the first entry of `git worktree list --porcelain`. A bare
  repository is refused. The session runs in
  `<main checkout>/.claude/worktrees/<name>`, which is where Claude Code's
  desktop app keeps its own worktrees.
  - An existing worktree of that repository at that path is reused as it is.
  - A new one is added with `git worktree add --detach` at the main checkout's
    `HEAD`.
  - A path that exists but is not one of the repository's worktrees is refused.
  - Unless something already ignores `.claude/worktrees`, Muster appends
    `/.claude/worktrees/` to the repository's `info/exclude`. This keeps the
    main checkout's `git status` clean.
  - Muster never fetches or changes branches. The session brings the worktree
    to the commit it needs.
- `trust` (7), Claude only: before the session is recorded, Muster sets
  `projects[<session dir>].hasTrustDialogAccepted = true` in Claude's global
  config. Claude only finds trust on a folder or one of its parents, so a
  worktree needs its own entry.
  - **Which file.** The file is `$CLAUDE_CONFIG_DIR/.claude.json`, taking the
    session's own `CLAUDE_CONFIG_DIR` before the daemon's, else
    `~/.claude.json`. If Claude would still read a non-empty legacy
    `.config.json` instead, trust is refused.
  - **Lock.** The file is shared with every running Claude instance, so the
    write follows Claude's own save protocol. Muster holds the same
    `<config>.lock` directory lock (proper-lockfile) that Claude takes for its
    saves. It waits up to five seconds for a busy lock, and clears a lock older
    than proper-lockfile's ten-second stale age as that library does.
  - **Write.** Muster re-reads the file under the lock and replaces it
    atomically with a same-mode temporary file and a rename.
  - **Other entries.** Every other key keeps its text and position. Values
    Muster does not change are copied as raw JSON, not re-encoded. A config
    that is not a JSON object is left untouched and the create fails.

The worktree and trust are prepared on a blocking thread, outside the journal
lock, before the session row is written. A failure leaves no session behind. A
worktree made before a later failure stays, and the next create reuses it.
The `creates` row records the requested directory. A replay must ask for the
same directory, agent, permission mode and worktree, or it is refused as a
reused operation. The daemon ledger still refuses a replay with a different
request body.

## Alternatives

- **Have the scheduler create the worktree and write the trust.** It has no
  shell on the device, and giving it one would be a larger power than these
  two bounded steps.
- **Rewrite `~/.claude.json` with `serde_json::Value`.** The map type here
  sorts keys, so every create would reorder a file the person and Claude both
  read. Switching on `preserve_order` would change map order across the whole
  workspace.
- **Pass `bypassPermissions` or `dontAsk`.** Both switch the device's prompts
  off for a remote caller; refused by design.
- **Trust the main checkout only.** Claude looks for trust at the session
  directory and its parents. The main checkout is a parent of the worktree, so
  this would also work. But it would trust everything under it on the
  scheduler's say-so, while a per-worktree entry is exactly what accepting the
  dialog in that folder records.

## Consequences

- Remote callers that may create sessions for a native CLI provider
  (`agents:claude-cli` or `agents:codex-cli`, or the historical `agents:orca`;
  ADR 0322) can now create worktrees under a project's `.claude/worktrees` and
  append one line to its `info/exclude`. Callers holding the Claude provider
  can also mark those worktrees trusted in Claude's config. These are bounded
  effects on paths the caller already names. No new scope is introduced, so
  the existing provider grant covers them.
- Muster never removes worktrees. A finished session's worktree stays until
  its owner removes it with `git worktree remove`. The scheduler that asked
  for it is expected to remove it when its work ends.
- A trusted folder's project hooks, MCP servers and permission rules apply to
  the session, as they would after a person accepted the dialog there.
- Auto mode needs a Claude CLI that supports it; an older CLI fails the first
  send.

## Verification

- **Unit tests:** `cargo test -p muster-agent --lib cli::` covers the
  permission-mode allowlist, worktree name rules, and the order-preserving
  trust edit. They also cover a config that is not an object, and waiting out
  or clearing Claude's lock.
- **Integration tests:** `cargo test -p muster-agent --test native_cli`
  creates a Claude session in a fixture repository and checks that:
  - the worktree is added at the main checkout's `HEAD`, and the main
    checkout's status is clean;
  - the exclude line is added once;
  - trust is written to the session's `CLAUDE_CONFIG_DIR` with the other keys
    kept;
  - the child is started with `--permission-mode auto`;
  - a replay returns the same session, and a changed replay is refused;
  - a second session reuses the worktree;
  - inapplicable options and a squatted worktree path are refused, with
    nothing created.
- **Installed Claude CLI 2.1.323, without a model request:** in a scratch
  project with a `permissions.allow` rule, the CLI accepted
  `--permission-mode auto`. Untrusted, it warned that the rule was ignored
  because the workspace had not been trusted. With
  `projects[<dir>].hasTrustDialogAccepted: true` it started without the
  warning.
- **Not yet verified:** a signed Muster build, its installation on the host,
  and a scheduler-driven session reviewing a real pull request.
