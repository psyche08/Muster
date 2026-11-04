# ADR 0122: Wait for Muster to exit before replacing its bundle

- Date: 2026-11-04
- Status: Accepted; source verified, install verified on the owner's Mac
- Related: [ADR 0121](0121-generated-xcode-project-follows-project-yml.md)
  (the same script, the same kind of silent damage),
  [ADR 0006](0006-desktop-presence-and-console-routing.md) (what the installed
  bundle registers)

## Context

`install-mac.sh` replaces a running copy of the app:

```sh
pkill -x Muster
launchctl bootout "gui/$(id -u)/dev.linsheng.musterd"
rm -rf /Applications/Muster.app
ditto "$app" /Applications/Muster.app
```

`pkill` sends SIGTERM and returns immediately. It does not wait, and a SwiftUI
app takes a moment to finish. `rm -rf` ran right behind it, so an app that had
not yet gone lost its bundle while it was still running.

That happened on 2026-11-04, during the third install of the day. The surviving
process kept its AppKit event loop — sampled, it sat in `mach_msg2_trap`
waiting for the next event at 1.6% CPU, which is exactly what an idle app looks
like — but Launch Services no longer had a bundle for it:

```
pid = 26089 !cgsConnection !signalled  bundleID=[ NULL ]
bundle path=[ NULL ]  executable path=[ NULL ]  Arch=!!none
```

With no window-server connection the Dock showed it as "Application Not
Responding", permanently. Worse, it was the only Muster GUI process on the
machine: the newly installed app was not running at all, and what the owner saw
in the Dock was the stranded shell of the one it replaced. The daemon was
unaffected — it is a separate launchd agent — so every command-line check kept
reporting a healthy install.

The failure is silent in both directions. The script exits 0, and the stranded
process looks idle rather than broken.

It is also systematic rather than unlucky. The script ends by relaunching the
app, so a second install always finds a running copy — the one the first
install started — and every install after the first raced it. Three installs
in a row on 2026-11-04 is what produced the stranded process.

## Decision

**The script waits for the app to leave before deleting its bundle, and
refuses to delete it if the app will not go.**

- After `pkill -x Muster`, poll `pgrep -x Muster` every 0.5 s. `-x` matches the
  app alone; `musterd` and `muster-browser-host` are different names and are
  not waited on.
- At 5 s (iteration 10), escalate once to `pkill -KILL -x Muster`.
- At 10 s (past iteration 20), print what is wrong and **exit 1** without
  touching `/Applications/Muster.app`. Leaving the old install in place is
  better than stranding a process and shipping a bundle nobody is running.
- The daemon's `launchctl bootout` stays after the wait, so the app is gone
  before the agent it registered is torn down.

## Alternatives

- **`pkill` then a fixed `sleep 2`.** Shorter to write and wrong at both ends:
  it still races a slow quit, and it costs two seconds on every install where
  the app was not running at all.
- **`osascript -e 'quit app "Muster"'`.** A polite quit that blocks until the
  app goes, but it needs Apple-events permission for whoever runs the script,
  which an installer should not require.
- **Replace the bundle in place with `ditto` and no delete.** `ditto` merges
  rather than replaces, so files dropped from the app would survive forever.
- **Let the app be stranded and relaunch it afterwards.** The stranded process
  is the problem, not the missing relaunch; a second copy against a replaced
  bundle is how this started.

## Consequences

- An install that finds the app running takes up to 10 s longer, and in the
  normal case where it quits on SIGTERM, about half a second.
- An install can now fail where it used to appear to succeed. That is the
  point: the message names the cause and nothing has been changed when it
  fires.
- `set -e` interacts badly with `[ … ] && cmd` as a statement, so the
  escalation is written as an `if`. Empirically `/bin/sh` on macOS 27 does not
  abort on the false AND-list, but the `if` does not depend on knowing that.
- Nothing about the app's behaviour changes; this is the installer only.

## Verification

The loop was exercised with a stubbed `pgrep`/`pkill` over four cases: the app
already gone (0 iterations, no signal), the app leaving during the wait (no
SIGKILL), the app surviving past 5 s (SIGKILL once, at iteration 10, not
repeated), and the app never leaving (exit 1, bundle untouched). `sh -n` on the
script.

End to end on the owner's Mac, 2026-11-04, with the app deliberately running
first: Muster was pid 29390 before the install; `install-mac.sh` completed in
57 s with exit 0; pid 29390 was gone afterwards and the script's own relaunch
left exactly one Muster GUI process, pid 43867, registered as
`bundleID="dev.linsheng.muster"`, `type="Foreground"`, `Version="111"`,
`Arch=ARM64`, with no `!cgsConnection`. That record is what the stranded
process did not have.
