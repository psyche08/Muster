# ADR 0071: Bound the execution cost of small changes

- Date: 2026-10-30
- Status: Accepted
- Related: AGENTS.md; ADR 0069

## Context

A localized keyboard change required repeated model requests in a long chat
and several verification iterations. The owner requests default execution rules
that reduce unnecessary context, scope expansion and polling for small edits.

## Decision

Add repository-wide small-change defaults to AGENTS.md: narrow reads, the
smallest coherent fix, focused verification selected in advance, and completion
notifications or bounded waits. Explain evidence-driven scope expansion before
undertaking it. Deeper verification remains necessary for material risks.
New chats, model changes and delegation remain subject to authorization; the
rules do not change runtime settings or automatically shorten existing history.
All existing architecture, ADR, publication, commit and landing gates remain.

## Alternatives

A hard request-count limit could stop necessary verification. Skipping mandatory
gates would weaken traceability. Leaving the guidance only in chat would not
make it a repository default.

## Consequences

Routine edits should use less context and fewer model round trips. These are
execution defaults, not a guaranteed token budget; actual cost depends on the
model, context and evidence encountered. Required risk checks take precedence.

## Verification

Review the rules against existing repository requirements; run public-content,
whitespace and ADR checks. This is documentation-only: no application build,
signing, publication, installation or device acceptance is required or claimed.
