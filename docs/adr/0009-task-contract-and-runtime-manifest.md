# ADR 0009: Provide Muster as a Multica runtime behind a shim command

- Date: 2026-10-21
- Status: Proposed
- Related: [ARCHITECTURE](../ARCHITECTURE.md) §3.4 scopes, §3.6.1 `muster-rpc`,
  §3.6.2 agent service; [ADR 0003](0003-confirmed-conversation-workbench.md)
  confirmed terminal execution; [ADR 0005](0005-isolate-desktop-rpc-requests.md)
  desktop RPC isolation
- Sources reviewed: `multica-ai/multica` (`CLI_AND_DAEMON.md`,
  `apps/docs/content/docs/daemon-runtimes.mdx`,
  `server/internal/daemon/`, `server/internal/handler/daemon.go`,
  `server/pkg/agent/`) and `nibfe/multica-bridge`
  (`server/modules/bridge/`, `docs/reliability-and-capability-implementation.md`),
  read on 2026-10-21
- Wire evidence: [MULTICA-DAEMON-2026-10-21](../validation/MULTICA-DAEMON-2026-10-21.md),
  the official CLI 0.6.5 daemon driven in a container against a recording
  stand-in, including a complete run through a stand-in Muster runtime

## Context

Muster's execution core already exists. `muster_agent::orca::runtime::NativeRuntime`
owns provider child processes, a revisioned SQLite journal and generation fencing;
`OperationLedger` makes every write idempotent under a caller-supplied `op_id`;
`muster-core::scope` and `policy.rs` decide what a roster member may ask for. What
is missing is not an execution engine but a way for team work to arrive.

Multica is where the team's work lives, and it already has the concept this needs:
a **runtime** is one computer plus one coding tool, registered by a daemon that
claims queued runs and reports results. Three facts from the reviewed sources and
the capture decide the shape:

1. **Multica's daemon is a complete, tested task plane.** It claims in batches,
   prepares a per-task working directory, enforces concurrency, reclaims orphans,
   retries, garbage-collects, streams messages under a sequence cursor, and reports
   terminal outcomes. None of that is worth reimplementing.
2. **A custom runtime profile makes the command Multica spawns somebody else's.**
   The daemon fetches the workspace's profiles, resolves the profile's
   `command_name` on that host, probes its version, and registers it as a runtime
   alongside the built-in ones. It then launches that command for the task. This
   is a supported extension point, and the capture confirms the whole path works
   with a command that is not a vendor CLI.
3. **Per-person networks are not an obstacle.** Each person's Muster roster stays
   their own; Multica is the shared plane, and it already owns the sharing
   decision — a runtime is private by default, only its owner can make it public,
   and revoking that unbinds other members' agents with `runtime_access_revoked`.

Earlier drafts of this record first invented a task envelope with a relay-mailbox
return path, then proposed a Muster-side connector speaking the daemon protocol
directly. Neither landed on `main`. Both put Muster where Multica's daemon already
is; this record keeps the daemon and puts Muster behind it.

## Decision

**1. Multica's own daemon runs on each device; Muster is the runtime behind it.**
Muster ships a command that the daemon spawns as the agent for a task. The command
is Muster's: it hands the turn to `NativeRuntime`, which owns the provider child,
the journal, approvals and the ledger, and streams the turn back to the daemon.
The daemon keeps claiming, concurrency, retries, cleanup and reporting.

**2a. Correction, 2026-10-21: the team's internal build has no custom runtime
profiles, so on it the command registers by name.** The internal installer
(`deployment-artifacts.example/install.sh`) pins CLI **0.3.54**, five minor versions behind the public
0.6.5 this record was written against. That build has no `multica runtime profile`
command, and its binary contains no `runtime-profiles` route, no `task-supplement`,
no `supplements/claim`, no `x.ai/interject` and no batch `api/daemon/tasks/claim` —
it claims per runtime, which is why `multica-bridge` uses that older call. So on the
deployment the team actually runs:

