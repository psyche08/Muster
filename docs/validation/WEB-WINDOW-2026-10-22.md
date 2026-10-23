# Web window across two machines — 2026-10-22

What ran: the web window (ADR 0026) on one Mac, reached from another over a
forward stream, and the bridge operation that hands a request to the person's
own browser. The scheduler's real internal web origin is
withheld here (docs/PUBLIC_CONTENT.md) and written below as
`https://scheduler.example`; every command was run against the real one. This
is a runtime record for that path only; it is not a release, signing or
installation record.

## Environment

- Both daemons are development builds of this branch (software key, no Secure
  Enclave), in their own state directories and their own network `wwtest`
  (`a2286c4c2a5a287a`), with vnet, the agent service and terminals off.
  Neither machine's installed production daemon was stopped, changed or used.
- `m4pro-ww` (`NN5C-HNV`, admin) on M4 Pro holds the window.
  `m1max-ww` (`6Z7W-SVE`, member) on M1 Max is the reader, standing in for a
  phone.
- They paired through the existing relay for rendezvous and then linked
  **direct**, 8–9 ms (one later request went over the relay instead, with the
  same result).
- The member was approved with exactly one scope: `net:tcp/8810`.

## Round one: the forward path and both halves of its gate

- **The window serves the origin.** On the holder,
  `muster policy --web-origin https://scheduler.example --web-port 8810
  --open-port tcp/8810` prints
  `web window: https://scheduler.example on 127.0.0.1:8810`, and a local
  `curl http://127.0.0.1:8810/` answers `200 text/html; charset=utf-8`.
- **The reader gets the page through a forward stream.**
  `muster forward NN5C-HNV 8810 --local-port 18810` prints
  `127.0.0.1:18810 stands for 127.0.0.1:8810 on 6b7a23b6b9f2`, and on the
  reader `curl http://127.0.0.1:18810/` returns **200, 436 600 bytes in
  0.31 s**, `server: openresty`, `x-powered-by: Next.js`, chunked, with
  `<title>Multica — Project Management for Human + Agent Teams</title>`.
  The reader has no route to that origin itself.
- **A same-origin redirect stays in the window.** `/issues` answers
  `307` with `location: /login?next=%2Fissues` — a path, not the origin's URL.
- **A request naming another origin is refused at the window**, over the
  forward stream: an absolute request target gets
  `HTTP/1.1 400 Bad Request` and the origin is never asked.
- **Both halves of the gate refuse, separately.**
  - Port closed on the holder (`--open-port tcp/22`, which drops 8810):
    `forward 127.0.0.1:8810 denied for f67f6954: refused by local policy`,
    and the reader's connection resets.
  - Scope changed to `net:tcp/22`:
    `forward 127.0.0.1:8810 denied for f67f6954: scope not granted`.
  - Each restored, and the page came back.

**What this round no longer evidences.** The page above was fetched by an HTTP
client inside the daemon. That upstream is gone: the window now hands every
request to the person's browser. So those 436 600 bytes stand as evidence for
the forward path, the gate and the window's refusals — all unchanged — and not
for how a page is fetched. Round two is that part.

## Round two: the browser as the upstream

- This branch's native host was registered in place of the installed one (the
  previous manifest kept) and the old host process stopped, so the person's
  own Chrome respawned this branch's host. `{"op":"ping"}` through it answered
  `agenthalo.browser` 0.2.0, Chrome 155: the new host serves the real browser,
  with the per-operation deadlines and the wider frame.
- A `fetch` operation, sent through a development daemon sharing that bridge,
  reached the extension in that Chrome and came back
  `Error: "unsupported operation"` — the service worker's own refusal. The
  whole chain carries it: CLI → daemon → bridge socket → native host →
  Chrome → the extension. Only the last hop is old code.
- Compatibility in the other direction was seen too: while the installed host
  was still registered, it refused the operation by name
  (`unknown variant 'fetch'`) rather than mishandling it.

**Pending, and it is the person's to do.** The extension that answered is the
installed 0.2.0 from the app bundle; this branch's `fetch` lives in
`src/browser/agenthalo-extension`. Loading it, and granting that one site, is
done in Chrome's own interface — and the session the window exists to reuse is
the person's. Two steps: load this branch's extension unpacked in place of the
installed one, then give it site access for the scheduler's origin. The live
round then repeats round one's chain with the browser as the upstream.

