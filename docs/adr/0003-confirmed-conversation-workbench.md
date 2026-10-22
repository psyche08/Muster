# ADR 0003: Confirm terminal commands and return results to their originating conversation

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

- Date: 2026-10-20
- Status: Accepted, retrospective
- Related: CONVERSATION_WORKBENCH.md; PR #156; Apple 1.0.72 (63); ADR 0001

## Context

The user requested a clearer mobile conversation layout, a tmux terminal in the
conversation's working directory, and execution of shell code blocks with results
returned to the original conversation. The chosen consent model is confirmation
before execution and automatic return after completion. This ADR records the
already-landed decision; it did not exist during the original implementation.

## Decision

Chat, Activity, and Terminal are the three main destinations. Activity groups
traces, subagents, and related sessions. iOS uses native navigation and sheets.
Shared Rust identifies complete explicit shell fences and supplies an opaque
handle. Confirmation displays the command, device, and directory; both client
and daemon reject stale input. Each execution owns a separate tmux window.

Durable records retain bounded output, exit status, initiating-device consent,
and a stable return operation ID. Delivery waits until the original provider is
idle and cannot follow UI navigation into another conversation. Unknown delivery
is not replayed. Native authorization remains required. Backgrounded clients
resume delivery after foregrounding; another device cannot inherit consent.

## Alternatives

Typing into the existing terminal pane could inject commands into an interactive
program. Automatic execution from streamed text would skip consent. Returning to
the selected chat could target the wrong session. Immediate return when a code
fence closes failed because a final message can precede turn completion.

## Consequences

Commands can outlive the mobile screen. Output is bounded and marked when
truncated. Provider acknowledgement, execution completion, and UI state are
separate. Existing native protections can still require user action.

## Verification

1.0.72 was notarized, installed on Mac-C, and published with iOS build 63.
Dedicated Codex CLI acceptance covered normal and deliberately busy provider
turns; both returned exactly one result and received one acknowledgement. The
busy run observed waiting -> sending -> sent. Nonzero exit, directory binding,
deduplication, and retained results after ending tmux passed. Simulator layouts
were inspected at 320/402 points. Native Claude acceptance was blocked by login;
physical iPhone installation and Android/HarmonyOS device acceptance were not
claimed. See DESKTOP_UPDATES.md for hashes, run IDs, and evidence paths.
