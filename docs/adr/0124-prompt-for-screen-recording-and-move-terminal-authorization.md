# ADR 0124: Prompt for Screen Recording at startup, and move terminal authorization to the Terminals page

- Date: 2026-11-04
- Status: Accepted; source and tests verified, installed and verified on the owner's Mac
- Related: [ADR 0116](0116-failure-screenshot-analysis.md) (the capture that
  needs Screen Recording), [ADR 0006](0006-desktop-presence-and-console-routing.md)
  (the bundled daemon whose permissions these are),
  `docs/architecture/runtime.md` §3.7 (the shells render a ViewModel)

## Context

Two of the three system permissions the daemon needs are asked for on this
Mac's screen; only one of them is ever mentioned at startup.

**Screen Recording is never checked.** ADR 0116 lets the owner's own LLM look
at an app window after a native send fails, and the capture is `musterd`'s:
`CGPreflightScreenCaptureAccess` is evaluated per process, so the app cannot
hold the permission on the daemon's behalf. The daemon reports it
(`musterd/src/vision.rs` sets `VisionStatus.screen_recording`), but that status
reaches one read-only line in `VisionSettings.swift`, behind a feature the
owner has to go and configure first. Nothing checks it at launch, nothing
prompts, and a Mac whose daemon cannot capture looks exactly like one whose
daemon can until a send has already failed and the explanation comes back
empty.

**The startup dialog mixes a permission with a workspace setting.**
`DesktopSetupView` is shown once at launch and asks for four unrelated things:
Accessibility, the Locked Use plugin installation, Automation over iTerm2 or
Ghostty, and a restart. The first two are about this Mac being reachable at
all. The third is about one workspace feature — terminals — and the owner
cannot act on it without going to a terminal app anyway: the Rust messages say
"start iTerm2 first", "enable the Python API in Settings → General → Magic".
Put at launch, it is a demand before the owner has any reason to care; put on
the Terminals page, it is the answer to "why is this list empty?".

`DesktopSetupView` is also dismissible, and the whole dialog is dismissed at
once. Terminal Automation and a missing Accessibility grant share one "later".

## Decision

**Screen Recording joins the startup check, and terminal Automation leaves the
startup dialog for the Terminals page.** Both decisions are made in Shared
Rust; the shells render what they are given.

### Screen Recording

- `v1.DeviceState` gains `screen_recording` (field 17), reported by `musterd`
  exactly as `accessibility` already is: `""` on an OS with no such
  permission, `"granted"` or `"missing"` on macOS. The daemon is what
  captures, so the daemon is what observes.
- A new local-only RPC, `DeviceRequestScreenRecording` (`Request` field 77),
  raises `CGRequestScreenCaptureAccess` inside `musterd`. Like
  `DeviceRequestAccessibility` and `DeviceRequestAutomation` it is
  `Access::LocalOnly`: the prompt appears on this Mac's screen, so no member
  may raise one.
- `render::desktop_setup` treats a missing grant as outstanding and a silent
  OS as nothing to ask for: `screen_recording_ready` is
  `device.screen_recording != "missing"`. `visible` and `ready` both take it
  into account, so a Mac missing only Screen Recording now prompts at launch.
- `RequestScreenRecording` is a top-level intent beside `RequestAccessibility`,
  not a `DesktopSetupAction`: the Network settings page offers the same grant,
  and neither entry point belongs to the dialog.
- The grant joins `observe_desktop_permissions`, so a `missing` -> `granted`
  transition sets `desktop_restart_required` and reopens a dismissed dialog.
  macOS answers a preflight with the new grant at once, but the process that
  asked keeps capturing nothing until it restarts -- which is why the system's
  own prompt offers "Quit & Reopen". `musterd` is that process, so without
  this the dialog would report Screen Recording as ready while every capture
  still came back empty. Observed on the owner's Mac: see Verification.

### Terminal authorization

- `TerminalsView` gains `authorization`, a `TerminalAuthorizationView` that
  Rust projects (`render::terminal_authorization`) rather than stores. It
  names the outstanding source — Ghostty needs Automation only, iTerm2 also
  needs its Python API — says what is missing, and is absent entirely when
  there is nothing to authorize, on another platform, or on a phone's embedded
  daemon.
- `RequestTerminalAuthorization` replaces `DesktopSetupAction.ITERM`, which is
  reserved. The work it does is unchanged: `DeviceRequestAutomation` plus the
  Automation pane.
- `DesktopSetupView` drops `iterm_ready`, `iterm_message`, `show_iterm` and
  `terminal_app` (fields 7, 8, 12 and 14, reserved). Terminal Automation no
  longer decides whether the startup dialog appears, and `ready` no longer
  waits on it.
- The Terminals page renders the section above the session list; Network
  settings keeps a read-only line that points there.

`app.proto` is the contract between the Rust core and the shells shipped in
the same bundle, so the removed field and enum numbers are reserved for
clarity, not for wire compatibility with an older peer.

## Alternatives

