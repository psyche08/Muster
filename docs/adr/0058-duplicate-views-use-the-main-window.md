# ADR 0058: Several views of one conversation use the main window's view

- Date: 2026-10-27
- Status: Accepted
- Related: [0056](0056-bounded-scans-skip-transcripts-and-retry-vanished-nodes.md),
  [0057](0057-targeted-deployment-1-1-0.md)

## Context

After 1.1.0 was installed, two Locked Use sends from the phone to the Codex
conversation «分析 licong41 网络不可访问» on M4Pro (13:07:37 and 13:08:42 UTC)
were refused with "more than one view shows this session". The send's own
trace shows M4Pro's ChatGPT app with nine conversation views, three of which
(web areas 1, 2 and 8) resolve to the target. The window server shows nine
full-size ChatGPT windows stacked on top of each other: the Codex app opens a
window per thread activation, and the unmerged native-runtime branch that ran
there until today activates threads that way. The same conversation had already
been refused for this reason on the old 1.0.98 build (background attaches at
07:51, 08:06, 08:14 and 08:34 UTC); sends to it failed earlier on the node
ceiling, which ADR 0056 removed, so the duplicate views became the visible
cause. The owner asked for the main window to decide.

`find_surface` refused whenever more than one web area resolved to the session,
so that an operation never had to pick between views.

## Decision

Shared Rust decides in `muster_agent::native_scan::choose_view` from facts the
platform reads (`ViewPlace`: whether the view's window is the app's
`AXMainWindow`, its position in the app's front-to-back `AXWindows` order, and
whether it is minimized):

- A single view is used as before.
- Among several, the one in the app's main window is used.
- An app that is not active may report no main window, as under Locked Use;
  then the frontmost view in the app's window order is used.
- A minimized window is never chosen, because its renderer need not update what
  the echo check reads.
- Two views in the main window, two frontmost views sharing one window, a view
  whose window cannot be placed, or only minimized windows still refuse with
  "more than one view shows this session".

The views are one conversation, so whichever is chosen reaches it. The
operation stays in that one view: the composer, the echo and the send control
are all scoped to the chosen web area, and `check_write_surface` keeps requiring
that web area to resolve to the session and to contain the composer. If the
front window changes during an operation, the next scan picks another view and
the echo check stops the send before any press, as for any replaced composer.
The choice and the facts behind it are logged.

## Alternatives

- **Keep refusing.** Leaves a conversation unreachable whenever the app holds
  it in two windows, which the Codex app does by itself.
- **Close the extra windows from Muster.** Closing the owner's windows is not
  Muster's to decide, and a locked console exposes no windows to close.
- **Pick the first view the walk finds.** The walk order is an implementation
  detail of the AX tree; the main window and the window order are what the
  owner sees.

## Consequences

Sends, answers and attaches reach a conversation shown in several windows, in
the window the owner is looking at. The sidebar selection used when the target
is not on screen still searches every navigation area and can still refuse with
many windows open; that path is unchanged.

## Verification

- **Source and tests:** Shared Rust tests cover the main window, the frontmost
  fallback with minimized windows skipped, and each refusal. The touched crates'
  tests and Clippy with warnings denied are recorded in the pull request.

### Build, installation and acceptance on 2026-10-27

- **Tests:** six `native_scan` tests including the three for `choose_view`;
  `muster-core`, `muster-agent`, `muster-platform` and `musterd` 499 passed,
  0 failed; Clippy with warnings denied passed.
- **Build:** 1.1.1 (91) from clean `299166e7ff6c14f94fa519b2982b59caf6bbc3fc`,
  with ADR 0057's gates, signing and packaging. Apple notarization
  `9100b14e-0ae3-4520-beec-44d54ab5f514` Accepted, stapled, Gatekeeper
  "Notarized Developer ID". Portable ZIP SHA-256
  `5372d9b01108cd78b9a5f27b8d15b5bf7ad0aa4fc2229d579bcc3b0940a63d05`, daemon
  SHA-256 `dec4ef2cd7efcfa0d854634d5a55935706212b14878c09fb49772759ef3da892`.
- **Installation:** M4Pro (13:29 UTC) and the Mac Mini (13:30) by ADR 0057's
  installer; unchanged identities, installed daemon equal to the package's,
  Locked Use plugin re-verified, relay and Vnet up, M4Pro `direct` with 3/3
  pings. M1Max was off the network and still runs 1.1.0.
- **Acceptance (M4Pro, owner's phone, Locked Use):** at 13:32:17 UTC the
  signed operation unlocked M4Pro under the cover. The scan found nine web
  areas, three on «分析 licong41 网络不可访问», with window ranks 0 (main), 1
  and 7, and chose the main window's view (`chosen=Some(0)`); every scan in the
  operation chose the same view. The composer was empty, the exact echo arrived
  in 236 ms, Send was pressed once, the Codex receipt arrived after 999 ms,
  the ledger recorded `applied`, and the console relocked under the cover. The
  conversation's transcript holds the owner's message as its user record, and
  Codex began working on it. No `CoverLost` or `WatchdogLost` occurred.

Not exercised: the frontmost-window fallback (the app reported a main window),
the minimized-window exclusion, and the remaining refusals on a device.
