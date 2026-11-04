# ADR 0125: Screen Recording is the bundle's, and only a restart takes it

- Date: 2026-11-05
- Status: Accepted; source and tests verified, device evidence recorded
- Related: supersedes the Screen Recording restart rule and the terminal
  source choice of [ADR 0124](0124-prompt-for-screen-recording-and-move-terminal-authorization.md);
  [ADR 0116](0116-failure-screenshot-analysis.md) (the capture that needs the
  grant), [ADR 0123](0123-the-daemon-leaves-the-dock.md) (the daemon inside
  the app bundle)

## Context

ADR 0124 landed and was installed on the owner's Mac. Driving the real
permission through System Settings contradicted two things it assumed.

**A running daemon cannot observe its own grant changing.** ADR 0124 said
macOS answers a Screen Recording preflight with the new grant at once, and
made `missing` -> `granted` set `desktop_restart_required`. Measured on the
installed build, `CGPreflightScreenCaptureAccess` answers from what the
process cached at launch, in **both** directions:

| step | musterd | reported |
| --- | --- | --- |
| grant in place | pid 97587, uptime 293s | `granted` |
| `tccutil reset ScreenCapture dev.linsheng.muster` | same process | `granted` |
| daemon restarted | fresh process | `missing` |
| owner switches the grant back on | same process, uptime 293s | `missing` |
| daemon restarted | fresh process | `granted` |

So the transition ADR 0124 waited for is never observed. The dialog would
have gone on asking for a grant the owner had already given, and the restart
that actually makes it take was offered only on a condition that could not
occur.

**The grant belongs to the app bundle, not to the daemon executable.**
System Settings lists `Muster.app` and `musterd` as separate rows, and ADR
0124's dialog pointed at `…/Contents/MacOS/musterd`. With only the `musterd`
row switched on, a **freshly started** daemon reported `missing`; switching
`Muster.app` on as well, a freshly started daemon reported `granted`. The
single variable was the bundle's row. `tccutil` agrees: it refuses
`dev.linsheng.musterd` with "No such bundle identifier", while
`tccutil reset ScreenCapture dev.linsheng.muster` resets three entries at
once — the daemon's row is filed under the bundle. So the dialog was sending
the owner to switch on the one row that does not decide the answer.

A third thing came from using the Terminals page ADR 0124 created. This Mac
runs iTerm2, authorized, with its Python API available; Ghostty is installed
but not running. The page asked for Ghostty Automation anyway, because ADR
0124 carried over the older rule of showing "the next outstanding source".
A Mac that already offers terminals has no use for the other app's grant,
and being asked for one reads as a fault.

## Decision

**The restart is offered while the grant is missing, the dialog names the
bundle, and one authorized terminal source is enough.**

- `desktop_restart_required` goes back to the three permissions whose
  transitions the app can actually see; `screen_recording` leaves
  `observe_desktop_permissions`, and so does the test that asserted a
  transition that cannot happen.
- `desktop_setup` instead reports `restart_required` whenever the grant is
  missing: `state.desktop_restart_required || !screen_recording_ready`. The
  owner switches the grant on and presses the restart button that is already
  there; the new daemon reads the new answer, `screen_recording_ready`
  becomes true, and the dialog closes on its own.
- The message names `Muster` as the row to switch on, says the listed
  `musterd` is the same grant, and says plainly that macOS gives a new grant
  only to a new process, so the restart below is what makes it take. The
  dialog shows `/Applications/Muster.app`, not the daemon's path.
- `render::terminal_authorization` reports `ready` when **either** source is
  authorized (`iterm_ready || ghostty_ready`), and the section then speaks
  about the source that works. Only with nothing authorized does it ask, and
  then for the source Rust picked, falling back to whichever app is
  installed. An iTerm2 whose Python API is unavailable is not a working
  source, so a Ghostty grant can still be what makes the Mac ready.

## Alternatives

