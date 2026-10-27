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

Target adoption found an existing installation with no installer URL and a
working host Multica CLI. Reuse that local executable in the selected data
root before requiring an installer URL. Bridge startup must refuse to advertise
runtimes until the host callback CLI exists. The first deployed candidate
required this provisioning manually; final acceptance must identify the fixed
build separately.

The first native protocol attempt was refused before submission because
`codesign -R` treated the inline requirement as a filename. Prefix inline
requirements with `=`; the unchanged strict bundle/team requirements then passed
for both target applications. This is a launch-argument correction, not a
signature-validation relaxation.

Windowless Claude opens its main Code page at `https://claude.ai/epitaxy`,
whereas a new standalone Code window uses `about:blank`. Bind the newly created
window by object identity and accept either exact page only after finding its
composer and verifying local mode and directory. Wait for late trust dialogs
until the form is ready; never repeat a trust press. Home-relative folder labels
are accepted only when they are the exact `~/` rendering of the requested full
directory beneath the known home, not a basename or suffix match.

Codex with a running process and zero windows acknowledged its New Window menu
action without creating a window. Activate the verified bundle and settle AX
window publication before taking the baseline for all launches. Only a later
new window may receive input; activation never authorizes a restored window.

### Target deployment and deferred acceptance

The final deployed candidate is desktop 1.0.97 (88), built from
`cf11f2f8c5e715d984f7fc91a03062094a812bd7`. Its daemon SHA-256 is
`f163020066976fa00f4780c4af14ae1bf19b030ee2b6f3a955f7dddaa80e6062`.
Developer ID verification, Apple notarization, ticket stapling and target
Gatekeeper assessment passed. Installation preserved device identity and
restored Accessibility, Relay and Vnet. No old application backup was retained;
public desktop/iOS feeds were not published.

The existing container now has the Codex and Native Claude providers registered
online; the old Kiro alias is offline. Login, container storage and the selected
`~/Documents/Multica` root were preserved, and the previous data root remains
absent. The one idle test agent still bound to the old alias has deliberately
not been rebound before successful native acceptance.

Final source checks passed 374 library tests (six environment/operator cases
ignored), strict Clippy, public-content and ADR checks. CLI fallback was tested
through a real isolated child process for argv, cwd, task environment, streamed
input/output and input closure. The target's desktop apps were not uninstalled
to simulate their absence; physical absent-app fallback is not claimed.

Desktop acceptance is **not complete**. The latest Codex attempt started from
verified zero windows, reached a new form, and was rejected by directory/mode
validation before submitting. Subsequent observation found the window in a
different business conversation; the exact cause of the earlier validation
failure remains unresolved. The latest Claude cold-launch preparation stopped
on a physical-input guard before its runtime request. Earlier isolated
calibration confirmed the windowless Claude main form, its full home-relative
folder label, and Codex activation followed by a uniquely new window, but these
are not provider receipt or scheduler task acceptance.

The operator explicitly requested retaining the installed candidate and
postponing further desktop acceptance. Stop desktop automation at this point.
Keep the PR in draft, with no completed native first/existing-turn or real
scheduler-task claim. Resume with exclusive-console confirmation, preserve
existing drafts/tasks, inspect the recorded refused operations, and never replay
an unknown send. Full scheduler receipt, cold-launch coverage and test-agent
rebinding remain open.

### Resumed exclusive-console verification

The operator has resumed acceptance and authorized exclusive target use and
merging PR 52 after successful receipts. A repeatable zero-window Claude case
showed that activation itself creates the only new main window, then the Code
route reuses it. Record a settled pre-activation window set as well as the
pre-route set. Only when the former was empty and activation created exactly
one retained window may that window be used; a pre-existing user window remains
ineligible. Shared Rust selects by opaque object equality, never title/order.

Codex's new form exposes its composer before its selected directory/mode and
project record settle. Wait boundedly for the initial full selection proof;
process/input/window changes still fail immediately. After drafting begins,
selection changes fail immediately without retrying or sending. Verify these
paths on the installed signed build, followed by real protocol and scheduler
receipts; calibration and a visible form alone do not count as success.

Ship resumed acceptance fixes as targeted desktop 1.0.98 (89). Publication
feeds remain unchanged; record its signed installation independently of 1.0.97.

Two installed cold-launch cases reproduced a retained old folder with no
submission. Once the renderer was ready, a guarded directory-only calibration
of the same refused case opened the exact requested workspace trust prompt.
Native window/menu publication is therefore insufficient launch readiness.
Before routing, wait for the retained window's composer to remain published
for a bounded stability interval; also apply this to Codex's newly created
window. Preserve draft checks and exact post-navigation directory proof. Do
not resend a route or prompt after an uncertain submission. Rebuild and repeat
cold/windowless and scheduler acceptance on the signed installed candidate.

