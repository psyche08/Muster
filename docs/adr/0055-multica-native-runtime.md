# ADR 0055: Desktop-first Multica runtimes

- Date: 2026-10-27
- Status: Accepted; device acceptance pending
- Related: ADRs 0009, 0046, 0048, 0053 and 0054

## Context

The Kiro ACP compatibility command actually creates Codex CLI sessions. The
owner requires separate Codex and Claude runtimes, preferring ChatGPT.app and
Claude.app, falling back to the corresponding CLI only if its application is
absent, and hiding providers with neither executable. Native first and existing
turns have operator acceptance; those scripts are not a production adapter.

## Decision

Keep destination selection, protocol translation, pending-session binding,
credential lifetime and receipt interpretation in Shared Rust. Publish real
Codex app-server and Claude stream-json entry commands, not a renamed ACP alias.
Choose the destination before creation and pin it for that session. Installed
but stopped, windowless, locked or unavailable applications never select CLI.

Native creation is a ledgered daemon operation through the normal console and
provider locks. Launch the signed bundle when necessary; identify a newly opened
window by object identity, preserve existing drafts, verify the full directory,
local mode and exact composer echo, record delivery before one Send, then bind a
new provider transcript by directory and first message. Revalidate retained
window/page objects before each mutation. An unknown result is never replayed.

Scheduler session creation may precede the first prompt. Persist an opaque
pending binding and a submit-attempted state before native dispatch, then replace
it with the confirmed provider ID. Resume must resolve that binding; it must
not create a new conversation after an ambiguous first turn. Completion requires
provider transcript evidence, not application visibility or an idle UI alone.

Task credentials stay in a private, task-scoped host context. A task-local CLI
launcher supplies them only to the scheduler CLI child; no token enters a
prompt, URL, global application environment or app ABI. The task receives only
the nonsecret launcher path. Delete credentials when the bridge turn ends.
Reject unsupported scheduler controls rather than silently pretending they
changed the desktop application's settings.

## Alternatives

Keeping the Kiro alias obscures the actual provider and protocol. Routing all
requests to CLI violates desktop preference. Injecting per-task secrets into an
already running application contaminates unrelated sessions. Reusing the old
native-create selectors fails on current standalone pages and window recovery.

## Consequences

Desktop execution needs an available console and Accessibility. The adapter
preserves application permission settings and fails explicitly on unimplemented
scheduler controls. Container storage/login and the selected Documents root are
preserved. This is a targeted desktop deployment; public desktop/iOS feeds are
not published or altered.

## Verification

Pending: routing matrix, absent/installed-but-stopped/windowless distinctions,
protocol transcripts, durable first-send ambiguity, task credential isolation,
Rust checks, signed/notarized build, target installation, native first/existing
turns, CLI fallback, and actual scheduler acceptance. Record each separately;
operator scripts and protocol handshakes are not scheduler execution evidence.

### Implementation notes

Native creation is local-owner-only; it does not mint a remote Locked Use grant.
An occupied or locked console is an explicit failure, never CLI fallback. The
bridge reuses installed application settings; callers can select `native-app`
or leave model/effort unset. Explicit unsupported overrides are refused. CLI
fallback passes the original protocol and options to the resolved native CLI.
Native compatibility probes name the minimum supported protocol baselines
(Codex 0.100.0 and Claude 2.0.0), explicitly marked as Muster compatibility;
CLI probes report the executable's own version. These are not application
version claims. Runtime discovery requires the scheduler to re-detect commands
when its installed tools change, as described in the upstream runtime guide:
https://multica.ai/docs/install-agent-runtime.

The container installer opts existing installations into the managed runtime
bridge without changing their storage/login layout. Removed providers lose only
Muster-owned wrappers. The old Kiro wrapper is removed only when it contains the
Muster bridge marker. Task bindings pin their destination across reconnects.
Restart revokes leftover private task launchers; ambiguous bindings remain
blocked. Native completion uses a new exact user-message occurrence followed by
provider-declared final-answer records, not an idle application or output alone.

The regenerated Harmony binding also incorporates schema additions which its
previous generated snapshot had not yet included. No handwritten shell state
or authorization logic was added.

### Source verification before target deployment

The affected ACP, agent, platform and daemon library suites passed 370 tests
(six operator/environment cases ignored). The first sandboxed run stopped at an
existing local-socket test; the run with loopback permission passed. Strict
Clippy passed for these libraries and their tests. Swift and Harmony protocol
bindings were regenerated. These results do not establish signed installation,
windowless/cold-launch acceptance, or a real scheduler task.

Existing-container adoption also installs the host Multica CLI when missing,
using the configured trusted installer. A container-only binary cannot supply
the native task callback; opting into the bridge must provision both sides.

Multica 0.3.56 distinguishes the enterprise Claude provider from `native-claude`.
Use a separate Muster-owned native Claude command and set
`MULTICA_NATIVE_CLAUDE_PATH` when starting the adopted daemon. Preserve unrelated
Claude commands. Codex uses its own explicit path; a missing owned wrapper must
not cause container PATH fallback. CLI fallback also accepts executable
standalone script launchers, since npm installations need not be Mach-O files.

CLI forwarding owns a separate child process group and tears it down when the
bridge disconnects or is cancelled. The fallback test checks original argv,
working directory, task environment and streamed input/output through that
actual process boundary, without changing installed application inventory.
