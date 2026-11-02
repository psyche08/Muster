# ADR 0095: Share AX navigation and reuse verified preparation

- Date: 2026-11-02
- Status: Accepted; installed Codex phone-send baseline verified; broader App acceptance pending
- Related: ADR 0093, ADR 0045, ADR 0046, ADR 0062, ADR 0091
- Partially supersedes: ADR 0039's sidebar-first restriction; exact native
  identity mapping and fresh destination verification remain required.

## Context

The owner asks to continue the earlier AX redesign, particularly failures in
long operation chains. ChatGPT and Claude remain App-owned AX controllers;
event-only plugins and CLI substitution were explicitly rejected. Recent local
logs distinguish pre-submit selection failures near three seconds from a send
whose provider receipt appeared several minutes later. They do not establish
one cause for all failures or prove that watchdog cleanup messages are failures.

Selection currently expands and searches sidebar pages before exact-ID
navigation, and discards its verified surface. The caller scans discovery again
and attachment scans once more. Each additional OS walk can fail during renderer
rebuilds. Preparation can also accept a snapshot that returns after its deadline.

## Decision

Implement the navigation portion of ADR 0093's shared Rust AX executor. Shared
Rust owns stages, bounded readiness, total navigation budget, single dispatch,
input-generation checks and postconditions. Platform callbacks read AX facts,
press an opaque control or dispatch an exact-session OS URL to the verified
bundle. Preserve existing mode, draft, submit, receipt and Locked Use gates.

First reuse an already verified visible session. Otherwise prefer an exact
native session route when identity is available: a canonical Codex thread UUID,
or the uniquely mapped Claude desktop registry ID. This is navigation only,
never proof of acceptance or permission to send. Unsupported/invalid identities
retain the guarded sidebar route. A dispatched route is never retried or switched
to another navigation strategy on an uncertain result. Require fresh AX proof
of the exact original session before continuing.

