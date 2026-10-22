# ADR 0026: A web window onto one origin, reached over a forward stream

- Date: 2026-10-22
- Status: Proposed
- Related: ARCHITECTURE §3.4 (authorization), §3.5 (forward streams),
  ADR 0009 (Muster as a scheduler's runtime), `crates/muster-web`

## Context

The work in ADR 0009 made this machine a runtime for an external scheduler
(Multica). The scheduler's own web interface lives on an internal origin that
a desktop here can reach and a phone cannot: the phone has no route to it and
never will. Reading an issue, watching a run, answering a comment therefore
stops at the desk, which is the opposite of what the rest of Muster does.

Forward streams already carry exactly this shape of traffic — "a member asks
the target to connect it to one of the target's own loopback ports" — and the
rule they are built around is that nothing is forwarded on to a third machine.
A member cannot ask a desktop to connect it to `multica.example`, and that
restraint is deliberate: a hop that connects a member to any address it names
is an open relay out of the network, authorized once and usable for anything
afterwards.

So the missing piece is not a change to forward streams. It is something on
the desktop's own loopback that a forward stream can already reach, and that
is itself held to one destination.

ARCHITECTURE keeps "browser access as a Muster web client" out of scope. This
is not that: Muster serves none of these bytes and gains no web UI of its own.
It carries somebody else's site to a device that cannot reach it.

## Decision

A **web window** (`crates/muster-web`): one loopback port on this machine that
stands for exactly one remote origin.

- **One origin by construction, not by inspection.** The window is built for
  one origin and dials no other. A request naming a second one is refused
  rather than rewritten — an absolute request target (`GET http://host/…`) is
  a 400, `CONNECT` is a 405 — and the client it fetches with follows no
  redirects, so a reply cannot move it either. A reply that points off the
  origin is passed back untouched; the browser leaving the window is visible,
  and the window still fetched nothing.
- **Local configuration, off until asked for.** `LocalPolicy::web_window` is
  `Option<WebWindow>`: an origin and the loopback port it listens on, `None`
  by default. A policy stored before the field existed reads as `None`.
- **Opening the window is not letting anyone in.** The window's port is not
  implicitly open. A member reaches it only under the gate that already
  governs every forward: `net:tcp/<port>` granted in the roster and the port
  open in this machine's policy, decided by `authorize_port`. Two decisions,
  taken separately: `muster policy` says so when the window has no open port,
  rather than opening one on the owner's behalf.
- **Three rewrites on the way back**, because the page is now served from a
  loopback port and not from the origin it came from:
  - `Strict-Transport-Security` is dropped. A pin is kept per host, so one
    site's header would force HTTPS on every other service a developer runs
    on `127.0.0.1`.
  - `Content-Security-Policy` is dropped. It names the origin the page no
    longer has, and would forbid the page its own scripts and styles.
  - `Set-Cookie` loses `Domain` (which the browser will not accept from a
    loopback page) and `Secure` (which would withhold the cookie from a
    loopback port that has no TLS). A `Location` back to the origin becomes a
    path, so the page stays inside the window.
- **The accessor side becomes product code.** `ForwardOpenRequest` is a local
  call that binds a loopback port here and gives every connection on it its
  own forward stream to the member (`muster forward <device> <port>`). It is
  `Access::LocalOnly`: this machine reaching into a member's loopback is the
  local owner's to ask for, and the member still decides its own side. The
  call streams `ForwardOpened` with the bound port and holds the port until it
  is cancelled. This is the same surface a phone's WebView will need, so it is
  written once, in the daemon, rather than in each shell.

The Android settings screen now edits the policy it holds instead of building
a fresh one, so saving its three switches no longer drops the fields it does
not show — which already lost `offer_tasks` and would have closed the window.

## Alternatives

- **Let a forward stream name a host.** One flag on an existing request, and
  the end of the property that makes forward streams safe to grant. Rejected:
  it is the open relay.
- **An HTTP proxy on loopback (`CONNECT`, absolute URIs).** What a browser's
  proxy setting expects, and already general-purpose: whoever reaches it
  chooses the destination. Rejected for the same reason, and it needs a
  browser-wide setting where a window needs a URL.
- **A proxy that checks the `Host` header.** Single-origin by inspection, so
  one parsing mistake widens it. The window holds its origin in the client it
  dials with instead, where no request text reaches.
- **Carry the bytes over the RPC envelope** instead of a forward stream, as
  terminal output is carried. Rejected: forward streams exist for exactly this
  and already have the authorization, the framing and the per-connection
  lifetime.

## Consequences

- A phone, once the shells carry a WebView and a listener of their own, can
  read an internal site through a desktop that can reach it. That slice is
  deliberately not in this change: the daemon side it needs is here.
- Protocol upgrades are refused (501): no WebSocket through the window yet, so
  a page that depends on one for live updates will not update live. The
  request/reply path is complete, including chunked bodies in both directions.
- HTTP/1.1 only on the window's own side (HTTP/2 upstream, through reqwest).
  Browsers speak 1.1 to a plain-HTTP loopback port, so this is not felt.
- A request body is held in memory, bounded at 32 MiB; replies stream.
- One window carries one origin, and the policy holds one window. A page whose
  assets live elsewhere arrives unstyled on a device that cannot reach that
  other origin too — the scheduler's own HTML names 87 URLs on a separate
  corporate CDN origin.
  Carrying such a page end to end needs a window per origin; that is the next
  slice's problem, and single-origin windows are what make it a listable
  decision rather than a general tunnel.
- The window carries whatever the reader's browser sends, cookies included.
  It is a hole in exactly one direction — to one origin — and the owner of the
  machine opens it per origin and per port.
- Nothing about it is automatic: a machine that is updated to this version
  without being asked has no window, no open port, and no new reachability.

## Verification

- **Source/tests.** `cargo clippy --workspace --all-targets` clean;
  `cargo test -p muster-web -p muster-core` green, including: an origin is a
  scheme, host and port and nothing else; only the window's own origin counts
  as its own; the three rewrites; a request naming another origin is refused
  (absolute target, `CONNECT`, upgrade); a chunked request body arrives whole;
  a body framed twice is refused. Against a stub origin: a page comes through
  under the origin's own host; a redirect off the origin is passed on and not
  followed, with the stub recording that nothing else was asked of it; a
  redirect back to the origin becomes a path; the listener is on loopback; and
  a request naming another origin never reaches the stub at all.
  `scripts/check-layers.py` passes with `muster-web` in the Services layer,
  depending on no Muster crate.
- **Generated code.** `scripts/gen-protos.sh` re-run; the generated Swift
  carries `WebWindow`, `ForwardOpenRequest` and `ForwardOpened`.
- **Runtime, two machines.** Recorded in
  `docs/validation/WEB-WINDOW-2026-10-22.md`.
- **Not verified here.** The Android screen's change is not built (no Android
  toolchain in this check); no iOS or macOS UI exists for the window yet;
  WebSocket upgrades are refused by design, so a page needing one is untested
  beyond that refusal.
