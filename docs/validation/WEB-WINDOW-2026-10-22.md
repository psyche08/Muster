# Web window across two machines — 2026-10-22

What ran: the web window (ADR 0026) on one Mac, reached from another over a
forward stream, against the scheduler's real internal web origin. That host is
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

## What was verified

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
- Cookie and HSTS rewriting was exercised by tests against a stub origin, not
  live: the pages fetched here set no cookie and sent no HSTS header.

## Cleanup

Both development daemons were stopped and their state directories removed;
the `wwtest` network exists nowhere else. Production state on both machines
was untouched throughout.
