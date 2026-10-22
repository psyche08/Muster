# Multica daemon protocol capture — 2026-10-21

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Release gate: **not applicable.** This is a protocol observation record for
[ADR 0009](../adr/0009-task-contract-and-runtime-manifest.md). No Muster code was
written, built, signed or installed, and nothing ran against a real Multica
server. It establishes what the official daemon puts on the wire, nothing about
Muster's own behaviour.

## Environment

- Host: macOS 27.0.0, Apple `container` CLI 1.4.1.
- Container: `debian:bookworm-slim`, linux/arm64, 2 CPU / 2 GiB, name
  `multica-test`, address 192.168.64.4.
- Multica CLI `0.6.5` (commit `654b8ca95`, built 2026-10-21T09:36:09Z, go1.26.10,
  linux/arm64), installed from the published GitHub release archive
  `multica-cli-0.6.5-linux-arm64.tar.gz`. SHA-256 verified against the release
  `checksums.txt`: `2b0684a47c72761ac810d7f67b14dc32e73bb6aa2b50e548b55b13d6e29c4dba`.
- No Multica server. A local HTTP stand-in on the host answered permissively and
  recorded every request; it does not implement Multica's semantics and proves
  nothing about server behaviour.
- No real agent CLI. A stub named `claude` on `PATH` answered `--version` with
  `2.0.44 (Claude Code)` and logged its own arguments.

## Startup gates, in order

1. **Agent CLI detection, before authentication.** With an empty `PATH`, start
   fails with `no agent CLI found: install claude, codebuddy, codearts, codex,
   copilot, opencode, deveco, openclaw, hermes, pi, omp, cursor-agent, kimi,
   reasonix, dsh, kiro-cli, agy, qodercli, qoderclicn, traecli, grok, qwen,
   qwenpaw, mcode, muse, dim, or zeroclaw and ensure it is on PATH`. The gate is a
   name lookup only — the stub was never executed at this stage. `muse` and
   `zeroclaw` appear in the binary but not in `CLI_AND_DAEMON.md`'s table.
2. **Authentication.** With the stub `claude` present but no credential, start
   fails with `not authenticated: run 'multica login' first`.
3. **Version probe at registration.** Once authenticated, the daemon executes
   `claude --version` and sends the captured string as the runtime's `version`.

## Observed request sequence

Pre-authentication requests carry `X-Client-Capabilities: stable_attachment_urls`
only. Every authenticated request carries the full list:

```text
skill-bundles-v1, coalesced-comments-v1, execution-manifest-v1, agent-skill-v1,
remote-mcp-v1, local-worktree-v1, source_context_quick_create_v1, rpc-v1,
platform-skill-v1, checkout-keeps-work-v1, joined-wakeups-v1
```

plus `claim-poll-hints-v1` on the WebSocket request. `task_supplement_v1` is not
in this header; the reviewed server source takes it from the `/start` body.

| # | Call |
| --- | --- |
| 1 | `GET /api/me` |
| 2 | `GET /api/workspaces` |
| 3 | `POST /api/tokens/current/renew` (body `{}`) |
| 4 | `GET /api/daemon/workspaces` |
| 5 | `GET /api/daemon/workspaces/{workspace}/runtime-profiles` |
| 6 | `POST /api/daemon/register` |
| 7 | `POST /api/daemon/runtimes/{runtime}/recover-orphans` (body `{}`) |
| 8 | `POST /api/daemon/tasks/claim` |
| 9 | `GET /api/daemon/ws?runtime_ids={runtime}` |
| 10 | `POST /api/daemon/heartbeat`, then 8–10 repeating |

Register body as actually sent:

```json
{
  "workspace_id": "…",
  "daemon_id": "ca5d34e5-48e2-5d83-b2a4-943f5031b3f1",
  "device_name": "muster-container-test",
  "cli_version": "0.6.5",
  "launched_by": "",
  "legacy_daemon_ids": ["multica-test", "multica-test.local"],
  "runtimes": [
    {
      "name": "Claude (muster-container-test)",
      "type": "claude",
      "version": "2.0.44 (Claude Code)",
      "status": "online"
    }
  ],
  "failed_profiles": null
}
```

