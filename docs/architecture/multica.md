# Multica Management

Part of the [Muster architecture](../ARCHITECTURE.md). Section numbers are shared across all parts, so a reference such as §3.6.2 means the same section wherever it appears.

## 3.6.5 Multica Management and the Native Runtime Bridge

The deployed Multica path is a **managed Apple container** running the Multica
daemon and CLI, plus a **native provider runtime on the Mac** behind a private
bridge (`MUSTER_NATIVE_V1`). It replaces ADR 0009's ACP shim (Proposed and now
historical; `deploy/multica/acp-bridge.py`, `muster acp` and the kiro-cli
compatibility alias are records only) and it supersedes ADR 0053's status
statement that desktop execution is unimplemented: the read-only workbench of
ADR 0053 remains, and scheduling through native provider runtimes exists as
recorded in ADR 0060/0064, whose Codex creation acceptance is still blocked.
ADR 0077 records the current state, the read-only version projection and the
manual upgrade boundary.

State machine ownership is entirely Shared Rust; the platform shells render a
projection and perform only OS lifecycle work:

| Concern | Owner | Invariant |
|---|---|---|
| Container install/adopt, config validation, keep-alive, stopped/installed/unauthenticated classification, data-root acceptance | `musterd::multica` (`mod.rs` manager) | Local-owner-only administration ABI; never silently replaces an existing container or erases its files; a changed root is adopted only after an attended offline migration with the old path absent |
| Guided install stages, bounded/redacted output, timeout, exclusivity | `musterd::multica::installation` | 64 KiB retained, 4096-byte lines, control sequences removed, credential-bearing lines suppressed; one install at a time; an uncertain outcome is reported, never retried |
| Private gateway listener, token handshake, session key, provider dispatch | `musterd::multica::bridge` | Binds only the container's private gateway; host UDS never bridged; handshake requires the bridge token and a task-scoped key that includes `MULTICA_TASK_ID`; wrapper environment accepts only `MULTICA_*` names |
| Provider protocol translation, pending-session binding, task credentials | `muster-acp::native_host`, `muster-acp::scheduler`; destination choice in `muster-agent::native_runtime` | Native application preferred, CLI only when the application is absent; pinned destination per session; task credentials revoked at turn end and stale launchers at daemon startup; no replay of an ambiguous outcome |
| Install progress, reconnect recovery, status/version/migration projection | `muster-app-core::update::multica` and the `update/` dispatch | Polls only during an outstanding install; recovers a daemon-owned installation after reconnect; never replays; the shell never infers success from logs |

Read-only status carries the guest `multica --version` and a
`migration_required` flag from the daemon, so the shells display state without
deciding it. Keep-alive opts container and daemon into supervision; with
keep-alive off a running container's bridge and wrappers are still reconciled
without starting stopped services. **There is no in-app Multica upgrade, image
pull, stop or rollback**: those remain manual operator steps documented in
[DESKTOP_UPDATES.md](../DESKTOP_UPDATES.md) and `deploy/multica/README.md`.

[ADR 0156](../adr/0156-container-process-resource-lifecycle.md) bounds successful
wrapper reconciliation to five minutes, with immediate invalidation on container
incarnation, bridge/configuration or provider changes. Daemon health/recovery
keeps its 15-second cadence. New managed containers use `--init`; existing
containers and the separately patched Apple runtime need an operator migration.

### Independent service and DSH App bridge: partial implementation

[ADR 0141](../adr/0141-multica-service-and-dsh-app-bridge-design.md) and the
[Multica service design](../MULTICA_SERVICE_DESIGN.md) propose task-owned
sessions outside Agents, a bridge to the existing DSH desktop App, and a
default-off remote Multica service over PeerLink. Shared Rust owns binding,
authorization, capability and navigation state. [ADR 0149](../adr/0149-multica-ownership-reservations.md)
adds durable ownership reservations. [ADR 0161](../adr/0161-multica-task-session-isolation.md)
enforces registered ownership at Agent/terminal boundaries and adds default-off,
workspace-selected remote metadata browsing with Mac/iPhone device selection.
New scope issuance requires all active roster consumers to prove compatibility.

[ADR 0162](../adr/0162-multica-workbench-operations.md) adds typed local-host
agent edits, backlog issue creation/editing, assignment/status with `--no-start`,
plain comments, exact-run cancellation, and bounded run/comment history. Rust
checks exact workspace/resource identity, editable revision and permission,
serializes writes and records no-replay operation results. Agent instructions
are exposed only in local detail; remote views remain read-only. Starting,
rerunning and mention scheduling are unavailable until the App binding exists.

Registry production writers, legacy migration, task transcript access, DSH App
scheduling and remote control are still pending; existing scheduler sessions are
not automatically imported. The descriptor does not advertise control. This is
not full P1/P3 or device acceptance. The Codex/Claude bridge above is unchanged;
its fallback policy must not be reused for the future DSH App destination.
