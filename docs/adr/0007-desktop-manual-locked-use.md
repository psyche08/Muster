# ADR 0007: Desktop manual Locked Use and selection observation

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

- Date: 2026-10-21
- Status: Accepted
- Related: ADR 0006; ARCHITECTURE.md §3.4; LOCKED_USE.md

## Context

On Mac-A 1.0.76, a manual lease held by iPhone remained active while native
Codex sends completed. Mac-B is a different principal and cannot borrow that
lease. Desktop Shared Rust hides the manual lease controls entirely, and the
existing status does not identify the owning device. Separately, a 13:20:09 CST
Codex selection failed before draft mutation: after one AXPress the cached
surface still resolved to the previous session for three seconds. A later
explicit request succeeded. This establishes a selection readiness failure;
it does not prove whether Chromium ignored the press or returned stale AX data.

## Decision

Expose manual lease controls through Shared Rust on desktop as well as phone.
Phones retain their authorized hardware signing path; desktops use the existing
operation-bound Presence effect, expiration and cancellation path. Never use a
software/device-signature fallback for desktop Presence. Keep owner identity,
provider grants and live admission enforced on the target. Show the owning
member's name and actionable refusal guidance; never cancel or borrow a foreign
lease. End only the lease established by this requester.

After a single native navigation press, bounded read-only renderer queries may
refresh observed web areas of the verified app. Re-resolve the exact session
before mutation; ambiguity and changed input/authority still stop execution.
The three-second readiness budget and all signature/lease deadlines stay fixed.
Never repeat navigation presses, draft writes or sends to recover an unknown
outcome. Log query acceptance separately from session readiness.

## Alternatives

Allowing another device to use an existing lease silently changes its principal
and signed scope. Retrying a press or sending to the focused composer risks
wrong-session mutations. Increasing timeouts alone does not refresh stale AX
state. A platform-only manual lease flow duplicates trust decisions.

## Consequences and verification

Test desktop versus phone signature routing, owner display/refusal, cancellation
and expired-auth busy-state cleanup. Test repeated operations preserve the same
manual owner and foreign principals cannot enter it. Verify native selection
readiness and device sends independently of source tests. Release macOS notarized
and iOS OTA together as 1.0.77 (68). Device acceptance must record owner
and nonce continuity, each receipt, and cleanup only after explicit end. Preserve
existing user terminal sessions; require their owner to end them before takeover.

### Installed 1.0.77 observations

Both Apple artifacts were built and published from `bc122a0e3dbbcedcd66062d9d1448c8af060bf3b`.
macOS notarization `cd3f5a01-fb64-4138-a53f-a50d6e542a31` was Accepted.
All three Macs ran this version; Mac-A's Vnet startup remained pending, now
tracked in ADR 0008. This was not a successful network acceptance.

On October 21 at 14:02 CST, Mac-B completed native Presence and established
a manual lease on Mac-A. At 14:03:09 the dedicated Claude acceptance session
received `MUSTER77-REUSE-1` and replied; operation `app-1792562586749560000-5`
was applied. At 14:08:00 `app-1792562877353627000-6` was applied through the
same owner and nonce, with no additional Presence challenge. However, the UI
automation failed to recheck selection and sent that second test to the current
Codex task (`01a15477`), not the dedicated Claude session. This demonstrates
lease reuse/admission but does not pass the intended two-message Claude
acceptance. The test message requested only a short acknowledgment and no tools.
It was not retried. At 14:09:19 Mac-B explicitly ended its lease; target logs
recorded Active -> Relocking -> Idle and gate cleanup. The OS reported Locked.

### Completed 1.0.78 dedicated-session acceptance

Mac-B and Mac-A both ran `a2995688`, version 1.0.78 (69). At 14:26:03 CST,
Mac-B completed native Presence for a fresh manual lease. The dedicated Claude
session `3e9c6153-3ed4-57ae-8024-981cd2f7ec47` then accepted two distinct messages:

- 14:29:38: `app-1792564176933175000-2`, token `MUSTER78-REUSE-A`, applied,
  followed by the matching Claude reply.
- 14:31:15: `app-1792564273267322000-3`, token `MUSTER78-REUSE-B`, applied,
  followed by the matching Claude reply.

Before, between and after these sends, the active owner remained Mac-B and the
nonce was byte-for-byte identical. Only the initial challenge/signature appears
in this interval. At 14:31:35, switching off Mac-B's control produced
Active -> Relocking -> Idle; the target confirmed gate cleanup and cover release.
Independent OS diagnostics reported Locked; owner/nonce were cleared and Vnet
remained up. Native UI guards stopped preparation attempts before any send when
provider or foreground changed. The earlier 1.0.77 selection mistake remains
recorded above; the two 1.0.78 sends both targeted the dedicated session. A UI
selection change was observed during activation/focus changes, but its cause was
not established; no causal claim or untested selection fix is made here.
