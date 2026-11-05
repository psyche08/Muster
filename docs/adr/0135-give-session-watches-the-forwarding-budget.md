# ADR 0135: Give session watches the forwarding budget and report why they ended

- Date: 2026-11-05
- Status: Accepted; source and tests verified, install acceptance pending
- Related: [architecture/network.md](../architecture/network.md),
  [ADR 0114](0114-restore-unlanded-1-0-21-field-fixes.md) (direct-path
  cooldown on the same links)

## Context

Desktop clients often showed a blank conversation for sessions hosted on another
Mac. The banner read "unavailable: session watch timed out. Reconnecting…". We
read three Macs' daemon logs for 2026-11-02 to 2026-11-05: Mac-A, which hosts
the sessions, and Mac-B and Mac-C, which are desktop clients.

### Two 3-second budgets on one path

- **The app's budget.** The app waits 3 s for a watch's first event, the
  `AgentWatch` State that marks the subscription.
- **The proxy's budget.** On a desktop the call goes through the local
  daemon's proxy. The proxy first opens a stream to the member, which
  `Node::open` bounds by `CONNECT_TIMEOUT` (3 s), and only then forwards the
  call.

A connection that takes close to 3 s therefore always times out in the app
first. The proxy's own answer arrives later and is discarded.

Over the three days, the proxies of Mac-B and Mac-C logged these failures
toward Mac-A:

| Client | Connect timeouts | "offline: unreachable" | Refused by peer | Ready after more than 1 s |
|---|---|---|---|---|
| Mac-B | 303 | 43 | 24 | 21 |
| Mac-C | 601 | 175 | 32 | 26 |

### The real error was not shown

When the proxy answers with an error response, the call's event stream closes.
The app then reported "session watch ended before initial state" and never read
that response, so the banner could not say whether Mac-A was restarting or the
client had lost its network.

### Triggers in the logs

- The clients' own outages. One hour on Mac-C had 431 timeouts, with "No route
  to host" and DNS-over-HTTPS failures.
- 30 daemon restarts on Mac-A, from development installs. Each left Mac-A
  offline for 10 to 25 s.
- Relay carrier switching. This happened hundreds of times a day on each Mac.
  It is real, but its effect on these failures is not established.

### Silent cached streams

The proxy reuses its cached stream to a member for as long as the stream is not
closed. A path that has gone silent is noticed only at QUIC's 15 s idle
timeout. The phone's embedded backend caches member clients in the same way.
This is a property of the code. The logs do not show which failures it caused.

## Decision

- **A first-event budget that covers the connection.** Session and catalog
  watches wait `CONNECT_TIMEOUT + 3 s` for their first event (`WATCH_START`).
  This keeps the old 3 s for the remote's answer and adds room for the
  proxy's connection.
- **Report why a watch ended.** When a session watch's stream ends before its
  first event, read the call's final response (for at most 1 s) and report its
  error. The catalog watch keeps its fallback to a snapshot request for older
  daemons.
- **A banner the user can act on.** If the daemon's presence says the member is
  offline or unreachable, the chat banner reads "\<name\> is offline.
  Reconnecting…". Otherwise it shows the error, with the device id replaced by
  the member's name.
- **Reconnect when the member returns.** The open chat may be showing a failure
  for a member whose presence changes from offline or unreachable back to
  available. Its watch then restarts at once, instead of waiting out a backoff
  of up to 8 s.
- **Forget silent streams.** In the proxy, a forwarded call may be cancelled
  before the target sent anything, with no response arriving in the
  cancellation grace. The proxy then forgets the cached stream, if it is still
  the same connection, so the next call opens a fresh one. Calls already on
  the old stream are left alone. In the embedded backend, a watch whose first
  event times out drops the cached member client (`Backend::discard`).
  `Client::same_connection` compares connections, and `with_target` copies
  count as the same.

## Alternatives

- **Only raise the timeout.** This would make the error appear rarely, but
  would not make it accurate.
- **Probe an idle stream before reuse.** This adds a round trip to the first
  call after any idle period, and the protocol has no ping request.
- **Close the silent client.** This would also end other forwarded calls on it,
  such as the catalog watch, on a link that may still be alive.
- **Classify errors by matching their text.** The offline verdict already comes
  from Rust-owned presence, so the banner only substitutes the device name.
- **Add hysteresis to relay carrier switching, and persist the GUI app's
  logs.** These are separate follow-ups and are not part of this record. They
  became [ADR 0136](0136-settle-relay-carrier-switches.md) and
  [ADR 0137](0137-keep-the-desktop-apps-log-in-a-file.md).

## Consequences

- A remote that accepts the call but never answers now takes 6 s rather than
  3 s before a retry.
- Forgetting a stream that was healthy costs one extra stream open.
- The banner now names the cause, for example "Studio: io: timeout: connect (3
  seconds). Reconnecting…" or "Studio is offline. Reconnecting…".

## Verification

- **Source and tests:**
  - `muster-rpc`:
    `silent_target_link_is_not_reused_after_a_cancelled_call`. It failed when
    the eviction line was disabled and passes with it.
  - `muster-app-core`:
    `a_watch_ended_before_state_reports_the_forwarding_error` and
    `a_chat_names_its_offline_mac_and_rewatches_when_it_returns`.
  - Existing chat, watch and proxy tests pass.
  - Clippy passes with `-D warnings`.
- **Not done:** building, signing, installation, and runtime acceptance. The
  acceptance run is: on a desktop client, open a session hosted on a Mac whose
  daemon restarts; the banner should name that Mac, and the conversation should
  load as soon as the Mac is back.
