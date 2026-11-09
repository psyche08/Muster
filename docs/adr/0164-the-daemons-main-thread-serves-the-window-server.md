# ADR 0164: The daemon's main thread serves the window server's event queue

- Date: 2026-11-09
- Status: Accepted; source, tests and a scratch-bundle runtime check verified;
  installation on the owner's Mac pending
- Related: [ADR 0132](0132-restore-the-daemons-background-registration.md)
  (whose `TransformProcessType` call this replaces),
  [ADR 0123](0123-the-daemon-leaves-the-dock.md) (the Dock tile),
  [ADR 0127](0127-the-daemon-stays-out-of-launch-services.md) (the duplicate
  Launch Services record), [ADR 0125](0125-screen-recording-is-the-bundles-and-only-a-restart-takes-it.md)
  (why the executable stays in `Contents/MacOS`)

## Context

The owner found `musterd` marked "Not Responding" in Activity Monitor on
1.1.27 (117), after ADR 0132 had removed its Dock tile. The daemon was not
hung. Measured on the installed daemon, 2.5 days after it started:

- Launch Services held its record under `dev.linsheng.muster`,
  `type="BackgroundOnly"`, checked in about one second after launch.
- The window server, asked the way Activity Monitor asks
  (`CGSEventIsAppUnresponsive` for the process's serial number, from a small
  probe linking the private SkyLight entry points), reported a connection for
  the daemon and **unresponsive = true**. The app and Finder read `false`.
  `l4forwarder`, which lives in `Contents/Helpers`, has no record at all.
- `sample` put the main thread in every one of 1782 samples inside tokio's
  `CachedParkThread::block_on`, parked on `pthread_cond_wait`. The worker
  threads were busy with Accessibility walks and SQLite reads, and
  `muster status` answered normally.

A registered process gets a window-server connection, and the window server
expects that connection's event queue to be read on the main thread. A main
thread parked in `block_on` never reads it, so after a few seconds the window
server flags the process unresponsive. Activity Monitor reports that flag for
any registered process, whatever its type. ADR 0123 and ADR 0132 changed only
the type, which decides the Dock tile and not responsiveness.

A throwaway `.app` with a headless executable in `Contents/MacOS`, its work
on a background thread, read through the same probe:

| main thread | registered | ~18 s later | quit Apple Event |
| --- | --- | --- | --- |
| parked, nothing else | no | — | — |
| parked, after `CGWindowListCopyWindowInfo` | no | — | — |
| parked, after `TransformProcessType` (the daemon today) | yes | **unresponsive** | — |
| `TransformProcessType`, then `CFRunLoopRun()` | yes | **unresponsive** | — |
| `-[NSApplication run]`, policy prohibited | yes, `BackgroundOnly` | responsive | **exits** |
| `nextEventMatchingMask:` + `sendEvent:` loop | yes, `BackgroundOnly` | responsive, 0% CPU | keeps running |
| `nextEventMatchingMask:` only | yes, `BackgroundOnly` | responsive, 0% CPU | keeps running |

One reading here differs from the comment ADR 0132 restored: in the
throwaway bundle, `TransformProcessType` registered the process at once (the
record was there at +3 s). ADR 0132's measurement that the installed daemon
registers at t+2.6 s *without* the call still stands. Both are true, so the
daemon is registered either way, and the fix has to be on the main thread's
side, not in avoiding the registration.

## Decision

**When `musterd` runs from an `.app`'s `Contents/MacOS`, its main thread
reads the window server's event queue and the tokio runtime runs on its own
thread.**

- `muster_platform::macos::events::AppEvents::take()` returns the shared
  `NSApplication` only on the main thread of an executable whose kernel path
  (`proc_pidpath`) is inside an `.app`'s `Contents/MacOS`, which is the layout
  Launch Services registers. Anywhere else (a `cargo` build, a test harness
  thread, Linux) `musterd` runs as before, with `block_on` on the main thread.
- `AppEvents::leave_the_dock()` sets the activation policy to prohibited,
  which Launch Services records as `BackgroundOnly`. This gives ADR 0132's
  outcome without `TransformProcessType`, and
  `muster_platform::macos::procs::leave_the_dock` is removed. A refusal is
  logged as a warning, as before.
