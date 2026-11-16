# ADR 0222: Locked Use dims the displays instead of covering them

- Date: 2026-11-16
- Status: Accepted; source, tests and locked Codex device runs verified,
  locked Claude device run pending
- Related: [LOCKED_USE.md](../LOCKED_USE.md),
  [0128](0128-the-panel-helper-keeps-its-own-identity.md) (the panel helper),
  [0221](0221-multica-workspace-folder-sessions.md) (the locked Claude first
  turn), [0171](0171-mobile-display-blackout.md) and
  [0007](0007-desktop-manual-locked-use.md) (the other black covers)

## Context

While a Locked Use transaction has the Mac unlocked, its watchdog in the panel
helper keeps an opaque black panel above every window on every display. The
panel hides the desktop, and it also occludes every window under it.

A granted Claude first turn did not start under that cover. After "New", and
on an earlier day after the directory route, the Claude app's new-session
form never appeared in its accessibility tree within the Locked Use window
(ADR 0221). The Claude app turns its main view's background throttling on and
off by itself, and it was on. An occluded Electron window under throttling
stops updating what Accessibility reads. The ChatGPT app turns background
throttling off, and covered Codex creations completed.

The owner decided on 2026-11-16 to darken the displays instead: brightness
zero, external displays over DDC/CI as the m1ddc project does it, and the
black panel only for a display that supports neither.

The first device runs dimmed each display as the cover went up and put its
level back as the cover came down. On the Mac tested, an Apple silicon Mac
mini with one external display on HDMI, that failed in two ways:

- DDC requests sent while the console locked, unlocked or a display woke
  coincided with the kernel resetting the display connection
  (`DCPAVControllerProxy::validateMessage: Fixed payload verification
  failed!`, then `AppleDCPDPTXController::handleUnplugEvent`). For about a
  second macOS showed a different display, which the cover counts as lost. In
  two hours of black panels alone that evening there was no such reset.
- A level written back as the console locked again read back correctly, and
  the display then undid it. One display was left at zero.

The display also sets its own level again about two seconds after it wakes,
and a display goes to sleep about five seconds after the console locks.

## Decision

1. **Black panel first.** When the watchdog raises the cover, every online
   display gets the black panel, as before, and the displays that are asleep
   are woken. The raise fails unless every online display is covered. A
   display that cannot be woken keeps its panel for the whole transaction.
2. **Dim once unlocked and steady.** The watchdog dims only after the console
   has been unlocked and every display has stayed as it is for three seconds:
   awake, and online under the same ID and UUID. It then reads every level
   before it changes any:
   - through DisplayServices for a display macOS controls itself (built-in and
     Apple displays);
   - otherwise over DDC/CI, VCP 0x10, for an external display, found as m1ddc
     finds it on Apple silicon: the display's `IODisplayLocation`, its
     `IOMobileFramebuffer`, the external `DCPAVServiceProxy` that follows it,
     and `IOAVService` I2C at chip address 0x37 (0xB7 behind an MCDP29xx
     bridge). A reply counts only with the requested code, no error and a
     valid checksum.

   It writes the levels, keyed by the display UUID macOS keeps across
   reconnects, to `locked-use.brightness` (0600) in musterd's state directory.
   It holds an exclusive `flock` on that file for as long as it lives, and
   syncs the file before any level changes. A level an earlier cover recorded
   and nobody has confirmed back is kept rather than read again, since the
   display may still be at that cover's zero.

   It then sets each level to zero and reads it back. A display that reads
   back zero is dimmed and loses its panel. A display that cannot be read,
   set or read back as zero, or has no UUID, gets its level back and keeps
   the panel. If the levels cannot be recorded, every display keeps its
   panel. A mirror set has its panel on the display it shows on.
3. **Health.** On every watchdog check, the online displays must be the same
   ones, each natively dimmed display must still read zero, and each panel is
   checked as before. A thread reads each DDC display back every 250 ms and
   opens its I2C service again after a read fails. A level other than zero,
   or no valid answer for five seconds, is a lost cover. A lost cover stops
   the transaction and relocks, as a lost panel does.
4. **Quiet before the relock.** As soon as the transaction stops, before the
   watchdog locks the console, the DDC thread stops and nothing more is
   dimmed. No DDC request goes out while the console locks.
5. **Putting levels back.** At release the watchdog puts back each native
   level that still reads zero and removes the panels. A DDC level stays in
   the record. The panel helper puts it back (`muster-panel
   --locked-use-restore`), started by musterd when it starts, before each
   transaction, and when it drops a guard while the record exists. The helper
   waits until a display has stayed as it is for three seconds, awake and
   with the console neither locking nor unlocking. It then writes the level
   if the display still reads zero, reads it back, and reads it again a
   second later. Only then does it remove the entry. A level that someone
   changed in the meantime stays as they set it.
   - Only one helper restores at a time (an exclusive `flock` on
     `locked-use.restorer`); another one exits.
   - It waits while a watchdog holds the record, and while the display sleeps.
   - It gives up when no display in the record is online, and after twenty
     rounds in which a steady display did not take its level back. The entry
     stays in the record for the next helper.
   - Display calls run only on a process's main thread, so musterd never
     makes them itself. It logs each line the helper prints.
