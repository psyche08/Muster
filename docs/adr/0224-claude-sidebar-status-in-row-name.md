# ADR 0224: Match Claude sidebar rows whose name includes their status word

- Date: 2026-11-17
- Status: Accepted; source and tests verified, device acceptance pending
- Related: [ADR 0010](0010-native-sidebar-metadata.md), [ADR 0201](0201-background-claude-card-selection.md), [ADR 0213](0213-bare-sidebar-badges-and-quiet-speculative-card-reads.md)

## Context

The owner answered a question in a local Claude Code session from the phone,
and the answer failed with "remote error Unavailable: The desktop app did not
expose this session in its sidebar; nothing was submitted". The session was
running, and its sidebar row carried a `Merged · 2 pull requests` badge.

Claude.app now puts the row's status word into the row's accessible name.
The Code sidebar row is an `AXButton` whose `AXTitle` is the visual-order join
of its passive labels, status included, for example
`<T> Running Merged · 2 pull requests`. Its child `AXStaticText` values are
`<T>`, `Running` and `Merged · 2 pull requests`.

ADR 0010's matcher skips recognized status words when it rebuilds the row name
from the labels. For this row it compares the name against
`<T> Merged · 2 pull requests`, finds no row and reports zero candidates.
Every Agent respond, send or inspect to such a session then fails before
anything is sent. A row with no metadata badge still matches, because only a
row with metadata has to reproduce its name. So the failure needs both a
visible status and a metadata label.

A read-only Accessibility pass over the same sidebar printed only the shape of
each row, with every title replaced by a placeholder. 23 rows had more than
one label, and every one of them was named exactly by the visual-order join of
its labels. Two of them had a status and metadata. Both were delegated
sessions shaped `<T> | Started by Claude from <P> | Running`, so the status
can come after the provenance rather than right after the title. The matcher
missed both. No running session had a pull-request badge at the time of the
read, so the reported shape itself was not visible then.

## Decision

`native_sidebar::title_matches` (Shared Rust) accepts a row with metadata when
its name equals either:
- the existing composite: the title, then the metadata labels in visual
  order, with status words left out; or
- the visual-order join of all the row's passive labels, status words
  included, each at its own position.

Every other rule of ADR 0010 and ADR 0213 stays:
- exactly one label equal to the expected title;
- every other label a recognized status (`Idle`, `Working`, `Running`,
  `Awaiting input`, `Unread response`) or a recognized metadata badge;
- exact equality with the whole row name, never a prefix or substring;
- a unique catalog title, the standalone-title profile, and a complete,
  bounded label read.

The status-free composite stays because Claude named rows that way until now,
and other rows may still be named that way. No status word or badge is added.
A match only selects a candidate. Before any inspection or mutation, the
selected surface's session UUID is still verified (ADR 0201).

## Alternatives

- Insert the status right after the title in the composite. That matches the
  reported pull-request row, but not the delegated running rows, whose status
  follows the provenance.
- Remove status words from the row name before comparing. That edits the
  accessible name by substring, and it cannot tell a status label from the
  same word inside a title. ADR 0010 rejects substring handling.
- Prefix or substring matching on the title. ADR 0010 rejects it, because it
  accepts unrelated or ambiguous rows.

## Consequences

Running or waiting sessions that also carry a pull-request badge, a
provenance label, an offline workspace or `SSH` can be selected again for
Agent respond, send and inspect, as long as Claude names the row by the full
join. A row is also matched when the title is not the first label, provided
the name is that exact join. This follows ADR 0010's rule that metadata
reproduce the row name in observed visual order. Unknown labels, a malformed
badge, a status word in any other position, or a status word that is not one
of the row's labels still fail closed. If Claude changes the separator or adds
a new status word, those rows fail closed again until the new form is
observed and added.

## Verification

- Source and tests (`muster-agent`):
  - `native_sidebar::row_named_with_status_and_metadata_matches_the_visual_join`
    replays the captured row shape (`AXButton` > `AXGroup` > title, status,
    badge) through `read_label` and `title_matches` for
    `<T> Running Merged · 2 pull requests`,
    `<T> Awaiting input Open · 1 pull request` and three other
    status/badge pairs. It checks that the status-free name still matches.
    It rejects a non-unique title, a non-standalone profile, a different
    title, the status after the badge, the status before the title, a doubled
    space, a missing badge, trailing text, a status word that is not a label,
    and a doubled title.
  - `native_sidebar::captured_delegated_running_row_matches_in_its_own_visual_order`
    replays the delegated `<T> | Started by Claude from <P> | Running` shape
    read from the sidebar, and rejects the status moved before the provenance.
  - `native_sidebar::visual_join_still_rejects_unknown_labels` rejects an
    unknown status (`Paused`), a lowercase status, an unknown badge
    (`Closed`), a malformed count and an unrelated label, each in a name that
    is the exact join.
  - The new positive cases fail with the previous matcher.
  - All titles in the tests are synthetic.
- `cargo fmt`, strict Clippy (`--no-deps -D warnings`, all targets) and the
  `muster-agent` and `muster-platform` suites are recorded in the pull
  request.
- Accessibility: the read-only pass above, run against the running Claude
  app on the authoring Mac. It printed only row shapes, never titles.
- Not done: building or installing Muster.app, restarting musterd, and a
  device run of an Agent answer to a running session with a pull-request
  badge. Another session holds this Mac's installed build for Locked Use
  device tests, so the device check waits until that session lands.
