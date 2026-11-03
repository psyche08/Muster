# ADR 0109: Restore two 1.0.21 field fixes that never landed

- Date: 2026-11-03
- Status: Accepted; source verified, release pending
- Related: [PROTOCOL.md](../PROTOCOL.md) (direct-path selection),
  [QUESTION_CARDS.md](../QUESTION_CARDS.md),
  [architecture/network.md](../architecture/network.md)

## Context

An audit of local branches on 2026-11-03 found `claude/v1021-field-fixes`
on one desktop (Mac-B). It holds two commits from 2026-10-01, written after
field failures on desktop 1.0.21. The branch was never pushed. Neither change
reached the private archive's main or the public tree: their identifiers
(`DIRECT_COOLDOWN`, `report_failure`, `form_miss`) appear in neither. The
owner asked for both to be restored.

The branch sits on the archive's history, which shares no commits with this
repository, and the code has moved since. The fixes therefore had to be
reapplied to the current code rather than cherry-picked.

1. **A direct path that answers probes but cannot carry QUIC.** On one desktop,
   members' home addresses were routed into a local 198.18.0.0/15 fake-IP proxy
   tunnel. The proxy relayed PING/PONG, so magicsock adopted it as a direct
   path. Every QUIC packet moved onto it, and the dial failed 3 seconds later.
   This repeated every 30 seconds, since the fixed cooldown never grew.
   Separately, a member that could not be reached at all was logged only at
   debug level.
2. **Question cards rejected without explanation.** Answers to multi-question
   Claude cards and to Codex cards were rejected before any submit, and the
   logs did not say why no form matched.

## Decision

1. **Growing direct-path cooldown (muster-net).**
   - A QUIC failure over a direct path clears the path and ignores PONGs for
     30 s. The cooldown doubles on each consecutive failure, up to 8 minutes.
   - QUIC received from a direct source resets the count, and so does a
     network change. A genuine direct path behaves as before.
   - PROTOCOL.md describes the cooldown.
2. **Unreachable members are reported.** A member whose reconnects keep failing
   over both direct and relay paths is logged at INFO, with the error, on the
   first failure and then every 60 attempts (about ten minutes at the capped
   backoff). Other attempts stay at debug.
3. **Content-free explanation of a missed question form (muster-platform).**
   - **The summary.** `form_miss` reports counts and AX roles only:
     - prompt matches: exact, inside a longer label, or as a fragment;
     - for each option, its controls anywhere;
     - per ancestor of the first exact prompt, options with one, no or several
       matching enabled controls, plus a free-text marker;
     - where the climb stops: at the composer or at wrap-up prose;
     - how many controls carry the header.
   - **Where it appears.** The `AX question controls timeout` warning carries it
     as `why=`, or `form matched; submit control not ready` when the form
     itself was found. The read-only probe adds it as `miss` for each
     unmatched question.
   - **Adapted to today's matcher.** It follows the current rules: wrap-up
     prompts, several labels per option, and the prose barrier. To keep the
     diagnostic and the matcher from drifting apart, the prompt labels and the
     prose check now live in shared helpers (`prompt_labels`, `crosses_prose`)
     that `form_for` also uses. Matching behaviour is unchanged.
   - **Not ported.** The original commit's separate timeout messages: step
     names already say which stage timed out.

## Alternatives

- **Cherry-picking the commits.** Not possible across unrelated histories.
  The question-card code has also grown from about 1,000 to 2,100 lines since.
- **Dropping the branch.** The cooldown addresses a failure that recurs
  wherever a local fake-IP proxy captures member addresses. The question
  diagnostic is the missing evidence for fixing card matching.
- **Logging the unmatched AX tree.** The read-only probe already does that
  locally. Logs must stay content-free.

## Consequences

- A proxy that answers probes but not QUIC is retried at most every 8 minutes
  instead of every 30 seconds, so connections stay on the relay.
- After a real network change, direct paths are tried at once again.
- An unreachable member shows up in the INFO log within one attempt.
- A failed answer explains where matching stopped without revealing card
  content. The next step for card matching needs such a summary from a live
  unanswered card.

## Verification

- Source and tests (macOS, Rust 1.98):
  - **muster-net:** 3 tests restored from the original commit, adapted where
    needed. They cover the cooldown sequence (30 s to 480 s), repeated
    failures backing off until a network change starts over, and the
    reporting cadence (first attempt, then attempts 60 and 120). The library
    suite: 53 passed, 1 ignored.
  - **muster-platform:** a new test checks the miss summary of a docked-option
    form against the composer barrier, and that no prompt, option, header or
    composer text leaks into it. All 21 question-form tests pass.
  - **Full suites:** `cargo test -p muster-net -p muster-platform -p musterd`:
    134 passed, 5 ignored, 3 failed. All three failures are in musterd's
    `locked_use` tests: they assert that a fake unlock has already started and
    lose that race under load (the host's load average was about 10).
    - Which tests fail varies from run to run.
    - The same tests fail on the unchanged base commit too: 1 of 2 parallel
      runs.
    - Run one at a time, they pass in 2 of 3 runs on this branch and 3 of 3
      on the base.
    - Their fixture has no relay, no peers and a fake OS, so none of the code
      changed here runs in them.
  - `cargo fmt` and workspace `clippy -D warnings` are clean.
- Not done:
  - a live proxy or fake-IP reproduction;
  - a live unanswered question card;
  - a release and installation;
  - hosted CI (Actions are disabled, [ADR 0016](0016-suspend-hosted-actions-budget.md)).
