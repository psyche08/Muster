# ADR 0036: Preserve native terminal events and bound cancelled requests

- Date: 2026-10-23
- Status: Accepted
- Related: [ADR 0035](0035-native-terminal-dev.md), [ADR 0033](0033-managed-terminal-compatibility.md)

## Context

Native host routing has simulation coverage, but iTerm2's notification coalescer
can discard termination and cursor-only changes. Cancelled requests can retain
pending response entries, and close responses are not checked for refusal.
These failures can be verified without native GUI permissions.

## Decision

Keep terminal behavior in Shared Rust. Preserve termination during coalescing,
publish cursor-only frames, resynchronize after notification loss and propagate
buffer/close failures. Bound request queueing and response wait together, remove
pending entries on cancellation, and do not replay submitted input.
Unsupported Ghostty resizing returns an explicit error instead of a successful
no-op receipt; its existing native window sizing policy is unchanged.

Extend simulated protocol, native RPC and app-core regression coverage for the
existing terminal capabilities. Record a reproducible acceptance matrix, keeping
real GUI, permission, physical-device and latency acceptance explicitly pending.
Build only the isolated local dev bundle.

## Alternatives

Retrying mutations after connection failure may duplicate input or windows.
Relying only on happy-path GUI tests would miss deterministic event races and
would require permissions the operator has deferred.

## Consequences

No new host capability, native permission grant, installation or release is
introduced. An unreadable or refused operation becomes an explicit error;
simulation cannot prove native accessibility rendering or real device latency.

## Verification

Source verification on the final local revision:

- The full terminal/daemon/app-core suite passed 233 tests, with five opt-in
  tests excluded from its default run. Four real-tmux tests then passed on
  dedicated private sockets; the remaining ignored test is an existing manual
  app-core diagnostic. The reproducible script completed successfully.
- Protocol fixtures cover cursor-only changes, coalesced output/termination,
  lost notifications, out-of-order/late responses, queue and response timeouts,
  cancellation during connection setup/fetch, close refusal and a receipt racing
  disconnect. A child-process Unix WebSocket fixture reconnects for the next
  input without replaying the previous input or scanning inventory first.
- Seven native RPC fixture tests cover independent sources, slow/failed reads,
  simulated permission denial/recovery, exact IDs, duplicate/conflicting input,
  concurrent/ambiguous creation, unsupported resize and view-only cancellation.
  Remote scope tests reject native operations before adapter lookup. Shared
  app-core tests check both host choices and managed handles on the terminal's
  device despite an unrelated global selection, plus frame projection/guards.
- Strict Clippy, formatting, shell syntax, architecture-layer and public-content
  checks passed. The final dev bundle built and passed local signature checking.
  The ADR gate is checked on the committed branch before landing.

Runtime: the rebuilt dev app started a separate software-identity daemon, and
read-only local RPC confirmed readiness, no virtual interface and no configured
relay. It neither drove native windows nor changed system permission settings.

Signing: local ad-hoc signature only; certificate signing/notarization not run.
Installation/publication: neither macOS nor iOS was installed or published.
Real iTerm2/Ghostty GUI, macOS grants, physical iOS and end-to-end latency remain
pending. The local toolchain still warns about Rust runtime objects targeting a
newer macOS than the Swift package minimum; older-macOS compatibility is not
established by this dev build. Hosted Actions stays disabled under ADR 0016.

The full coverage matrix and commands are in [terminal development](../TERMINAL_DEV.md).

## Follow-up local installation acceptance (2026-10-23)

After the simulation-only acceptance above, the operator explicitly requested
local compilation, certificate signing, installation and verification. This
follow-up adds device evidence for the same terminal decision; it changes no
terminal behavior or permission policy.

- Clean source `3ed95562dd1091aebe641c084117f28606b6d898` built the Release
  daemon with SEP, CLI, browser helper, arm64 FFI and macOS app. The existing
  Developer ID identity and matching existing provisioning profile signed the
  app. Strict nested signatures, Presence signing and release source ancestry
  checks passed. No Keychain unlock command or permission-setting change ran.
