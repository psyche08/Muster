# ADR 0053: Native read-only Multica workbench

- Date: 2026-10-26
- Status: Accepted for the read-only workbench; desktop runtime execution remains unimplemented
- Related: [ADR 0051](0051-multica-management-and-live-catalog.md), [ADR 0052](0052-targeted-multica-deployment.md), [native creation review](../NATIVE_SESSION_CREATION_REVIEW.md)

## Context

The current managed bridge is detected as kiro-cli, but dispatches every ACP
session to Codex CLI. This is a protocol compatibility alias, not a Kiro runtime.
The owner requests Codex through ChatGPT.app and native Claude through Claude.app,
with CLI fallback only when the corresponding application is absent, and no
advertised capability when neither exists. An installed application alone is
not proof of a supported native session creation or scheduler credential path.
The existing native-create prototype has not passed provider acceptance.

The owner also requests native Multica UI, scoped to workspace selection,
issue lists/details, and agent/runtime status. Creation, assignment and posting
comments are expressly excluded from this iteration.

## Decision

The requested routing policy remains a requirement, not an implemented feature
in this decision. Keep future routing, availability and credential state in Shared
Rust, and distinguish execution destinations from scheduler protocol aliases.
Never change an unavailable or failed native operation into a
CLI send after it could have taken effect; unknown outcomes are not replayed.
Native identity, working directory, task-scoped environment, Locked Use and
provider receipts remain required gates, not inferred from a successful launch.

Replace the WebKit page with typed native views over a local-owner read-only
Multica query ABI. Reuse the logged-in container CLI without exporting its login
credentials, switching its default workspace or executing arbitrary CLI strings.
Allowlist commands and arguments, bound output/time/page size, and project only
explicit public fields. Use request generations to discard stale workspace or
detail responses. CLI errors must not surface raw credential-bearing output.

## Alternatives

Renaming the existing Kiro alias would misrepresent the executor. Always using
CLI would violate the requested application preference. Browser scraping would
retain the web dependency and duplicate authentication. Decoding raw service JSON
in Swift would move business state out of Shared Rust.

## Consequences

The native page is initially local to a desktop with an installed container.
It retains the existing CLI login and server configuration. Native provider
routing requires the full session lifecycle, not only application discovery.
A missing prerequisite is explicit and cannot authorize silent fallback.

## Verification

The native read-only workbench is implemented. The desktop-first execution
mapping is not implemented and the existing Kiro-compatible bridge still targets
Codex CLI. In particular, this change does not advertise desktop execution, add a
CLI fallback or change Multica's registered runtime providers. The native-create
candidate lacks accepted identity/directory/receipt calibration and a task-scoped
credential channel for desktop-created processes; its environment handling must
not be replaced by a global user credential or a token embedded in a prompt.

- The Rust query code was executed against an operator-supplied read-only
  container adapter on the target desktop: two workspaces, a 50-issue page,
  54 agents and 66 runtime records decoded successfully. The first issue and
  agent details matched their requested IDs; the second workspace and an empty
  final page also passed. No CLI configuration was created,
  default workspace changed, task created, assignment changed or comment posted.
- Fixture tests cover fixed read-only command arguments, pagination, workspace
  isolation, omission of custom environment/MCP secrets, and stale workspace and
  detail responses. The native macOS MusterUI target compiles. Clippy with
  warnings denied passed for daemon/app-core libraries and tests.
- The relevant full Rust library suites passed 180 tests (three manual tests
  ignored), and the separate real-link access test refused remote Multica
  browsing. Its first run hit the sandbox loopback restriction; the permitted
  loopback run passed.
- Build/source verification is separate from installation: no signed release,
  notarization, deployment, native desktop task creation, scheduler task, phone
  UI or physical-device execution acceptance is claimed for this change.

The WebKit tab decision in ADR 0051 is superseded for the native desktop page.
Container installation, login, keep-alive and the attended migration from
ADR 0052 are unchanged. Desktop-first runtime execution requires a separate
implementation and provider acceptance; it is not implied by this UI landing.