## Round three (2026-10-23): no TCP port, the browser's scopes

The window moved off loopback TCP onto a Unix socket, and members onto a `web`
service stream decided by `browser.view` / `browser.control` (ADR 0026,
revised). Same two Macs, roles reversed so the window sits with the signed-in
Chrome: `wb-dev` (`HHEF-HYP`, admin) on M1 Max holds the window, sharing the
real bridge socket through a symlink; `m4pro-reader` (`7E2F-PRM`) on M4 Pro
reads. Development builds of this branch on both, own network `wbtest`
(`3c2b5daeed2cbc55`), linked direct at 10–17 ms.

- **No TCP listener.** `muster policy --web-origin https://scheduler.example`
  prints `web window: https://scheduler.example through this machine's
  browser` and `members read with browser.view and act with browser.control;
  not while the screen is locked`. The window is
  `drwx------ …/web` / `srw------- …/web/window.sock`, and
  `lsof -iTCP:8810 -sTCP:LISTEN` finds nothing.
- **The owner, through the socket.**
  `curl --unix-socket …/web/window.sock http://window/` reaches the browser:
  `502` with `the browser did not answer: "unsupported operation"` — the
  installed extension's refusal, as in round two.
- **A port scope is no way in.** With the reader holding only
  `net:tcp/8810`, `muster window HHEF-HYP https://scheduler.example
  --local-port 18810` opens its local port, and a request on it answers
  `403` with `the browser is not lent to you: scope not granted`; the holder
  logs `web window … refused f93457c5: the browser is not lent to you: scope
  not granted`. (The first try showed the 403 but lost the reason to a reset:
  the refusal closed the socket with the request unread. Fixed by reading the
  request before answering, then shutting down cleanly.)
- **`browser.view` reads and does not act.** Scopes set to `browser.view`
  alone: a GET reaches the browser (`502`, `"unsupported operation"` — the
  same last hop); a POST answers `403` with `this reader may read the site
  through the window, not act on it`, refused before the browser is asked.
- **Not exercised live:** the window while the holder's screen is locked
  (covered by tests; locking a machine someone is working on is not a test
  step), and `browser.control` acting through the window, which needs the new
  extension too.

The last hop is still the person's step: `ping` through the bridge reports
`version 0.2.0`, while this branch's extension is 0.3.0 at
`src/browser/agenthalo-extension` in this repository's checkout. Two older
copies exist on that machine — the earlier repository's and the app bundle's —
both 0.2.0 and without `fetch`.

## Found while verifying

The page's own HTML references **87 URLs on a separate corporate CDN origin**
(fonts, the Next.js asset bundle) and a few on two further internal hosts. One
window onto the scheduler's origin therefore delivers the document but not its
styling to a device that cannot reach those other origins. A reader on the
corporate network is unaffected. Carrying a page whose assets live elsewhere
needs a window per origin, which the current policy field (one window) does
not express — the next slice's problem, recorded here so it is not discovered
in front of a phone.

## Not covered

- No phone: the apps have no WebView and no in-app listener yet, so nothing
  here exercises iOS or Android.
- No WebSocket: upgrades are refused by design (501), so the site's live
  updates were not exercised.
- No signed build, no installation, no production daemon involvement.
- Cookie and header handling was exercised by tests, not live: the window
  drops `Set-Cookie` and passes on no reader's `cookie`, and the extension
  refuses a caller-supplied one.

## Cleanup

All development daemons were stopped and their state directories removed on
both machines; neither `wwtest` nor `wbtest` exists anywhere else. The
native-host manifest was re-registered by the installed app's own helper and
is byte-identical to the one kept before round two; Chrome respawned the
installed host, and `ping` through it answers `agenthalo.browser` 0.2.0.
Production state on both machines was untouched throughout.

**The last hop waits for a release.** The person chose to verify the
browser's own `fetch` against the real site once an installed Muster includes
this change — extension, native host and daemon as one signed build — rather
than with an unpacked copy. Installed today: Muster 1.0.87 (build 78), whose
bundled extension is 0.2.0 without `fetch`.
