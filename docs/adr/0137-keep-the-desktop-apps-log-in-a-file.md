# ADR 0137: Keep the desktop app's log in a file

- Date: 2026-11-05
- Status: Accepted; source and tests verified, install acceptance pending
- Related: [ADR 0135](0135-give-session-watches-the-forwarding-budget.md)
  (the investigation that lacked this log),
  [src/native/apple/MusterApp/README.md](../../src/native/apple/MusterApp/README.md)
  (file locations)

## Context

The macOS app initialized its shared-Rust core's logging with
`muster_log_init`, which writes to stderr. An app opened from Finder, the
Dock or at login has nowhere for stderr to go, so none of these lines was kept.
That includes `session live updates interrupted; retrying …`, written each time
the chat shows a failure banner.

During the ADR 0135 investigation, the daemons' logs on Mac-A, Mac-B and Mac-C
were all available, but nothing showed what Mac-B's or Mac-C's app saw. The
moment of the user's screenshot could not be matched to a cause.

## Decision

- **A file-logging entry point.** `muster_log_init_file(level)` in
  `muster-ffi` logs as `muster_log_init` does, and also appends to `app.log`
  in `muster_paths::log_dir()`. That is the daemon's log directory:
  `~/Library/Logs/Muster` on macOS, or `$MUSTER_DIR` when it is set, so a
  development app's log sits beside its development daemon's.
- **The macOS app uses it.** `MusterMacApp` calls `initFileLogging(level: 3)`.
  The iOS app, `MusterDev`, Android and HarmonyOS keep stderr: their consoles
  or debuggers collect it.
- **A bounded file.** When a record would take `app.log` past 8 MiB, the file
  becomes `app.log.1`, replacing the previous one, and a new `app.log` starts.
  Records are never split, since env_logger hands over one whole record per
  write. The disk use is about 16 MiB at most.
- **A marker and a fallback.** The first line of each launch is `app log
  opened, pid N`. If the file cannot be opened, logging stays on stderr, with
  one warning saying so.
- **Same format as the daemon.** The line format is env_logger's default, the
  same as `musterd.log`, so the two files can be read side by side by
  timestamp.

At info level, the app's lines are status and error lines: startup phases,
cache, update, Presence, chat-watch failures, and the local proxy's
connection results. They contain no message, prompt or answer text. The file
stays in the user's own Library, as `musterd.log` does.

## Alternatives

- **The unified log (`os_log`).** Its retention is short and set by the
  system, and reading it needs `log show` with predicates. It would also need
  a separate bridge from Rust's `log` facade.
- **Redirect stderr to a file in Swift.** This would also catch unrelated
  framework output, would leave the size unbounded, and would put
  log-location policy into the platform shell.
- **Rotate by date.** Size bounds the disk use whatever the volume. With a
  timestamp on each line, a date in the file name adds nothing.

## Consequences

- An app-side failure can be matched with the daemon's log by timestamp.
- At most about 16 MiB on disk. Lines older than two files' worth are lost.
- If `app.log` is deleted while the app runs, records go to the deleted file
  until the next rotation starts a new one.
- A second app instance with the same log directory would append to the same
  file. This is not a supported setup.

## Verification

- **Source and tests:**
  - Unit test `the_log_keeps_two_files_and_whole_records`: rotation at the
    limit, records kept whole, never a third file, and a log deleted while
    open.
  - Integration test `the_app_log_lands_in_the_log_directory`, through the C
    ABI with `MUSTER_DIR` set: the marker line, an info record in the daemon's
    format, no debug record at info, and no terminal colour codes.
  - `MusterRuntime.swift` type-checks against the new header
    (`swiftc -typecheck -I Sources/CMuster`), and fails to when the new
    function's name is misspelt. A full `swift build` of the package was not
    run: fetching its GitHub dependencies broke off mid-transfer.
- **Not done:** an Xcode build of the macOS app, signing, installation, and
  runtime acceptance. After installing, open the app from the Dock and check
  that `~/Library/Logs/Muster/app.log` starts with the marker line. With a
  member's daemon stopped, a chat for one of its sessions should add a
  `session live updates interrupted` line.
