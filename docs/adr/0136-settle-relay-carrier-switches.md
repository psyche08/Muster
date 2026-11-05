# ADR 0136: Settle relay carrier switches

- Date: 2026-11-05
- Status: Accepted; source and tests verified, install acceptance pending
- Related: [architecture/network.md](../architecture/network.md) (dual
  carriers, "hysteresis"), [PROTOCOL.md](../PROTOCOL.md) (probe rules),
  [ADR 0135](0135-give-session-watches-the-forwarding-budget.md) (the log
  reading this follows up)

## Context

A desktop keeps two relay carriers, QUIC over UDP and TLS over TCP. It probes
both every 5 s and sends traffic over one of them. `Selection::choose` picked
TLS whenever QUIC's smoothed probe RTT was above 1.25 times TLS's. It then
moved traffic once the other carrier had three probe successes and 10 s had
passed since the last switch.

Mac-A's daemon log for 2026-11-02 to 2026-11-05 shows 799 carrier switches:
364, 203 and 128 on the first three days, and 104 on the partial last day.

- **Switches without a failure.** 367 switches went from QUIC to TLS. 319 of
  them had no relay error within 5 s, so they were RTT decisions, not failover.
- **Quick returns.** Of 360 returns from TLS to QUIC, the median came 11 s
  after the switch out, and 197 came within 10 to 15 s. That is the minimum
  dwell.
- **Leaving again at once.** 32 of the RTT-driven switches to TLS came 10 to
  11 s after the previous switch, as soon as the dwell allowed.

The pattern is an RTT that sits near one line: the same 1.25 ratio decided
both leaving QUIC and coming back to it, with no gap between them. A single
5 s probe sample could start a switch, and 10 s later the next one could
reverse it.

The code shows two more ways the samples were skewed:

- **Resent probes.** A QUIC probe or its reply that is lost is resent after
  1 s with the same nonce, and the reply was timed from the first send. One
  lost datagram therefore produced a sample of more than a second. Each new
  sample counts for a quarter of the smoothed RTT, so that one sample alone
  kept QUIC "slower" for about a minute.
- **The first sample.** A carrier's first sample was its whole connection
  time: DNS, the handshake, registration and the first probe. A carrier that
  had just reconnected looked several times slower than it was.

The log does not record RTTs, so it cannot show how many switches each cause
produced. What a switch costs is not established either: traffic moves to the
other carrier, and the inner QUIC recovers any datagrams lost in between.

## Decision

All of this is in `muster-net::secure_relay`. Failover is unchanged: a carrier
that fails or stops answering probes is left at once.

- **A gap between leaving and returning.**
  - QUIC is left for TLS only when its RTT is more than 1.5 times TLS's
    **and** more than TLS's plus 20 ms.
  - While traffic is on TLS, or nothing is chosen yet, QUIC is preferred
    unless it is more than 1.25 times TLS's and more than TLS's plus 10 ms.
  - The absolute margins keep a few milliseconds on a fast path from counting
    as "slower".
- **A preference must settle.** A switch that RTT drives happens only after
  the other carrier has been preferred, without a break, for `SETTLE` (30 s),
  with at least three probe successes. This takes the place of the 10 s
  dwell. After any switch, the next one that RTT drives is therefore at least
  30 s away.
- **Clean samples.**
  - A reply to a resent probe proves the carrier is live but is not an RTT
    sample (Karn's rule).
  - A carrier's first sample is the round trip of its first full-frame probe,
    not its connection time.
- **Switches name their reason.** The switch log line keeps its prefix and
  adds both RTTs, for example `relay carrier Some(Quic) -> Some(Tls) (rtt
  quic=84ms tls=25ms)`. The next log reading can then tell an RTT decision
  from a failover.

Phones (`Strategy::Fallback`) run one carrier at a time, so they rarely have
two live carriers to choose between. The resent-probe and first-sample rules
apply to them too.

## Alternatives

- **Only lengthen the dwell.** This slows the flapping but keeps switching on
  a single sample.
- **Never switch for RTT, only on failure.** This would be simpler, but TCP
  that is really faster, for example where UDP is shaped, would never carry
  traffic. The architecture keeps RTT selection with hysteresis.
- **Compare minimum RTTs instead of smoothed ones.** This is less affected by
  load on the active carrier, but it needs a sample window and has no data
  yet to tune it against. With the RTTs now logged, it can be revisited.

## Consequences

- RTT-driven switches should be rare and at least 30 s apart, and failovers
  are as fast as before.
- After a failover, traffic returns to the recovered carrier no sooner than
  30 s after it is preferred again, rather than about 10 s. In the meantime
  the other carrier, which is working, carries it.
- A path whose QUIC is consistently slower only by less than the gap stays on
  QUIC.
- Relay RTTs shown elsewhere (`carrier_rtt`) no longer include connection
  time or retry delays.

## Verification

- **Source and tests:**
  - `rtt_near_the_line_or_briefly_high_keeps_the_carrier`:
    - QUIC at 40 ms against TLS at 25 ms stays on QUIC.
    - QUIC at 80 ms stays on QUIC when it lasts 25 s at a time, and moves to
      TLS exactly 30 s after the slowdown begins.
    - QUIC at 34 ms is back in favour and returns after 30 s.
    - With the gap removed (1.25 and 0 ms both ways), the test fails.
  - `selection_failure_hysteresis_and_rtt` now settles for 30 s. A failure
    still switches at once.
  - The integration test `lost_probe_is_retried_inside_original_deadline_without_reconnecting`
    also asserts that the resent probe's reply left the RTT unchanged.
  - All `muster-net` secure-relay tests pass.
- **Not done:** building, signing and installation. For install acceptance,
  read three days of the installed daemon's log. Most switch lines should then
  follow a relay error, and the RTTs on the remaining lines should show which
  rule moved them.