- Custom profiles are unavailable. The only way in is the daemon's name-based
  detection: Muster's command goes on `PATH` under the name of a CLI that build
  detects. Its ACP families are `kiro-cli`, `hermes`, `kimi` and `dsh`, which match
  what `muster acp` speaks.
- Mid-run steering is absent from the protocol entirely, not merely gated. §4a's
  conclusion holds for a different reason, and nothing about it is recoverable by
  configuration on this build.
- Naming Muster's command after another CLI is a deliberate substitution inside a
  container whose `PATH` is ours. It is not a fact about Muster, and the reported
  version must stay what Muster implements.

**2b. On a build that has them, the command registers through a workspace custom
runtime profile**, with
`command_name` pointing at the Muster command and a `protocol_family` Multica
already supports. A profile cannot add a protocol family, so the command must be
wire-compatible with an existing one. The daemon resolves it per host, so only
devices with Muster installed register the runtime; the per-machine override
(`multica runtime profile set-path`) pins it when it is not on `PATH`.

The server is `muster-acp`, served by `muster acp`: newline-delimited JSON-RPC 2.0
on stdio, with `initialize`, `authenticate`, `session/new`, `session/load`,
`session/prompt`, `session/cancel`, `session/close`, the two accepted-and-ignored
configuration calls, and the steering extension in 4a. A turn runs as its own task
while stdin keeps being served, because the scheduler delivers steering as a
request on the same pipe; a server that only reads between turns cannot receive
one. The command also accepts and ignores the launch arguments the scheduler
appends for its protocol family, since those are the scheduler's, not Muster's.

**3. The family is chosen for mid-run replies, not convenience.** Multica delivers
a reply into a running turn only when the daemon advertises `task-supplement-v1` on
`/start`, and it advertises that only when
`SupportsTaskSupplement(provider, version)` holds **and the task carries an issue
id**. The floors are codex ≥ 0.100.0, claude ≥ 2.1.110, grok ≥ 1.0.14,
kiro ≥ 2.33.0, cursor ≥ 2026.10.14; every other family returns false. Of these,
grok and kiro are ACP, so Muster's command targets one of them and implements its
steering method: grok's `_x.ai/interject` (which must answer
`{"result":{"status":"queued"}}`) or kiro's `_session/steer`. CLI 0.6.5 ships grok
only — its binary contains neither `2.33.0` nor `_session/steer` — so grok is the
v1 target and kiro becomes available as deployments move past 0.6.5.

**4a. Correction, 2026-10-21: Muster cannot honour `task-supplement-v1` yet, so it
does not claim it.** An earlier revision of this record said Muster would declare
the capability from the first version because it has a live session and a journal.
Implementing the server disproved that. `NativeRuntime` holds one turn per session:
a second write while a turn is live is refused with `conflict: Wait for the current
turn, answer its prompt, or interrupt it`. Landing a reply *inside* the turn needs
the provider's own steering — Codex's `turn/steer`, Claude's pre-tool context — and
no Muster RPC exposes it. So:

- `muster acp` reports Muster's own version by default, which sits below every
  steering floor, and the scheduler therefore holds a reply for the next turn
  instead of believing it was delivered.
- `--report-version` exists for the operator to raise that deliberately, and must
  not be used until the steering operation exists. Reporting a version whose
  behaviour this build does not implement would have the scheduler dispatch work
  the runtime then fails to honour.