6. **A killed watchdog.** Its panels end with the process, but a brightness
   level does not, and a DDC level survives even a restart. The record keeps
   the level, the next cover takes it from there, and the helper puts it back.
7. **Reporting.** The watchdog tells its owner how many displays it dimmed
   natively, over DDC, and covered (a `D` frame on its status pipe), at the
   raise and again when it dims. A lost cover says what was lost (an `L`
   frame), and a cover that keeps every panel says why (an `S` frame).
   musterd logs all three.
8. **Scope.** Only the Locked Use cover changes. The desktop privacy cover
   (ADR 0007) and the phone's blackout (ADR 0171) stay black panels.

## Alternatives

- **The black panel on every display:** occludes the Claude app's window.
- **Dimming as the cover goes up and putting levels back as it comes down:**
  sends DDC requests while the display connection changes. On the Mac tested
  the connection reset, the cover was lost, and one level was undone.
- **A translucent panel:** does not occlude, but does not hide the screen
  either.
- **A black gamma table** (`CGSetDisplayTransferByTable`): works on any
  display, needs no DDC request and so no connection reset, and gives a black
  picture rather than a dim one. macOS restores the tables when the process
  ends. It was not what the owner chose. It remains the option if a dim
  picture is not private enough.
- **Display sleep:** the unlock wakes the displays.
- **Turning off the Claude app's throttling:** that setting is the app's.

## Consequences

The Claude app's window is no longer occluded on a dimmed display, from
about three seconds after the unlock. Until then, and for the whole
transaction on a display that falls back to the panel, the windows are
occluded as before.

A dimmed display is not black. At brightness zero, a DDC display shows its
minimum backlight, and many external displays then still show a dim picture.
A person at the display can raise the level with its own buttons; the
read-back catches that within about 250 ms and stops the transaction.
Whether a built-in display goes fully dark at zero was not tested; no Mac
with one was available.

No window lies over the apps on a dimmed display. A local click lands in an
app instead of on the panel. Local input still stops the transaction.

A DDC display stays dark for a few seconds after the console locks again,
until the helper has seen it steady. If it goes to sleep first, it stays at
zero until it wakes and has been steady for three seconds.

The kernel's `Fixed payload verification failed!` messages are frequent on
this Mac with or without DDC; only the connection resets went away.

A watchdog that loses its owner keeps the displays dark until the console
changes, as it kept its panels. A watchdog killed with SIGKILL leaves the
displays dark until musterd starts the helper again.

The helper now also calls `IOAVServiceCreateWithService`,
`IOAVServiceReadI2C`, `IOAVServiceWriteI2C`,
`CoreDisplay_DisplayCreateInfoDictionary` and the DisplayServices brightness
functions. All are private and looked up at run time; a macOS without one
leaves the display to the panel. None asked for a privacy permission.

## Verification

- Local checks: library tests of `muster-platform`. The record round-trips
  and drops lines it could not put back; a restorer leaves a record a
  watchdog holds, and only one restorer runs; a level that cannot be put back
  yet stays in the record, a pass that puts nothing back leaves the file as
  it was, and an empty record is removed; a holder that waited for a record
  its previous holder removed takes the new file; a later cover keeps the
  level an earlier one recorded; the displays count as steady only after
  three unchanged seconds, and as ready to dim only when unlocked, awake and
  the same set; a fault frame names what failed.
- Device, read only, 2026-11-16, the Mac above, locked and with the display
  asleep: the display had no native brightness; DDC VCP 0x10 answered three
  requests out of three, each with a valid checksum; the ColorSync display
  UUID matched CoreDisplay's and mapped back to the display.
- Device, Locked Use, 2026-11-16, the same Mac, with dimming as the cover
  went up and levels put back as it came down: the display connection reset
  at lock, unlock or wake during several transactions, and one level was left
  at zero, as described under Context.
- Device, Locked Use, 2026-11-17, the same Mac, locked, this design, a
  Codex Multica first turn each:
  - display asleep at the raise: the panel went up, the console unlocked, the
    display was dimmed over DDC three seconds later and lost its panel, the
    run completed, and the helper put the level back four seconds after the
    relock;
  - display awake at the raise: the same, dimmed three seconds after the
    unlock, put back four seconds after the relock.

  The kernel logged no connection reset, the display kept its ID and UUID
  throughout, it read back its original level afterwards, and no record was
  left.
- Device, Locked Use, a Claude Multica first turn under the dimmed display
  (pending; it needs the owner's grant).
