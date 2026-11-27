# ADR 0328: Let a supervised l4forwarder exit on a signal

- Date: 2026-11-27
- Status: Accepted; source verified, release pending
- Related: continues decision 3 of
  [ADR 0107](0107-l4forwarder-in-muster-app.md) (the helper cannot outlive
  musterd); [addons/l4forwarder](../../addons/l4forwarder/README.md)

## Context

musterd runs the bundled l4forwarder as `l4forwarder --config <file>
--supervised` and keeps its standard input open
([ADR 0107](0107-l4forwarder-in-muster-app.md)). The helper ends on the first
of four events: a rule task ending, SIGINT, SIGTERM, or (supervised) its input
closing.

On a forwarding member running Muster.app 1.1.48, a crash-recovery test sent
the helper a plain `kill` (SIGTERM). The helper closed every listener but did
not exit. musterd still saw a live child, so it did not restart it, and every
forwarded port stayed down until someone ran `kill -9`. SIGKILL and musterd's
own stop path (close the input, wait 2 s, kill) both worked.

The cause is how the input is watched. `tokio::io::stdin()` reads on a
runtime blocking thread, and that read returns only when the input ends.
`main` ran under `#[tokio::main]`. After the `select!` returned on a signal,
`main` returned and dropped the runtime. The listeners closed because their
tasks were dropped, but dropping a runtime waits for its blocking threads. That
thread stayed blocked in `read(0)` while musterd held the pipe open. Closing
the input, or a SIGKILL, was the only way out.

## Decision

1. **`main` builds the runtime by hand** and runs the forwarding future with
   `block_on`, then calls `Runtime::shutdown_background()` instead of
   dropping the runtime. The blocked input read is abandoned, and the process
   exits as soon as the `select!` returns, whatever ended it.
2. **Nothing else changes.** The four exit events, exit status 0 for each,
   `--check`, and the configuration format are as before. musterd needs no
   change: a helper that exits on a signal is "exited on its own" and is
   started again after its back-off.
3. **No graceful drain.** `shutdown_background` does not wait for connection
   tasks. Process exit closes every socket, which is what the dropped tasks
   did before.

## Alternatives

- **`std::process::exit(0)` after the `select!`.** Exits just as fast, but
  skips destructors and hides the reason in an unconditional call. Keeping
  `main` returning normally is clearer and keeps `?` error paths.
- **`Runtime::shutdown_timeout`.** Also bounded, but any timeout is a delay
  with no benefit: nothing the runtime still owns needs to finish.
- **Watch the input on a plain `std::thread` and signal a channel.** Fixes this
  read, but leaves the runtime free to hang on the next blocking task someone
  adds. Not waiting for the runtime covers both.
- **Make musterd treat "no listeners" as dead.** Puts a health probe of the
  forwarding rules into the supervisor, which ADR 0107 keeps unaware of them.

## Consequences

- SIGTERM and SIGINT end a supervised helper promptly, so musterd's restart
  covers the case that left ports down.
- A Muster release must include this before installed members change. Until
  then, restart a stuck helper with `kill -9`, or let musterd's own stop path
  end it.

## Verification

- Source and tests (macOS, Rust 1.98), in `addons/l4forwarder`:
  - Two new tests in `tests/supervised.rs` start `--supervised` with its input
    held open, wait until it listens, send SIGTERM or SIGINT with `kill`, and
    require exit status 0 within 1 s with the input still open. Before the fix
    both failed ("still running 1 s after SIGTERM/SIGINT") while the input-EOF
    test passed; after it, all three pass. Ten repeated runs of the file
    passed.
  - `cargo test`: 12 passed (4 configuration, 5 forwarding, 3 supervised).
  - `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are
    clean.
- Manual check of the release build: `--supervised` with a held-open input
  pipe exited with status 0 about 1 ms after SIGTERM and about 1 ms after
  SIGINT.
- Not done: an Xcode build of Muster.app, signing, notarization, a release,
  installation, and a repeat of the crash-recovery test on an installed
  member; hosted CI (Actions are disabled,
  [ADR 0016](0016-suspend-hosted-actions-budget.md)).
