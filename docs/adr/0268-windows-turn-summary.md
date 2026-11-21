# ADR 0268. Display turn accounting in Windows conversations

- Date: 2026-11-21
- Status: Accepted
- Related: [ADR 0032](0032-remove-cache-ttl-display.md), [ADR 0210](0210-refresh-turn-pricing.md), architecture §3.7

## Context

macOS displays each completed turn's end time, model, usage and API-equivalent cost. The Windows transcript renders the same summary block as a generic role and message, so readers cannot inspect those figures.

## Decision

Render a footer at each Shared Rust summary block using its message timestamp and optional TurnUsage. Format local time, input and output counts, cache hit percentage, USD cost source and pricing date. Use the projected availability flags and request count to distinguish partial, unavailable and known-zero values. Keep changed-file actions attached to their existing summary IDs.

## Alternatives

Recalculating token ratios or looking up model prices in WinForms would duplicate Shared Rust accounting and lose its evidence of completeness. A separate details window would hide the footer when reading historical turns.

## Consequences

Windows readers can inspect completed turns in transcript order, including legacy summaries with missing metadata. The same lines feed the plain and image-bearing readers.

## Verification

Exercise complete, partial, absent and zero usage, recorded and estimated cost, end times and neighboring messages in the native transcript tests. Verify the visible footer on a Windows form using a synthetic conversation. Verify portable build, user upgrade and node state separately.
