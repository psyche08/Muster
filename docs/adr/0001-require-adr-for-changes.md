# ADR 0001: Require an ADR for every repository change

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

- Date: 2026-10-20
- Status: Accepted
- Related: AGENTS.md; docs/ARCHITECTURE.md; user requirement after the Mac-C evening log review

## Context

Release notes described device outcomes but did not consistently preserve the
reasoning behind protocol, UI, and operational choices. The user requires an ADR
for all subsequent changes, including the current fixes.

## Decision

Every PR with repository changes adds or updates a numbered ADR. No size or
file-type exemption applies. A coherent change may share one ADR; an evidence-only
follow-up may update that decision's verification section. A changed decision
gets a new superseding ADR instead of rewriting accepted history. Read-only work
that changes no repository files does not require an empty ADR or commit.

AGENTS.md defines the rule and the ADR index/template defines its format. The
landing script checks the committed branch diff against origin/main for an added
or modified numbered ADR. PR descriptions link the relevant ADRs.

## Alternatives

Recording only large architectural changes would omit the small API and timeout
choices that caused this incident. Release notes alone describe results without
capturing alternatives. A mandatory new ADR for every evidence update would
fragment a single decision; updating the existing record preserves continuity.

## Consequences

Small changes also carry a short decision record. The automated check detects a
missing record, not whether its reasoning is sufficient; review must establish
that it covers the actual change. Deleting an ADR or editing only the index or
template cannot satisfy the check. Local bypasses are not approval to omit ADRs.

## Verification

The check is exercised with temporary Git histories for missing, index-only,
added, modified, deleted, and renamed ADR cases. It runs before publishing a PR
from finish-session.sh. Current work adds ADRs 0001–0003; the workbench record is
explicitly retrospective rather than claiming a pre-existing decision process.