Claim body as actually sent:

```json
{
  "daemon_id": "ca5d34e5-48e2-5d83-b2a4-943f5031b3f1",
  "runtime_ids": ["…"],
  "max_tasks": 20
}
```

Heartbeat body: `{"runtime_id": "…", "supports_batch_import": true}`.

## What this changes for ADR 0009

- **Batch claim is the live path.** The official daemon claims with
  `POST /api/daemon/tasks/claim` across all of its runtime ids at once, not the
  per-runtime `POST /api/daemon/runtimes/{id}/tasks/claim` that `multica-bridge`
  uses. A Muster connector registering several devices should use the batch call;
  the per-runtime call remains available for a single runtime.
- **`workspace_id` is singular in the register body**, so a connector watching
  several workspaces registers once per workspace and keeps the returned runtime
  ids per workspace.
- **`daemon_id` is a generated UUID, not the hostname**, with the hostname forms
  passed in `legacy_daemon_ids` for row migration. A Muster connector needs its
  own stable identifier, generated once and persisted.
- **`runtimes[].name` is composed by the daemon** as `<Provider> (<device>)`, and
  `profile_id` is omitted for built-in runtimes.
- **Custom runtime profiles are fetched per workspace before registering**, which
  is where a `muster`-as-agent-command profile would arrive from.
- **The WebSocket is optional in practice.** The stand-in never upgraded the
  connection; the daemon retried, kept heartbeating over HTTP and kept claiming,
  and stayed up for the full 45-second run. This supports the ADR's position that
  no inbound channel is needed: pull plus an optional wake signal is the whole
  dispatch path.

## Second run: a stand-in Muster runtime behind the official daemon

The stand-in server then served one workspace custom runtime profile
(`command_name: muster-shim`) and one task, and `/usr/local/bin/muster-shim` was a
shell/Perl stand-in for a Muster-provided runtime command. Nothing of Muster ran.

**Registration.** The daemon fetched
`GET /api/daemon/workspaces/{ws}/runtime-profiles`, resolved the command on `PATH`,
probed `muster-shim --version`, and registered a second runtime beside the built-in
one:

```json
{
  "name": "Muster (muster-container-test)",
  "profile_id": "ced5c31a-584d-5a57-9f36-42d1f0030cf5",
  "type": "grok",
  "version": "1.0.20",
  "status": "online"
}
```

The register *response* must carry `provider` per runtime. With `type` instead, the
daemon tracked an empty provider and the run died with
`create agent backend: unknown agent type: ""`.

