# ADR 0246. Distinguish unobserved desktop chat history from an empty conversation

- Date: 2026-11-20
- Status: Accepted
- Related: [ADR 0142](0142-session-history-and-live-push-latency.md), architecture §3.6.2

## Context

Desktop chat transcripts come from an observed provider window. A restarted
daemon can receive an earlier receipt with a temporary title-based session ID
or a stable conversation ID before observing that conversation. Both history
APIs currently return successful empty results. During Windows acceptance an
original conversation was verified in the provider's history while its stable
ID was absent from the local typed catalog. Both receipts returned zero rows.

## Decision

Shared Rust reports unavailable history for an existing desktop chat ID until
the daemon has observed its page. Keep a new-chat placeholder empty and keep a
genuinely observed empty conversation empty. Paged RPC reads and the service
message API use the same observation lookup. An unavailable result tells the
user to reopen the conversation from the desktop Chats list. Opening and
observing a listed conversation restores reads for its current identity.

Accept the human-facing provider label in the chat CLI by resolving it through
the existing product app table before issuing the RPC. Preserve the existing
wire provider and identities. The CLI imports the shared app registry from
`muster-core` to avoid a second provider table. The RPC validation error lists the wire names.

## Alternatives

Persisting a title-to-conversation guess could read a different conversation.
Importing unrelated provider history would change the source of the desktop
capability. An empty history obscures the distinction between a fresh page and
a missing observation.

## Consequences

Clients can display the existing history error instead of presenting an
unobserved transcript as empty. This decision leaves the typed catalog and
the target's admission rules in place. A conversation outside the catalog
can remain accessible in the provider itself.

## Verification

Test both history readers on unobserved temporary and stable IDs, on a blank
new chat and after observing a conversation. Exercise ordinary, outline and
paged reads, provider argument parsing and the actual Windows named pipe.
Verify the live node and isolated daemon separately.

## Recovering an unavailable provider window

A complete typed directory can be available while the provider has no usable
desktop window. On Windows the signed app can remain in the tray with all
its windows hidden. The live catalog probe then reports the same availability
as a closed app. Opening the official app from Start made the same probe
succeed while leaving the directory count unchanged.

Use the shared provider label and ask the user to show a single main window
and retry. Use this guidance for host selection, CLI catalog output and native
process lookup. Keep a missing observation or detached controller scoped to
reopening the actual conversation. Inferring process exit from a window probe
would give the wrong recovery step. Listing saved conversations still provides
no authority to attach to a hidden or ambiguous window. Verify the offline
catalog with saved rows, alternative-host selection and the typed history
read after observation. Keep the device's before/after probe distinct from a
verified read of a particular conversation.