The subsequent installed cold case also showed replacement of the preliminary
startup window before navigation. Include composer readiness in settling the
activation window set, before binding any target identity. A changed startup
object resets settling; after the target is bound, replacement remains an
immediate failure. This case was refused before draft or submission.

Codex's zero-window case exposed the equivalent page-level race after routing:
its old blank composer can be published while the requested page is replacing
it. Retain the owned window, but bind its page/composer only after that same
page proves the full requested directory and local mode. Late trust prompts
remain in the preparation loop. Once binding/drafting begins, page or composer
replacement still refuses delivery; revalidate immediately before draft write.

### Latest targeted candidate and remaining acceptance

Desktop 1.0.98 (89) is installed from source
`5822072f7be6b630034f512cfaee97e13fe094b1`, with daemon SHA-256
`f2672ce6a3e9a432e1b802e9ef704209fa7c684f4fe73211b7f4b8d60e0d56ae`.
Apple notarization `dc8bc926-f9eb-4430-9bf7-b38961b60753` was accepted;
signature, ticket and target Gatekeeper verification passed. Device identity,
Accessibility, Relay and Vnet recovered after installation. Container data and
keep-alive configuration remain in the selected Documents root; the old root
is absent. Public desktop/iOS feeds remain unchanged.

The affected library suite passed 375 tests with six operator/environment cases
ignored. After the final platform adjustment, all 37 platform library tests
passed again. Strict Clippy, public-content and ADR checks passed. These checks
do not establish desktop or scheduler acceptance.

With operator approval, the identified pre-existing running Codex task was
stopped once. Zero-window baselines and actual Claude process exit were verified.
Earlier refused forms had not submitted the test prompt. On the latest candidate,
the next native cold-start requests were interrupted by the process/input guard.
A later 45-second quiet-admission check could not obtain 15 consecutive seconds
without input, so it started no test. Read-only mouse-event attribution then
reported sustained events with source PID 0; it does not identify a responsible
application or distinguish hardware from system-level remote input.

Native first/existing-turn receipts and actual scheduler tasks remain unverified.
No scheduler issue was created and the legacy idle test agent was not rebound.
The PR remains in draft, unmerged, pending a stable exclusive console and both
providers' real receipts. Do not weaken the input guard or replay an uncertain
submission to complete acceptance. Record any resumed results separately from
this signed installation and its source commit.

### Acceptance resumed with quiet input

After the operator disconnected input sources, event counters stayed unchanged
through a Claude cold launch, but activation readiness timed out. Both Swift
and the production Rust AX reader reproduced initial accessibility-enablement
failures (`-25204`) while the process existed before its UI was ready. Retry
only that idempotent AX adaptation until the application acknowledges it; do
not retry navigation, drafting or submission. Readiness failure diagnostics
report only counts and booleans, never window contents or credentials.

The operator has explicitly deferred Codex zero-window/cold-start checks that
would require pausing another existing task. Continue independent Claude and
scheduler checks and Codex creation in a new window; preserve that running task
and report the deferred coverage separately.

### Native Codex and scheduler receipts

On candidate `5822072`, a new-window Codex protocol turn returned
`MUSTER_NATIVE_OK` with a completed turn and a confirmed provider transcript.
The subsequent resume resolved the same binding but failed before submission:
the existing desktop selector could not uniquely locate that session. The
binding remains conservative after that attempted continuation; it was not
reset or replayed. Continuation acceptance remains open.

The existing private idle test agent was then rebound from the retired alias
to the registered Codex runtime. One synthetic scheduler issue completed on
its first attempt. Its result contained the expected marker, its opaque native
alias resolved to a real Codex transcript with phase `ready`, and transcript
inspection confirmed task-scoped Multica CLI calls and the final assistant
marker. This establishes actual Codex scheduler execution and host CLI callback
use, separately from the earlier protocol-only test. No Claude scheduler issue
has yet been created.

The next installed 1.0.98 (89) candidate is built from
`035152cf8a92c7469906706f831f1c6066289023`, with daemon SHA-256
`7a3f47443f679ddf18bcb1e7d25e8c9786fb96104d00e82b5a6aa5eef51103bf`.
Apple notarization `fb2f517f-f019-420b-b435-673013559a89`, signature, stapling and
target Gatekeeper checks passed. An initial signing attempt failed on the
timestamp service and succeeded on retry without unlocking any Keychain.
Identity, Accessibility, Relay and Vnet recovered. Its platform library tests
and strict Clippy passed; this initialization change still needs installed
Claude cold/windowless acceptance. The OS reported a locked screen after
installation, so no further desktop write was attempted. Await local unlock;
Codex cold/zero-window coverage remains explicitly deferred by the operator.
Keep the PR unmerged until the remaining required receipts and continuation
checks are resolved. Public release feeds remain unchanged.