**Launch.** The daemon prepared
`~/multica_workspaces/{ws-slug}/task-{short-id}/workdir`, then launched the command
with the family's own prefix — `muster-shim --no-auto-update agent --always-approve
stdio` for grok, `muster-shim acp --trust-all-tools` for kiro, `muster-shim acp` for
dim — in that directory, with this environment:

```text
MULTICA_TOKEN (task-scoped), MULTICA_TASK_ID, MULTICA_AGENT_ID,
MULTICA_WORKSPACE_ID, MULTICA_SERVER_URL,
MULTICA_AGENT_NAME, MULTICA_TASK_CONFIG_ROOT, MULTICA_TASK_WORKSPACES_ROOT,
MULTICA_TASK_SLOT, MULTICA_DAEMON_PORT
```

**Full run, in order.** `tasks/claim` → `start` → `progress` → `session` →
`messages` (seq 1) → `supplements/claim` → the interjection delivered to the live
process → `supplements/{id}/ack` → `messages` (seq 2) → `progress` → `status` →
`complete`. The completion body was
`{"output": "READYSTEERED", "session_id": "muster-session-1", "work_dir": "…/workdir"}`,
so the supplement reached the running turn and changed its result.

**Mid-run replies are gated three ways.** The capability on `/start` is spelled
`task-supplement-v1`, and the daemon sends it only when
`SupportsTaskSupplement(provider, version)` holds **and** the task carries an issue
id — a chat task got `"capabilities": null`. The floors in source are codex
0.100.0, claude 2.1.110, grok 1.0.14, kiro 2.33.0, cursor 2026.10.14; everything
else is false. The shipped 0.6.5 binary contains `1.0.14` and `x.ai/interject` but
neither `2.33.0` nor `_session/steer`, so kiro steering is newer than this release.
A `dim`-family profile produced `"capabilities": null`, as expected.

**The family's quirks come with it.** As grok, the command had to advertise an
`authMethods` entry and answer `authenticate`; with `authMethods: []` the run failed
with `Grok advertised no usable authentication methods`. The interjection arrived as
`_x.ai/interject` with `{sessionId, text}`, where the text was a daemon-composed
`[ADDITIONAL GUIDANCE] …` block wrapping the human's words, and it had to be
answered with `{"result":{"status":"queued"}}`.

An earlier attempt reported `NO-STEER` in the output. That was the stand-in's own
fault — single-threaded, it did not read stdin during the turn — not the daemon's.
Serving stdin while the turn was open produced `READYSTEERED`.

## Third run: the real `muster acp` server, one local turn

`muster-acp` and `muster acp` (this branch) were driven by a local harness
speaking the scheduler's protocol, against the `musterd` already running on this
Mac. No scheduler took part.

- **Version probe.** `muster acp --version` prints `0.1.0`, Muster's own version.
  With the launch arguments a grok-family profile would append —
  `muster acp --agent claude --report-version 1.0.20 --no-auto-update agent
  --always-approve stdio --version` — it prints `1.0.20` and ignores the rest,
  which is what a command launched by someone else's prefix has to do.
- **Codex turn, settled.** `initialize` → `authenticate` → `session/new` created a
  real session (`orca:…:…`) through `orca.create` in a scratch directory →
  `session/prompt` ran a live Codex turn that executed
  `zsh -lc 'cat readme.txt'`, streamed as a `tool_call` update, then the file's
  line `MUSTER_ACP_LOCAL_ROUND` as `agent_message_chunk`, and answered the prompt
  request with `{"stopReason": "end_turn"}`. `session/close` returned cleanly.
- **Claude turn, failed honestly.** The same round with `--agent claude` reached
  the provider and came back `Failed to authenticate: OAuth session expired and
  could not be refreshed`. The server reported
  `turn ended with: the provider reported a failed turn` instead of a success.
  That is the error path working, not a passing round; the Claude CLI on this Mac
  needs an interactive re-login, which was not done.
- **Mid-turn steering is not available.** `_x.ai/interject` during a live turn was
  refused by the daemon: `conflict: Wait for the current turn, answer its prompt,
  or interrupt it`. `NativeRuntime` holds one turn per session, so a reply cannot
  be landed inside a turn through `agent.send`. This contradicts an earlier
  revision of ADR 0009, which is corrected in its §4a: the server now reports the
  refusal, reports Muster's own version by default so no scheduler believes the
  capability is present, and the follow-up is a steer operation in Shared Rust.

## Fourth run: the internal build, and the container-to-host bridge

The internal installer was reviewed before running and then run inside a second
container (`debian:bookworm-slim`, arm64, `curl` and `socat` added from apt).

- **It pins CLI 0.3.54** (commit `ad8f59665`, built 2026-10-20), installed to
  `/usr/local/bin/multica` from
  `deployment-artifacts.example/releases/download/v0.3.54/multica-cli-0.3.54-linux-arm64.tar.gz`.
  Default mode installs the CLI only; `--with-server` clones the repo to
  `~/.multica/server` and needs Docker and `docker compose`, which this Mac does
  not have (only Apple `container`).
- **0.3.54 is missing what the design assumed.** `multica runtime` offers only
  `activity | list | update | usage` — no `profile` subcommand — and the binary
  contains no `runtime-profiles`, no `task-supplement`, no `supplements/claim`, no
  `x.ai/interject`, no `_session/steer`, and no batch `api/daemon/tasks/claim`.
  `api/daemon/register` and `api/daemon/ws` are present. So: no custom runtime
  profiles, no mid-run steering at all, and per-runtime claiming — the older path
  `multica-bridge` uses.
- **Its startup gates run in the opposite order** to 0.6.5: with no credential it
  stops at `not authenticated: run 'multica login' first` before complaining about
  agent CLIs. Its detection list is `claude, mc, codex, copilot, opencode,
  openclaw, hermes, gemini, pi, cursor-agent, kimi, dsh, kiro-cli, agy` (or
  CatDesk) — `grok` appears in the binary but not in that list.
- **The container-to-host bridge works.** With a host listener on
  `192.168.64.1:8799` (token-gated, spawning `muster acp --agent codex`) and
  `/usr/local/bin/kiro-cli` in the container handing its stdio to it:
  - the version probe answered `0.1.0` from the container;
  - `initialize` and `session/new` from inside the container opened a **real
    session on the host** (`orca:…:example-session-a`) through `musterd`;
  - a second exchange resumed it with `session/load` and ran a turn: the host
    executed `zsh -lc 'cat readme.txt'`, streamed it as a `tool_call` update, then
    `MUSTER_ACP_LOCAL_ROUND` as `agent_message_chunk`, and settled
    `{"stopReason": "end_turn"}`.
  - The host log records the token handshake and the `muster acp` pid per
    connection. Scripts and the topology are in `src/muster/deploy/multica/`.

## Fifth run: the internal server, a registered runtime and a claimed task

Install steps came from a deployment-specific guide (private reference removed):
`install.sh`, then `multica config set server_url https://scheduler.example.invalid` and
`app_url https://scheduler-ui.example.invalid` (a comment on that page, endorsed by its
owner, adds the `app_url` line), then `multica login --token`, then
`multica daemon restart`. The token was a `mul_` PAT the user had saved; it was
copied into the container and read there, never printed.

