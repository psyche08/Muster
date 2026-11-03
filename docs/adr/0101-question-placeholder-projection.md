# ADR 0101: Recognize the empty optional question reply projection

- Date: 2026-11-03
- Status: Accepted; device acceptance pending
- Related: ADR 0055, ADR 0096, ADR 0098; architecture/agent-halo.md
- Partially supersedes: ADR 0096's zero-character requirement for placeholder values

## Context

Repeated attended replies failed even after ADR 0098's bounded selection wait.
The current field reports seven value/document characters, matching focus, and
an unchanged `(0, 0)` selection through the entire wait. No deletion or Submit
was dispatched. The user then supplied a screenshot of the expanded single
free-text question: the field visibly contains only the gray `Reply…` prompt,
and Send is disabled. The screenshot is visual evidence, not a raw AX dump or
proof of the exact AXValue bytes. It contradicts treating a nonzero character
count alone as sufficient evidence of an editable draft.

## Decision

Keep the original draft and deletion rules. Add a separate Shared Rust empty
projection rule for one exact, free-text-only pending question. The profile
explicitly lists its known reply hints (`Reply…` and `Reply...`); other profiles
have none. A readable value, after trimming surrounding whitespace, must match
one of these hints. A supplied nonempty native placeholder must agree. Require
an explicit character count equal to the full value's UTF-16 length, collapsed
selection `(0, 0)`, and the exact card's uniquely located Submit reporting
AXEnabled=false. Missing or contradictory evidence never uses this rule.

Only this complete combination authorizes the existing one answer write without
first deleting the visible hint. A requested answer literally equal to the hint
still requires a write on an empty projection; an enabled Submit preserves the
existing exact-answer/draft behavior. Multiple questions, option-bearing cards,
unknown profiles and unreadable Submit remain on the existing path. Shared Rust
owns question cardinality, profile configuration and the decision; the macOS
adapter only collects field/Submit observations from the freshly bound form.
The two-second preparation deadline, authorization, process/session binding,
no uncertain replay and existing nonblocking final answer semantics remain.

The command resolver must stop at a local matching button even when disabled,
and reject ambiguous same-label buttons. Previously filtering disabled commands
before determining the card boundary could walk upward and borrow another card's
enabled Submit. This fixes that related target-binding gap for command lookup;
no disabled button is pressed, and aliases on one control still deduplicate.

## Alternatives

More clearing attempts or longer selection waits cannot establish that a hint
is actual editor text. Treating every seven-character value or every collapsed
selection as empty could merge a real draft. A hint whitelist alone cannot
distinguish text typed literally. A disabled shared Submit cannot establish the
emptiness of one field in a multi-question form; those forms remain excluded.

## Consequences

The observed single optional reply can use the native text write instead of
trying to select/delete its placeholder. Unsupported languages/layouts retain
explicit failure until separately evidenced. No screenshot, private session or
user content is committed. This does not establish success of text entry or
final Submit; phone receipt and lock cleanup still require separate acceptance.

## Verification

Test projected hints with whitespace and nonzero counts, absent native hints,
literal hint answers, enabled/unknown Submit, noncollapsed/missing selections,
unknown text, count disagreement, unrelated profiles and multiple/choice
questions. Verify disabled Submit remains scoped to its own question card.
Run affected Rust tests, Clippy, formatting, layer, public-content and ADR gates.
Build/sign/notarize locally without CI, preserve the installed source ancestry,
and install using the guarded update lease. Report build/install separately
from attended answer and Locked Use acceptance; no public release is requested.

Local source validation passed 313 agent, 39 platform and 97 daemon library
tests (six existing ignored), including three placeholder-evidence regressions
and the disabled/ambiguous local Send boundary regression. Strict all-target
Clippy, formatting, layer and public-content checks passed. Build, installation
and actual provider delivery remain separate from these synthetic checks.