**Leave Screen Recording to the Vision settings page.** That is where it is
today, and it is where it stays invisible: a permission the daemon silently
lacks is only discovered through a failure whose explanation it was supposed
to produce. The other two daemon permissions are both asked for at launch;
this one being different is an omission, not a design.

**Ask for Screen Recording only when the owner enables the LLM.** `vision.rs`
already does this inside `save`. It is the right moment for the feature and
the wrong moment for the permission: the feature exists to explain a failure
that has already happened, so the permission has to be in place before the
owner has any reason to open those settings.

**Ask the app for Screen Recording and hand the capture to the daemon.** The
permission is per process, and the capture is the daemon's. A grant the app
holds would not let `musterd` read a single pixel.

**Trust the preflight and skip the restart.** The preflight answers `true` as
soon as the grant lands, so the dialog would close itself and report the
daemon ready. It would be wrong until the next restart, and the failure it
hides is the silent one this ADR exists to remove.

**Keep terminal Automation in the startup dialog and only add Screen
Recording.** The dialog's point is "this Mac is not yet reachable". Automation
over a terminal app the owner may not use, in a dialog they must dismiss
before they can reach the terminal app the message tells them to start, is not
that. Moving it also removes the one item that made the dialog's `ready`
depend on a workspace feature.

**Give terminal authorization its own dismissible card.** The Terminals page
is already the only place the answer is useful, and the section disappears on
its own once the grant lands. Another dismissal state would be state to own
for no gain.

## Consequences

- A Mac that is missing Screen Recording now sees the startup dialog it did
  not see before. That is the intent, and it is the only new prompt: a grant
  already in place, or an OS that reports nothing, leaves the dialog shut.
- Every `DeviceState` now carries one more macOS permission probe.
  `CGPreflightScreenCaptureAccess` reads a cached TCC answer and does not
  prompt, so it sits beside the Accessibility and Automation probes the same
  call already makes.
- The startup dialog is three items instead of four, and its `ready` means
  "this Mac is reachable", not "this Mac is reachable and has a terminal".
  Network settings reports terminal authorization separately and sends the
  owner to the Terminals page for it.
- The HarmonyOS protobuf bindings
  (`src/native/harmonyos/.../proto/muster.{js,d.ts}`) are not regenerated
  here: the pinned generator (`protobufjs-cli` 1.3.3) is not installed on this
  machine, and the shell renders neither view. They lag the schema the same
  way they did after ADR 0116; the next regeneration picks both up. The Apple
  bindings are regenerated and CI diffs them.
- `VisionSettings.swift` still shows its own read-only Screen Recording line.
  It is now a second view of a permission the startup flow owns, which is
  duplicated information rather than a conflict; folding it in is left alone.

## Verification

- **Source and tests**: `cargo test --workspace` (all suites pass),
  `cargo clippy --workspace --all-targets -- -D warnings` (clean),
  `cargo fmt --all --check` (clean), `scripts/check-layers.py` (18 crates),
  `python3 scripts/check-public-content.py` (882 files, 0 findings).
  New Rust tests: a missing Screen Recording grant prompts while a silent OS
  does not; terminal Automation no longer drives the dialog; the authorization
  section is offered on this Mac only, names the outstanding source, and is
  absent when the app is not installed. A new daemon test asserts
  `DeviceRequestScreenRecording` is `Access::LocalOnly`.
- **Apple shell**: `scripts/gen-protos.sh` regenerated at the resolved
  SwiftProtobuf 1.38.1 and verified to reproduce the committed output from the
  unmodified schema; `swift build` and `swift test` pass (109 tests), and the
  `testRenderDesktopSetup` snapshot shows the three-item dialog.
- **Signing and installation**: verified. Built with
  `MUSTER_MAC_PROFILE="Muster Developer ID Presence G2"` and signed with
  `Developer ID Application: Sheng Lin (89LGY6BD53)`;
  `scripts/install-mac.sh` installed 1.1.21 (111) into `/Applications`, and
  `codesign --verify --deep --strict` passed. Not notarized; not published.
- **Runtime, on the owner's M1Max**: verified for both halves.
  - The freshly installed `musterd` reported `screen_recording: "missing"`
    while Accessibility and the Locked Use plugin were both ready — a state
    the previous build said nothing about. The startup dialog came up with
    the Screen Recording section outstanding, naming
    `/Applications/Muster.app/Contents/MacOS/musterd`.
  - Granting it in System Settings moved the daemon's report to
    `"granted"`, and the dialog stopped appearing after the restart that the
    install performs.
  - The Terminals page showed the Ghostty Automation section above the
    session list (this Mac's `ghostty_automation` is `not_running`), with
    Rust's own message and the "授权并检查 Ghostty…" button, while the
    startup dialog no longer carried any terminal item.
- **Unverified**: the `missing` -> `granted` transition raising the restart
  requirement was observed only as the problem, not as the fix. Reproducing
  it needs `tccutil` to revoke the grant on a Mac the owner is using, which
  was not done; the behavior rests on `a_screen_recording_grant_requires_a_restart`
  and on the macOS rule the Decision cites. Notarization, publication and the
  other three Macs in the fleet are untouched.
- **Device acceptance**: partial — this Mac only.
