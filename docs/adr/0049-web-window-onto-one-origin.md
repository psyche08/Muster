# ADR 0049: A web window onto one origin, fetched by the person's own browser

- Date: 2026-10-22 (revised 2026-10-23)
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

Forward streams carry a related shape of traffic — "a member asks the target
to connect it to one of the target's own loopback ports" — and the rule they
are built around is that nothing is forwarded on to a third machine. A member
cannot ask a desktop to connect it to `scheduler.example`, and that restraint
is deliberate: a hop that connects a member to any address it names is an open
relay out of the network, authorized once and usable for anything afterwards.

**Reaching the site is only half of it.** A page served from anywhere but its
own origin carries no session for that origin, and nothing a daemon can do
changes that. A credential of the daemon's own would mean a second copy of the
person's session, kept worse than the browser keeps it, and an internal
sign-on flow is not something a fetch library completes. The machine already
holds a working session for that site: the one in the browser the person is
signed in with, which Agent Halo already talks to through its extension.

**And lending that session is lending the browser.** Whoever can talk to the
window acts on the site as the signed-in person. So the window must not be
something any process on the machine can connect to, and who may use it is the
same question Muster already answers for the browser — not a question about a
port number.

ARCHITECTURE keeps "browser access as a Muster web client" out of scope. This
is not that: Muster serves none of these bytes and gains no web UI of its own.
It carries somebody else's site to a device that cannot reach it.

## Decision

A **web window** (`crates/muster-web`): one origin, fetched by the person's
own browser, reachable by members through a service of its own and by the
owner through a Unix socket — never on a TCP port.

- **No TCP port, two doors.** The window listens on
  `<state>/web/window.sock`: a socket of mode 0600 in a directory of mode 0700,
  so only the machine's own user can open it, and a socket another process
  still serves is refused rather than replaced. Members never touch that
  socket. They open a `web` service stream (version 1) naming the origin; the
  daemon hands the stream to the window in process (`Window::serve`). Nothing
  on the network and no other user's process on the machine can reach it, and
  it is no longer part of the vnet's surface or the open-port policy.
- **The browser's permission is the window's permission.** The window lends
  the browser, so `Permission::Browser` decides, unchanged: `browser.view`
  reads through the window (GET, HEAD, OPTIONS), `browser.control` acts
  through it (any method), both under the machine's `offer_agent`. A
  `net:tcp/<port>` scope — the earlier way in — is no way in at all.
- **While the screen is locked: `allow_locked_control`.** The browser's lock
  rule applies to the window as it is: locked, it is lent only if the owner
  offers control while locked. Unlike a `browser.call`, a window request does
  not also need `locked.control` and a Presence signature per request: one
  page is dozens of requests, and the window opens nothing on screen and
  drives nothing in the browser. A refusal while locked says so in those
  words, because the same member is let in again once the screen unlocks.
- **Asked per request, not per connection.** The grant is a `Gate` the window
  asks before every request on a stream, so a scope revoked, a policy changed
  or a screen locked while a browser keeps a connection open stops the next
  request on it. A reader with `browser.view` is refused a POST before the
  browser is ever asked.
- **One origin by construction, not by inspection.** The window is built for
  one origin and asks for no other. A request naming a second one is refused
  rather than rewritten — an absolute request target (`GET http://host/…`) is
  a 400, `CONNECT` a 405 — and a `web` stream naming another origin is told
  there is no such window. The browser follows redirects as a browser does, so
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
- **Chrome's own site access stands beside Muster's.** The extension asks for
  no host permission when it is installed (`optional_host_permissions`,
  nothing required). Its `fetch` checks `permissions.contains` for the origin
  and refuses with a legible message until the person grants that one site in
  `chrome://extensions`. Its version moves to 0.3.0, so `ping` can tell which
  copy Chrome is running.
- **Local configuration, off until asked for.** `LocalPolicy::web_window` is
  `Option<WebWindow>` holding the origin alone, `None` by default. A policy
  stored before the field existed reads as `None`, and one stored with the
  earlier draft's port still loads.
- **Fit for where the page now arrives.** `Strict-Transport-Security` is
  dropped (a pin is kept per host, so one site's header would force HTTPS on
  every other service the reader runs on that host), `Content-Security-Policy`
  is dropped (it names the origin the page no longer has, and would forbid the
  page its own scripts and styles), and a `Location` back to the origin
  becomes a path.
