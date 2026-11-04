# ADR 0123: The daemon leaves the Dock instead of moving out of the bundle

- Date: 2026-11-04
- Status: Accepted; source verified, install verified on the owner's Mac
- Related: [ADR 0006](0006-desktop-presence-and-console-routing.md) (the
  bundled daemon's stable identity), [ADR 0122](0122-wait-for-the-app-to-exit-before-replacing-it.md)
  (the other way this bundle stranded a process in the Dock)

## Context

The owner found a second Muster in the Dock marked "Application Not
Responding". It was not the app. Launch Services held two foreground
registrations for `dev.linsheng.muster`:

```
152) "Muster"  executable=…/Contents/MacOS/Muster    pid 43867  type="Foreground"
154) "Muster"  executable=…/Contents/MacOS/musterd   pid 43917  type="Foreground"
```

The first is the app, healthy. The second is the daemon.

`musterd` is installed at `Muster.app/Contents/MacOS/musterd` — the bundle's
main executable folder — and its LaunchAgent runs it through
`BundleProgram = "Contents/MacOS/musterd"`. Launch Services therefore treats it
as an instance of the app and registers it in the foreground, with a Dock tile.
The bundle's `Info.plist` sets neither `LSUIElement` nor `LSBackgroundOnly`,
and it could not: those keys belong to the bundle, so setting one would hide
the real app as well.

A headless Rust binary has no event loop to service, so the window server never
hears from the tile it was given, and the Dock says the application is not
responding. It has presumably said so for as long as the daemon has shipped
inside the bundle. Nothing is actually wrong with the daemon: measured over ten
seconds it used 22.5% CPU, against 30.9% for the older build on another Mac in
the same fleet.

The same bundle already shows what the alternative looks like:
`Contents/Library/LaunchServices/muster-helper` and the three helpers in
`Contents/Helpers` never appear in the Dock.

## Decision

**The daemon transforms itself out of the Dock at startup; the executable stays
where it is.**

`muster_platform::macos::procs::leave_the_dock` calls
`TransformProcessType(&{0, kCurrentProcess}, kProcessTransformToBackgroundApplication)`.
`musterd` calls it once, immediately after the logger is initialised. A failure
is logged at warning level and startup continues: a Dock tile is cosmetic and
is not worth refusing to run over.

Background, not `kProcessTransformToUIElementApplication`: this process has no
interface to element-ise and nothing ever brings it forward.

Only the daemon path. `--locked-use-watchdog` and the notice subcommand run
from the same binary but return before the runtime is built, and they are
short-lived; the watchdog in particular observes the cover, and taking it out
of the foreground was not something this decision needed to reason about.

## Alternatives

- **Move `musterd` to `Contents/Helpers` or `Contents/Library/LaunchServices`.**
  The conventional layout, and what the rest of this bundle does. Rejected
  because TCC grants are keyed to the executable's path: moving it would
  invalidate the daemon's Accessibility grant, and the owner would have to
  re-approve it before Locked Use and AX automation worked again.
  `install-mac.sh` says as much in its own header — "TCC and launchd keep a
  stable identity for the bundled daemon". The owner chose this decision over
  that one with the cost stated.
- **`LSUIElement` in the bundle's `Info.plist`.** It would hide the app too.
- **A second, daemon-only bundle inside the app.** It would carry its own
  `Info.plist` and so its own `LSUIElement`, and would also change the
  executable path, which is the thing being avoided.
- **Leave it.** It is only a Dock tile — but a permanent "not responding" next
  to the real app is exactly what made the owner go looking for a hang twice in
  one evening.

## Consequences

- The daemon no longer appears in the Dock or the ⌘-Tab switcher.
- Its path, code-signing identity and therefore its TCC grants are untouched.
  No re-approval.
- A background application cannot bring itself to the front. The daemon never
  did; anything user-facing it needs goes through the app or the notice
  subcommand, which are separate processes and unaffected.
- The transform happens after launch, so there is a brief moment at startup
  where the tile exists. It is not visible in practice and no decision depends
  on it.
- The watchdog subprocess is still registered the old way while it runs.

## Verification

`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D
warnings`, `cargo test --workspace`.

Installed on the owner's Mac, 2026-11-04. Launch Services still holds two
`dev.linsheng.muster` registrations — the daemon does not disappear from its
records — but the daemon's type changed, which is what decides the Dock tile:

```
before: …/Contents/MacOS/musterd  pid 43917  type="Foreground"
after:  …/Contents/MacOS/musterd  pid 52120  type="BackgroundOnly"
```

The app's own registration is untouched at `type="Foreground"`. The daemon
keeps working from the background: `muster status` answers, it reports its own
commit, and nothing in its log mentions an Accessibility denial, which is the
grant that moving the executable would have cost.
