# ADR 0015: Offering this machine as a task runtime is opt-in

- Date: 2026-10-22
- Status: Accepted
- Related: [ADR 0009](0009-task-contract-and-runtime-manifest.md) defines the
  runtime surface this switch governs; `muster-core::policy::LocalPolicy`
- Numbering: 0013 and 0014 are taken by an open pull request, so this record
  takes the next free number rather than reusing theirs

## Context

[ADR 0009](0009-task-contract-and-runtime-manifest.md) made Muster serve an
external task scheduler as a runtime: the scheduler launches `muster acp`, and
every turn runs in `musterd` under its journal and approvals. Nothing in it said
whether a device does that at all. Built as described, any device with the
command installed would serve, which is the wrong default for this capability
specifically: unlike viewing or driving the owner's own sessions, it admits work
the owner did not start, chosen by whoever can assign a task on the scheduler's
board.

`LocalPolicy` already models this kind of choice — "this machine's own say over
what it offers" — with `offer_agent` and `offer_terminal`, both on by default
because they only expose what the owner is already doing.

## Decision

1. **`LocalPolicy` gains `offer_tasks`, default `false`.** It is the machine's
   own configuration, never part of the roster, like its neighbours. A stored
   policy written before the field existed reads as off (`#[serde(default)]`),
   so an upgrade does not silently start offering.
2. **`muster acp` asks before it serves, and the probe asks too.** Both the
   version probe and the server read the policy and refuse together. Refusing
   the probe is deliberate: a version a scheduler can read is a runtime it will
   register and then send work to, so answering it while refusing the work would
   turn an off switch into a queue of failures.
3. **Not knowing is a refusal.** An unreachable daemon or a reply that is not a
   policy refuses, rather than assuming either answer.
4. **The refusal names what actually works today**: `turn it on with
   `muster policy --offer-tasks true``. It deliberately does not point at a
   Settings screen, because no app exposes the toggle yet; the proto field
   carries it to the apps over the existing `PolicyGet`/`PolicySet` path, and the
   message can name that screen once one exists.

## Alternatives

- **On by default, like the other two offers.** Rejected: those expose the
  owner's own work, and this one accepts someone else's.
- **Gate it inside `musterd` on the session instead.** Rejected for now: tasks
  arrive over the local socket as the owner, so there is nothing in the request
  to distinguish a scheduler's session from the UI's own "new CLI session", and
  gating `orca.create` wholesale would break that feature. This is named as a
  limit below rather than papered over.
- **A scope on the roster** (`Scope::Tasks`). That governs what a *remote
  member* may ask of this device and remains the right place for that; it is a
  different question from whether this device offers the capability at all.
- **An environment variable or a config file of its own.** Rejected: the answer
  belongs where the other offers live, so one screen shows what this machine
  gives out.

## Consequences

- A device serves a scheduler only after its owner says so, and the switch is
  visible next to the other two offers.
- The check is in the command, not in the daemon's request path. A local process
  that calls `orca.create` directly still opens a session, as the UI does — the
  switch governs whether this machine *offers itself*, not whether the owner can
  use their own daemon. Closing that would need a marker on the request
  distinguishing a scheduler-originated session; it is not attempted here.
- Turning it off does not stop a turn already running. Nothing here revokes a
  live session; the scheduler's own cancel path and `agent.interrupt` remain the
  way to stop work.

## Verification

- `muster-core`: the default is off, and a policy serialized without the field
  loads as off. Both asserted.
- Live against an isolated daemon from this build: with the switch off,
  `muster acp --version` exits 1 with the message naming the setting, and
  serving refuses the same way; `muster policy --offer-tasks true` then makes
  `--version` print the version and exit 0; the value round-trips through
  `policy` reads.
- `cargo fmt --all --check`, `scripts/check-layers.py` and
  `cargo clippy --workspace --tests` are clean.
- `cargo test --workspace` failed once on `muster-net`'s
  `secure_relay::integration::peer_quic_large_bidirectional_rpc_survives_carrier_changes`,
  then passed three times out of three alone and on the next full run. That is a
  third intermittent in that crate, in a different module from the two an open
  pull request addresses, and nothing in this change touches it. Observed, not
  addressed.
- Not established: no app Settings screen exposes the toggle yet. The field
  reaches the apps over the existing policy path, so that is a native UI change,
  not a protocol one. Until an app ships it, the switch is reachable only
  through `muster policy --offer-tasks`, which makes the refusal's mention of
  Settings a promise about where it will be rather than where it is.
