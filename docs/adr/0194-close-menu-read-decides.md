# ADR 0194 – A finished read decides when the model/effort menu closed

- Date: 2026-11-12
- Status: Accepted (not yet verified on a device)
- Related: [0181](0181-native-create-model-and-effort.md), [0190](0190-background-desktop-admission.md)

## Context

A host service creates Claude sessions with a model and an effort through
`agent_native_create` (ADR 0181). On Mac-A, on 2026-11-12, four of nine such
creations were rejected with:

```text
unavailable: Accessibility step 'close model/effort menu' timed out after 1056 ms (budget 1000 ms)
```

The other times were 1080, 1137 and 1080 ms. The service retried each
rejection with a new operation id and backed off up to 600 seconds, so the
creations succeeded tens of minutes late. Every rejection was a Claude
creation with the service's default `Opus 5.5` and `Extra`, which is also the
app's own new-session default.

`close` presses the picker once to toggle the panel shut, then polls:

```text
loop { step.check; snapshot; step.check; if panel gone { Ok }; step.check; sleep 50 ms }
```

Each `snapshot` re-proves the scope before and after reading. On a
new-session form that proof walks the page and reads attributes from every
node, so a read walks the page about three times. The creations that
succeeded spent 4.6–5.6 s in `prepare_options`, about twenty reads. That is
roughly 250 ms a read, more when the app is busy. Those creations ran while
other sessions in the same Claude app were writing replies.

Two things made a 1 s budget fail:

- **The read's answer was discarded.** A read that began inside the budget and
  ended after it was thrown away by the `step.check` between the read and
  its use. That held even when the read showed the panel gone. All four
  times are the budget plus less than one read.
- **The budget barely covered the reads.** One second allows three or four
  reads, and the first runs right after the press, while the panel is still
  closing. A close that takes a little over 0.6–0.85 s to reach the
  Accessibility tree leaves no read in the budget that can see it.

The panel opening waits up to 2 s (`wait_menu`), and that loop had the same
discarded-read pattern. The other panel waits in this flow (effort levels,
Power level, closing the Power panel) also use 2 s, and they already use a
read before checking the clock.

## Decision

- `Step::until(interval, read)` polls an async read. The budget decides
  whether another read may start, not whether a finished read counts.
  - Authorization (`control_guard`) and the budget are checked before every
    read.
  - Authorization is checked again after every read that does not settle the
    step.
  - The read re-proves its own scope, as `snapshot` already does before and
    after it walks. So the "Physical input changed" guard and the session or
    form identity checks run on every read, as before.
  - A timeout now also says how many reads were made (`; 3 reads`). Many
    reads that all saw the panel means the app kept it open; few reads means
    the reads were slow.
- `close` waits through `Step::until`. Its budget is 2 s, the same as for the
  panel to open. A code comment there gives the reason.
- `wait_menu` keeps its loop, which can press `Select model` and start the
  next step, but now uses a finished read before checking the clock.

Nothing else about what is pressed changes. `close` still toggles only the
uniquely identified picker, once, and never sends Escape or clicks another
window. A panel that really stays open still times out, now after 2 s, and
still refuses the creation as `unavailable:`.

## Alternatives

- **Only raise the budget.** This would have hidden the discarded read, which
  other steps would keep hitting.
- **Only stop discarding the read.** That alone fixes a read that ends just
  after the deadline. But with ~250 ms reads, a 1 s budget still leaves one or
  two reads after the first, and a busy app turns that into the same
  rejection.
- **Make the reads cheaper by proving the form less often.** Re-proving the
  form, the person's input and the app's identity before and after every
  read is what makes this safe. It is not traded for speed.
- **Retry the toggle when the panel is still open.** A second toggle could
  reopen a panel that was only slow to close. One toggle and a read are
  enough.

## Consequences

- A creation whose panel closes within 2 s is no longer rejected, including
  when the read that shows it ends after the deadline.
- A real failure to close now takes up to about 2.3 s to report, not about
  1.1 s.
- The timeout message for this step carries the read count. Callers that
  classify by the `unavailable:` prefix are unaffected.
- `muster-agent` gets tokio's `test-util` as a dev-dependency, for
  paused-time tests, as `muster-terminal` already has.

## Verification

- **Source/tests.** `native_step` tests, run with paused time:
  - a read that starts at 930 ms with a 1 s budget and ends at 1190 ms
    decides the step, the 2026-11-12 case;
  - no read starts after the deadline, and the timeout names the step, the
    budget and the read count;
  - without authorization nothing is read.

  The macOS adapter (`native_options.rs`) has no unit seam for Accessibility
  reads. It is covered by `cargo clippy` and `cargo test` builds of
  `muster-platform`.
- **Builds/checks.** `cargo fmt --all --check`; `cargo clippy` and
  `cargo test` for `muster-agent` and `muster-platform` on macOS;
  `scripts/check-adr.sh origin/main`; `scripts/check-public-content.py`.
  The results are in the pull request.
- **Not done.** No signed build, release or installation, and no device run.
  Device acceptance: on an unlocked Mac whose Claude app is busy writing
  replies in other sessions, create ten Claude sessions with a model and an
  effort, with the person's approval to send the test messages. Expect no
  `close model/effort menu` rejection, or one that reports how many reads saw
  the panel open.
