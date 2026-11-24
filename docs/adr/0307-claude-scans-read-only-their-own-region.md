# ADR 0307: Claude's control and sidebar scans read only their own region of the window

- Date: 2026-11-24
- Status: Accepted; source and tests verified, installation and runtime
  acceptance pending
- Related: extends [0056](0056-bounded-scans-skip-transcripts-and-retry-vanished-nodes.md)
  (which deferred pruning inside the control scan until evidence) and
  [0080](0080-session-list-scan-prunes-conversation-views.md) /
  [0091](0091-prune-conversation-views-only-where-separate.md) (which leave
  Claude's conversation web area unpruned); [0221](0221-multica-workspace-folder-sessions.md)
  (side chat regions); [app profiles](../APP_PROFILES.md),
  [Locked Use](../LOCKED_USE.md)

## Context

On 2026-11-24 a Mac running Muster 1.1.47 could neither attach to one long
Claude Code conversation (session A) nor select any other Claude session
while session A was in front.

Both scans that failed are breadth-first walks bounded by `ax::MAX_NODES`
(4,096) that refuse to act on an incomplete result:

- **The control scan** (`find_controls`) walks the conversation's web area
  for the composer, Send and Stop. For session A it first stopped at the
  limit at 08:07 UTC (`controls stopped early (node limit) ... not sent`).
  It stopped 26 more times before noon, and 254 times in the next hour as
  retries came in (`controls composers=0 ... walk=(4096, complete=false,
  312 ms) stop=Some("node limit")`). At 11:35 the same walk had completed at
  901 nodes. Claude renders only part of a long transcript, so the tree's
  size changes with what is rendered. While it is past the limit, every
  attempt fails.
- **The sidebar scan** (`scan_sidebar`) walks the whole app to find the
  session sidebar. Claude nests that sidebar inside the conversation's web
  area, so the walk cannot skip the web area (ADR 0080, ADR 0091), and every
  message counts against the limit. While session A was in front, selecting
  other sessions was refused as `cannot uniquely select the session (1
  matches; complete=false)`: the walk had found the one matching row but had
  not finished.

Selection refuses an incomplete scan on purpose. Pressing a sidebar row is an
operating-system action. Uniqueness is proved only when the whole tree was
read, since an unread part could hold a second sidebar or a second matching
row. Relaxing that rule would trade the failure for a wrong press.

A read-only `muster debug ax-dump` of the same window, with another, shorter
session in front, shows how the window is divided. The app has 1,325 nodes,
259 of them in the menu bar, which the sidebar scan already skips. The
claude.ai web area holds two landmarks side by side:

- **On the left, the session sidebar:** a complementary landmark named
  "Sidebar", 606 nodes.
- **The main landmark, 412 nodes:**
  - the session's toolbar;
  - the transcript, 219 nodes: an application group named "Chat messages"
    holding one article per message;
  - the activity panel on the right, 117 nodes: a complementary landmark
    named "Activity panel", with Environment, Context, Connectors and
    Changes;
  - the composer with its controls.

The transcript grows with the conversation, and the panel grows with what it
shows. An older recording (with a question card) had a split view open
beside the conversation, 516 nodes, most of them lines of text in a code
view. In none of the five Claude Code recordings is the composer, Send or
Stop inside the transcript or a complementary landmark. In the one recording
that still names its session sidebar, the sidebar is outside the main
landmark.

A second failure in the same report is unrelated to the limit: two Claude
sessions had the same title, so neither could be matched by title even when
the scan completed (`0 matches; complete=true`). This ADR does not change
that rule (see Consequences).

## Decision

Each scan reads only the region that holds what it looks for.

1. **The sidebar scan reads beside the main landmark.** `[controls.sessions]`
   gains `outside_main`, which the Claude profile sets. The sidebar walk keeps
   the main landmark as a node and does not walk below it. If that walk finds
   no session list, it walks again with the main landmark included, as
   before. Reasons for no list: the sidebar is hidden, Claude shows an alert
   in its place, or an update moved it.
2. **The control scan skips the asides.** `[controls]` gains
   `skip_complementary`, which the Claude profile sets. The control walk does
   not walk below a complementary landmark, which covers the session sidebar
   and the activity panel.
3. **Both scans skip the transcript.** `[controls]` gains
   `transcript_regions`, names of application groups (`AXApplicationGroup`)
   that hold the conversation's transcript. The Claude profile names "Chat
   messages". `Controls::is_transcript_region` matches a group by its title
   or description, like `auxiliary_regions` matches landmark regions. Neither
   scan walks below one, so the transcript counts even in the sidebar scan's
   second walk.
4. **Selection rules do not change.** The control scan still needs a
   complete walk and exactly one composer. The sidebar scan still needs a
   complete walk and exactly one matching row. What remains for the control
   scan is the session's toolbar and the composer area: 99 nodes in the
   recording instead of 1,038. The sidebar walk reads 50 nodes beside the
   main landmark instead of 461 before it reaches the sidebar.
5. **Diagnostics report what was skipped.** The control scan reports
   `transcripts=N asides=N`. The sidebar scan reports `transcripts=N
   main_skipped=N` and records a line when it walks the main landmark after
   all.
6. **The queue and question scans do not change.** A queued Claude prompt is
   a message in the conversation (ADR 0158), so the queue scan still reads
   the transcript.
7. **The redactor keeps profile labels.** `scripts/redact-ax-dump.py` also
   takes the profiles' quoted strings as vocabulary, so labels that now live
   only in `profiles/*.toml`, such as "Chat messages", survive redaction.

Other apps' profiles set none of the three, and their scans do not change.

## Alternatives

- **Raise `MAX_NODES`.** This only moves the failure to a longer
  conversation or a larger panel, and every walk would get slower.
- **Accept one match from an incomplete sidebar scan.** This drops the proof
  of uniqueness that makes a press safe.
- **Scroll the conversation to its end and search from the bottom.** The
  rendered end of a long conversation is smaller than all of it, but this
  has three costs:
  - It moves the window the user sees and loses their place in it. Locked
    Use runs while the Mac is locked, and Muster does not change the visible
    window from there.
  - The end of a running turn can itself be large.
  - A search that stops at its first match no longer proves that the match
    is the only one.

  Skipping the regions makes both scans independent of the scroll position
  without moving anything.
- **Locate the composer first, and read what is above it as the
  conversation.** In effect the control scan now does this by region: it
  skips the transcript above the composer and the panels beside it.
  Reading in reverse document order would not do it alone. The activity
  panel sits between the transcript and the composer in document order, and
  the older split view came after the composer.
- **Find the transcript by shape (a group of articles).** A name read from
  the recorded app is a narrower rule, and the composer itself is already
  found by its name ("Prompt"). An unmatched label falls back to reading the
  transcript instead of skipping something unexpected.
- **Prune the whole conversation web area in the sidebar scan.** Claude's
  sidebar lives inside it (ADR 0091). The main landmark is the part of it
  that the sidebar is not in.
- **Select sessions by URL (`claude://code/continue?session=`).** That route
  switches the user's visible window and is not available as a background or
  window-scoped action, so it does not replace the sidebar press here.

## Consequences

- What the two scans read no longer depends on how long the open
  conversation is or how much the activity panel shows.
- The control scan no longer counts a `Prompt` text area inside a panel or a
  message. While the session's composer is present, the scan picks that one
  instead of refusing as ambiguous. When the session's composer is missing,
  the scan finds no composer instead of the stray one.
- A hidden sidebar costs a second walk before the scan reports that no list
  was found.
- Each rule degrades to the old walk rather than to a skipped control:
  - If Claude renames the transcript group, or shows it in an untranslated
    UI language, both scans read the transcript as before, and the profile
    needs the new label.
  - If the session sidebar moves into the main landmark, the sidebar scan
    reads the main landmark in its second walk.
  - If the composer moves into a complementary landmark, the control scan
    finds none and refuses.
- No current recording shows a terminal, file or diff view.
  - If such a view is a complementary landmark, both scans skip it.
  - If it sits inside the main landmark as something else, as the older
    split view did, the sidebar scan skips it, but the control scan still
    reads it.
- The queue and question scans can still hit the limit in a long
  conversation. The recorded question card sits in the composer area,
  outside the transcript. Queued prompts are in the transcript, near its
  end.
- Sessions with the same title still cannot be selected by title. Renaming
  one of them is the workaround. Choosing between them by order, or by a
  press-and-verify, is a separate decision.

## Verification

- **Profile switches:** `profile::tests` checks that only Claude's profile
  skips complementary landmarks, keeps its session list outside the main
  landmark, and names a transcript group.
- **Recorded layout:** a new redacted recording (`claude-code-transcript.dump`,
  2026-11-24, menu bar left out) shows the layout above. In it and in the
  four older Claude Code recordings (idle, running, with a draft, with a
  question card), the composer, Send and Stop are outside the message list
  and outside every complementary landmark. The older recordings' sidebar
  names were redacted, so only the new recording shows where its sidebar is.
- **Long and large trees:** a test grows the recording's transcript, its
  activity panel, or both, until each is more than twice the node limit.
  - The old control and sidebar walks then stop at the limit.
  - The new walks complete and read exactly the nodes they read in the
    recording.
  - They find the same composer and the same single sidebar.
- **Fallback:** a test shows the second sidebar walk:
  - With the sidebar moved into the main landmark, it is found there.
  - With the sidebar removed, the scan completes with no list.
- **Platform walks:** `find_controls_with_budget` and `scan_sidebar` need
  live Accessibility elements, so the tests mirror their traversal over the
  recorded tree.
- **Not verified:**
  - installation and a restart of `musterd`;
  - a send to a long Claude conversation in the live app, or a selection
    beside one;
  - Locked Use sends while the Mac is locked;
  - terminal, file and diff views in the current app;
  - a localized UI.
