# ADR 0023: Bootstrap a sanitized repository with one commit

- Date: 2026-10-22
- Status: Accepted
- Related: [ADR 0022](0022-sanitize-current-public-tree.md); [public content policy](../PUBLIC_CONTENT.md)

## Context

The user changed the publication plan: rename the existing private repository,
create a new Muster repository, and import the sanitized tree as one commit.
This avoids carrying old authors, file versions, PR diffs and hosted caches into
the new repository. Historical release ancestry checks cannot resolve old commits
in a root-only snapshot and must remain meaningful for future builds.

## Decision

Keep the original repository and all its PRs/history private under an archive
name. Point existing local worktrees at that archive; do not rewrite or remove
their history, unpublished branches or local files. Create a new private Muster
repository with Actions disabled before pushing any source. Import tracked files
only, using a fresh Git object database and a single root commit with a public
noreply author identity. Never push old refs, PR refs, objects, reflogs or local
credentials into the new repository. The user explicitly requests this bootstrap,
so the one-time initial push replaces the usual PR/merge workflow; subsequent
changes still follow AGENTS.md.

Preserve original released build stamps, artifact hashes and release records.
Before export, verify that every recorded release is an ancestor of the archived
source snapshot. A bootstrap manifest binds the new root tree (excluding only
that manifest) by digest and records the archived source identity. The release
gate may accept historical release/version pairs only when present in that
verified root ledger. Later releases still require normal commit ancestry;
unknown pairs, modified root contents, malformed manifests or multiple roots
fail closed. The clean-HEAD daemon stamp, signing and binary fingerprint checks
remain unchanged. This records a source baseline, not rebuilt historical binaries.

The new repository stays private until its root history and hosted surfaces have
been reviewed. No visibility change, app publication or device operation is part
of the bootstrap. The archive must remain private; its old personal data is not
sanitized by renaming it.

## Alternatives

Rewriting the old repository would retain PR/cache cleanup dependencies and risk
recontamination from existing clones. The user instead selected a fresh repository.
Dropping release checks would lose protection against missing past fixes; importing
old commit objects would defeat the single-commit privacy boundary.

## Consequences

Old commit and PR references in documentation describe the private historical
record, not navigable objects in the new repository. Existing clones continue to
serve the private archive; new development uses a fresh clone. Public product
identifiers retained by ADR 0022 remain intentionally visible. The website privacy
template still requires a configured private contact before a separate deployment.

## Verification

Before export, all 50 recorded releases passed ancestry checks against the
archived snapshot. Seven release-gate regression tests passed: exact historical
pairs, tampered baseline, future ancestry, current-manifest tampering, unrelated
history, legacy behavior, and malformed/duplicate ledger rejection. Shell syntax
and the four existing privacy-checker tests also pass.

The initial import must pass the current-tree privacy check, a single-root count,
root tree digest validation, and release ancestry verification. Re-clone the new
remote to verify that only the sanitized root and its reachable tree/blobs were
uploaded, with no PRs, tags, logs or old Git refs. Keep both repositories private
and Actions disabled. Hosted state and final commit IDs are reported outside the
root commit to avoid creating a second commit merely to record its own identity.

No app build, signing, publication, installation or physical-device acceptance
is claimed. The seven gate tests are isolated temporary-repository tests.
