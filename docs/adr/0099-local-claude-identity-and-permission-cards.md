# ADR 0099: Bind local Claude sessions and permission cards by identity

- Date: 2026-11-03
- Status: Accepted; source verified, device acceptance pending
- Related: ADR 0040, ADR 0075, ADR 0095, ADR 0018
- Supersedes: ADR 0095's title fallback for local Claude sessions

## Context

The owner reports missing Claude authorization controls and requests UUID-based
session lookup. The owner explicitly limits this adapter to local sessions;
Remote Control `session_…` surfaces are outside scope. Current exact navigation
can fall back to sidebar titles when registry mapping fails. Forward resolution
selects the first account's file and does not validate its session ID. Approval
observation retains controls based only on option labels and presents no request
content, so successive requests with identical buttons are indistinguishable.
The reported screenshots alone do not prove a local-device failure mechanism.

## Decision

Shared Rust resolves local desktop IDs and CLI UUIDs through the same bounded,
unique registry mapping in both directions. Reject remote IDs, SSH-owned entries,
archived or ambiguous mappings, malformed evidence and mismatched file identity.
Do not resolve remote aliases or consult titles for local Claude navigation.
Require the fresh opened surface to resolve to the requested UUID as before.

Observe Claude's named permission-request card as a bounded AX snapshot. Shared
Rust selects its exact enabled provider buttons and projects its request text.
Bind the interaction to that content and the current native elements; replace
its interaction ID when either changes. Re-observe before answering and reject
stale decisions. Incomplete observations provide no permission authority. Keep
legacy provider selectors for older unnamed cards and other providers, with the
same fresh-element checks. Do not create grants, synthesize options, change
approval policy, or retry an uncertain press. All decisions remain Rust-owned.

## Alternatives

Titles change and collide, so choosing the most recent or unique-looking title
is insufficient. Broad label matching cannot distinguish a permission request
from sidebar quick actions or a later command with the same options. Remote
session aliases would violate the explicitly requested local-only boundary.

## Consequences

Unmapped sessions fail explicitly instead of selecting by title. Provider UI
changes can make cards unavailable; physical local-session acceptance remains
necessary. Request text is shown only in the existing authorized interaction
view and is not logged or included in repository evidence.

## Verification

Cover distinct desktop/CLI UUIDs, duplicate titles/accounts, rename, resume,
archived/remote/SSH entries and malformed mappings. Cover command-card text,
exact buttons, disabled options, multiple cards, incomplete reads and stale
card replacement. Run affected Rust tests, Clippy and repository gates. Source
checks, build, signing, publication, installation and local/phone/Locked Use
acceptance are separate; no screenshot command is authorized for execution.

### Source verification

On macOS arm64, 309 agent tests and 38 platform tests passed on the final
source (four pre-existing agent tests ignored). The daemon suite passed 97 tests
(two ignored) before the final profile-driven card refresh and question-priority
refinement; final strict Clippy compiled all three crates and their tests.
The agent/platform suites were rerun after that refinement. The initial
sandbox run refused five existing Unix-socket tests; they passed when rerun
with local sockets allowed. Layer, formatting, public-content and ADR gates
are checked before landing.

Registry entries are parsed once per file modification-time/length stamp;
every resolution re-enumerates current files and checks uniqueness. Cached
metadata contains only identity/exclusion fields, never titles or transcripts.
Approval observations are serialized so a slower poll cannot overwrite a newer
request snapshot. Named permission cards take priority over pending questions;
older unnamed controls retain question priority.

No provider command or approval was executed. No signed/notarized artifact,
macOS installation, iOS publication, phone or Locked Use acceptance is claimed.
The inspected screenshot surface was Remote Control and is excluded; local
permission-card activation still requires installed-device acceptance.
