# ADR 0307: Control and sidebar scans skip the conversation's transcript

- Date: 2026-11-24
- Status: Accepted; source and tests verified, installation and runtime
  acceptance pending
- Related: extends [0056](0056-bounded-scans-skip-transcripts-and-retry-vanished-nodes.md)
  (which deferred pruning inside the control scan until evidence) and
  [0080](0080-session-list-scan-prunes-conversation-views.md) /
  [0091](0091-prune-conversation-views-only-where-separate.md) (which leave
  Claude's conversation web area unpruned); [app profiles](../APP_PROFILES.md),
  [Locked Use](../LOCKED_USE.md)

## Context

On 2026-11-24 a Mac running Muster 1.1.47 could neither attach to one long
Claude Code conversation (session A) nor select any other Claude session
while session A was in front.

Both scans that failed are breadth-first walks bounded by `ax::MAX_NODES`
(4,096) that refuse to act on an incomplete result:

- **The control scan** (`find_controls`) walks the conversation's web area
  for the composer, Send and Stop. For session A it first stopped at the
  limit at 08:07 UTC (`controls stopped early (node limit) ... not sent`),
  26 more times before noon, and 254 times in the next hour as retries came
  in (`controls composers=0 ... walk=(4096, complete=false, 312 ms)
  stop=Some("node limit")`). At 11:35 the same walk had completed at 901
  nodes: the transcript's size changes with what Claude renders, and while it
  is past the limit every attempt fails.
- **The sidebar scan** (`scan_sidebar`) walks the whole app to find the
  session sidebar. Claude nests that sidebar inside the conversation's web
  area, so the walk cannot skip the web area (ADR 0080, ADR 0091) and every
  message counts against the limit. While session A was in front, selecting
  other sessions was refused as `cannot uniquely select the session (1
  matches; complete=false)`: the one matching row was found, but the walk had
  not finished.

Selection refuses an incomplete scan on purpose. Pressing a sidebar row is an
operating-system action; uniqueness is proved only when the whole tree was
read, since an unread part could hold a second sidebar or a second matching
row. Relaxing that rule would trade the failure for a wrong press.

A read-only `muster debug ax-dump` of the same window with another, shorter
session in front measured where the nodes are: 1,325 in the app, 259 of them
in the menu bar (which the sidebar scan already skips), 606 in the session
sidebar, and 412 in the main pane. Of those, 219 are one application group
named "Chat messages" that holds one article per message; the composer and
its controls are a separate branch beside it. The transcript is the only part
that grows with the conversation; the sidebar grows with the number of
sessions, a few nodes per row.

A second failure in the same report is unrelated to the limit: two Claude
sessions had the same title, so neither could be matched by title even when
the scan completed (`0 matches; complete=true`). This ADR does not change
that rule (see Consequences).

## Decision

1. `[controls]` gains `transcript_regions`: names of application groups
   (`AXApplicationGroup`) that hold the conversation's transcript. The
   Claude profile names "Chat messages". Other profiles name none and scan as
   before. `Controls::is_transcript_region` matches a group by its title or
   description, like `auxiliary_regions` matches landmark regions.
2. `find_controls` keeps a transcript group as a node and does not walk below
   it. Composer, Send and Stop selection is unchanged; the transcript held
   none of them in any recording.
3. `scan_sidebar` stops at a transcript group the same way. The sidebar
   landmark is still found inside the conversation's web area, and the scan
   still has to finish before a single match is pressed.
4. Both scans report the number of transcripts they skipped
   (`transcripts=N`) in their existing diagnostics.
5. The queue and question scans are unchanged. A queued Claude prompt is a
   message in the conversation (ADR 0158), so those scans still read the
   transcript.
6. `scripts/redact-ax-dump.py` also takes the profiles' quoted strings as
   vocabulary, so labels that now live only in `profiles/*.toml`, such as
   "Chat messages", survive redaction.

## Alternatives

- **Raise `MAX_NODES`.** This only moves the failure to a longer
  conversation, and every walk would get slower.
- **Accept one match from an incomplete sidebar scan.** This drops the proof
  of uniqueness that makes a press safe.
- **Prune the whole conversation web area in the sidebar scan.** Claude's
  sidebar lives inside it (ADR 0091).
- **Find the transcript by shape (a group of articles).** A name read from
  the recorded app is a narrower rule. A localized or renamed label falls back
  to the old full walk instead of skipping something unexpected.
- **Select sessions by URL (`claude://code/continue?session=`).** That route
  switches the user's visible window and is not available as a background or
  window-scoped action, so it does not replace the sidebar press here.

## Consequences

- A long Claude conversation no longer stops the control scan or the sidebar
  scan. What both scans read no longer depends on how long the open
  conversation is.
- The control scan no longer sees controls inside the transcript. None of
  composer, Send or Stop were there in any recording. A text area opened
  inside a message would now be ignored rather than counted as a second
  composer.
- If Claude renames the group, or shows it in an untranslated UI language,
  the label does not match and both scans walk the transcript as before. The
  profile would then need the new label.
- The queue and question scans can still hit the limit in a long
  conversation.
- Sessions with the same title still cannot be selected by title. Renaming
  one of them is the workaround; choosing between them by order or by a
  press-and-verify is a separate decision.

## Verification

- Source and tests: `profile::tests` checks that only a named application
  group in Claude's profile is a transcript region. A new redacted recording
  (`claude-code-transcript.dump`, 2026-11-24, menu bar left out) shows that
  its message list is the group the profile names, and that the list holds
  neither the composer nor the session sidebar. In the four older Claude Code
  recordings (idle, running, with a draft, with a question card), the message
  list holds no composer, Send or Stop; their sidebar names were redacted, so
  they cannot show where the sidebar is. A test grows the recording's transcript to more than twice the
  node limit. The unpruned control and sidebar walks then stop at the limit.
  The pruned walks complete, read the same nodes as in the short recording,
  and find the same composer and the same single sidebar.
- The platform walks themselves (`find_controls_with_budget`,
  `scan_sidebar`) need live Accessibility elements; the tests mirror their
  traversal over the recorded tree.
- Not verified: installation, a restart of `musterd`, a send to or a
  selection beside a long Claude conversation in the live app, and Locked Use
  sends while the Mac is locked.
