# ADR 0121: `project.yml` is the Mac project's only version, and drift is reported

- Date: 2026-11-04
- Status: Accepted; source verified, fleet build pending
- Related: [ADR 0006](0006-desktop-presence-and-console-routing.md) (the
  provisioning profile the install needs),
  [ADR 0107](0107-l4forwarder-in-muster-app.md) (the build phase that ships the
  addon)

## Context

`src/native/apple/MusterApp/Muster.xcodeproj/project.pbxproj` is generated from
`project.yml` by `xcodegen generate`, and `install-mac.sh` runs that generator
on every build. Both files are tracked.

They had drifted. The generated project carried `MARKETING_VERSION = 1.1.20`
and `CURRENT_PROJECT_VERSION = 110` — the version the fleet runs — while
`project.yml` still said `1.1.15` / `105`. Whoever bumped the fleet's version
edited the generated file and not its source, so the numbers survived only
until the next build on any machine.

The next build was an install of the ADR 0120 branch on 2026-11-04. The
generator reverted the version to 1.1.15 (105), the revert was picked up by a
`git add -A` for an unrelated documentation commit, and it reached main in
PR #124 without anyone deciding it. The same regeneration also replaced a
hand-edited "ADR 0106" in a build-phase comment with the correct "ADR 0107",
which is the other half of the same drift: two people's edits to a generated
file, one right and one wrong, both invisible until a build threw them away.

Nothing warned. `xcodegen` has no opinion about what it overwrites, and a
version going backwards breaks no test.

## Decision

**`project.yml` holds the version, and the generated project is expected to
match it.**

- `project.yml` is bumped to `1.1.21` / `111`. Not back to 110: that number
  shipped from a hand-edited project, so the first build this file produces on
  its own gets a number of its own rather than a second meaning for an existing
  one. A comment in `project.yml` says why the numbers live there.
- `install-mac.sh` runs `xcodegen generate` and then **fails** if
  `Muster.xcodeproj/project.pbxproj` no longer matches the tracked copy,
  printing the diff and saying to move the edit into `project.yml` or commit
  the regenerated project. The trap fires at the build, so that is where it is
  reported. The check is skipped outside a git work tree.
- An edit to the generated project is not forbidden by a gate — it is reported
  the first time anyone builds, which is as early as this can be caught without
  putting `xcodegen` into CI.

What the check can and cannot see follows from its order: `xcodegen` overwrites
the file before the diff runs, so the comparison is against the **committed**
project. A committed hand-edit — the case that drifted here, and the only one
that can reach anyone else — stops the install. An uncommitted hand-edit is
still thrown away silently, because by the time anything could look, the
generator has already replaced it. Reversing the order would mean diffing the
file the previous build left behind, which says nothing about `project.yml`.

## Alternatives

- **Stop tracking `project.pbxproj`.** Cleanest in principle, and wrong here:
  opening `Muster.xcodeproj` from a fresh clone would need `xcodegen` first, and
  the project file is how the Xcode build is reproduced on a machine that has
  no generator installed.
- **Run `xcodegen generate` in CI and fail on a diff.** The honest gate, but it
  puts a new macOS-only dependency in the workflow, and this repository's
  Actions have been unable to pick up a run since 2026-11-02, so a gate there
  would not have caught this one either.
- **Restore 110 in `project.yml`.** It keeps the fleet's number continuous, but
  it gives one version string two different builds — the thing the daemon's
  `commit` field exists to work around.
- **Leave it and remember.** This drift lasted through at least two commits
  without anyone noticing; the memory was the control that already failed.

## Consequences

- The next Mac build is 1.1.21 (111). The fleet's running 1.1.20 (110) has no
  build on main that corresponds to it any more, which was already true the
  moment ADR 0120 landed.
- An edit to the generated project now stops an install instead of being
  silently reverted. A person who regenerates deliberately commits the result.
- `install-mac.sh`'s prerequisite comment was also wrong, and is corrected in
  the same change: it omitted `MUSTER_MAC_PROFILE`, `-p muster-cli` and
  `-p muster-browser`, each of which fails the build phase. Two installs on
  2026-11-04 failed on exactly those omissions.
- Nothing about the app's behaviour changes.

## Verification

`xcodegen generate` leaves the tracked project unchanged, and a second run
changes nothing further. `sh -n` on the script.

Checked on the owner's Mac, 2026-11-04: a committed hand-edit of
`CURRENT_PROJECT_VERSION` to 999 made `install-mac.sh` exit 1 after
regeneration with "Muster.xcodeproj/project.pbxproj no longer matches
project.yml", the one-line diff, and the instruction to move the edit into
`project.yml`. Nothing was built or installed. The same edit left uncommitted
did not stop the install, which is the limitation described above, found by
running it that way first.

Still pending: an install reporting 1.1.21 (111).
