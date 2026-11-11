# ADR 0186: Dev builds keep line tables only

- Date: 2026-11-11
- Status: Accepted; clean test builds measured, backtraces and the workspace
  tests checked with the new profile
- Related: [ADR 0013](0013-ci-must-be-askable.md) (CI runs
  `cargo test --workspace`)

## Context

`cargo test` in `src/muster` was slow on the owner's 16 GB Mac. Building,
not running the tests, took the time. Several `rustc` and linker processes ran
at once, often next to builds from other worktrees, and more than 5 GB of swap
was in use. Under that pressure a clean test build of the workspace took 15 to
23 minutes.

The workspace had no `[profile.dev]` section, so dev and test builds used
Cargo's default of full debuginfo (`debug = true`). `rustc` then generates and
writes variable and type information for every crate, including all
dependencies. Line tables alone are enough for file and line numbers in panics
and backtraces, and the workspace has no tooling that reads more than that from
a dev build.

Each setting below is a clean `cargo test --workspace --no-run -j 4`. They ran
one at a time, each in its own target directory on `/Volumes/Developer`, on an
export of `main` at `6f1d57c0`, with no other build running. The process RSS
column is the largest from `/usr/bin/time -l`. The summed column adds up the
RSS of all `rustc`, `ld` and `clang` processes. It was sampled every 2 s, so it
is approximate and depends on scheduling.

| Setting | Wall | User CPU | Largest process RSS | Summed RSS peak | `target/debug` | Test binaries |
| --- | --- | --- | --- | --- | --- | --- |
| Default: full debuginfo | 157 s | 477 s | 1088 MB | 1669 MB | 12.5 GB | 3.2 GB |
| No debuginfo for dependencies | 173 s | 476 s | 1048 MB | 2149 MB | 11.3 GB | 3.0 GB |
| No debuginfo for dependencies, line tables for the workspace | 121 s | 363 s | 955 MB | 1544 MB | 8.7 GB | 3.0 GB |
| Line tables everywhere (chosen) | 120 s | 354 s | 1054 MB | 2105 MB | 9.3 GB | 3.1 GB |

Two earlier runs used `/Volumes/Data`. Other builds were running and the Mac was
swapping. The default took 882 s and the dependencies-only variant took
1409 s. Those numbers show the cost of memory pressure, not of debuginfo, so the
decision does not rely on them.

## Decision

The workspace `Cargo.toml` sets:

```toml
[profile.dev]
debug = "line-tables-only"
```

The test profile inherits it, so `cargo build`, `cargo test` and `cargo run`
all use it. To debug a dev build with full variables and types, set
`CARGO_PROFILE_DEV_DEBUG=full` for that build. The release profile keeps
`debug = 1`.

## Alternatives

- **No debuginfo for dependencies only**
  (`[profile.dev.package."*"] debug = false`). It saved 1.1 GB of disk but no
  time, because most of the CPU went to the workspace crates.
- **No debuginfo for dependencies, line tables for the workspace.** It was as
  fast as the chosen setting and 0.6 GB smaller. Backtrace frames in
  dependencies such as `tokio` would lose their file and line, which costs more
  than 0.6 GB saves.
- **`debug = false`.** Not measured. Panics and failing tests would no longer
  show file and line numbers in backtraces.
- **Fewer jobs.** A lower `-j` lowers peak memory but makes builds slower when
  memory is free. The right value depends on the Mac and on what else is
  running, so it stays the caller's choice (`-j` or `CARGO_BUILD_JOBS`). It is
  not set in the repository.

## Consequences

- A clean test build uses about a quarter less CPU and wall time. Each dev
  target directory is about 3.2 GB smaller. Incremental builds also generate
  less debuginfo; that saving was not measured.
- The first build of each existing target directory after this change rebuilds
  everything, because the profile changed.
- `lldb` on a dev build shows no locals or types without
  `CARGO_PROFILE_DEV_DEBUG=full`. Switching that variable rebuilds everything,
  so a debugging session should use its own `CARGO_TARGET_DIR`.
- Release builds, `sep` packaging and symbolication of release reports do not
  change.
- Several builds running at once can still exhaust memory and swap. This change
  only lowers the cost of each build.

## Verification

- The measurements above.
- A scratch crate built with `debug = "line-tables-only"` and run with
  `RUST_BACKTRACE=1` printed frames with file, line and column, for example
  `./src/lib.rs:3:9`.
- `cargo test --workspace --no-fail-fast -j 4` with the new profile: 1214 passed,
  0 failed, 15 ignored.
- `scripts/check-adr.sh origin/main`