- **Logged in** against `https://scheduler.example.invalid` as the test operator
  (`operator@example.invalid`), workspace `b80fb2ce-8141-5e1f-a041-e94e610ebb3e`.
- **The runtime registered.** The daemon reported `Agents: example-provider-a, kiro,
  example-provider-b, example-provider-c` — `kiro` being the bridge command on `PATH` as `kiro-cli` —
  and `multica runtime list` showed `Kiro (muster-host-bridge)`,
  `b9c22372-37d8-5764-98a5-d4340c698908`, provider `kiro`, online. It also
  registered `example-provider-c` and `example-provider-b` runtimes for this container, which nothing
  here provides; they were not investigated.
- **A task was claimed and executed across the boundary.** Agent
  `muster-bridge-test` (`1ca63242-62bd-5ec4-bbed-e02abeca7ec5`) bound to that
  runtime, issue **EXAMPLE-1** (`35e1eb1a-84f7-52fa-bbc7-fe2f80825025`) assigned to
  it. The daemon log shows `agent command exec=kiro-cli args="[acp
  --trust-all-tools]"`, `cwd=/Users/developer/multica_workspaces/example-workspace/example-run-a/workdir`
  — the shared path — and `kiro session created session_id=orca:…`, i.e. the
  host's `musterd` opened a real session for a container-side claim. The host
  bridge log records the connection and the `muster acp` pid. Messages streamed
  back (`reported task messages count=1`) and both runs finished `completed`:
  `example-run-a` and, after the env forwarding was added, `example-run-b`.
- **The agent could not act on its own issue.** Both runs ended with
  `Blocked: multica CLI is unavailable (command not found)`. The forwarding does
  deliver all nine `MULTICA_*` variables to `muster acp` (the bridge log lists
  them), but `musterd` spawns the provider child, so the child inherits the
  daemon's environment instead. `OrcaCreateRequest` has no environment field.
  Recorded as ADR 0009 §7a; the fix is per-session environment in Shared Rust.

Left behind in the workspace, for deletion: agent `muster-bridge-test` and issue
EXAMPLE-1. The `multica` darwin binary fetched for the host lives in the session's
scratch directory, not on `PATH`.

## Sixth run: the session environment reaches a real provider child

