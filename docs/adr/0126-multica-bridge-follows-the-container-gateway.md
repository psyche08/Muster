# ADR 0126: The Multica runtime bridge follows the container gateway

- Date: 2026-11-05
- Status: Accepted; source and tests verified, installation and runtime pending
- Related: [ADR 0064](0064-multica-native-runtime.md),
  [ADR 0077](0077-multica-runtime-state-and-manual-upgrade.md),
  [ADR 0078](0078-multica-daemon-detection-without-pid-file.md);
  ARCHITECTURE §3.6.5 and §8.12

## Context

`musterd::multica::bridge` binds the runtime-bridge listener on the container's
private gateway and writes the provider wrappers (`runtime-codex`,
`runtime-claude`) with that address and port. The container's `codex` and
`muster-native-claude` commands are those wrappers, so every Multica task that
should run in a desktop application or the host CLI connects to this listener.

`bridge::ensure`, called by keep-alive supervision every 15 seconds, only asked
whether the listener task was still alive. If it was, it refreshed the wrappers
with the files already written. It never compared the bound gateway with the
container's current one.

A Mac whose `mlt` container had been moved from another Mac showed the result
on 2026-11-05. The container was restarted, and Apple's default network came
back on another subnet: gateway `192.168.68.1` instead of `192.168.64.1`. The
host's `bridge100` interface then carried only the new address. `musterd` still
listened on `192.168.64.1` and the wrappers in the guest still named it, so no
task could reach the bridge. Nothing reported an error: the Multica projection
said `installed=true` and `running=true`, and `last_error` was empty, because
the listener task had not ended. A connection from the guest to the old
address appeared to succeed, because a proxy on the host accepted TCP for any
port at that address. So a failed connection was not a reliable signal either.

## Decision

The bridge records the gateway its listener is bound to (`bridge::Bridge`,
held in `Manager.bridge`). `bridge::listen` keeps the listener only while it is
alive **and** bound to the gateway the container reports now. Otherwise it:

1. binds a new listener on the current gateway and mints a new token (a failed
   bind returns the error and leaves the old bridge in place for the next
   supervision pass);
2. aborts the old listener task and waits for it, which ends its connections:
   they came through an address the guest can no longer reach;
3. revokes task launchers left by the old bridge, as a daemon start already
   does;
4. rewrites the token and the wrappers, then starts serving.

`ensure` then installs the rewritten wrappers into the container (`cmp`, then
`install`), and removes the retired kiro-cli alias only when a new listener was
published. The gateway is read from the container inventory as before, the
listener still accepts only a private address, and the guest daemon is not
restarted: the wrappers read the host, port and token at each launch.

A small change in order follows from this. The listener now starts before the
wrappers are refreshed, so a failed refresh keeps the listener and retries only
the wrapper installation on the next pass. Before, a failed refresh dropped the
listener, and the next pass minted a new port and token.

## Alternatives

- **Bind on all interfaces or on loopback.** Rejected: the guest cannot reach
  host loopback, and listening on every interface would expose the task bridge
  beyond the private container network.
- **Rebind when a test connection to the old address fails.** Rejected: on the
  affected Mac a host proxy accepted connections to the vanished address, so a
  probe would have reported the dead bridge as healthy.
- **Restart the bridge on every supervision pass.** Rejected: each restart
  changes the port and token and ends every running task's connection.
- **Ask the operator to restart Muster.** Rejected: it works once, but keep-alive
  supervision exists so that container restarts need no attended step.

## Consequences

A container that returns on another subnet is served again within one
supervision pass (15 seconds) while keep-alive or the runtime bridge is on. Any
connection still open on the old listener is ended. It was already unreachable
from the guest, but a host-side provider turn that was still working for it
stops. A configuration with keep-alive and the runtime bridge both off still
starts nothing.

## Verification

- Source/tests: `bridge_follows_a_changed_container_gateway` places a live
  bridge on `192.168.64.1`, calls `listen` for the current gateway and checks
  that the old task ended, a stale launcher was revoked, the wrapper names the
  new host and port, and the published port accepts connections. A second call
  with the same gateway must keep the token and wrapper unchanged. The test
  uses loopback, which only test builds accept as a gateway, so it does not
  depend on Apple's container network or on installed provider applications.
- Checks: `cargo test -p musterd --lib multica`,
  `cargo clippy -p musterd --all-targets` with `RUSTFLAGS=-D warnings`,
  `scripts/check-public-content.py` and `scripts/check-adr.sh origin/main`.
- Diagnosis (read-only, before the change): on the affected Mac, `container
  inspect` reported gateway `192.168.68.1`, `musterd` listened only on
  `192.168.64.1`, the host had no `192.168.64.1` address, and the guest
  wrappers named `192.168.64.1`. The guest daemon was running with the
  Muster-provided `MULTICA_CODEX_PATH` and `MULTICA_NATIVE_CLAUDE_PATH`, and
  heartbeats for its ten runtimes were acknowledged by the server.
- Not performed: a signed build, installation on the affected Mac, and a real
  Multica task that runs through the rebound bridge.