- The follow-up is a steer operation in Shared Rust (a new RPC mapped onto the
  provider's own steering), which per the cross-platform rules is where it belongs.
  Until then the `_x.ai/interject` handler reports the refusal rather than queueing
  the text for a later turn.

**4b. Impersonating a family means inheriting its quirks, and that is a cost, not a
detail.** Registering as grok means the daemon runs grok's handshake: the command
must advertise an `authMethods` entry (`cached_token` or `xai.api_key`) and answer
`authenticate`, because an empty list is a hard failure. Every future quirk in that
backend applies to Muster's command. This argues for revisiting the direct-protocol
connector (below) once cross-device routing is actually wanted, rather than
accumulating vendor-shaped workarounds.

**5. `muster-task` maps a Multica task onto `NativeRuntime`.** Shared Rust. `op_id`
derives from the Multica task id and the `dispatched_at` claim timestamp, which the
server already requires as a generation fence — the same discipline as the
runtime's own per-child generation, so a replayed launch cannot execute twice. A
turn settles only on a provider-confirmed terminal event.

**5a. The daemon may run in a container while Muster stays on the host, with only
the ACP stream forwarded.** The scheduler's daemon installs into a container by
default; Muster, `musterd` and the signed-in provider CLIs are on the host. The
daemon therefore spawns a container-side command that hands its stdio to a host
listener, which spawns `muster acp` (`src/muster/deploy/multica/`). What crosses is
the ACP stream and nothing else: `musterd`'s socket stays on the host, because the
daemon authenticates the local principal by the socket's peer uid and bridging that
socket to TCP would hand every process that can reach the port the local owner's
authority. The listener binds the container bridge address and requires a token
line before the stream — a guard, not isolation, since every container on that
bridge can reach it.

The task directory is the one thing this does not paper over: the daemon prepares
it in the container and passes it as the session's `cwd`, which `musterd` then opens
on the host. One directory must be mounted at the same path on both sides. A
mismatch fails to open the session, which is the honest outcome.

**6. The daemon owns the working directory, so v1 executes on the device that
claimed.** The task's checkout is prepared under the daemon's workspaces root and
the command starts there. Routing a turn to another device in the roster would
require that directory on the target, which vnet makes possible and this record
does not attempt. The gain over the built-in runtimes stands without it: the turn
runs under Muster's journal, approvals and phone visibility.

**7. The five contract environment variables pass through to the provider child and
are never recorded.** `MULTICA_TOKEN`, `MULTICA_TASK_ID`, `MULTICA_AGENT_ID`,
`MULTICA_WORKSPACE_ID` and `MULTICA_SERVER_URL` are what let the agent write back to
the issue as itself. The token is task-scoped and inherited by every grandchild by
default, so it must never reach a journal row, a diagnostic, a prompt or a log.

**7a. Correction, 2026-10-21: there was nowhere to put them, and two real runs
proved it.** `musterd`, not `muster acp`, spawns the provider child, so the child
inherited the daemon's environment; `OrcaCreateRequest` carried `op_id`,
`directory` and `agent` and no environment. Forwarding the variables across the
bridge to `muster acp` therefore did not reach the agent, and neither did
installing the scheduler's CLI on the host. On the internal server both runs of the
test issue ended with the agent reporting `Blocked: multica CLI is unavailable
(command not found)` — once before the forwarding existed and once after.

**7b. Resolved: `OrcaCreateRequest` carries a session environment.** A new
`map<string, string> env` field threads from the request through
`orca::Rpc::create` into `NativeRuntime`, which applies it when it spawns the
session's provider child. The design points that matter:

- **Session-scoped, held in memory only.** It is deliberately not part of
  `Session`, which is serialized into the journal: these values carry a caller's
  credential. A session resumed after a daemon restart therefore starts without it
  and the caller supplies it again, rather than launching a provider with a
  credential that expired while the daemon was down. A test asserts nothing
  containing the value reaches the journal directory.
- **A replayed create re-arms it.** The ledger has already refused a replay with a
  different body, so a replay carrying the same environment restores what only
  memory held.
- **Validated in Shared Rust, refusals by name.** Names must match
  `[A-Za-z_][A-Za-z0-9_]*`; at most 32 entries, 4 KiB per value, no NUL. `DYLD_*`
  and `LD_*` are refused outright: they change what *every* binary the child
  starts will load, which is a larger power than running a command in the session
  the caller already controls. `PATH` is allowed, because a caller's own command
  has to be findable.
- **The shim forwards by prefix, not by hardcoded knowledge.** `muster acp` takes
  `--forward-env <PREFIX>` (repeatable) and `--forward-path`, both off by default,
  so which variables cross is the operator's explicit choice rather than a
  scheduler's name baked into Muster.

Verified against a real provider: a Codex turn in an isolated daemon from this
build ran `printf '%s' "$MULTICA_TASK_ID"` and returned the value supplied to
`muster acp`, settling `end_turn`. Then end to end on the internal server — the
same test issue, reran against this build — the agent read its own issue through
the scheduler's CLI and posted the word it asked for, where the two earlier runs
on that issue had reported the CLI as unreachable.

§4a's steering remains open and is the one gap of this shape still outstanding.

**8. Scopes, unchanged in shape.** `Scope::Tasks` as the device-level switch and
`Scope::TaskSource(String)` for one source, appended at the end of the enum because
the roster encodes scopes in derived order and a decoder requires byte-identical
re-encoding (PROTOCOL §3.1). A task-submitting principal is never granted
`LockedControl` or `NetworkAdmin`: the bounded temporary-unlock chain in
[LOCKED_USE.md](../LOCKED_USE.md) exists for a person operating their own Mac, and
an inbound task must not be able to trigger it. The `cwd` the daemon hands over is
still checked against an owner-configured allowlist before any child starts.

**9. Sharing is Multica's decision, not a second model in Muster.** Opening a
Muster runtime to colleagues means marking that runtime public in Multica. Muster
adds no cross-person authorization of its own and enrolls no one else's device in
the owner's roster.

## Alternatives

- **Run Multica's daemon with its built-in runtimes only.** No work at all, and the
  right answer if nothing about Muster's execution is wanted. Rejected because the
  turn then runs outside Muster's journal, scope checks and lock-state rules, and a
  phone sees nothing.
- **A Muster connector speaking the daemon protocol directly** (`register`,
  batch `tasks/claim`, `start`, `messages`, `session`, `complete`/`fail`, the
  `/api/daemon/ws` wake signal), as `multica-bridge` does for Loom. This is the only
  option that gives cross-device routing, because Muster would own task preparation
  and could run the turn on any device in the roster. Deferred, not rejected: it is
  a much larger surface, and it would be reached by choice once cross-device
  execution is the goal. `multica-bridge` remains the reference for its hard parts —
  lease takeover, `runtime_gone` convergence, seq-cursor delivery, terminal-call
  retry — and its documented limits (`custom_args` and `runtime_config` cannot be
  honoured without accepting arbitrary argv) transfer.
- **An invented task envelope with a relay-mailbox return path.** This record's
  first draft. Dropped: Multica's protocol is pull-based, so no inbound channel and
  no cross-network mailbox is needed. The `muster-relay` mailbox stays available for
  delivering a result into another person's Muster network directly, out of scope
  here.
- **A `PersonId` dimension on `DeviceRecord`.** Deferred: with one network per
  person and Multica holding the shared identity, it buys nothing yet.

## Consequences

- Multica keeps queueing, retries, run history, cost accounting, review gates and
  the board; Muster keeps execution, the journal, approvals and phone visibility.
  Neither grows a copy of the other, and the integration is one command plus one
  workspace profile row.
- The cost is the family masquerade: Muster's command must satisfy a specific
  vendor's handshake and track its changes, and the supplement floors mean the
  version it reports is load-bearing. Reporting a version it does not behave like
  would be a lie to the scheduler, so the command must actually implement what that
  version implies.
- Cross-device routing is not available on this path. It is the reason to keep the
  direct-protocol connector on the table.
- Provider accounts stay personal: the executing device uses its own signed-in CLI,
  and quota and cost stay attributed there.
- Manual prerequisites: a Multica workspace, a workspace owner or admin to create
  the profile, Muster installed on each participating device, and the `cwd`
  allowlist, which has no safe default.
- Adding `env` to `OrcaCreateRequest` is additive on the wire, but it has three
  in-tree obligations, and the first landing met none of them because CI was not
  running (see [ADR 0013](0013-ci-must-be-askable.md)):
  1. Every constructor of the message has to name the field, including the one in
     `muster-app-core` behind the UI's own "new CLI session". That was missed and
     `main` did not compile until the follow-up.
  2. The generated Swift under `src/native/apple/MusterApp/Sources/MusterCore/Generated`
     has to be regenerated with `scripts/gen-protos.sh`; CI rebuilds the
     generator and diffs the output, so a stale binding fails the Swift job. An
     earlier revision of this record claimed those bindings stayed valid because
     a platform that never sets the field keeps working — true on the wire,
     wrong about the check.
  3. A new crate has to be registered in `src/muster/scripts/check-layers.py`,
     with the edge that reaches it. `muster-acp` sits with `muster-cli` as a
     client of the daemon's socket.

## Verification

- Source and tests, done for the server: `muster-acp` implements the ACP surface
  above and the `musterd`-backed host, with 17 unit tests covering the handshake,
  session creation and its refusal, a streamed turn, a turn that never settles
  being reported as unknown rather than as success, steering served while a turn is
  open, cancel and close, unknown methods, an unparseable frame, and the turn-row
  projection including every non-settling state. `cargo clippy` is clean.
- Live local round, done: against a running `musterd` on this Mac, `muster acp`
  opened a real session through `orca.create`, ran a Codex turn that executed a
  real tool call, streamed the tool row and the assistant text, and settled
  `end_turn`. The same run through `claude` failed at the provider with an expired
  OAuth session, and the server reported it as a failed turn rather than a success
  — the error path working as designed, not a passing test. Both are recorded in
  [MULTICA-DAEMON-2026-10-21](../validation/MULTICA-DAEMON-2026-10-21.md).
- Third-party observation, done: CLI 0.6.5's daemon was installed in a container
  and driven against a recording stand-in. A stand-in runtime command registered
  through a custom profile and completed a full run — claim, start with
  `task-supplement-v1`, progress, session pin, seq-numbered messages, a supplement
  delivered into the live turn and acknowledged, and `complete`. Evidence and its
  limits are in
  [MULTICA-DAEMON-2026-10-21](../validation/MULTICA-DAEMON-2026-10-21.md). It
  exercises Multica's side of the contract; the stand-in was a shell script, not
  Muster.
- Build, signing, notarization, publication, installation: not started.
- Runtime and device acceptance: not started. Nothing has run against a real
  Multica server; no scheduler has registered this command as a runtime, claimed a
  task, or reported one. The live round was driven by a local test harness speaking
  the same protocol, which is not the same as the scheduler driving it.
- Unresolved: whether the internal Multica deployment accepts a workspace custom
  runtime profile under the team's own policy, and which family its CLI version
  supports for supplements.

### Layer-map integration follow-up (2026-10-22)

The merged ACP crate and CLI dependency were missing from the architecture gate.
Register `muster-acp` as a composition adapter restricted to `muster-proto` and
`muster-rpc`, and permit the CLI to call it. Update MODULES.md with the same
edges. This records the already implemented RPC-only boundary, not a new
execution engine or permission to depend on platform/daemon implementation.
The layer gate covers every crate, including this adapter.

CI also found that the merged `OrcaCreateRequest.env` field had not been
regenerated in the Apple wire bindings. Run `MusterApp/scripts/gen-protos.sh`
with SwiftProtobuf 1.38.1 (the CI-resolved version); the only generated change
is that map field's declaration, name, decode, traversal and equality. This
adds no Swift business logic. Regeneration is byte-for-byte stable afterward;
CI must verify the Apple shell build/tests before landing.
