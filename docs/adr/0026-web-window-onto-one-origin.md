# ADR 0026: A web window onto one origin, fetched by the person's own browser

- Date: 2026-10-22
- Status: Proposed
- Related: ARCHITECTURE §3.4 (authorization), §3.5 (forward streams),
  §3.9 / docs/MODULES.md (layers), ADR 0009 (Muster as a scheduler's runtime),
  `crates/muster-web`, `crates/muster-browser`, `src/browser/agenthalo-extension`

## Context

The work in ADR 0009 made this machine a runtime for an external scheduler
(Multica). The scheduler's own web interface lives on an internal origin that
a desktop here can reach and a phone cannot: the phone has no route to it, and
no account on it either. Reading an issue, watching a run, answering a comment
therefore stops at the desk, which is the opposite of what the rest of Muster
does.

Forward streams already carry exactly this shape of traffic — "a member asks
the target to connect it to one of the target's own loopback ports" — and the
rule they are built around is that nothing is forwarded on to a third machine.
A member cannot ask a desktop to connect it to `scheduler.example`, and that
restraint is deliberate: a hop that connects a member to any address it names
is an open relay out of the network, authorized once and usable for anything
afterwards.

So the missing piece is not a change to forward streams. It is something on
the desktop's own loopback that a forward stream already reaches, and that is
itself held to one destination.

**Reaching the site is only half of it.** A page served from
`127.0.0.1:8810` is a different origin, so the reader's browser holds no
session for the real one — and nothing a daemon can do changes that. A
credential of the daemon's own would mean a second copy of the person's
session, kept worse than the browser keeps it, and an internal sign-on flow is
not something a fetch library completes. The machine already holds a working
session for that site: the one in the browser the person is signed in with,
which Agent Halo already talks to through its extension.

ARCHITECTURE keeps "browser access as a Muster web client" out of scope. This
is not that: Muster serves none of these bytes and gains no web UI of its own.
It carries somebody else's site to a device that cannot reach it.

## Decision

A **web window** (`crates/muster-web`): one loopback port on this machine that
stands for exactly one remote origin, whose requests are made by the person's
own browser.

- **One origin by construction, not by inspection.** The window is built for
  one origin and asks for no other. A request naming a second one is refused
  rather than rewritten — an absolute request target (`GET http://host/…`) is
  a 400, `CONNECT` a 405. The browser follows redirects as a browser does, so
  the window checks the *destination* afterwards: a reply that ended off the
  origin is refused with 502 and its body is never served. Ending elsewhere on
  the same origin is passed to the reader as a redirect (302, or 303 after a
  non-GET), so its address and the page's relative links agree.
- **The browser fetches, not the daemon.** `muster-web` fetches nothing; it
  hands each request to an `Upstream`. In Muster that is `musterd::web`, which
  calls the Agent Halo bridge's new `fetch` operation — one request, made by
  Chrome as itself, opening no tab and touching nothing on screen.
- **The session stays in the browser.** The reader sends no cookie (`cookie`,
  `authorization` and `host` are not passed on, and the extension protocol
  refuses them) and is given none: `Set-Cookie` is dropped, which `fetch`
  does not expose anyway. A reader needs no account on the site and cannot
  come away with one.
- **Chrome's own site access is a third key.** The extension asks for no host
  permission when it is installed (`optional_host_permissions`, nothing
  required). The `fetch` operation checks `permissions.contains` for the
  origin and refuses with a legible message until the person grants that one
  site in `chrome://extensions`. So the browser's own gate stands next to
  Muster's.
- **Local configuration, off until asked for.** `LocalPolicy::web_window` is
  `Option<WebWindow>`: an origin and the loopback port it listens on, `None`
  by default. A policy stored before the field existed reads as `None`.
- **Opening the window is not letting anyone in.** The window's port is not
  implicitly open. A member reaches it only under the gate that already
  governs every forward: `net:tcp/<port>` granted in the roster and the port
  open in this machine's policy, decided by `authorize_port`. Two decisions,
  taken separately: `muster policy` says so when the window has no open port,
  rather than opening one on the owner's behalf.
- **Two headers are rewritten and two are dropped**, because the page is now
  served from a loopback port and not from the origin it came from:
  `Strict-Transport-Security` is dropped (a pin is kept per host, so one
  site's header would force HTTPS on every other service a developer runs on
  `127.0.0.1`), `Content-Security-Policy` is dropped (it names the origin the
  page no longer has, and would forbid the page its own scripts and styles),
  and a `Location` back to the origin becomes a path.
- **The accessor side becomes product code.** `ForwardOpenRequest` is a local
  call that binds a loopback port here and gives every connection on it its
  own forward stream to the member (`muster forward <device> <port>`). It is
  `Access::LocalOnly`: this machine reaching into a member's loopback is the
  local owner's to ask for, and the member still decides its own side. The
  call streams `ForwardOpened` with the bound port and holds the port until it
  is cancelled. This is the same surface a phone's WebView will need, so it is
  written once, in the daemon, rather than in each shell.
- The bridge gains **one deadline per operation** rather than one for all:
  driving a tab already on screen stays at 3 seconds, reaching a site gets 20,
  and a `fetch` frame may be larger (768 KiB) than an operation that only
  names a node.

The Android settings screen now edits the policy it holds instead of building
a fresh one, so saving its three switches no longer drops the fields it does
not show — which already lost `offer_tasks` and would have closed the window.

## Alternatives

- **Let a forward stream name a host.** One flag on an existing request, and
  the end of the property that makes forward streams safe to grant. Rejected:
  it is the open relay.
- **The daemon fetches the origin itself** (an HTTP client in `muster-web`,
  which is how this record's first draft stood). It works until the site wants
  a session: the reader lands on a sign-in page it cannot complete, and giving
  the daemon its own cookie jar would put a second, worse-kept copy of the
  person's credentials on disk. The browser already holds one, and holds it
  properly.
- **An HTTP proxy on loopback (`CONNECT`, absolute URIs).** What a browser's
  proxy setting expects, and already general-purpose: whoever reaches it
  chooses the destination. Rejected for the same reason as the first
  alternative, and it needs a browser-wide setting where a window needs a URL.
- **A proxy that checks the `Host` header.** Single-origin by inspection, so
  one parsing mistake widens it. The window holds its origin in the URL it
  asks for and in the destination it checks on the way back, where no request
  text reaches.
- **Drive a tab with the debugger** (`Page.navigate` plus a DOM read), which
  Agent Halo can already do. Rejected: it moves the person's own browser
  around, carries one page at a time rather than a page's sub-resources, and
  returns a rendering rather than the bytes a reader's browser needs.

## Consequences

- A phone, once the shells carry a WebView and a listener of their own, can
  read an internal site through a desktop that can reach it, **as the person
  already signed in there**. That slice is deliberately not in this change:
  the daemon side it needs is here.
- **Whoever can reach the window acts as that signed-in browser, for that one
  origin.** That is the point, and it is why the window is off by default,
  names one origin, does not open its own port, and sits behind both the
  roster scope and Chrome's own site access. It is a wider grant than
  forwarding SSH to a phone, and the record says so rather than leaving it
  implied.
- Chrome must be running with the extension connected; otherwise the window
  answers 502. The bridge serves one operation at a time, so a page's
  sub-resources arrive one after another — fine for reading, slow for a heavy
  app.
- A reply is bounded at 5 MiB and a request body at 512 KiB (the extension
  frame). Large downloads and uploads do not go through the window.
- Protocol upgrades are refused (501): no WebSocket, so a page that depends on
  one for live updates will not update live.
- One window carries one origin, and the policy holds one window. A page whose
  assets live elsewhere arrives unstyled on a device that cannot reach that
  other origin too — the scheduler's own HTML names 87 URLs on a separate
  corporate CDN origin. Carrying such a page end to end needs a window per
  origin; that is the next slice's problem, and single-origin windows are what
  make it a listable decision rather than a general tunnel.
- An older native host refuses the new operation by name instead of
  mishandling it, so a mixed install fails closed.
- Nothing about this is automatic: a machine updated to this version without
  being asked has no window, no open port, no granted site and no new
  reachability.

## Verification

- **Source/tests.** `cargo clippy --workspace --all-targets` clean;
  `cargo test -p muster-web -p muster-core -p musterd` green. The window's own
  tests, against a stub upstream that records what it was asked: a page comes
  through under the origin's own URL while the reader's `host` and `cookie`
  are not passed on; `Set-Cookie`, HSTS and CSP never reach the reader; a
  reply that ended on another origin is refused with 502 and its body is not
  served; ending on another path of the same origin becomes 302 (303 after a
  POST); an absolute target and `CONNECT` never reach the upstream at all; one
  connection carries two requests. `node --test` in the extension: a window
  request is refused until the person grants that site and nothing is fetched
  before then, the granted request goes with `credentials: include` and no
  caller-supplied cookie, a reply over 5 MiB is refused rather than truncated,
  and the manifest asks for no host permission at install time.
  `scripts/check-layers.py` passes with `muster-web` in the Services layer,
  depending on no Muster crate.
- **Generated code.** `scripts/gen-protos.sh` re-run; the generated Swift
  carries `WebWindow`, `ForwardOpenRequest` and `ForwardOpened`.
- **Runtime.** Recorded in `docs/validation/WEB-WINDOW-2026-10-22.md`: the
  forward path and both halves of its gate were verified across two Macs, and
  a `fetch` operation reached the live extension in the person's own Chrome
  through the new native host. The last hop — the extension's own `fetch`,
  with the browser's session — awaits the branch's extension being loaded in
  that Chrome, which the person does themselves.
- **Not verified here.** The browser-side fetch against the real site (above);
  the Android screen's change is not compiled (no Android toolchain in this
  check); no iOS or macOS UI exists for the window yet; WebSocket upgrades are
  refused by design.
