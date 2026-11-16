# ADR 0215: Side chats are independently addressable sub-sessions

- Date: 2026-11-16
- Status: Accepted; device acceptance pending
- Related: [ADR 0102](0102-session-sections-hierarchy-search.md), [ADR 0173](0173-direct-native-cli-protocols.md), architecture §3.6.2 and §3.7

## Context

Users want to ask about a local Codex or Claude Code conversation without
steering its main turn. They need to find the conversation under its parent
like other sub-sessions and read it after it ends.

## Decision

A dedicated host operation creates a child of the verified source session.
Muster owns the child ID, timeline, admission and ended state in a host-local
SQLite journal. The existing catalog, sub-session list, history, and watches
project the child. Leaving the view does not end it. End is explicit and
irreversible; confirmed ended sessions cannot receive new messages. Completion
of a single turn is distinct from the end of its session.

Each side turn uses a temporary provider fork plus the last eight successful
side exchanges from that child's journal. Codex app-server uses a distinct
ephemeral read-only fork; Claude print mode uses fork-session without session
persistence or tools. The client supplies no executable, directory, or history.
The main session is never resumed for writing. Remote mutations need both
source provider and CLI control; task ownership and host policy still apply.
An uncertain mutation is not automatically replayed.

## Alternatives

A transient panel loses the child on navigation. A literal slash command can
land in the main composer. Provider side panes do not expose a uniform
identity, history or delivery contract across both desktop applications.

## Consequences

The host needs a recent signed-in native CLI matching the source provider.
Answers use that provider account. Child history lives in Muster; it does
not create a sidebar item in the provider's own application. Ending the child
does not interrupt the parent's task. Interrupted workers are not replayed.

## Verification

Check both fork protocols, family and timeline projections, restart, concurrent
and late actions, ended-session refusal and provider grants. Regenerate Swift
bindings and build the Apple UI. Separate source, installation and paired-device
acceptance.

Source tests cover both providers' catalog families, transcript paging, distinct
fork IDs, busy and ended child admission, late completion and persisted End
after reopening the database. A daemon RPC test creates and replays a child,
checks its parent and catalog, ends it, reads its history, and checks that both
the side operation and ordinary AgentSend refuse a later message.

Both installed native CLIs also completed a forked turn against an isolated
loopback model fixture using synthetic parent context. Claude left the source
transcript byte-for-byte unchanged. These were real CLI protocol checks, not
live account inference. MusterUI built for macOS and the iOS simulator;
signed installation and phone-to-host UI acceptance remain unverified.
