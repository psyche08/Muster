# ADR 0329: Give a CLI child the user's directories on its PATH

- Date: 2026-11-27
- Status: Accepted; source and tests verified, release and installed-device acceptance pending
- Related: [ADR 0173](0173-direct-native-cli-protocols.md), [ADR 0323](0323-cli-session-worktree-trust-and-auto-mode.md), [native CLI sessions](../NATIVE_CLI.md)

## Context

The desktop app starts `musterd` with launchd's PATH,
`/usr/bin:/bin:/usr/sbin:/sbin`. Muster finds the native CLI itself: lookup
searches the daemon's PATH, then `~/.local/bin`, `/opt/homebrew/bin` and
`/usr/local/bin`. The CLI child, however, inherits the daemon's PATH, and so
does every command the agent then runs.

An external scheduler opens Claude sessions through `orca_create` to review
pull requests (ADR 0323). Its reviews run tools that live outside the system
directories: a code-hosting CLI and Node from Homebrew, tools the user
installed in `~/.local/bin`. In those sessions the tools were not found. One
session prefixed `PATH=/opt/homebrew/bin:$PATH` to 32 separate commands;
another reported the missing tool in its result. The same work in a desktop
app session, started from a login shell, found them. Every new session has to
rediscover the gap.

A caller can already pass `PATH` in `OrcaCreateRequest.env`, but that
environment is held in memory only. A session resumed after a daemon restart
starts without it (ADR 0173's session environment). The caller also cannot
know the device's directories.

## Decision

Each time Muster starts a CLI child on Unix, it works out the child's PATH:

- The candidates are, in this order: the current nvm Node's `bin`, then
  `~/.local/bin`, `/opt/homebrew/bin` and `/usr/local/bin`. This is the order
  in which a login shell puts them in front of the system directories.
- The nvm entry is `~/.nvm/versions/node/<version>/bin`, where `<version>` is
  what `nvm current` prints. Muster asks only when `~/.nvm/nvm.sh` exists. It
  runs `/bin/bash -c '. "$NVM_DIR/nvm.sh" >/dev/null 2>&1; nvm current'` with
  `NVM_DIR=~/.nvm`, in the home directory, with stdin and stderr closed and a
  five-second limit. nvm is a shell function, so this is the one shell Muster
  runs for a child; the script is fixed and carries no caller input. Only
  `v` followed by dot-separated numbers is accepted. `none`, `system`,
  anything else, a timeout or a failure adds no nvm entry. The question is
  asked again at every start, so a later `nvm alias default` is picked up.
- Candidates that are directories and are missing from the daemon's PATH go
  in front of it. The daemon's PATH follows unchanged. A PATH someone set on
  purpose, for example by starting the daemon from a terminal, is only added
  to, never reordered. When nothing is missing, the child keeps the daemon's
  PATH.
- A `PATH` in the session's own environment is applied last and still
  replaces all of this.
- On Windows nothing changes.

Lookup of the CLI executable itself is unchanged. It shares the list of user
directories, without the nvm entry, and searches them after the daemon's
PATH, as before.

## Alternatives

- **Have the scheduler pass PATH in `env`.** This was rejected. The value is
  lost when a resumed session restarts its child after a daemon restart. Every
  caller would also have to know the device's layout.
- **Set PATH in the desktop app's launch of `musterd`.** This was rejected
  because it fixes one launcher. A daemon started any other way would still
  hand a bare PATH to its children, and nvm's current version changes
  without the daemon restarting.
- **Run the user's login shell and copy its environment.** This was rejected.
  It runs the user's whole shell profile, with its side effects, prompts and
  latency, at every child start. Only the PATH was missing.
- **Read `~/.nvm/alias/default` instead of running nvm.** This was rejected.
  The alias can be a partial version, `lts/*` or `node`, and resolving it is
  nvm's own logic. `nvm current` gives the answer nvm itself would give.
- **Append the user's directories after the system ones.** This was rejected.
  `/usr/bin` would then win over Homebrew's `git` and `python3`, unlike in the
  user's own shell and in desktop sessions.

## Consequences

- Sessions find Homebrew tools, the user's own tools and the current nvm
  Node, as they would in a terminal.
- With nvm installed, each child start costs one short shell run, bounded at
  five seconds. It happens while the session's start is already serialized
  with its provider handshake.
- A tool in one of these directories now wins over a system one of the same
  name, as it does in the user's shell. A session that needs a different
  PATH can still pass its own `PATH`.
- This takes effect only once `musterd` is released and installed on a
  device. Children already running keep the PATH they started with until
  they are started again.

## Verification

- Source and tests:
  - `cargo test -p muster-agent` passes.
  - The unit tests cover the candidate order, prepending only the missing
    directories while the inherited PATH keeps its order, and the version
    filter.
  - They also run `nvm_current` against a fake `~/.nvm/nvm.sh`: it gives
    `v20.11.1`, and `system` or a home without nvm give nothing.
  - `native_cli.rs` checks the PATH the fixture child reports. The daemon's
    PATH stays at the end, and every existing user directory is present. A
    session's own PATH is used exactly. These tests also pass with the test
    binary run under `PATH=/usr/bin:/bin:/usr/sbin:/sbin`, where the user
    directories had to be added.
  - `cargo clippy -p muster-agent --all-targets -- -D warnings` is clean.
- Not verified:
  - The real nvm. The test machine has no nvm, and the test uses a stand-in
    `nvm.sh`.
  - Windows, where the behaviour is unchanged by construction.
- Release and installation: pending.
- Installed-device acceptance: pending. On a device, it is shown by a
  scheduler-opened Claude session running a Homebrew tool without
  prefixing PATH.