- **The accessor side becomes product code.** `ForwardOpenRequest` is a local
  call that binds a loopback port on the *reader's* machine — a browser there
  still speaks TCP — and gives every connection on it its own stream to the
  member: a forward stream for a `port`, a `web` stream for a `window`
  (`muster forward <device> <port>`, `muster window <device> <origin>`). It is
  `Access::LocalOnly`; the member decides its own side. A refused window
  connection is answered as the browser reading it will show it — a 403 with
  the member's reason — rather than reset.
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
- **The window on a loopback TCP port, reached by a forward stream** (this
  record's second draft). Loopback TCP carries no identity: every process on
  the machine, of any user, could connect to the port and use the person's
  session with no gate at all, and the gate members did pass was about a port
  number rather than about lending the browser. Rejected for the socket and
  the service stream above.
- **The daemon fetches the origin itself** (this record's first draft). It
  works until the site wants a session: the reader lands on a sign-in page it
  cannot complete, and giving the daemon its own cookie jar would put a
  second, worse-kept copy of the person's credentials on disk.
- **A new `web` scope instead of the browser's.** More precise, and one more
  word in the roster vocabulary that older versions do not know. The window
  does lend the browser; its existing view/control split is exactly the
  read/act split the window needs.
- **The browser's full lock rule** (`locked.control` plus a Presence signature
  per request). Correct for driving a page on a locked screen; unusable for a
  page made of dozens of requests, and stronger than reading through `fetch`
  needs. The owner's `allow_locked_control` is the switch.
- **An HTTP proxy on loopback (`CONNECT`, absolute URIs).** Already
  general-purpose: whoever reaches it chooses the destination. Rejected for the
  same reason as the first alternative.
- **Drive a tab with the debugger** (`Page.navigate` plus a DOM read), which
  Agent Halo can already do. Rejected: it moves the person's own browser
  around, carries one page at a time rather than a page's sub-resources, and
  returns a rendering rather than the bytes a reader's browser needs.

## Consequences

- A phone, once the shells carry a WebView, can read an internal site through
  a desktop that can reach it, **as the person already signed in there**. That
  slice is not in this change; the daemon side it needs is. A phone's WebView
  is best served by a URL-scheme handler calling into Rust, which needs no
  listening socket at all; the loopback port the accessor side binds today is
  for a desktop browser.
- **Granting `browser.view` now also lends the window for reading, and
  `browser.control` for acting.** Both scopes already lent the browser; this
  is the same lending through a narrower door. It is why the window is off by
  default, names one origin, and sits behind Chrome's own site access as well.
- The owner reaches the window through the socket with every grant; that is
  the same trust as the daemon's own socket (ARCHITECTURE D3).
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
  being asked has no window, no granted site and no new reachability.

## Verification

- **Source/tests.** `cargo clippy --workspace --all-targets` clean;
  `cargo test -p muster-web -p muster-core -p musterd -p muster-browser`
  green. The window, against a stub upstream: the socket is 0600 in a 0700
  directory and a live one is not replaced; a page comes through under the
  origin's own URL while the reader's `host` and `cookie` are not passed on;
  `Set-Cookie`, HSTS and CSP never reach the reader; a reply that ended on
  another origin is refused with 502; ending on another path becomes 302 (303
  after a POST); an absolute target and `CONNECT` never reach the upstream;
  one connection carries two requests; a read-only reader gets the page and is
  refused a POST before the browser is asked; a grant withdrawn mid-connection
  stops the next request. The daemon: `browser.view` reads, `browser.control`
  acts, a port scope gets nothing, a locked screen refuses unless
  `allow_locked_control`, and `offer_agent` off closes it. The bridge: a
  caller may not set `cookie`/`host`, a GET is not a mutation and a POST is,
  and each operation carries its own deadline and frame. `node --test` in the
  extension: nothing is fetched before the person grants the site, the granted
  request goes with `credentials: include` and no caller-supplied cookie, an
  oversized reply is refused, and the manifest asks for no site access at
  install time. `scripts/check-layers.py` passes with `muster-web` in the
  Services layer, depending on no Muster crate.
- **Generated code.** `scripts/gen-protos.sh` re-run with the pinned
  generator.
- **Runtime.** Recorded in `docs/validation/WEB-WINDOW-2026-10-22.md`, three
  rounds across two Macs: the forward path and its gate (first draft); a
  `fetch` reaching the live extension in the person's own Chrome through the
  new native host; and this design — no TCP listener, the owner through the
  socket, a member with only a port scope refused with its reason, a member
  with `browser.view` reaching the browser with a GET and refused a POST.
- **Not verified here.** The browser-side fetch against the real site, which
  awaits this branch's extension (0.3.0) being loaded in that Chrome — the
  person's to do; the window while the screen is locked, which is covered by
  tests rather than by locking a machine someone is using; the Android
  screen's change is not compiled (no Android toolchain in this check); no iOS
  or macOS UI exists for the window yet; WebSocket upgrades are refused by
  design.
