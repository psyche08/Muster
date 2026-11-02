# ADR 0091: Prune conversation views only in apps that expose them apart

- Date: 2026-11-02
- Status: Accepted; source verified; device acceptance pending
- Related: [ADR 0080](0080-session-list-scan-prunes-conversation-views.md)
  (refined here), [ADR 0056](0056-bounded-scans-skip-transcripts-and-retry-vanished-nodes.md),
  [ADR 0089](0089-paired-release-1-1-10.md), [APP_PROFILES](../APP_PROFILES.md)

## Context

The hourly send-anomaly review found a regression on desktop 1.1.10 (100),
build `18d542f4`: three remote sends to a Codex session that was not the
conversation currently open in ChatGPT.app were refused before the ledger on
2026-11-02 at 15:58:40, 15:58:53 and 15:59:00 CST:

```text
Agent send … target="01a18c74-…": native preparation failed before ledger
elapsed_ms=3092: unavailable: session selection: desktop content is not ready
```

The AX trace names the cause. ChatGPT.app (26.1027.51404, pid 58338) showed
one other conversation. The session scan completed with one web area; the
sidebar scan reported

```text
navigation_areas=0 conversation_views=1 app_walk=(254, "complete", 20)
```

ADR 0080 had `scan_sidebar` skip any web area whose key resolves to a
conversation. That rule came from Claude.app, where the app shell (holding
`app-shell-sidebar`) and each conversation are separate web areas. ChatGPT.app
exposes the opposite layout: a single web area (`app://-/index.html`) whose
subtree holds both `AXGroup id="app-shell-sidebar"` (with its landmark lists)
and the open conversation. ADR 0080's prune therefore skipped the one web area
that also held the sidebar, leaving `navigation_areas=0` and making every
remote send to a non-open Codex session fail as unready. The daemon log holds
79 such `navigation_areas=0 conversation_views=…` sidebar traces; the same
build had no successful selection of a non-open Codex session.

The universal prune was an over-generalisation of a Claude-specific layout
fact. Which layout an app has is app knowledge and belongs in its profile.

## Decision

The profile's sidebar declaration gains a required
`separate_conversation_views` boolean:

- `claude.toml` sets it true: the app shell and each conversation are separate
  web areas, so the sidebar walk keeps ADR 0080's prune.
- `codex.toml` sets it false: ChatGPT.app's one web area holds both the
  session list and the open conversation, so the walk enters it and finds
  `app-shell-sidebar` as it did before ADR 0080.

`scan_sidebar` prunes a conversation-resolving web area only when the profile
declares separate views. Row matching, uniqueness, the exact-id fallback and
the opened-session identity check are unchanged, and a walk that stops early
still fails closed. The schema makes the field mandatory for `sidebar`, so an
app profile cannot inherit the wrong layout silently.

## Alternatives

- **Probe each web area for the sidebar before pruning.** The probe has to
  descend into the view, which is exactly the walk ADR 0080 prunes to avoid;
  it would also read the transcript it must not depend on.
- **Prune only inner transcript subtrees in ChatGPT.app.** The transcript
  boundary is not exposed as a stable, app-independent node; per-app DOM
  knowledge in the Accessibility walker would be more fragile than profile
  data.
- **Special-case the provider in platform code.** The layout is a property of
  the app, so it belongs in the compiled app profile (ARCHITECTURE §3.6.4),
  next to the other selector and navigation data.
- **Keep the universal prune.** That is the observed regression: remote and
  phone sends to a Codex session that is not the open one cannot navigate.

## Consequences

- Claude.app keeps ADR 0080's benefit: an invalidated transcript view no longer
  stops the sidebar walk.
- ChatGPT.app again finds its sidebar when the target session is not open, so
  cross-device sends to Codex sessions work as before 1.1.8.
- For an app whose sidebar shares the conversation web area, an unreadable
  child anywhere in that view can still stop the walk (the class ADR 0080 fixed
  for Claude) and refuses closed; no partial walk is patched.
- Adding a desktop app now requires stating its conversation-view layout; the
  build refuses a sidebar profile without it.

## Verification

- `cargo test -p muster-agent` compiles both profiles and asserts in
  `only_separate_conversation_views_are_pruned` that Claude declares true and
  Codex declares false.
- `cargo check -p muster-platform --all-targets` and
  `cargo clippy -p muster-platform --all-targets` pass on macOS.
- The daemon-log evidence above is from the installed 1.1.10 build; the fix
  needs an installed build and one remote send to a Codex session that is not
  the open one. No real conversation is used as a release test.
- Build, signing, notarization, publication and installation are recorded in
  the paired-release ADR for the version that ships this change.
