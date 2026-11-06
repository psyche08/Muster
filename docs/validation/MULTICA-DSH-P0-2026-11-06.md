# Multica DSH App-host P0 evidence — 2026-11-06

Source stage: [ADR 0149](../adr/0149-multica-ownership-reservations.md).
This is an isolated App-host probe and a Rust ownership foundation, not a
completed Multica runtime or cross-device release.

## Versions and isolation

- Installed DeepSeek Harness App: 0.2.1-rc.5. Its bundled desktop-host entrypoint
  runs with the installed App executable. One fresh temporary `DSH_HOME` used
  the web profile without opening a window; the personal desktop profile and
  all business tasks were untouched. The fixture lives in
  `src/muster/deploy/deepseek-harness/test/multica-contract-probe.mjs`.
- Upstream runtime contract inspected at
  [571d7c0](https://github.com/multica-ai/dsh-multica-runtime/tree/571d7c036d15f58ee88c89fd80afb0a6c7f0023c):
  `src/protocol.ts` defines protocol 1 JSONL execute/cancel and result frames;
  `src/index.ts` is the independent stdio runtime. It is not an ACP schema and
  its headless approval behavior is not adopted by the App bridge.
- No deployed Multica protocol negotiation or task callback was performed.
  The upstream pin is source-contract evidence, not a deployed compatibility claim.

## Observed facts

| Contract | Result and limit |
|---|---|
| Create with pre-reserved native identity | Passed: `sessionController.create` returned the exact supplied native ID. Header ID/cwd matched; no initial turn or user message existed. |
| Prompt acceptance | Passed: one synthetic no-tools prompt returned `accepted: true`; the native `user/message` retained that exact `source.rpcId`. |
| Restart/read-only recovery | Passed: after the isolated host restarted, `sessionQuery.observeSession` recovered the same header and prompt request ID. The live-agent count was zero. Lookup performed no create, resume or prompt. |
| Turn facts | Persisted `turn/start` and `turn/end` carry `data.turn`; the prompt event lies in that native turn's event sequence. The terminal reason was `error`, not a completed model answer. |
| Model selection | The installed controller has `selectModel`; effective requested model/reasoning settings were not verified. Presence of a method is not capability acceptance. |
| Task credential callback | Not verified. No real Multica credential or business task was used. |
| GUI and production profile | Not exercised. The isolated installed App host does not establish production-window, locked-screen or phone acceptance. |

The host accepts an explicit session ID, but its creation method can mint a
*different* ID when an existing session cannot be reused. Therefore any future
bridge must require a fresh reserved identity and exact receipt equality; it
must not implement lookup by repeating create. The storage foundation reserves
ownership before creation so that a future catalog barrier can consult it.
The barrier, scheduler bridge and local workbench are still to be integrated.

## Source verification and remaining gates

The complete `muster-store` suite passed 13 tests, including six ownership tests:
exact request idempotency, full device/provider/store identity, requester
separation, same-run duplicate rejection, atomic rollback, conflicting receipt
retention, restart-to-unconfirmed, generation-bound resume and retained ended
ownership. Strict store Clippy passed. No UI, provider controller or wire scope
was enabled by this stage.

The existing plugin suite passed 16 tests after allowing its local Unix socket
fixture (the sandboxed socket test could not start). The new fixture passed Node
syntax validation; focused Rust formatting and diff checks passed. The layer
check reports the existing `muster-ffi -> muster-paths` allowlist mismatch. Neither
the FFI manifest nor the layer checker changed from `origin/main` at this stage.

P0 remains incomplete for requested model settings, real task credential delivery
and service-side callback, and production App acceptance. P1 remains incomplete
for legacy evidence migration, Agents filtering/cache/notification isolation and
the task-session workbench. P2–P4 remain implementation and acceptance work.
No signing/notarization, application installation, macOS/iOS publication or
cross-device acceptance is claimed. Do not advertise DSH scheduling from these
intermediate results or replace it with a CLI success.

## Installed CLI entrypoint follow-up

The App includes `Contents/Resources/runtime/cli/bin/dsh`. Its launcher uses the
App executable in Node mode and the bundled desktop-host `lib/cli.js`. That
entrypoint calls `runCli` with desktop-profile management enabled and the
bundled immutable package manager. Help exposes web/headless/tui profiles,
`tui --resume`, and profile-targeted plugin installation.

This confirms a bundled CLI, not a proven RPC entrypoint into the already-running
Desktop host. No production profile was started, resumed, modified or restarted
during this inspection. The App plugin remains the integration destination;
manual reruns must create a new App session while infrastructure recovery must
recover the verified original binding. See [ADR 0154](../adr/0154-multica-workbench-operations.md)
for local workbench implementation and its explicit scheduling restrictions.
