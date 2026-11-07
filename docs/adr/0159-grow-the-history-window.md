# ADR 0159: Grow the conversation history window instead of sliding it

- Date: 2026-11-07
- Status: Accepted; source and tests verified, installation and device acceptance pending
- Related: [ADR 0153](0153-large-session-history-costs.md)

## Context

ADR 0153 bounds the conversation the phone projects to a 200-record window.
Loading earlier history moved that window up by 184 records, so the old and
new windows shared only 16 records. A Claude Code turn made mostly of tool
calls renders 200 records as two or three rows: a few collapsed "Worked"
groups and the final answer. A reader of such a session reaches the top of
the list with a short scroll, which loads the earlier page. The window then
left the rows the reader had just been on, final answer included, and showed
only the earlier page with Load newer and Load earlier.

This was reported on 2026-11-07 against the installed 1.1.27 on iPhone, in
a long Claude Code session that had been compacted twice. A replay of that
session's own records reproduces it. The tail page held 172 tool records and
28 others, among them a compaction summary and the final answer. After one
earlier page, the window held 184 tool records and 16 text records, one of
them a compaction summary, and no prompt or answer. This is the "Worked /
Compact context / Worked" view in the report.

## Decision

Earlier and newer navigation grow the window in Shared Rust instead of moving
it. `Chat` keeps the window's first ID (`history_start`) and, while browsing,
its last ID (`history_end`). `None` means the window reaches the latest record
and follows live output.

- Revealing cached history, or loading an earlier page, moves the start up by
  a page or to the first record loaded. The end stays where it was.
- Load newer moves the end down by a page and keeps the start.
- A browsed window holds at most `CHAT_HISTORY_LIMIT` (1,000) records. Beyond
  it, the end the reader moved away from is dropped: the newest records when
  loading earlier, the oldest when loading newer.
- Live output past a window that reaches the latest record extends it up to
  the limit. After that it stays below the window, behind Load newer.
- Jump to latest and an authoritative reset return to the 200-record tail
  window.

The unbrowsed tail window, page requests and caches are unchanged. The
16-record overlap and `CHAT_HISTORY_STEP` are removed. Native shells keep
sending the same intents and rendering `has_earlier` and `has_newer`.

## Alternatives

- Keep the sliding window and widen the overlap. Any fixed record count can
  still fold into fewer rows than the screen holds.
- Size the window in rendered rows. Rows are only known after projection,
  which is the cost ADR 0153 bounds.
- Align windows to turn boundaries. One turn can hold thousands of records.
- Drop the bound. ADR 0153 bounds the projection for large sessions; a
  1,000-record limit keeps that bound while a reader browses.

## Consequences

- Scrolling up keeps the rows the reader was on. Earlier rows appear above
  them, and Load newer appears only after the window passes 1,000 records.
- While browsing, each projection covers up to 1,000 records instead of 200.
  The tail window, the one projected during live output when the reader is at
  the bottom, still covers 200.

## Verification

- Source and tests: `cargo test -p muster-app-core` passes. New tests cover
  growth on reveal and on a loaded earlier page, the 1,000-record limit, live
  output past a window that ends early, newer navigation back to the latest,
  and the reported shape: a tool-only page loaded above a tail that ends in
  a final answer keeps that answer and hides nothing newer. The render and
  watch tests were updated for the grown windows. `cargo clippy -p
  muster-app-core --tests` reports nothing.
- Not done: no Apple or Android build. No native code changed. Signing,
  installation and device acceptance on iPhone are pending.
