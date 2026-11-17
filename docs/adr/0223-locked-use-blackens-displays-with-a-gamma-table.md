# ADR 0223: Locked Use blackens the displays with a gamma table

- Date: 2026-11-17
- Status: Accepted; device verified
- Related: supersedes [0222](0222-locked-use-dims-displays.md) (DDC
  dimming); [LOCKED_USE.md](../LOCKED_USE.md),
  [0221](0221-multica-workspace-folder-sessions.md) (the locked Claude first
  turn), [0128](0128-the-panel-helper-keeps-its-own-identity.md) (the panel
  helper), [0171](0171-mobile-display-blackout.md) and
  [0007](0007-desktop-manual-locked-use.md) (the other black covers)

## Context

ADR 0222 took the black panel off a display once the watchdog had dimmed it to
brightness zero, because the panel occludes the Claude app's window and an
occluded Electron window stops updating what Accessibility reads. External
displays were dimmed over DDC/CI.

A locked Claude Multica first turn then ran under that design on the Mac
ADR 0222 tested, an Apple silicon Mac mini with one external display on HDMI.
The display was dimmed over DDC and lost its panel three seconds after the
unlock. Claude's new-session form, which had never appeared under the panel,
appeared and was verified 3.8 seconds after admission. Two seconds after the
dimming, the kernel reset the display connection, macOS showed a different
display for about a second, and the cover counted as lost. The transaction
stopped and relocked before anything was submitted.

So removing the occlusion works, but the DDC traffic that keeps a dimmed
display checked does not. The watchdog read the level back every 250 ms
(ADR 0222, decision 3). On this Mac the kernel logs a failed payload
verification and a retrained link many times a minute with or without DDC;
I2C requests over the same connection made a reset likely. Reading back less
often would leave a person's change of level unseen for longer, and not
reading back at all would leave the cover unchecked.

The owner decided on 2026-11-17 to use a gamma table instead.

## Decision

1. **A black table instead of brightness zero.** Once the console has been
   unlocked and every display has stayed as it is for three seconds (awake,
   and online under the same ID), the watchdog sets each display's transfer
   table to all zeros (`CGSetDisplayTransferByTable`) and reads it back
   (`CGGetDisplayTransferByTable`). A display whose table reads back black,
   every entry at most 1/1024, loses its panel. A display that cannot be set
   or read back black gets its previous table again and keeps the panel. A
   mirror set has its panel on the display it shows on.
2. **Unchanged from ADR 0222:** the black panel on every display first, with
   the displays that are asleep woken, and the raise fails unless every
   display is covered; a display that cannot be woken keeps its panel; only
   the Locked Use cover changes, and the desktop privacy cover (ADR 0007) and
   the phone's blackout (ADR 0171) stay black panels.
3. **Health.** On every watchdog check the online displays must be the same
   ones, each black display's table must still read back black, and each
   panel is checked as before. Anything else is a lost cover, which stops the
   transaction and relocks. Nothing is sent to a display; the check reads the
   window server's table.
4. **Putting tables back.** At release the watchdog has ColorSync put every
   display's table back (`CGDisplayRestoreColorSyncSettings`) and removes the
   panels. When the watchdog ends any other way, killed included, macOS puts
   the tables back itself. There is no record, no restore helper and no
   signal handling. Once the transaction stops, no table is set any more.
5. **Reporting.** The `D` frame on the watchdog's status pipe now carries two
   counts: displays with a black table, and panels. The `L` frame names a
   change of displays, a replaced table or a failed panel. The `S` frame
   reports displays that could not be woken.
6. **Removed:** DisplayServices and DDC/CI, the brightness record
   (`locked-use.brightness`), its lock file (`locked-use.restorer`), the
   panel helper's `--locked-use-restore` mode, and musterd's calls to it.

## Alternatives

- **Keep DDC and read back less often:** fewer requests, but the same
  requests on the same connection, and a person's change of level unseen
  for longer.
- **Dim over DDC once and never read back:** leaves a display whose level was
  raised at its own buttons uncovered without anyone noticing.
- **Brightness zero for displays macOS controls, a table for the others:** a
  built-in display at zero may still show a picture, and two mechanisms need
  two checks; a table works the same on both.
- **The black panel on every display:** occludes the Claude app's window
  (ADR 0222).
- **A translucent panel, display sleep, or turning off the Claude app's
  throttling:** rejected in ADR 0222 for the reasons recorded there.

## Consequences

A blackened display shows black, not a dim picture, and a person at it cannot
make it brighter with its own buttons.

The Claude app's window is no longer occluded on a blackened display, from
about three seconds after the unlock, as under ADR 0222. Until then, and for
the whole transaction on a display that keeps its panel, the windows are
occluded as before.

The table also blacks out the lock screen. Between the stop and the release
the console is locked again under a black table, as it was under the panels.
A watchdog that lost its owner keeps the displays black until the console
changes, as it kept its panels.

Another program that sets a display's table while the cover is up (a
colour-calibration or night-light tool, for example) replaces the black
table. The next check finds it, and the transaction stops. Releasing the
cover puts back ColorSync's tables, so such a program's own table is lost
until it sets it again.

A screenshot does not show the table: screen capture sees the picture
before the table is applied. The read-back is the only evidence
that a display is black, and the owner's eyes the only other one.

No window lies over the apps on a blackened display. A local click lands in
an app instead of on the panel. Local input still stops the transaction.

A Mac upgraded from ADR 0222 may keep an empty `locked-use.restorer` file in
musterd's state directory. Nothing reads it.

## Verification

- Local checks: library tests of `muster-platform`. A fault frame names what
  failed; the displays count as ready only when unchanged for three seconds,
  unlocked, awake and the same set. Strict Clippy with `--no-deps` on
  `muster-platform`, `muster-acp`, `muster-agent` and `musterd`.
- Device, 2026-11-17, the Mac of the Context, locked, its display asleep.
  A probe set the all-zero table and read back 0 while the display slept,
  held it three seconds, and ColorSync put back 1.0. When the probe exited
  without putting it back, and when it was killed with `SIGKILL` while
  another process read 0, the next read was 1.0: macOS put the table back.
- Device, a locked Claude first turn, display asleep: the panel went up and
  the display woke. Black table, panel off 2.2 seconds after admission;
  another process read the table as 0. Claude's form verified at 4.1
  seconds, submitted at 4.7, applied; the task completed. The table was 1.0
  again at release, the display went back to sleep, and the kernel logged no
  display unplug during the transaction.
- Device, two locked Codex first turns through the in-app route, one with
  the display asleep (woken under the panel) and one with it awake. Black
  table, panel off 2.4 and 2.2 seconds after admission; submitted at 5.1 and
  4.3 seconds; both reconciled from Codex's transcripts and completed. The
  tables were 1.0 again at release, and the kernel logged no display
  unplug.
- Device, before those runs: two Codex runs and one Claude run were refused
  before submission because a composer held an unsent draft. The cover went
  up, the table was set where the run lasted long enough, and the cover came
  down with the table back at 1.0.
