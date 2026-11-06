# Apple container 1.6.1 exec lifecycle patch

See [ADR 0156](../../../../docs/adr/0156-container-process-resource-lifecycle.md).
This is an operator-applied infrastructure patch, not an automatic runtime
upgrade. It applies only to Apple container tag `1.6.1`, commit
`aa8d76b44a834eee3ebf6a0bc577e376f65f6361` (Apache-2.0).

The patch releases exited `LinuxProcess` objects, their RPC clients, completed
monitor tasks and callbacks, and resumes then releases live wait continuations.
It also closes runtime-owned stdin on exec completion. The most recent 1024
exit values remain available for late or repeated wait requests. After eviction,
a wait returns the existing unknown-process status (-1); running waits are
never evicted. A cleanup error does not replay the exit callback. Start failures
also retire runtime-owned process and monitor state.

## Reproduce and build

Use a checkout and build directory on a volume with room for Swift dependencies.
Run `apply-runtime-patch.sh /path/to/apple-container-1.6.1`, then in that checkout:

```sh
swift test -j 2 --filter ProcessExitRegistryTests
swift build -c release -j 2 --product container-runtime-linux
```

These commands do not install a runtime. Sign the resulting executable with the
existing runtime's entitlements and the operator's existing identity. Never
unlock a keychain from a script; if needed, unlock it interactively in Keychain
Access. Preserve the original signed executable for rollback. Stop affected
containers and the container service before replacing the executable through
the operator's runtime installation mechanism. Do not overwrite a running
runtime or change its identity/privacy grants. A Homebrew reinstall may replace
the patch; rebase and retest before applying it to another release.

After installation, run a disposable idle container and obtain its *host*
`container-runtime-linux` PID, then:

```sh
python3 probe_exec_resources.py test-container RUNTIME_PID
```

The probe runs only `/bin/true`, compares heap type counts, detects runtime
restarts, and prints no task data or heap contents. Unrelated exec activity can
skew results. A passing sample is not a claim of all-path memory acceptance.

## Existing Multica containers: enable init without losing data

New managed containers use `--init`. An existing container is not migrated by
Muster. At an attended maintenance window:

1. Finish active tasks, turn Muster keep-alive off, and preserve the original
   `container inspect` result privately. Record the name, image, CPU/RAM,
   environment, init command, mount modes, networking, DNS, ports, user and
   working directory. Inventory output may contain credentials; do not publish it.
2. Stop the container. Commit its writable layer to a unique local rollback
   image with `container commit CONTAINER IMAGE`. Back up bind-mounted data
   separately: a commit does not include that data. Verify the snapshot/image
   exists before any destructive step.
3. Create a *different*, temporary container from that image with `--init` and
   the recorded settings. Do not run its scheduler concurrently against the
   live mounts. Validate init/reaping using an isolated test mount and a no-op
   command. Keep the original container stopped and intact for rollback.
4. Only after the snapshot and configuration have been verified, recreate the
   original name with `--init`, using the saved image and exact settings. This
   is an explicit operator replacement, not an app action. If uncertain about
   an option or mount, stop the migration instead of guessing.
5. Start it, verify Multica login/daemon, provider wrappers and a real task, then
   restore the original keep-alive setting. Confirm orphaned test children are
   reaped. Keep the rollback image until acceptance is complete.

For rollback, stop the replacement and recreate the original configuration from
the retained image, without `--init`. Bind-mounted data must be restored from
its separate backup if a task changed it. Do not delete the old image or any
volume as part of routine verification.