- `musterd`'s `main` spawns `musterd-runtime` (8 MiB stack, the size of the
  main thread the daemon's future ran on until now) and the main thread
  calls `AppEvents::serve()`. That is a loop of
  `nextEventMatchingMask:untilDate:distantFuture…` and `sendEvent:`, with one
  autorelease pool per event. It blocks in the run loop while idle.
- It is not `-[NSApplication run]`. `run` installs AppKit's default Apple
  Event handlers, and its `quit` handler would end the daemon without
  `daemon.shutdown()` whenever a quit aimed at "Muster" resolved to the
  daemon's record, which `NSRunningApplication` lists first (ADR 0127).
- The runtime thread ends the process with `std::process::exit`, using the
  exit status `main` returning its result would have produced: 0 on success;
  `Error: {e:?}` on stderr and 1 on an error; 101 after a panic, which
  `catch_unwind` stops from leaving the main thread serving events for a dead
  daemon.

Invariants: whenever Launch Services has registered the daemon, its main
thread reads the event queue; its record stays `BackgroundOnly`; the
executable path, and with it the daemon's TCC grants, does not change.

## Alternatives

- **`CFRunLoopRun()` on the main thread.** Measured: still unresponsive. A
  run loop alone does not dequeue window-server events.
- **`-[NSApplication run]`.** Responsive, but a quit Apple Event ended the
  probe within 4 s. For the daemon that would mean an abrupt exit, skipping
  the shutdown that SIGTERM runs, and then a launchd restart.
- **Avoid the registration.** Moving the executable out of `Contents/MacOS`
  is ruled out by ADR 0125. The trigger that registers it without the call is
  unknown (ADR 0132), and an embedded `__info_plist` was rejected in ADR 0127.
- **The `objc2` and `objc2-app-kit` crates.** These are a new dependency for
  five messages. The raw runtime calls stay in one module with their
  signatures spelled out.
- **Leave it.** A permanent "Not Responding" sends the owner looking for a
  hang that is not there. ADR 0123 rejected leaving the Dock tile for the
  same reason.

## Consequences

- Activity Monitor stops reporting the daemon as not responding. There is
  still no Dock tile. Launch Services still holds two records for the bundle
  id, as it has since ADR 0132.
- The main thread now belongs to AppKit and the daemon's top-level future
  runs on `musterd-runtime`. Nothing in the daemon needs the main thread: its
  Accessibility, Core Graphics and ScreenCaptureKit calls already ran on
  tokio workers.
- A quit Apple Event sent to the daemon is now dequeued and answered with "not
  handled" instead of timing out. The daemon keeps running, as it did before.
  Shutdown remains SIGTERM from launchd, or Ctrl-C.
- AppKit is never "launched" (no `finishLaunching`, delegate or menu). It only
  hands events to `sendEvent:`.
- When the runtime thread calls `exit`, the main thread's stack is not
  unwound. Nothing is owned there.

## Verification

- Source and tests: `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, `src/muster/scripts/check-layers.py`,
  `scripts/check-public-content.py`, `scripts/check-adr.sh origin/main`. New
  unit tests:
  - `AppEvents::take` refuses off the main thread and outside a bundle.
  - `exit_code` maps success, an error and a panic to 0, 1 and 101.
- Runtime, scratch bundle: an unsigned debug build without `sep`, copied into
  a throwaway `.app`'s `Contents/MacOS` and run with a scratch state
  directory and no vnet, agent or terminal.
  - At +34 s it was registered with a window-server connection and read
    `unresponsive = false`, `BackgroundOnly`, 0.0% CPU.
  - `sample` showed the main thread in `AppEvents::serve` →
    `nextEventMatchingMask:`.
  - After a quit Apple Event (`NSRunningApplication.terminate()`) it was still
    running and still responsive.
  - SIGTERM logged `musterd stopping` and it exited 0.
  - A startup error (an overlong socket path) printed `Error: path must be
    shorter than SUN_LEN` and it exited 1.
  - The same binary run outside a bundle was never registered and exited 0
    on SIGTERM.
- Not done here: a signed `sep` release build, installation on the owner's
  Mac, and an Activity Monitor reading of the installed daemon. The scratch
  daemon was not Accessibility-trusted and did no agent work. Install
  acceptance: with the agent watchers running, the installed daemon reads
  responsive for more than a minute, and Activity Monitor does not mark it.
