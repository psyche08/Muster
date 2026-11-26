# ADR 0322: Give native CLI sessions provider-specific identities

- Status: Accepted
- Date: 2026-11-26
- Related: [ADR 0173](0173-direct-native-cli-protocols.md), [architecture](../architecture/runtime.md)

## Context

Muster runs native Codex and Claude child processes. Its first CLI adapter named both providers `orca` in session listings, access settings and saved references. Owners need independent grants and users need to see which CLI owns each session.

## Decision

Identify native sessions as `codex-cli` and `claude-cli` from the agent stored in the owning journal. Use separate IDs for new sessions. Preserve old `orca:` session IDs, callback bindings, operation fingerprints and the existing SQLite journal. Preserve the creation wire field and its existing agent values for old clients. New clients negotiate the provider-specific service and request its catalog. Handle the legacy `orca` catalog as a filtered union of the caller's authorized native sessions.

Historical `agents:orca` covers the two native CLIs only. Explicit native provider grants take precedence over that historical aggregate for the same subject. An owner editing an old aggregate can save explicit provider grants. Desktop Codex and Claude scopes stay independent. An Orca IDE integration requires its own new permission identity.

## Alternatives

Migrating session IDs would invalidate bindings and prompts. Inferring a provider from user-visible titles or client-supplied IDs would make authorization depend on untrusted input.

## Consequences

Both providers share the existing journal implementation. Each session still resumes the correct child protocol and owns its transcript and approval history. The receiver uses the stored provider when admitting session requests and the requested native provider when admitting creation.

## Verification

Exercise creation, idempotent operation replay, upgrade, catalog filtering, provider-specific access, side questions and prompt responses against synthetic data. Check that a single native grant cannot read or operate the other provider's sessions. Device installation and interactive provider acceptance require separate checks.

The Rust suites passed 913 tests, with 10 existing tests ignored. Both native protocol fixtures exercised their child processes. The two-node RPC suite verified canonical and legacy creation replay and provider-scoped visibility. The macOS client passed 123 Swift tests. The Windows x64 Release GUI cross-build completed with zero compiler warnings or errors.
