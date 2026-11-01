# Multica runtime bridge: current managed setup

This directory contains the pieces a local Muster desktop uses to run the
Multica scheduler daemon in an Apple container while provider work stays on the
Mac. The supported path is the **managed bridge**: Muster installs and
supervises the container, starts the daemon with the provider wrappers this
directory ships, and forwards only an authenticated task handshake to
`musterd`. The historical ACP stdio shim is kept only as a record (see
"Legacy path" at the end) and is not the current setup.

Decision records: [ADR 0009](../../../../docs/adr/0009-task-contract-and-runtime-manifest.md)
is the historical shim proposal; [ADR 0051](../../../../docs/adr/0051-multica-management-and-live-catalog.md),
[0053](../../../../docs/adr/0053-multica-native-workbench-and-runtime-routing.md),
[0054](../../../../docs/adr/0054-native-multica-targeted-installation.md),
[0060](../../../../docs/adr/0060-multica-runtime-recovery.md),
[0064](../../../../docs/adr/0064-multica-native-runtime.md) and
[0077](../../../../docs/adr/0077-multica-runtime-state-and-manual-upgrade.md)
describe the current state, recovery rules and acceptance limits.

## Geometry

```text
Apple container (guest)                         host (macOS)
┌──────────────────────────────────────┐        ┌──────────────────────────────┐
│ multica daemon                       │        │ musterd::multica::bridge      │
│   └─ spawns provider wrapper ────────┼─ TCP ─▶│   token + MUSTER_NATIVE_V1    │
│      (/usr/local/bin/codex,          │ private│   handshake                   │
│       /usr/local/bin/muster-native-  │ gateway│   └─ native app or CLI        │
│       claude)                        │  port  │        └─ task-scoped creds   │
│                                      │        │ musterd local UDS (never      │
│ data root: /muster, workspaces also  │        │ bridged)                      │
│ mounted at the same host path        │        └──────────────────────────────┘
└──────────────────────────────────────┘
```

- The daemon runs inside the container, so scheduler state and its login stay
  there. Provider processes run on the Mac, where the desktop applications and
  CLIs are.
- Only the scheduler runtime stream crosses. `musterd`'s own Unix socket never
  leaves the host: it authenticates the local principal by peer uid, so
  bridging it would hand every reachable container the local owner's
  authority.
- The host bridge binds the container gateway address (a private address);
  binding `0.0.0.0` is rejected. The guest connects to that address and port.

## What Muster installs

Muster's managed install (`musterd::multica`) creates the container from the
Apple `container` runtime and provisions both sides:

- Data root (default `~/Documents/Multica`): `config/` is mounted at
  `/root/.multica`, `workspaces/` is mounted at the **same absolute path** on
  host and guest, and `bin/` holds the host-side Multica CLI. The root carries
  a `muster-owned` marker and is created mode `0700`; files Muster writes there
  are mode `0600`.
- Guest: a Multica CLI installation from the operator-supplied trusted HTTPS
  installer, plus `curl`, `ca-certificates`, `socat`, `git` and
  `openssh-client`.
- Host: the same installer (or an already installed `multica` CLI) copied into
  `bin/multica`. Settings shows the terminal login command; login is never done
  by the app.
- Wrappers: `runtime-command` and `runtime-codex` / `runtime-claude` are written
  under the data root, then installed in the container as
  `/usr/local/bin/codex` and `/usr/local/bin/muster-native-claude`. When the
  runtime bridge is managed or adopted, the daemon is started with
  `MULTICA_CODEX_PATH` and `MULTICA_NATIVE_CLAUDE_PATH` pointing at them. Each
  task's wrapper hands its own environment (`MULTICA_*` names only), provider,
  working directory and arguments to the host bridge.
- Credentials: Muster generates a random bridge token per listener, writes it
  mode `0600` inside the data root, and the wrapper reads that file at launch.
  A connection without a task identity is rejected before any provider process
  starts, so anonymous connections cannot share a task's session key. Task
  credentials are created per task by `muster-acp` and revoked when the turn
  ends; a daemon restart revokes launchers left behind by a previous run.

An existing container is adopted without changing it: Muster reads its id,
configured mounts and `app_url` / `server_url`, and keeps its tools and login.
Adoption only adds supervision and, if the host CLI is missing, the host CLI.

## keep-alive, version and status

