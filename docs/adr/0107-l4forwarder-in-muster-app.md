# ADR 0107: Ship l4forwarder in Muster.app and let musterd run it

- Date: 2026-11-03
- Status: Accepted; source verified, release and device migration pending
- Related: supersedes decision 6 (deployment) of
  [ADR 0105](0105-l4forwarder-addon.md); its other decisions stand.
  [addons/l4forwarder](../../addons/l4forwarder/README.md),
  [MusterApp README](../../src/native/apple/MusterApp/README.md)

## Context

[ADR 0105](0105-l4forwarder-addon.md) deployed l4forwarder on its own, as a
launchd user agent set up by `deploy/install.sh`:

- the binary in `~/.local/bin`;
- the configuration in `~/.config/l4forwarder`;
- on the first forwarding member, the source in a hand-made copy under
  `~/.local/src`.

That worked (see ADR 0105's device acceptance), but the owner rejected the
layout. An addon that is part of Muster belongs in the Muster app, at Muster's
standard paths, and is upgraded with it.

Three ways to start the bundled helper were offered:

- the app registering a bundled agent through SMAppService, as it does for
  musterd;
- the helper installing its own user agent;
- musterd running it.

The owner chose musterd.

## Decision

1. **The helper ships in the bundle.** The MusterMac post-build step copies
   `addons/l4forwarder/target/release/l4forwarder` to
   `Muster.app/Contents/Helpers/l4forwarder`. It signs it with hardened runtime
   and identifier `dev.linsheng.muster.l4forwarder`, as it already does for
   `muster` and `muster-browser-host`. A missing build fails the app build, as
   it does for those helpers. The addon stays a separate Cargo package with
   its own target directory and no Muster crate dependency.
2. **musterd supervises it** (`musterd::l4forwarder`). musterd never reads the
   rules or decides anything about forwarding; it only manages the process:
   - **Configuration.** The configuration is `l4forwarder.json` in musterd's
     state directory (`~/Library/Application Support/Muster` on macOS).
     musterd looks at it every 5 seconds and compares contents. Its appearance
     starts the helper, a change restarts it, and its removal stops it.
   - **Program and log.** The program is `Contents/Helpers/l4forwarder` when
     musterd runs from `Contents/MacOS`, else the file beside musterd. Output
     is appended to `l4forwarder.log` in musterd's log directory
     (`~/Library/Logs/Muster` on macOS).
   - **Restarts.** A helper that exits on its own is started again after 1 s,
     doubling up to 60 s. A run of 60 s or longer resets the delay. A missing
     program is retried the same way and logged once per distinct error, so
     an app update that brings it is picked up.
   - **Phones.** A phone's embedded daemon never starts the supervisor.
3. **The helper cannot outlive musterd.** musterd starts it with
   `--supervised` and keeps its standard input open. The new flag makes the
   helper exit when that input closes, which happens even when musterd is
   killed.
   - The input handle is held apart from the child, because tokio's
     `Child::wait` closes the child's input before waiting.
   - Stopping closes the input, waits 2 s, then kills the process.
   - Dropping the supervisor kills the process (`kill_on_drop`).
4. **The standalone deployment is removed.** `deploy/install.sh` and the
   launchd template are deleted. `l4forwarder --config <file>` still runs on
   its own, for development and tests.

## Alternatives

- **SMAppService agent registered by the app.** It matches musterd's own
  registration. But the app would decide when forwarding runs, from the
  presence of a file. That puts product state into platform code, against the
  thin-app rule. It also needs Swift work on every desktop platform.
- **The helper installing its own user agent.** It keeps a lifecycle apart
  from Muster's, and the running process keeps an old binary after an app
  update until someone restarts it.
- **Keeping ADR 0105's layout.** Rejected by the owner.

## Consequences

- One install, one upgrade path. An app update replaces the helper with the
  daemon, and a daemon restart restarts the helper.
- Turning forwarding on or off is writing or removing one file. The two Muster
  grants per port from ADR 0105 are still required.
- A configuration change restarts the helper and drops its connections.
- **Nothing changes on installed machines until a Muster release includes
  this.** Until then, the first forwarding member (Mac-B in ADR 0105) keeps
  ADR 0105's interim agent. When the release is installed there:
  - move the configuration to `~/Library/Application Support/Muster/l4forwarder.json`;
  - remove the `dev.linsheng.l4forwarder` user agent and its `~/.local` and
    `~/.config` files.

  Otherwise the bundled helper and the interim agent compete for the same
  port. The later one waits, logging that it cannot bind.

## Verification

- Source and tests (macOS, Rust 1.98):
  - **musterd:** `l4forwarder` has 5 tests. They cover the program path
    (bundle and beside the daemon), and start, keep running across polls,
    restart on change and stop on removal. They also cover restart after
    exit, the helper ending when the supervisor is dropped, and a missing
    program picked up later. They passed six consecutive runs.
    - An earlier version of these tests found the `Child::wait` input closing
      described in decision 3. The helper restarted in a loop. The fix is
      covered by the keeps-running assertion.
  - **Full musterd suite** (`cargo test -p musterd`): 129 passed, 3 ignored
    (existing ignores). Every daemon these tests start now also starts the
    supervisor.
  - **Addon:** a new test checks that a `--supervised` forwarder exits when
    its input closes. The earlier 4 configuration and 5 forwarding tests
    still pass.
  - `cargo fmt`, `clippy -D warnings` (workspace and addon) and
    `scripts/check-layers.py` are clean.
- End to end, local: a debug musterd with an isolated state directory and the
  real addon beside it.
  - Writing the configuration started the helper as musterd's child, with
    `--supervised`. A TCP echo forwarded through it 3.7 s after the write.
  - `kill -9` of musterd ended the helper 0.06 s later.
  - The helper's log went to the configured log directory.
- Packaging: the new post-build lines were run against a stand-in bundle with
  ad-hoc signing. They installed `Contents/Helpers/l4forwarder` with hardened
  runtime and identifier `dev.linsheng.muster.l4forwarder`. XcodeGen 2.46.0
  regenerated the project with a 4-line `project.pbxproj` change.
- Not done:
  - a full Xcode build of Muster.app;
  - Developer ID signing and notarization of the new helper;
  - a release;
  - installation, and the Mac-B migration above;
  - hosted CI (Actions are disabled, [ADR 0016](0016-suspend-hosted-actions-budget.md)).
