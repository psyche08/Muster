# ADR 0022: Sanitize the current source and documentation for publication

- Date: 2026-10-22
- Status: Accepted
- Related: [ADR 0001](0001-require-adr-for-changes.md); [public content policy](../PUBLIC_CONTENT.md)

## Context

The current tree includes operator identities, private deployment access details,
internal task references and device-specific acceptance evidence. Publishing it
would disclose more than the implementation and its verification require.

## Decision

Sanitize the current tree in an isolated branch. Preserve technical decisions,
observed outcomes and unresolved verification limits. Replace personal paths,
private host names, task titles and deployment identifiers with explicitly
labelled examples. Historical documents remain historical; anonymization is not
new device acceptance and does not change their accepted decisions.

Keep public product identifiers, repository URLs, download/relay endpoints,
release hashes, public verification keys and code-signing trust requirements.
These are compatibility or verification inputs, not credentials. Do not change
security checks or deployed service configuration as a side effect of redaction.
Use private environment configuration for deployment access instructions.
Remove personal contact details from checked-in website templates; operators
must configure a private contact channel before deploying those templates.

Add a local current-tree content check and ignore local credential files. Keep
hosted Actions disabled. This decision does not authorize rewriting commits,
editing old PRs, changing repository visibility, or publishing a new release.

## Alternatives

Deleting all validation records would lose useful failure and acceptance limits.
Blindly replacing all domains, signing teams and hashes would break installed
clients and trust checks. Rewriting Git history is a separate operation requiring
its own reviewed plan; cleaning HEAD cannot remove earlier disclosures.

## Consequences

Public records retain reproducible technical reasoning but no longer identify
individual test devices or internal accounts. Original evidence must be retained
outside the public tree if later forensic correlation is required. The policy
lists intentionally retained public identities and the remaining publication work.
The automated check detects selected patterns, not every possible personal datum.

## Verification

Source verification on 2026-10-22:

- Current-tree privacy rules report zero findings; four checker tests cover
  permitted examples, rejected identities/tokens, private versus public key
  markers, and tracked-text scope without following symlinks or printing secrets.
- 25 Rust tests pass: native sidebar (9), roster (6), diagnostics (3), attachment
  RPC (4), and local network link integration (3). These use fixtures/local test
  services, not the three installed devices.
- Both edited Swift test files pass frontend syntax parsing. Hosted iOS tests
  and full Apple/HarmonyOS application builds were not run; HarmonyOS changes
  only the publisher display field, and Swift changes only fixture text.
- 61 privacy-policy/renamed-document links resolve. Diff whitespace checks pass.
  Signing requirements, Xcode signing configuration and release fingerprint
  match strings are unchanged. Source/artifact hashes remain intact; public
  notarization IDs remain available for verification.
- The numbered ADR is included for the landing gate. Hosted Actions remain
  disabled; absence of hosted checks is not a CI pass.

No signing, notarization, publication, installation or physical-device/provider
acceptance is performed for this cleanup. Website templates require a configured
private support contact before a separate deployment. Git history and remote PR
metadata are outside this change.