`OrcaCreateRequest` gained `env`, threaded into `NativeRuntime`'s spawn
(ADR 0009 §7b). Checked against a second `musterd` built from this branch, with
its own state directory and socket under `/private/tmp/macp-dev` — the installed
daemon predates the field and was left running untouched. A short path was needed
because `MUSTER_DIR` derives socket paths and the session scratch directory
exceeds `SUN_LEN`.

- `muster acp --socket … --forward-env MULTICA_ --forward-path`, started with
  `MULTICA_TASK_ID=WS-ENV-PROOF` in its own environment.
- A real Codex turn ran `printf '%s' "$MULTICA_TASK_ID"` and the assistant text
  was exactly `WS-ENV-PROOF`, settling `{"stopReason": "end_turn"}`. The value
  crossed from the shim's environment, through `orca.create`, into the child
  `musterd` spawned.
- Unit and integration tests cover the rest: linker-name refusal, malformed names
  and values, the entry and value bounds, a stable order, the environment arriving
  at the fixture child, and nothing containing the value appearing anywhere in the
  journal directory. `cargo clippy` is clean across the four touched crates.

## Seventh run: the agent acted on its own issue, end to end

The same internal server and container daemon, now against a `musterd` built
from this branch (`MUSTER_DIR=/private/tmp/macp-dev`, software key, its own
socket; the installed daemon was left running and untouched). The host bridge
was started with `MUSTER_DIR` pointing at it, so the `muster acp` it spawns
resolves that socket, and with the host CLI's directory as its extra-path
argument; it spawns
`muster acp --agent codex --forward-env MULTICA_ --forward-path`. The container
CLI had self-updated 0.3.54 → 0.3.55 between runs.

- The bridge log lists all nine `MULTICA_*` variables reaching `muster acp`.
- The daemon log shows `exec=kiro-cli args="[acp --trust-all-tools]"`,
  `cwd=…/example-workspace/example-run-c/workdir`, and
  `kiro session created session_id=orca:example-session-b` — a new id prefix, i.e. the
  branch daemon rather than the installed one.
- Run `example-run-c` **completed**, and took about a minute against the 15–30
  seconds of the two runs that were blocked immediately.
- **The agent used the scheduler's CLI itself.** Its comments on EXAMPLE-1 are
  `我会读取任务要求，并通过 Multica 提交一行测试回复。` and then `READY` — the
  single word the issue asked for, which it could only have learned by reading
  the issue. The two earlier comments on the same issue still read
  `Blocked: multica CLI is unavailable`, so the before and after sit side by
  side in one thread.

This closes the chain the record set out to test: a task claimed in a container,
executed on the host through Muster, with the agent reading its own issue and
posting its own result.

## Not established

- No registration against a real Multica server, and none against the team's
  internal deployment or its auth.
- Nothing about whether a workspace custom runtime profile is accepted by the
  internal deployment's policy, or which family its CLI version supports.
- No real agent CLI ran, no model was called, and no repository was checked out —
  the task carried `repos: []`.
- The two container runs used a stand-in command that returns canned frames; they
  prove Multica's side of the contract, not Muster's. The third run used the real
  server but no scheduler: no registration, claim, or task report took part, so
  the two halves have not met.
- No signing, notarization, packaging or installation of `muster acp`; the binary
  was a debug build run from the worktree.
- A turn that does real work: the test issue asked for one word. No repository was
  attached, nothing was edited, and no pull request was produced.
- Anything run by the installed, signed `musterd`. The run that worked used a
  debug build with a software key, started by hand from the worktree, so this is
  not an installation, signing or release claim.
- Resumption across a daemon restart: a session's environment is held in memory
  by design, and that path was reasoned about but not exercised here.
- Steering on the internal build: absent from 0.3.54's protocol, so untested there.
- The bridge was driven by hand-written ACP frames, not by the daemon. The
  shared-path requirement for the task directory is reasoned from the daemon's
  behaviour observed on 0.6.5; it has not been exercised with a mounted volume.
- Nothing about cross-device routing: the daemon prepared the working directory on
  its own host and launched the command there.
