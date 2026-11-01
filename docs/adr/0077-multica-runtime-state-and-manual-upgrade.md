# ADR 0077: Multica support: current state, version visibility and manual upgrade

- Date: 2026-11-01
- Status: Accepted; device and end-to-end acceptance pending
- Related: ADR 0009 (historical ACP shim), ADR 0051, ADR 0052, ADR 0053,
  ADR 0054, ADR 0060, ADR 0064, ADR 0072;
  [ARCHITECTURE](../ARCHITECTURE.md) §3.6.5 and §8.12;
  [DESKTOP_UPDATES](../DESKTOP_UPDATES.md) Multica section;
  [deploy/multica README](../../src/muster/deploy/multica/README.md)

This decision supersedes the **status statement only** of ADR 0053 ("desktop
runtime execution remains unimplemented"); ADR 0053's read-only workbench
decision stands. It does not rewrite the historical decisions or evidence of
ADR 0009, 0051, 0052, 0054, 0060, 0064 or 0072. ADR 0009 remains Proposed and
historical. ADR 0060, 0064 and 0072 keep their own pending acceptance and are
not elevated by this record.

## Context

A read-only audit of the Multica ABI, Apple UI, install/service management and
runtime routing found a mismatch between the documented and the deployed
product, and three concrete correctness gaps.

1. **Documentation described the first iteration as the current one.** ADR 0009
   proposed reaching Multica through an ACP shim (`acp-bridge.py`, `muster acp`,
   a kiro-cli compatibility alias). The deployed path is different: a managed
   Apple container runs the Multica daemon and CLI, and provider wrappers in the
   container hand a task's context to `musterd` over a private gateway using the
   `MUSTER_NATIVE_V1` framing, where Shared Rust routes to the native desktop
   application or its CLI fallback. `deploy/multica/README.md` still presented
   the shim as the supported setup, and ADR 0053 still said desktop execution
   was unimplemented even though ADR 0064 implemented it (with acceptance
   pending).
2. **No version visibility.** `MulticaStatus` exposed install/running state but
   not which Multica CLI the container runs, so an operator could not tell what
   a device actually had before deciding on a manual upgrade.
3. **Migration was only a failure.** A saved data root that no longer matched
   the installed container's bind mounts failed at save time as a generic
   conflict; the projection had no flag that a stopped container needed the
   attended offline migration first.
4. **Bridge lifecycle gaps.** The session key was derived from provider,
   working directory and an optional `MULTICA_TASK_ID`, so a connection without
   a task identity shared a key with other anonymous connections. Provider
   wrappers were reconciled only under keep-alive supervision, so an opted-in
   runtime bridge with keep-alive off could keep stale wrappers after a daemon
   restart. Task launchers left by a previous bridge process were not revoked
   at daemon startup.
5. **UI status was thin.** The Multica tab had no version or migration surface,
   and a second request while one was outstanding was dropped silently.

## Decision

**Record the actual architecture and keep it in Shared Rust.** The Multica
management state machine lives in `musterd::multica` (container install/adopt,
config validation, keep-alive supervision, offline data-root acceptance,
stopped/installed/unauthenticated classification), its installation diagnostics
in `musterd::multica::installation`, its private bridge in
`musterd::multica::bridge`, provider protocol and task credentials in
`muster-acp::native_host` / `muster-acp::scheduler` with destination choice in
`muster-agent::native_runtime`, and install progress/reconnect recovery in
`muster-app-core::update::multica`. Apple shells render the projection and keep
only OS lifecycle work. ADR 0009's shim, `acp-bridge.py`, `muster acp` and the
kiro-cli alias are labeled legacy/historical in the deployment README and are
not documented as a supported setup.

**Additive read-only projection.** `MulticaStatus` gains:

- `version` — the guest CLI's `multica --version`, read-only, empty when unknown.
  It is diagnostic. It does not install, upgrade or stop anything.
- `migration_required` — an installed container's bind mounts no longer match
  the saved data root; saving a changed root stays refused until the attended
  offline migration is complete.

No new administration action is added: the local-owner ABI remains CHECK, SAVE,
INSTALL and PROGRESS.

**Bridge correctness.** A runtime-bridge session key requires `MULTICA_TASK_ID`
and the connection is rejected before launch when it is missing, so anonymous
connections cannot share a task's key. A daemon start revokes task launchers
left by a previous bridge process, because no live turn can own them while the
listener does not yet exist. When keep-alive is off but the runtime bridge is
opted in, the supervisor reconciles only an already running container's bridge
and wrappers; it never starts the container, its daemon or a task.

**Scope boundary: no in-app Multica upgrade or stop.** This change does not add
an upgrade, image pull, stop or rollback action, and none is implied by the new
version field. Those remain manual operator steps documented in
[DESKTOP_UPDATES.md](../DESKTOP_UPDATES.md) and
[deploy/multica README](../../src/muster/deploy/multica/README.md): note the
version, disable keep-alive, stop the container, install the desired CLI in the
container and on the host from the trusted installer, restart and verify, with
the data root preserved. Muster does not verify an upgrade and provides no
downgrade.

**Tests and UI.** Guided-install tests exercise stage/log bounds, timeout,
exclusivity and the install runner with fake executables; bridge tests cover
the token handshake, the task-scoped session key and wrapper provisioning;
app-core tests cover an interrupted install and the visible busy notice; Claude
scheduler tests cover a rejected turn, interruption and successful completion
as terminal frames; the Apple UI shows status, version, migration guidance and
installation progress from the Rust projection.

## Alternatives

- **Add upgrade/stop to the ABI now.** Rejected: an in-app upgrade would need
  image/CLI provenance, a task-quiescence fence and rollback semantics that this
  release cannot verify; a stop action would undermine the keep-alive promise
  and the documented manual migration. The honest boundary is a read-only
  version plus a runbook.
- **Keep documenting the ACP shim as current.** Rejected: it is not what the
  deployed code installs, and presenting it invites wiring a kiro-cli alias that
  misrepresents the provider and protocol.
- **Keep migration as a save-time error only.** Rejected: the operator needs
  the condition before attempting the save; the flag is read-only and does not
  change acceptance rules.
- **Let the shell infer status from container JSON or logs.** Rejected by
  AGENTS.md: trust-bearing state stays in Shared Rust; the shell renders.
- **Make the session key fall back to cwd when the task identity is absent.**
  Rejected: it silently merges unrelated sessions and weakens the task-scoped
  credential boundary.

## Consequences

Operators can read the actual Multica CLI version and migration state from the
projection, and the deployment README now describes the managed bridge instead
of the shim. Bridge connections without a task identity fail explicitly rather
than sharing a key. A runtime-bridge-only configuration keeps a running
container's bridge current without turning supervision back on.

Tradeoffs remain: version reporting costs one guest `exec` when installed and
running, and is empty otherwise; migration guidance is advisory and the actual
move is still attended; no upgrade/stop/rollback exists in the app; a keep-alive
off configuration still never starts stopped services. ADR 0009's shim files
stay in the tree as records and may confuse a reader who skips the legacy
heading, so the README labels them explicitly.

## Verification

This change records source and test status, not device acceptance.

- Source/tests: the new projection fields and bridge/startup changes are covered
  by the tests named above; these use fake executables, temporary directories,
  duplex streams and a fake scheduler host, so they prove Rust behavior and
  wiring, not a real container, installer or provider turn. No test result is
  copied from an earlier ADR.
- Checks run for this change: `cargo test -p musterd --lib multica` (26 passed,
  1 ignored), `cargo test -p muster-acp --lib` (24 passed),
  `cargo test -p muster-app-core --lib update::multica` (5 passed),
  `cargo clippy -p musterd -p muster-acp -p muster-app-core --all-targets` with
  `RUSTFLAGS=-D warnings`, `scripts/check-layers.py`, and
  `scripts/check-public-content.py` (0 findings). These are Linux-side/library
  checks on this host; the full workspace suite and hosted CI are separate.
- Bindings: `Generated/muster.pb.swift` and the Harmony `muster.js`/`muster.d.ts`
  were regenerated from the additive schema with the repository's pinned
  generators. The `MusterUI` Swift package target was compiled on macOS
  (`swift build --target MusterUI --disable-sandbox`); a signed Xcode
  application build, an iOS build and any device run were **not** performed.
- Device/end-to-end: **not performed.** A real Apple container install, real
  installer, terminal `multica login`, reboot and app-upgrade recovery, a real
  native provider turn and a real scheduler task remain unverified. The device
  evidence in ADR 0052, 0054, 0060, 0064 and 0072 is historical and was not
  reproduced; ADR 0060's Codex creation acceptance is still blocked, so this
  ADR does not claim working Codex desktop creation.
- Documentation: the repository docs and the deployment README are English;
  public-content and ADR checks are reported with the change.
