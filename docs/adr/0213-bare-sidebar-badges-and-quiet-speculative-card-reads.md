# ADR 0213: Recognize bare sidebar badges and keep speculative card reads quiet

- Date: 2026-11-15
- Status: Accepted; device acceptance pending
- Related: [ADR 0010](0010-native-sidebar-metadata.md), [ADR 0109](0109-uuid-session-refresh-and-permission-lifetime.md), [ADR 0201](0201-background-claude-card-selection.md)

## Context

A phone opening a local Claude Code session on Mac-A showed two red toasts:
"Could not read Claude's permission card: unavailable: The desktop app did not
expose this session in its sidebar; nothing was submitted". The session was
idle with a running worker, and it was not waiting for approval.

Mac-A's daemon log shows two entry reads three seconds apart. Each read walked
the Claude sidebar, expanded it from 47 to 83 rows and found no candidate. An
accessibility dump of the same sidebar shows the session's row. Its name is
the exact title followed by a bare `Merged` badge, because the session's one
pull request had just merged. ADR 0010's grammar only knows the counted form
(`Merged · 1 pull request`), so it rejected the row. The same dump shows a
second unknown badge on a forked session, `Forked from <parent title>`.

The other badges in that sidebar were already recognized. These were the
counted PR states, `Started by Claude from …`, `… · Offline`, `Running` and
`Unread response`.

The logs of Mac-B and Mac-C over the same day hold only transient events:
- relay QUIC reconnects that recovered within two seconds;
- Codex Cloud request timeouts, seen on all three Macs at the same time;
- the phone's presence links ending on idle timeout.

## Decision

**Sidebar grammar.** The Shared Rust grammar accepts these labels:
- The PR states it already knows, without a count: `Open`, `Merged` and
  `Merge conflicts`. Only `Merged` was observed; Claude dropped the count for
  a single pull request, and the other two states are taken to follow it.
- `Forked from <title>` provenance, with a non-empty origin, the same as
  `Started by Claude from <title>`.

Every other rule of ADR 0010 stays:
- one exact standalone title;
- a unique catalog title;
- a complete, bounded label read;
- metadata that rebuilds the whole row name.

Unknown labels, such as `Closed`, still fail closed. Before any inspection or
mutation, the selected surface's session UUID is still verified (ADR 0201).

**Toasts.** ADR 0109 lets the phone read a local Claude session on entry when
its worker is running, even though no approval is reported, in case the file
hint missed a card. If such a read fails, the core now logs the failure and
shows no toast. A read of a session that reports `waiting_approval` still
shows the toast, because the user is waiting on that card. The deferred retry
for `INSPECT_LOCAL_INPUT` is unchanged.

## Alternatives

- Substring or prefix matching would accept any trailing text, which ADR 0010
  rejects.
- Accepting only the observed `Merged` would fail again for the first session
  whose single pull request is still open, if that row has the same form.
- Dropping the speculative entry read would lose cards that the file hint
  misses (ADR 0109).
- Showing a softer toast would still interrupt a user who has no card waiting.

## Consequences

Sessions with a single-PR badge or a fork badge can be selected again, both
for card reads and for other sidebar navigation. A failed speculative read is
visible only in the app log. A real pending approval that cannot be read is
still reported. If Claude introduces new badges, they keep failing closed
until they are observed and added.

## Verification

- Source and tests:
  - `native_sidebar::captured_bare_pr_state_and_fork_rows_are_selected`
    replays the captured `Merged` and `Forked from` rows through
    `read_label` and `title_matches`. It covers bare `Open` and
    `Merge conflicts`, a non-unique title, a non-standalone profile and extra
    trailing text. It also rejects `Closed`, a lowercase state, a trailing
    space and an empty fork origin. The test fails with the previous grammar.
  - The app-core test `permission_reads_are_bounded_cancelled_and_yield_to_initial_cards`
    now checks that a sidebar failure toasts nothing for an idle or running
    worker and toasts once for `waiting_approval`. It fails without the new
    guard.
- The full `muster-agent`, `muster-app-core` and `musterd` suites and Clippy
  results are recorded in the pull request. As in ADR 0212, debug builds on
  the authoring Mac run with `RUST_MIN_STACK=16777216`.
- Not done: signing, installation and a device run on the three Macs and the
  phone.
