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