**Have the daemon re-read the grant without restarting.** There is nothing to
re-read: the preflight is the API, and its answer is fixed for the life of
the process. Apple's own prompt says "Quit & Reopen" for the same reason.

**Restart the daemon automatically when the grant is missing.** It would
restart on a schedule for a grant the owner may never give, dropping remote
connections each time. The restart is cheap and already has a button; what
was missing was offering it at the right moment.

**Keep pointing at `musterd` and ask the owner to switch on both rows.** The
dialog does mention both, because the daemon's row is what the owner sees
being added. But naming the bundle first is what the evidence supports, and
sending someone to the row that does not decide the answer is how ADR 0124
produced a grant that looked given and was not.

**Leave the terminal rule alone and let the owner authorize both apps.** Two
grants for one capability, where the second changes nothing a Mac already
has. The prompt would never clear for anyone who keeps a second terminal
installed.

**Prompt for the source that is not yet authorized whenever one exists.**
That is ADR 0124's rule, and it is what produced the Ghostty prompt on a Mac
driving iTerm2.

## Consequences

- The dialog now stays open on a Mac missing Screen Recording until the
  grant is given **and** the daemon restarts, which is the first point at
  which the capture actually works. That is a longer path than ADR 0124
  described, and it is the real one.
- `restart_required` no longer means only "a grant just landed"; it also
  means "a grant is missing and a restart is how it takes". The restart
  section's existing wording covers both, and `can_restart` still refuses
  while a send, terminal or Locked Use is in flight.
- A Mac with two terminal apps installed is asked about at most one of them,
  and never once it has a working source. Nothing authorizes less than
  before: the second app's Automation was never used while the first worked.
- ADR 0124's field layout, RPC, intents and the move of terminal
  authorization to the Terminals page all stand. Only its restart rule, the
  path it named, and its choice of which terminal source to prompt for are
  replaced here.
- The `musterd` row in System Settings is still what the owner sees being
  added when the daemon asks, so the message has to mention both rows. If a
  later macOS files the daemon separately, this needs measuring again.

## Verification

- **Source and tests**: `cargo test -p muster-app-core --lib` (156 passed),
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --all --check`, `scripts/check-layers.py`,
  `python3 scripts/check-public-content.py`. The dialog test now asserts the
  restart is offered while the grant is missing and withdrawn once it is not;
  three terminal tests cover an authorized source never asking for the other
  app, which source an outstanding grant names, and an iTerm2 without its
  Python API not counting as working.
- **Device evidence, on the owner's M1Max**, against the installed 1.1.21
  (111) build: both tables in Context were measured there, through
  `tccutil`, System Settings and `muster rpc device_info`. The daemon's
  signing identifier is `dev.linsheng.musterd`; `tccutil` refuses it as a
  bundle identifier and resets it under `dev.linsheng.muster`.
- **The corrected build, installed on the owner's M1Max**: with the grant
  revoked and the daemon restarted (`screen_recording: "missing"`,
  Accessibility granted, the plugin ready), the running app showed the
  dialog with the new message, `/Applications/Muster.app` as the path, and
  the restart section with its button — the restart being offered while the
  grant is missing, which is the correction this ADR makes. No terminal item
  appeared in the dialog.
- **Not verified**: pressing that button and watching the dialog clear, and
  the Terminals page under the new rule, were not driven on the device.
  Notarization, publication and the fleet's other Macs are untouched.
- **A separate defect, not from this change**: the app's windows are absent
  from the accessibility tree on this Mac, so the automation used for these
  checks could not see or drive them, and `app_menu` reported Muster as an
  agent process with no menu bar. The window itself is healthy —
  `CGWindowListCopyWindowInfo` shows it on screen at layer 0, the owner sees
  and uses it, and `screencapture -l` captured the evidence above. This
  predates the change (it appeared with the first install of ADR 0124's
  build) and is untouched here.