- Keep-alive opts the container and daemon into `musterd`'s supervisor. The
  daemon then starts the container runtime and container when stopped,
  reconciles the bridge listener, token and provider wrappers, and starts the
  Multica daemon. Closing the app does not stop any of it.
- With keep-alive off, Muster does not start stopped services; it still
  reconciles the runtime-bridge listener and wrappers without starting them, so
  a running container's next task uses the current bridge instead of a stale
  one.
- The Multica tab and Settings show a read-only guest `multica --version`
  report. It is diagnostic; it does not install or upgrade anything.
- A stopped container, an installed-but-stopped daemon and missing login are
  reported as distinct states. Login itself remains a terminal operation.

## Manual upgrade (no in-app upgrade)

This Muster release has **no in-app Multica upgrade or stop action**. Upgrading
the guest CLI, the host CLI or the container base is an operator operation:

1. Note the current version and finish or cancel scheduler work; do not upgrade
   while a task is running.
2. Turn keep-alive off and stop the container with Apple
   `container stop <name>`. Do not delete the data root.
3. Run the trusted installer for the desired Multica CLI version inside the
   container and on the host (the host copy goes to the data root's `bin/`,
   with `MULTICA_BIN_DIR` pointing there), or install the CLI by your normal
   package path. Keep the existing mounts and login configuration.
4. Start the container and the daemon (re-enable keep-alive to let Muster
   supervise them), then verify the reported version, that the workspace and
   task lists load, and that a task can run.
5. If a bridge wrapper is missing or stale after an upgrade, save the Multica
   settings once (or re-enable keep-alive) so Muster reconciles them.

Do not expect Muster to pull a new container image, upgrade a CLI, or roll an
upgrade back. There is no in-app rollback either; keep your own backup of the
data root before changing it.

## Migration of the data root

Changing the data root is attended and offline, never automatic:

1. Disable keep-alive and stop the container; finish running tasks.
2. Move the data on the same filesystem and update **every** bind mount
   (including the guest workspace path and the daemon workspace configuration).
3. Remove the old path. Muster never creates or keeps a compatibility symlink.
4. Save the new root in Settings. Muster accepts it only when the old path is
   absent, the new root is a real directory (not a symlink), and the stopped
   container inventory confirms the required mounts. It never moves data and
   never rewrites Apple's private container metadata.

Cross-volume moves and version-specific container metadata changes remain
manual. The repository does not record operator hosts, ports, tokens or roots;
obtain any installer URL from your deployment operator and verify it before
running it.

## Legacy path (historical, not supported)

The first Multica integration (ADR 0009) selected a "kiro-cli"-style ACP
command in the container and forwarded ACP stdio to a Python host process,
`acp-bridge.py`, which spawned `muster acp` on the Mac:

- `acp-bridge.py` listens on the container bridge, requires the shared token as
  the first line, then spawns `muster acp` with
  `--forward-env MULTICA_ --forward-path`; the file remains in this directory
  as a record.
- `runtime-command` still exists but is installed by the managed bridge path;
  the old manual `install -m 0755 runtime-command /usr/local/bin/kiro-cli`
  instruction is historical.
- The kiro-cli compatibility alias was removed by the current install: a stale
  `/usr/local/bin/kiro-cli` written by the old bridge is deleted, because it
  misrepresented a Codex CLI bridge as a Kiro runtime.

Do not use `acp-bridge.py`, `muster acp` or the kiro-cli alias as a current
setup path. They are retained for reading the history in ADR 0009 and the
validation record
[MULTICA-DAEMON-2026-10-21](../../../../docs/validation/MULTICA-DAEMON-2026-10-21.md);
they are not a supported configuration and should not be wired into a new
container.

## Tests and verification

- Rust: `musterd::multica::installation` (bounded/redacted output, timeout,
  exclusivity), `musterd::multica` (config validation, migration acceptance,
  supervision with a fake `container` executable), `musterd::multica::bridge`
  (handshake, session key, wrapper installation) and the app-core install
  progress/reconnect tests under `muster-app-core/tests/multica_install.rs`.
- Python: `tests/test_runtime_command.py` drives the wrapper with a fake
  `socat` to prove an open stdin cannot keep a failed transport alive.
- These tests use fake executables and temporary directories. A real container,
  real installer, terminal login, reboot/app-upgrade recovery and a real
  scheduler task remain device/e2e acceptance and are not claimed here.
