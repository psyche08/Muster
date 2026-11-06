# ADR 0154: Edit Multica agents and operate issues through the host workbench

- Date: 2026-11-06
- Status: Accepted for the local workbench slice; full service and device acceptance pending
- Related: [ADR 0141](0141-multica-service-and-dsh-app-bridge-design.md),
  [ADR 0153](0153-multica-task-session-isolation.md)

## Context

The workbench only reads metadata. The operator requests agent configuration
editing and the issue lifecycle after consulting deployment documentation.
Installed CLI 0.3.58 confirms agent update, issue create/update/assign/status,
comment, runs, cancel-task and rerun commands. The installed version uses
`assign --to-id` and `cancel-task <task-id> --issue <issue-id>`, unlike the older
command summary. Documentation distinguishes a
new-session manual rerun from same-session infrastructure recovery.

The installed DSH App also bundles a CLI. Inspection confirms that its launcher
calls the ordinary profile CLI using the App's immutable runtime; its help boots
web/headless/tui profiles. This alone does not establish IPC to the running
Desktop profile. The existing in-App plugin remains the desktop destination;
no headless runtime substitution is authorized by discovering the launcher.

## Decision

Expose typed, local-host workbench mutations through Shared Rust and the existing
container CLI. Cover ordinary agent configuration fields, issue creation/editing,
assignment, status, plain comments and cancellation. Starting/rerunning remains
unavailable until verified App task binding is connected; preserve the distinction
between new-session rerun and same-session recovery. Keep credentials, shell commands, environment variables, arbitrary CLI
arguments and provider identity administration outside this interface.

Validate workspace and exact resource identity before each mutation. Re-read the
resource and reject stale editable revisions. This is a preflight check, not an
upstream compare-and-swap guarantee. Serialize host workbench writes. Persist the
operation before launching its command; identical submissions never rerun and a
changed request with the same operation ID conflicts. Any uncertain post-launch
failure remains unknown, with no automatic retry. Read-only refresh is separate
from mutations. Report accepted changes separately from task completion.

Only the host may perform these administrative operations in this phase. Remote
views never acquire edit authority from connection, local-owner forwarding or
view scope. Continue to report remote task control as unavailable. Swift collects
form input and renders Rust result state; validation, CLI construction, operation
tracking and authorization stay in Rust. New issues default to an unassigned
backlog. Assignment and status use the verified `--no-start` flag. Comments
containing `@` or a `mention:` link are refused because mentions can launch
unbound runtime work.
These restrictions prevent this new UI from substituting a CLI launch for the
requested App task route. Existing external automation remains platform-owned.

## Alternatives

Exposing raw CLI or JSON gives the UI broad host authority. Replaying a command
on timeout can duplicate issues, comments or runs. Treating every CLI failure as
rejection hides possible accepted effects. Reusing a DSH headless profile because
its executable ships in the App violates the requested execution destination.

## Consequences

Local workbench operations become useful without waiting for the DSH scheduler
bridge. The full task service still needs verified production bindings, task
transcripts, in-App scheduling and scoped remote execution. Upstream permission
checks still govern edits; only non-secret editable fields are projected locally.
Private deployment documentation and observed account/task data stay outside the
public repository. No live task is mutated merely to test this implementation.

## Verification

- Source tests: the focused daemon Multica suite passed 38 tests (one live test
  ignored), app-core passed 163 (one ignored), and two real-link access tests
  passed. Coverage includes stale/wrong-workspace/editor denial, literal stdin
  and argument preservation, exact run cancellation, no-start assignment/status,
  scheduling refusal, duplicate/conflicting requests and ambiguous results
  without replay. App tests cover host-only, single-flight and late replies.
- Build: strict Clippy passed for daemon and app-core across all targets.
  `MusterUI` compiled for macOS and arm64 iOS Simulator. Swift protobufs were
  regenerated with the checked-out package's generator.
- Read-only installed CLI help and response shapes were compared with deployment
  documentation. No real agent, issue or task was changed to obtain evidence;
  subprocess tests use synthetic fixtures. The App's bundled CLI launcher and
  its help were inspected without booting a personal profile.
- No signing/notarization, publication, installation, GUI acceptance or physical
  cross-device task acceptance. Production task bindings, complete transcripts,
  App scheduling/credentials and remote control remain unimplemented. This is
  not full P0–P4 acceptance.
