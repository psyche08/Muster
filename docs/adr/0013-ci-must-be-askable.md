# ADR 0013: CI has to be runnable on demand, and "no checks" is not a pass

- Date: 2026-10-21
- Status: Accepted; enabled-setting decision superseded by [ADR 0016](0016-suspend-hosted-actions-budget.md)
- Related: [AGENTS.md](../../AGENTS.md#turn-completion-and-commits) landing rules,
  `scripts/finish-session.sh`, `.github/workflows/muster.yml`

## Context

Two pull requests landed on 2026-10-21 (#165, #166) with no CI having run on
either. The cause was not the workflow: it already triggers on `push` to `main`
and `claude/**` and on `pull_request`. Actions was switched off for the whole
repository — `GET /repos/psyche08/Muster/actions/permissions` answered
`{"enabled": false}` — so nothing had run since 2026-09-29, and the last seven
`muster` runs before that had failed (`cargo test --workspace` on both the Linux
and macOS jobs), which is the likely reason it was switched off.

Three things made that silent rather than loud:

1. `scripts/finish-session.sh` treats "no checks reported" as permission to
   merge. That is reasonable for a repository without CI and wrong for one whose
   CI is merely unavailable; the two are indistinguishable to `gh pr checks`.
2. The workflow had no `workflow_dispatch`, so there was no way to ask for a run
   and see the answer. The only triggers were events that may legitimately
   produce no run at all, because of the `paths` filters.
3. A reviewer reading only the workflow file sees `pull_request` and concludes CI
   covers pull requests. The repository-level switch is not in the file.

The consequence was real: #165 merged a `proto` change without the
workspace-wide build that would have caught `muster-app-core` failing to
compile, and `main` was broken until #166.

## Decision

1. **The workflow gains `workflow_dispatch`.** CI must be answerable on demand,
   against any ref, independent of path filters and of whether a push or pull
   request happens to match them.
2. **Repository Actions stays enabled.** It was re-enabled on 2026-10-21 at the
   owner's request. Switching it off again silently disarms the landing gate, so
   it is a decision to take deliberately and not a quiet default.
3. **"No checks" is recorded as an unknown, not a pass.** This record states the
   rule; `finish-session.sh` still merges on it, which is a known gap left
   deliberately rather than changed in the same breath as re-enabling CI. The
   honest reading of its `no CI checks on this PR` line is "CI did not answer",
   and a landing that prints it has not been checked.

## Alternatives

- **Push an empty commit to `main` to trigger a run.** Rejected: `main` takes
  merges, not direct pushes, and a commit whose only purpose is to poke CI is
  noise in the history.
- **Change `finish-session.sh` to refuse to merge when no checks report.**
  Deferred, not rejected: it is the real fix for the gap above, but it changes
  how every session lands, and a repository genuinely without CI would need an
  explicit opt-out. Worth its own record.
- **Drop the `paths` filters so every commit runs CI.** Rejected: the filters are
  deliberate and the cost of running the macOS job on documentation-only commits
  is not worth it. `workflow_dispatch` covers the case where someone needs an
  answer anyway.

## Consequences

- CI can be asked for a verdict at any time, which is what makes "is `main`
  green?" a question with an answer rather than an inference.
- Re-enabling Actions brings back the failures from 2026-09-29. They predate this
  branch's work and are not caused by it; whether they are still failing is now
  observable, which is the point.
- Nothing here makes the landing script safe on its own. Until that is changed, a
  session that lands while Actions is off will still merge unchecked, and the
  only signal is the script's own line saying no checks reported.

## Verification

- Source: the workflow is the only file changed, plus this record.
- CI: this record's own pull request is the test. It touches
  `.github/workflows/muster.yml`, which the `pull_request` path filter matches,
  so the run it produces is both the proof that pull-request CI works again and
  the current verdict on `main` plus this change.
- The run it triggered (39624978993) came back red, and the failures were this
  branch's, not September's: the Swift job's generated-protobuf diff (stale
  bindings after ADR 0009's new field) and `check-layers.py` (the new
  `muster-acp` crate unregistered). Both are fixed in this record's own pull
  request, which is exactly what re-enabling CI was for. The macOS build-and-test
  job passed, so the September `cargo test --workspace` failure did not reproduce
  there.
- The run after those fixes was red again, for two causes that predate this
  work: `muster-net`'s recovery tests racing in their own helpers, and
  `swift test` running on a Linux runner where the test graph's SwiftUI
  dependency cannot exist. Both are fixed in
  [ADR 0014](0014-recovery-test-races.md) and, for the Swift half,
  [ADR 0011](0011-linux-ci-artifact-budget.md), which is where
  their diagnosis lives.
- Not established: that the generated HarmonyOS TypeScript binding is current. No
  CI job checks it, and it was not regenerated.