Codex's existing-thread URL is documented in the official
[App commands reference](https://learn.chatgpt.com/docs/app/commands).
Claude keeps ADR 0040's unique native-registry mapping and existing OS route.

Return the verified surface as an opaque process/session/input-bound token to
attachment, removing redundant discover/attach scans. It is not cached write
authority: sending still resolves current controls and validates the exact draft
immediately before its single submit. Run preparation's synchronous OS work in
the existing guarded blocking-worker framework, retaining its lifetime after
caller cancellation. Refuse snapshots that return after their readiness budget.

No device RPC, protobuf, Apple ABI, CLI lifecycle or authorization/lease duration
changes. No Hook plugins. No automatic resend of an uncertain submission.

## Alternatives

Longer independent timeouts keep repeated scans and navigation-dependent title
failures. Choosing a sidebar row by recency, treating URL dispatch as success,
or reusing a stale element for submission would weaken identity checks. Replacing
the desktop session with a CLI process violates the agreed domain boundary.

## Consequences

Native IDs avoid sidebar pagination and generated-title mismatch on supported
App sessions. Unknown IDs still need the existing UI path. Navigation may open
a provider-managed view; ambiguity, input changes, stale tokens and absent
destination evidence still fail closed. Chat mode keeps its separate navigation
rules. This is an incremental implementation of the prior design, not a claim
that every AX workflow or physical failure scenario has been accepted.

## Verification

Test staged navigation with synthetic host facts: existing target, exact route,
sidebar progression, ambiguity, stale identity/input, budget exhaustion, delayed
snapshots and one dispatch without replay. Run affected Rust suites, formatting,
Clippy, layer/public-content/ADR checks locally. Verify source/native build and
signing separately from dedicated-session App and physical phone/locked-screen
acceptance. Do not dispatch hosted CI. Record remaining acceptance limits.

Local source evidence on 2026-11-02: the three affected crate library suites
passed (290 agent, 38 platform, 97 daemon tests; six ignored). The `deepseek`,
`send_status` and `unconfirmed` integration suites passed seven tests (one ignored).
After adding late-dispatch, late-verification and total-budget regressions, all
11 navigation tests passed. Clippy with warnings denied, targeted formatting,
layer and public-content checks passed. Hosted CI was not dispatched.

Installed-App navigation/send and physical phone/Locked Use acceptance remain
pending: the local console was locked during the read-only probe. Existing
installation logs are diagnostic evidence, not acceptance of this source.
Production build/signing/notarization results are reported separately with the
artifact's source commit; no publication or installation is implied by tests.

### Installation evidence — 2026-11-02

The owner subsequently authorized installation and attended phone acceptance.
The local Mac now runs source `700870b6609b88ac024789c1d6ce727ba02efc12` in
desktop 1.1.11 (101). The existing Rust update lease admitted replacement;
the installer preserved the previous bundle and verified unchanged device
identity, fingerprint and Secure Enclave backend. Accessibility is granted,
Locked Use setup reports ready, Vnet is up and the relay is registered. The
console remains locked. No public feed or iOS release was published.

The first installation correctly failed its version check: the notarized raw
Xcode bundle omitted Velopack's `sq.version` and `UpdateMac`, so the daemon
reported its crate version, 0.1.0. The same clean source was repackaged using
the existing pinned Velopack 1.2.158 process, signed and notarized again, then
installed through a new update lease. Runtime now reports 1.1.11. No Keychain
unlock or credential bypass was used. The raw artifact is superseded.

- Corrected notarization: `cbffb7e9-8f64-492e-b828-30f52d9c8857`, Accepted;
  stapling, strict signature and Gatekeeper checks passed.
- Installed daemon SHA-256:
  `bdb54b24406f2aa7aaef2906393a95384b32f0554c3d89fe96d473acfb8c37d8`.
- Dedicated native-App sending and repeated lock/unlock/navigation/send/relock
  acceptance remain pending. The owner selected an enrolled phone as requester;
  one uniquely marked send was requested, with explicit instructions not to
  resend an unknown result. Installation and ready-state probes do not count
  as a provider send, duplicate check or successful relock-after-send test.

### Phone send and cleanup evidence — 2026-11-02 (Asia/Shanghai)

The owner sent the dedicated Codex App acceptance prompt from an enrolled
phone on the installed `700870b6` build. At 21:26:36 the host issued and verified
the operation's challenge/signature. At 21:26:39 it observed plugin consumption,
Allow and unlock, then admitted the bound operation under the cover.

Exact-ID navigation dispatched once and verified the target in 952 ms; native
preparation took 1,096 ms. The trace records one successful Send press, followed
by provider Admission and a durable `applied` ledger result at 21:26:41. A later
read-only `agent_send_status` query, bound to the original operation/session/text,
returned `APPLIED` with an empty unconfirmed token. No send/replay was issued by
the verifier. History contains one marked user message and one exact expected
assistant response (`MUSTER-AX-1102-CODEX-01-OK`), timestamped 21:26:44 and
21:26:50 respectively. No duplicate was observed in the operation trace or
returned history. Admission is distinguished from the later transcript reply.

At 21:26:41 the owner logged provider-queue and AX-work drain, relocking under
cover, `Relocking -> Idle` with `stop=Completed`, then display-guard release.
A subsequent phone operation to a different Codex App session at 21:27:04–06
also used one exact-ID navigation (840 ms), one successful Send press, an applied
ledger result, drain, relock and cover release. These are two successive normal
operation cycles with different sessions, not a stress or failure-injection run.

The follow-up device query reports `locked=true`, `covered=false`, no active
manual lease/owner and Locked Use setup ready. No watchdog process belonging to
the installed bundle remains. Together with the operation's terminal-state
ordering, this verifies cleanup for this run rather than merely observing an
already locked console. The read-only evidence collection changed no policies,
permissions or Keychain state.

This accepts the Codex App normal phone-send/navigation/relock baseline on one
Mac. Claude App, sustained multi-round load, cancellation/local takeover and
physical multi-display coverage still require separate acceptance. It does not
establish that every previously reported AX failure has been eliminated.
