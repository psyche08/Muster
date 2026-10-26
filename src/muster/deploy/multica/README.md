# Multica runtime bridge

Run the scheduler's daemon in a container while Muster stays on the host, and
forward only the Agent Client Protocol stream between them (ADR 0009).

```text
container                                   host
┌──────────────────────────────┐            ┌────────────────────────────────┐
│ multica daemon               │            │ acp-bridge.py                  │
│   └─ spawns runtime-command ─┼── TCP ────▶│   └─ spawns `muster acp`       │
│      (named as a detected    │  :8799     │        └─ UDS ─▶ musterd       │
│       agent CLI, see below)  │            │              └─ codex / claude │
└──────────────────────────────┘            └────────────────────────────────┘
```

Only the ACP stream crosses. `musterd`'s own socket never leaves the host,
which matters because the daemon authenticates the local principal by the
socket's peer uid — bridging that socket to TCP would hand every reachable
process the local owner's authority.

## Host

```sh
TOKEN=$(openssl rand -hex 16)
python3 deploy/multica/acp-bridge.py 192.168.64.1 8799 "$TOKEN" "$(command -v muster)" codex "$HOME/.local/bin"
```

The container side forwards the task's own `MULTICA_*` variables (including the
task-scoped `MULTICA_TOKEN`) after the token line; the bridge accepts only
names starting with `MULTICA_` and puts them on the `muster acp` process, and
prepends the last argument to its `PATH`. The task token therefore crosses the
hop — another reason the bridge is token-gated, bound to the container
interface, and run only while needed.

For those variables to reach the agent, `muster acp` has to be told which ones
may cross into a session, because `musterd` spawns the provider child and the
session carries the environment for it (ADR 0009 §7b). The bridge passes
`--forward-env MULTICA_ --forward-path` for exactly that; both are off in
`muster acp` by default, so which variables cross is the operator's choice
rather than a scheduler's name baked into Muster. Edit `FORWARD_ARGS` in
`acp-bridge.py` to change it.

The scheduler's CLI also has to be installed on the host, since that is where
the turn runs. Obtain `MULTICA_INSTALLER_URL` from your deployment operator
and verify it before running the installer:

```sh
curl -fsSL "${MULTICA_INSTALLER_URL:?set the trusted installer URL}" | MULTICA_BIN_DIR="$HOME/.local/bin" bash
```

A session's environment lives in memory for as long as the daemon runs and is
never journaled, so a session resumed after a daemon restart starts without it
and the scheduler's next task supplies it again.

Bind to the container bridge address, not `0.0.0.0`. The first line a client
sends must be `$TOKEN`, so reaching the port is not by itself enough to drive
Muster. That is a guard, not isolation: every container on that bridge can
reach the port, so run it only while it is needed, and treat the token as a
credential.

## Container

Install the CLI, `socat`, and the bridge command:

```sh
curl -fsSL "${MULTICA_INSTALLER_URL:?set the trusted installer URL}" | bash
```

```sh
apt-get install -y socat
```

Then place `runtime-command` on `PATH` under the name of an agent CLI the
daemon detects, because the internal build (0.3.54) has no custom runtime
profiles — detection is by command name only:

```sh
install -m 0755 deploy/multica/runtime-command /usr/local/bin/kiro-cli
```

`kiro-cli`, `hermes`, `kimi` and `dsh` are the ACP families in that build's
detection list, so they match what `muster acp` speaks. This substitution is
deliberate and worth stating plainly: the daemon will drive this command with
that family's launch arguments and quirks, so pick the family whose dialect
Muster actually implements and keep the reported version honest.

Configuration, all through the environment:

| Variable | Meaning |
| --- | --- |
| `MUSTER_BRIDGE_TOKEN` | required; the host bridge's token |
| `MUSTER_BRIDGE_HOST` | host address on the container bridge (default `192.168.64.1`) |
| `MUSTER_BRIDGE_PORT` | default `8799` |
| `MUSTER_BRIDGE_VERSION` | what the daemon's probe reads (default `0.1.0`) |

## The working directory has to exist on both sides

The daemon prepares a task directory **in the container** and passes it as the
ACP session's `cwd`; `musterd` then opens the session at that path **on the
host**. A path that exists only in the container fails there. Mount one
directory at the same path on both sides and point the daemon at it:

```sh
container run --volume "$HOME/Documents/Multica/workspaces:$HOME/Documents/Multica/workspaces" …
```

and set `MULTICA_WORKSPACES_ROOT` to that path. Nothing here works around a
mismatch: the session simply fails to open, which is the honest outcome.

## What is verified

A container-side command reached the host's `musterd` through this bridge,
opened a real session, and ran a Codex turn that executed a tool call and
settled `end_turn`. No scheduler drove it — the frames were sent by hand. See
`docs/validation/MULTICA-DAEMON-2026-10-21.md`.

## Managed desktop setup

Muster Settings now provides the local Multica installation entry (ADR 0051).
Install Apple `container` first, then supply the trusted operator installer URL
and server/application HTTPS URLs. New installations use `~/Documents/Multica`:
`config/` persists the guest CLI configuration, `workspaces/` is mounted at the
same absolute path on host and guest, and `bin/` holds the host CLI needed by
provider tasks. The installer runs inside the container and on the host with
`MULTICA_BIN_DIR` set to that private `bin/` directory. Complete Multica login
using the terminal command shown in Settings. Enabling keep-alive opts the
container and daemon into musterd's supervisor; closing the UI does not stop it.

An existing `mlt` container is discovered without changing it. Save its settings
to opt into supervision; its tools and credentials are preserved. The installed
container makes the Multica tab visible after Agents and Terminals. The native
page lists workspaces, issues, agents and runtime status using the logged-in
container CLI. Workspace selection affects only that query, not the daemon
profile. Issue and agent details are read-only; no browser login or remote proxy
is involved. The configured application URL remains part of container setup.
See ADR 0053 for the separate, still-unimplemented desktop execution mapping.

To move existing data, disable keep-alive and save first, finish running tasks,
then stop the selected container using Apple `container stop`. Choose a new,
nonexistent directory on the same filesystem and save again. Rust moves the data
and keeps an old-path symlink so existing task and mount paths continue to work.
Re-enable keep-alive after saving. The destination parent must exist; a different
filesystem requires a separate manual migration. Do not remove the compatibility
symlink while the container still uses that mount path.
