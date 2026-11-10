# ADR 0168: Coalesce live session list updates

- Date: 2026-11-10
- Status: Accepted
- Related: ADR 0051, ADR 0148, ADR 0161

## Context

An open desktop session list repeatedly changes its scrollbar as hosts publish
catalog snapshots. A catalog-only watch builds and sends a whole page for each
list or session-state event, including events outside the displayed page.

## Decision

Each Shared Rust daemon sends the initial authorized page immediately. During
a catalog-only watch, ordinary notifications mark the page dirty. Rebuild the
latest page at most once every five seconds, even during a continuous burst,
and send it only if it differs from the last delivered page. An idle watch
does no periodic page projection. A fresh watch, including manual refresh and
reconnect, gets its own initial page. Changes to ownership or provider access
bypass the display interval. Conversation streams keep their live delivery.

## Alternatives

A delay in the native view still asks every host to project and transmit each
intermediate page. Restarting a debounce timer on each event can indefinitely
postpone a busy catalog's latest state.

## Consequences

Ordinary list changes can take about five seconds to appear per host. Explicit
paging and refresh retain their request paths. Each updated host controls its
own outgoing updates; older hosts keep their previous cadence.

## Verification

Exercise the RPC watch with a burst, an unchanged page, and an ownership change
during the quiet period. Check that the latest page arrives and ordinary chat
watches retain their access checks. Visible scrollbar behavior on the installed
desktop build requires device acceptance.

All three daemon access integration tests passed on macOS, including a real
catalog stream with consecutive title edits, a change that returns to the
previous page, provider grant narrowing on a peer connection, and task ownership
withdrawal. Strict Clippy passed for the daemon and its access test target.
