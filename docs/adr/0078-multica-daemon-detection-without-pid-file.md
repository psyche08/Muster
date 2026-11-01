# ADR 0078: Detect the guest Multica daemon without a pid file

- Date: 2026-11-01
- Status: Accepted; device verification recorded below, end-to-end acceptance pending
- Related: [ADR 0051](0051-multica-management-and-live-catalog.md),
  [ADR 0054](0054-native-multica-targeted-installation.md),
  [ADR 0064](0064-multica-native-runtime.md),
  [ADR 0077](0077-multica-runtime-state-and-manual-upgrade.md);
  ARCHITECTURE §3.6.5 and §8.12

## Context

`musterd::multica` reports `MulticaStatus.running` from a fixed shell probe in
the guest: read `/root/.multica/daemon.pid`, require a numeric pid, and confirm
that pid's command line matches `multica.*daemon`. Keep-alive supervision uses
the same predicate to decide whether to start the daemon at all.

Device acceptance on 2026-11-01 (a dedicated acceptance Mac, Apple container 1.4.1, guest Multica
0.3.57, adopted `mlt` container) showed that this version runs `multica daemon
start` without writing `/root/.multica/daemon.pid`; the guest directory instead
holds `daemon.id`, `config.json` and daemon logs. A process-table inspection
found the live daemon (pid 76294, `multica daemon start`) while the probe
returned failure, so the CHECK projection said `running=false` and the
supervisor behaved as if no daemon were running. The same guest also cannot
resolve the Multica server host because the container's configured DNS
(`192.168.64.1:53`) refuses UDP queries; that is a container-network condition,
not a Muster decision, and it is not changed here.

## Decision

Detect the daemon with two signals inside one bounded guest command:

1. the existing pid-file fast path, still validated against
   `/proc/<pid>/cmdline`; and
2. a fallback scan of `/proc/<pid>/exe` for a Multica binary.

The fallback matches the executable link, not the command line, so the probe's
own `sh -c` argument cannot match itself. Failure of both signals means "not
running". No ABI field, action or platform behavior changes; `running` continues
to mean "a daemon process is present", not "the daemon is authenticated or
healthy".

## Alternatives

- **Keep the pid-file-only probe.** Rejected: it reports a running daemon as
  stopped on 0.3.57, and keep-alive would try to start a second instance or
  report a false state.
- **Trust `daemon.id`.** Rejected: the file is identity state that survives a
  stopped daemon, so it is not proof of a live process.
- **Run `multica daemon status` and parse text.** Rejected: it is an extra
  provider-version-specific surface with unverified output for this release and
  adds parsing of untrusted text to a boolean decision.
- **Classify the DNS failure inside the probe.** Rejected for this change: the
  query error already tells the operator to check login and network, and
  inferring a network root cause from CLI text would be a guess.

## Consequences

`MulticaStatus.running` and keep-alive supervision tolerate a daemon that no
longer writes a pid file, at the cost of one bounded process-table scan in the
guest per status/maintain evaluation. Detection remains presence-only: a
daemon that runs but cannot resolve its server still reports running, which is
accurate for the process and misleading only if read as task availability; the
workbench query fails separately with its own error.

## Verification

- Source/tests: `daemon_detection_falls_back_to_the_guest_process_table` and
  `daemon_detection_is_false_when_no_guest_process_matches` cover the fallback
  and the empty case with a fake `container` executable. The existing
  supervision tests still cover opt-in restart behavior.
- Device probe: the fixed probe was first run read-only on the target guest
  before the code change: the old command exited 1 while a proposed
  process-table command exited 0, with the daemon process present. After the
  change, the acceptance probe on that Mac reports `running=true`; other observed
  values are recorded in ADR 0077's device-acceptance section.
- Not performed: a real terminal login, scheduler task, restart/reboot
  recovery and a provider turn. The container DNS failure that blocks the
  workspace query remains an operator/container-network issue.
