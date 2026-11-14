# ADR 0210: Refresh the offline turn pricing catalog

- Date: 2026-11-14
- Status: Accepted
- Related: [Agent interface](../AGENT_INTERFACE.md), [history projections](0153-large-session-history-costs.md)

## Context

The September catalog predates GPT-6.1 Sol and Terra and Claude Sonnet 5.5.
Even with complete token records, their turn summaries have no estimate.
Speed tables and the date shown by the viewer were maintained independently.

## Decision

Generate the shared Rust model and speed tables from one ccusage revision.
Keep the existing OpenAI and Claude model scope and retain historical model IDs
removed upstream. Bundle the revision and its date alongside the speed tables.
Use the published Fast, Flex and Ultrafast rates where available, including
the documented Claude Fast rates. Preserve recorded costs and unavailable
estimates for unknown rates. Use per-request model metadata when Codex provides it.

## Alternatives

Fetching prices during session reads would add a network dependency to offline
history. Copying individual new rates by hand would leave the next catalog
refresh dependent on remembering the other models, speed tables and date.

## Consequences

A release carries a reproducible catalog. Historical records are estimated
against that bundle when reopened. The existing history projection cache
includes the installed binary identity. Exact historical invoices and models
without published rates remain outside the estimate.

## Verification

Regenerate the catalog twice at the pinned revision. Exercise completed turns
with new models, speed settings, cache reads and writes, long contexts,
and missing rates. Run the agent crate tests and the repository checks.
Device installation and billing reconciliation require separate verification.

On macOS the agent crate passed 463 tests with five ignored. Two imports of
ccusage revision `83d3f209a86e8b6f1e86eaf93f10568a262fd4e4` produced
byte-identical tables. The catalog contains 170 model IDs. The Mythos 5.2
entry supplements the upstream snapshot using the [provider's model specifications](https://platform.claude.com/docs/en/models/mythos-5-2/overview).
