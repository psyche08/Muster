# ADR 0128: The AppKit panel helper ships outside `Contents/MacOS`

- Date: 2026-11-05
- Status: Accepted; source verified, before/after measured on the owner's Mac
  with the banner up and the Locked Use cover up. Install acceptance pending.
- Related: [ADR 0127](0127-the-daemon-stays-out-of-launch-services.md) (the
  same defect for the daemon; this record finishes the invariant it states),
  [ADR 0087](0087-idle-compaction-on-a-locked-or-busy-mac.md) (the banner),
  [ADR 0125](0125-screen-recording-is-the-bundles-and-only-a-restart-takes-it.md)
  (why the daemon's own path cannot move),
  [ADR 0107](0107-l4forwarder-in-muster-app.md) (the `Contents/Helpers`
  build phase this follows), [LOCKED_USE.md](../LOCKED_USE.md)

## Context

ADR 0127 states the invariant: **`dev.linsheng.muster` has exactly one
running-application record, the app's.** A second record takes the app's own
bundle id and name, and an AX client resolving "Muster" by name or by bundle
id takes the first record it is handed — so it lands on something that answers
no AX message, and reports that the app has no windows.

That record held permanently by the daemon is gone. Two short-lived ones are
not. The idle-compaction banner and the Locked Use console cover both need an
AppKit main thread, which the daemon has not got, so each runs in a helper
process spawned from `std::env::current_exe()` — that is
`Muster.app/Contents/MacOS/musterd` — and each calls
`[NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory]`.
Measured on the owner's Mac (1.1.21/111), a running banner is a third record
named "Muster", for up to the banner's 10-minute `LIFETIME`:

```
pid=10785 policy=prohibited  …/Contents/MacOS/musterd   (the daemon)
pid=10812 policy=regular     …/Contents/MacOS/Muster    (the app)
pid=54409 policy=accessory   …/Contents/MacOS/musterd   (the notice helper)
```

The cause is the path, not the code. A process started from `Contents/MacOS`
takes `Muster.app` as its main bundle, which is where it picks up
`CFBundleIdentifier` and the name. Measured with one binary, one signature and
one flag, moved between two directories of the same bundle:

| the helper runs from | records for `dev.linsheng.muster` | its own record |
| --- | --- | --- |
| `Contents/MacOS/muster-panel` | **3** | `dev.linsheng.muster`, name "Muster" |
| `Contents/Helpers/muster-panel` | 2 | no bundle id, name `muster-panel` |

`Contents/Helpers` has no `Info.plist` above it, so `NSBundle.mainBundle`
there is the directory itself and `bundleIdentifier` is `nil`.

Moving the daemon the same way is not available: ADR 0125 measured that its
Screen Recording and Accessibility grants are filed under the app bundle
precisely because its main bundle resolves to `Muster.app`, and ADR 0127
measured that the same daemon run from another path is not trusted. The panel
helpers carry no such cost, and that was checked rather than assumed: between
them they call `muster_notice_show`, `muster_console_cover`,
`muster_console_cover_healthy`, `muster_console_pump`,
`muster_console_uncover`, `CGSessionCopyCurrentDictionary` and the two
`IOPMAssertion` calls. None is TCC-gated. `muster_console_cover_healthy` reads
`CGWindowListCopyWindowInfo`, but only the owner pid, layer, alpha and
on-screen keys, which need no Screen Recording grant — `kCGWindowName`, the
key that does, is never read.

## Decision

**The AppKit panel bodies are their own executable,
`Muster.app/Contents/Helpers/muster-panel`, and `musterd` no longer runs
them.**

- A new bin target in `muster-platform`, `muster-panel` (582 KB), dispatches
  the two flags that were `musterd` subcommands:
  `--muster-notice` (`macos::notice::run`) and `--locked-use-watchdog`
  (`macos::locked_use::watchdog`, whose flag is now the constant
  `WATCHDOG_FLAG` beside `notice::FLAG`). The bodies themselves are unchanged.
- `musterd`'s `main` loses both branches. The daemon has no subcommand that
  makes it an AppKit process, so the registration cannot come back through it.
- `macos::bundle::beside` resolves the helper from the running executable:
  `Contents/Helpers` when the caller runs from `Contents/MacOS`, else beside
  the caller, which is the rule `musterd::l4forwarder::program` already
  follows for the addon. `macos::bundle::panel` adds the existence check.
- The build phase in `project.yml` installs it next to `muster`,
  `l4forwarder` and `muster-browser-host` and signs it
  `--identifier dev.linsheng.muster.panel`, the pattern the other three use.
  A missing binary fails the build, as theirs do.

The invariant ADR 0127 states is now whole for every process in the bundle:
nothing but the app registers under `dev.linsheng.muster`.

**What this does not claim.** The helper still takes a running-application
record while it runs — `.accessory`, with no bundle identifier, named
`muster-panel` after its executable. Drawing a window is what creates it, and
moving the executable does not change that. What moving it changes is whose
identity the record carries, which is the whole of the defect: a record under
no bundle id is not a second "Muster" for an AX client to resolve to.

## Alternatives

- **An embedded `__TEXT,__info_plist` in the helper.** The route ADR 0127
  considered for the daemon and left open here, since the TCC objection does
  not apply. Measured both ways: from `Contents/MacOS` it works as ADR 0127
  recorded — the record carries the helper's own `CFBundleIdentifier` and its
  `CFBundleName` — but from `Contents/Helpers` the record's bundle identifier
  is `nil` regardless, so the section buys nothing once the executable has
  moved. Keeping it would mean a second place where the helper's identity is
  declared, for a name the executable's own filename already supplies.
  Rejected as redundant, not as wrong.
- **Leave the helper in `Contents/MacOS` with only the embedded plist.** One
  fewer directory and the record gets a readable name. Rejected: it leaves an
  AppKit process in the application's own executable directory, where the next
  helper added without the section inherits the defect silently. The
  convention that `Contents/Helpers` holds helpers is the thing that makes
  this hard to get wrong again.
- **A second copy of `musterd` in `Contents/Helpers`.** No Rust restructuring
  at all, and the helper bodies could not drift from the daemon's because they
  would be the same bytes. Rejected on size: `musterd` is 37 MB against the
  helper's 582 KB, in a 124 MB bundle, to run two panels.
- **A symlink in `Contents/Helpers` pointing into `Contents/MacOS`.** Measured
  and it does work — `NSBundle.mainBundle` follows the path the process was
  started from, not the resolved one, so the record carries no bundle id.
  Rejected: it keeps one binary at no disk cost but makes the bundle's layout
  claim something false, and the property it relies on is an implementation
  detail of how dyld reports the executable path.
- **Stop calling `setActivationPolicy:`.** It is what registers the process.
  Rejected: an AppKit process that never becomes an application does not get
  a window on screen, and drawing without AppKit means private window-server
  calls.
- **Move `musterd` itself as well.** Rejected by ADR 0125 and ADR 0127: its
  grants are filed under the app bundle, and the move would cost the owner a
  re-approval of Accessibility and break the Screen Recording arrangement.

## Consequences

- An AX client resolving "Muster" finds the app, including during the ten
  minutes a banner can be up and during a Locked Use transaction. The
  intermittency ADR 0127 removed for the daemon is removed for the helpers
  too, and for the same reason.
- **The bundle gains a file, and an installation without it loses two
  features rather than mis-registering.** `Notice::show` returns an error,
  which `idle_compact` already logs and carries on without a banner; the
  Locked Use guard returns an error, so no transaction starts. Both fail in
  the safe direction, and the build phase refuses to produce such a bundle.
  Nothing falls back to `Contents/MacOS`: a fallback is exactly the defect.
- **`musterd --muster-notice` and `musterd --locked-use-watchdog` are gone.**
  They were internal, spawned by the daemon and never documented as a user
  interface, but anything reproducing ADR 0127's measurement by hand needs the
  new spelling: `Contents/Helpers/muster-panel --muster-notice`.
- A new daemon with an old bundle beside it — an interrupted install — draws
  no banner until the install finishes. The previous arrangement would have
  drawn one and taken the app's identity to do it.
- `muster-platform` gains a bin target. Its `build.rs` now also emits
  `rustc-link-arg-bins=-Wl,-rpath,/usr/lib/swift` under the `sep` feature, so
  the helper links the same way `musterd` does if it is ever built with it.
  The layer map is unchanged: the bin is in the crate that already owns these
  bodies, so nothing new depends on anything.
- Screen Recording and terminal authorization are untouched. The daemon's
  executable does not move, so ADR 0125 continues to hold as written, and the
  helper needs no grant of its own.

## Verification

- **Source.** No `current_exe()` spawn of an AppKit body remains; `musterd`'s
  `main` has no macOS subcommand block. `macos::bundle` has three unit tests
  covering the bundle layout, a build tree, and a helper resolving from
  `Contents/Helpers` (which must not climb out of it).
- **Reproduced first, on the owner's Mac (2026-11-05, 1.1.21/111).**
  `( printf 'Muster\nhello\n'; sleep 9 ) | /Applications/Muster.app/Contents/MacOS/musterd --muster-notice`
  took the count for `dev.linsheng.muster` from 2 records to 3, the third
  `.accessory`, named "Muster", executable `…/Contents/MacOS/musterd`. It went
  when the helper exited.
- **Cause isolated by placement, not by code.** The new `muster-panel`,
  one binary signed once as `dev.linsheng.muster.panel`, run with
  `--muster-notice` from a copy of the bundle: from `Contents/MacOS` it added
  a third record carrying `dev.linsheng.muster` and the name "Muster"; from
  `Contents/Helpers` it added none, and its own record had `bundleIdentifier`
  `nil` and the name `muster-panel`. Same bytes, same signature, same flag.
- **The banner still draws.** Through `Notice::show` from a probe in the
  bundle's `Contents/MacOS`: the helper resolved to
  `Contents/Helpers/muster-panel`, the panel appeared at
  `CGWindowListCopyWindowInfo` layer 25 (`NSStatusWindowLevel`), alpha 1,
  `onscreen=1`, 340x64 at the top right of the main display — ADR 0087's
  geometry — and closed when the handle dropped. Records for
  `dev.linsheng.muster` stayed at 2 throughout.
- **The Locked Use cover still works, through the real guard.**
  `MacLockedUse::guard(nonce, 8 s, Screen::Unlocked)` from the same probe:
  the cover came up full-screen at `CGShieldingWindowLevel()+1`, alpha 1,
  `onscreen=1`, 1728x1117. Twelve consecutive `pulse()` calls reported
  `alive=true` and `failed=false`, eleven of them `covered=true` with status
  ages of 2–40 ms — that is `muster_console_cover_healthy` returning true from
  a `Contents/Helpers` process with its own code-signing identity and no
  Screen Recording grant, which is the claim that needed checking rather than
  assuming. `release()` returned `Ok(())`, the cover came down, the
  `locked-use.pending` marker was removed, and the screen was not locked.
  Records for `dev.linsheng.muster` stayed at 2 while the cover was up, and
  the watchdog's own record had `bundleIdentifier` `nil`.
- **Gates.** `cargo test --workspace` on an otherwise idle machine: 1100
  tests over 86 suites, exit 0, the three new `macos::bundle` tests among
  them. Also `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --all --check`, `src/muster/scripts/check-layers.py`,
  `scripts/check-public-content.py`, `scripts/check-adr.sh origin/main`.
  An earlier run of the suite, started while another `cargo test --workspace`
  was still winding down on the same machine, failed
  `musterd::pair::create_invite_join_approve_call_and_remove` — two daemons
  and a local relay behind `wait_for` timeouts. It passed in the idle run and
  touches nothing in this change.
- **Not verified here.** The generated `project.pbxproj` was regenerated with
  `xcodegen generate` and matches `project.yml` (ADR 0121), but the build
  phase itself has not run: signing the bundle needs the login keychain, which
  an agent session must not unlock. The owner's Mac still runs 1.1.21/111, so
  nothing on it is installed from this change.
- **Unresolved.** Install acceptance. After a build that includes this change
  and ADR 0127's, the end-to-end check is that
  `lsappinfo find bundleID=dev.linsheng.muster` names one ASN while a banner
  is up, and that an AX client lists the app's window at the same moment.
  `lsappinfo list` alone will not show a second record even when one exists
  (ADR 0127), so it is not the check.
