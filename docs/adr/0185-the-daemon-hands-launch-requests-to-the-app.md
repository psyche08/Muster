# ADR 0185: The daemon hands launch requests to the app

- Date: 2026-11-11
- Status: Accepted; source, tests and a scratch-bundle runtime check verified;
  installation on the owner's Mac pending
- Related: [ADR 0164](0164-the-daemons-main-thread-serves-the-window-server.md)
  (the daemon's main thread and its Apple Event handling),
  [ADR 0132](0132-restore-the-daemons-background-registration.md) and
  [ADR 0127](0127-the-daemon-stays-out-of-launch-services.md) (the daemon's
  Launch Services record), [ADR 0125](0125-screen-recording-is-the-bundles-and-only-a-restart-takes-it.md)
  (why the executable stays in `Contents/MacOS`),
  [desktop and iOS updates](../DESKTOP_UPDATES.md)

## Context

The owner quit Muster and opened it again 20 seconds later. No window
appeared, and no app process started. The system log showed Launch Services
resolving the open request to the record of the still-running `musterd`,
making that record a foreground application, and delivering the request
there. The daemon has no window, so the request ended there. Afterwards the
daemon's record stayed a regular application until the daemon restarted. One
`open -n /Applications/Muster.app` brought the window back.

`musterd` runs from `Muster.app/Contents/MacOS`, so Launch Services records
it under the app's bundle id (ADR 0132). Since ADR 0164 its main thread
serves window-server events but installs no Apple Event handlers, so a
request that reaches it is answered "not handled".

A throwaway `.app` reproduced this. Its `CFBundleExecutable` is a small
AppKit GUI, and a headless second executable in `Contents/MacOS` does what
`musterd` does: `sharedApplication`, policy prohibited, and the same event
loop. Results:

| Situation | Where the request went | What arrived |
| --- | --- | --- |
| Daemon starts | the daemon | `aevt/oapp`, as for every registering process |
| `open` the bundle, no GUI running | the daemon, made regular first | `aevt/rapp` |
| `open` the bundle, GUI started after the daemon with `open -n` | the daemon, made regular first | `aevt/rapp` |
| `open -a` the bundle with a URL | the daemon, made regular first | `GURL/GURL` |

So the daemon's record wins even while an app process is running, and a
plain `open` issued from the daemon would come back to it. The probe's
handlers then showed a fix that works in both cases. They forwarded the
event to a running GUI by pid, or started one with `open -n`, and set the
policy back to prohibited. The GUI received the reopen and the URL, the
daemon's record returned to `BackgroundOnly`, and no Automation prompt
appeared.

## Decision

**When Launch Services delivers a reopen request or a URL to `musterd`'s
record, the daemon hands it to the app and leaves the Dock again.**

- `AppEvents::hand_launches_to_the_app()` installs Apple Event handlers for
  `aevt/rapp` and `GURL/GURL` on the main thread, before
  `AppEvents::serve()`. `musterd` logs whether they were installed. The code
  is in `muster_platform::macos::handoff`.
- The app is the bundle that holds the daemon's executable (`proc_pidpath`),
  and the executable that its `Info.plist` names as `CFBundleExecutable`.
  `CFBundleCopyExecutableURL` cannot be used: for the main bundle it returns
  the running executable, which is the daemon. Outside a bundle, or when the
  plist names the daemon itself, the request is logged and ignored.
- If a process runs that exact executable, the same event is sent to its pid
  (`typeKernelProcessID`, `kAENoReply | kAENeverInteract`), and the app is
  activated with all its windows. SwiftUI's `WindowGroup` opens a window on
  reopen when none is visible, and `onOpenURL` receives the link. A process
  that is still starting cannot take Apple Events yet (`procNotFound`, -600).
  A reopen it misses is dropped, since the starting app opens its window. A
  URL is retried off the main thread until the app takes it, for at most 10
  seconds.
- If no app process runs, `/usr/bin/open -n <bundle>` starts one. For a URL
  the command is `open -n -a <bundle> <url>`. A plain `open` would resolve to
  the daemon's record again. The URL is passed only if it is an absolute URL
  with a valid scheme and no control characters, so `open` cannot read it as
  an option.
- While that `open` runs, a second request starts nothing. A reopen is
  dropped, because the new app opens its window anyway. A URL is queued and
  forwarded once the app's process appears, waiting at most 10 seconds.
  Without this, a double click on the Dock tile could start two instances.
  The app does not guard against a second instance.
- After either path the handler sets the policy back to prohibited, then
  reads it back. When two requests arrived together, the setter once
  returned `NO` although the policy ended prohibited, so its answer is not
  used.
- `aevt/oapp` stays unhandled. A daemon that launchd starts at login opens no
  window. `quit` stays unhandled as well (ADR 0164).
- The log names the request kind and the outcome, never the URL.

## Alternatives

- **Keep the daemon out of Launch Services.** This was tried in ADR 0127 and
  reversed in ADR 0132, because the daemon registers anyway. ADR 0125 keeps
  the executable in `Contents/MacOS`. A nested helper bundle with its own
  bundle id would change the executable's path, and the daemon's TCC grants
  with it.
- **`-[NSApplication run]` with a delegate.** This would get reopen handling
  for free. ADR 0164 rejected it because its default `quit` handler ends the
  daemon without its shutdown.
- **Forward with a plain `open`.** Measured: it resolves to the daemon's
  record again.
- **Document `open -n` and change nothing.** The Dock, Finder and Spotlight
  cannot pass `-n`. Those are the usual ways to open the app.

## Consequences

- Opening Muster from the Dock, Finder, Spotlight or `open` shows a window,
  whichever record Launch Services picks. A `muster:` link reaches the app the
  same way.
- The daemon's record becomes regular only while a request is delivered. It
  no longer stays regular until the daemon restarts.
- The forwarded event is sent with `kAENeverInteract`. If a signed build ever
  needed Automation consent for a sibling process of the same bundle, the
  send would fail and be logged, and activation would still bring the app
  forward. The scratch bundle needed no consent; a signed release has not
  been checked.
- Daemons from before this change still swallow these requests. On those
  Macs, `open -n /Applications/Muster.app` starts the app when the daemon is
  running without it.

## Verification

- Source and tests: `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, `src/muster/scripts/check-layers.py`,
  `scripts/check-public-content.py`, `scripts/check-adr.sh origin/main`. New
  unit tests:
  - The app is found from the `CFBundleExecutable` that its `Info.plist`
    names, and never as the daemon itself.
  - `open` gets `-n`, and `-a` only with a URL.
  - Each request is forwarded as the event it arrived as.
  - Only absolute URLs with a valid scheme are passed on.
  - `pids_by_executable` matches an exact path.
- Runtime, scratch bundle: the probe `.app` from the Context, with an
  unsigned debug `musterd` without `sep` in place of the probe's daemon. It
  ran with a scratch state directory and no vnet, agent, cloud or terminal.
  The screen was locked during this run.
  - At start the daemon read policy prohibited, and no GUI started.
  - `open` with no GUI: one GUI started, and the daemon was prohibited again.
  - `open` with the GUI running: the GUI received the reopen, with no second
    instance.
  - A URL with the GUI running reached the GUI by forwarding. A URL with no
    GUI reached a new GUI through `open -n -a`.
  - Three `open`s at once with no GUI were repeated three times. Each time,
    one GUI started and received one reopen, and one reopen was dropped as
    starting. An `open` followed 50 ms later by a URL started one GUI, which
    received the URL.
  - After every case the daemon read policy prohibited. SIGTERM logged
    `musterd stopping`.
- An earlier run of the probe's own daemon, with the screen unlocked, found
  that activation succeeded. In this run activation reported `false`; the
  screen was locked.
- Not done here: a signed `sep` release build, installation on the owner's
  Mac, clicks on the Dock tile, and a `muster:` link to the real app. The
  late-URL retry (`procNotFound` on a URL) was not triggered. Install
  acceptance: with the app quit and the daemon running, opening Muster from
  the Dock shows its window, and the daemon stays out of the Dock.