- The Rust desktop update lease guarded local replacement of the installed app.
  The first launch failed because launchd could not open a requested log path;
  an ordinary LaunchServices open succeeded. Read-only RPC confirmed version
  1.0.87 at `3ed95562`, the original device identity and SEP backend,
  Accessibility granted, Vnet up and relay registered. The GUI process ran.
  The previous bundle was archived locally. This local build was not notarized.
- During the remaining validation, the separate 1.0.88 release installation
  recorded in [ADR 0039](0039-exact-session-navigation.md) replaced that local
  build. Final on-disk and RPC checks identify
  `ae8c2002162b0ace403a64c4dc76adcbdc471c65`. Its strict signatures and stapled
  ticket validate, and identity/network state remain healthy. This descendant
  contains unchanged terminal adapters and daemon terminal routing. The newer
  installation was preserved; it is not the locally signed artifact above.
- The following real Ghostty acceptance belongs to the final 1.0.88 runtime:
  native new-window creation; repeated create returning the same qualified ID
  with exactly one new terminal; history reading; benign shell input and its
  output; duplicate input returning the deduplicated receipt with exactly one
  output marker; attachment delivering an initial full frame and output delta;
  unsupported resize returning an explicit error; close removing the created
  session. Only the dedicated test window received input and was then closed.
- iTerm2 still reports its API unavailable; the GUI setup sheet also requests
  automation/API authorization. Its live operations remain pending, as do
  locked-console terminal use, remote/iPhone terminal acceptance, sustained
  latency measurement and older-macOS compatibility. Simulation coverage above
  does not establish these results.

This follow-up published neither macOS nor iOS and did not submit notarization.
The separate paired release evidence is attributed to ADR 0039. Generated
artifacts and raw device/session data remain outside tracked source. Hosted
Actions remain disabled; these are local checks.

## Both native hosts accepted on the replacement installation

The operator subsequently requested real iTerm2 and Ghostty acceptance and
replacement of the locally installed Muster. No terminal implementation change
was needed for this follow-up. The plan was to read existing sessions without
input, exercise a dedicated new window for each source, check reconnect and
operation deduplication, then close only those test windows.

- Clean source `8a455e404b4d921057502c9413b3510597f32f7f` built the Release
  SEP daemon, CLI, browser helper, arm64 FFI and macOS app, version 1.0.90 (81).
  Developer ID signing, Presence signing, strict nested signatures and source
  ancestry checks passed. The Rust update lease guarded replacement of the
  installed bundle; the previous bundle was archived locally. Normal app
  startup succeeded and final app/daemon hashes still matched this local build.
- iTerm2 3.7.6 became available at its permanent application location after the
  former temporary test copy had been cleaned up. The operator enabled its
  Python API and connection/automation authorization. The installed daemon
  reported both native sources, iTerm2 API available and Automation granted
  for both applications, with no inventory errors. No authorization setting
  was enabled programmatically.
- For **each** source, one pre-existing session returned nonempty history and a
  full attachment frame under its exact source-qualified ID. Cancelling that
  subscription preserved the original session. History checks requested up to
  300 rows; short trailing-row reads in iTerm2 can contain blank screen rows.
- For **each** source, creation produced exactly one new session in a distinct
  native window. Repeating its operation ID returned the same session without
  creating another window. After confirming the test window's shell prompt,
  octal-encoded benign `printf` input produced an exact Unicode marker through
  both a streamed output delta and history. Repeating the input operation ID
  returned the deduplicated receipt, and the output marker appeared once.
- Detaching and reattaching to each test session preserved its output. A second
  input on the reattached session produced a further output delta. Closing
  the dedicated window removed only that session from inventory; both original
  sessions remained. Failed harness preflight attempts were cleaned up before
  the completed runs. No input was sent to the original user sessions.
- Final RPC and file checks confirmed source `8a455e40`, the original device
  identity, SEP backend, Accessibility granted, Vnet up, relay registered,
  both native automation grants and the live iTerm2 API. The installation did
  not change to another release during these acceptance runs.

These are unlocked local native-adapter tests through the installed daemon.
They do not establish remote/iPhone behavior, locked-console operation,
multipane targeting, native UI rendering parity, sustained latency or older OS
compatibility. No new simulation suite run is claimed for this documentation-only
follow-up. No notarization submission, macOS publication or iOS OTA publication
was performed. Hosted Actions remain disabled. Generated applications and raw
session/device evidence are excluded from tracked source.
