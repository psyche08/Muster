# ADR 0048: Acknowledge a send against the conversation, not only its outcome

- Date: 2026-10-25
- Status: Accepted
- Related: complements ADR 0046; ADR 0045; ADR 0038; Architecture §3.6 and §3.7

## Context

The owner reports conversation content going missing, above all messages
inserted while the agent is working, and asks for a confirmation mechanism.

ADR 0046 settles a send's *outcome*: the host's receipt ledger says whether the
request was applied, rejected or unknown, and the durable client record is
cleared as soon as an authenticated terminal result arrives. That answers
"was it delivered". It does not answer the question the reader actually asks:
"is the text I sent in this conversation". Between those two questions the text
exists nowhere in the client once the composer is cleared:

- A message inserted during a running turn appears only as a row of the
  provider's queue view. That view is read from the provider's own state, so a
  catalog gap, a hidden session, a rewritten transcript tail or a transcript
  rotation empties it, and the queue reader then reports the row as resolved.
  Nothing distinguishes that from the provider having taken the message.
- A page merge with no overlapping identity replaces the local tail with the
  authoritative recent one. That is correct for history, but a just-sent
  message that the host has not written yet disappears with it.
- The reader is not following the latest row while the agent works, so even a
  message that does arrive lands below the fold.

## Decision

Shared Rust records every send this client dispatches into the open
conversation and keeps that record until the conversation's own records account
for the text. The record lives in `State.admitted`, keyed by device and
session, so it survives navigation between conversations and tabs within a run.

A record is accounted for only by a new occurrence of its exact text as a user
message of that conversation. The number of occurrences already present when
the send was admitted is stored with it, so a repeated prompt is retired by its
own later occurrence and never by an identical older message. A matching row of
the provider's queue marks the record as queued; it does not retire it, and the
row disappearing again does not either. An applied, rejected or unknown RPC
outcome only annotates the record. Nothing removes a record automatically
except the conversation itself; an explicit user release, the one ADR 0046
already offers after the user has checked the Mac, drops it.

Rust projects each unaccounted record as a tail block of the timeline
(`kind = "admitted_send"`), carrying the text and a title that states exactly
what is known: being sent, submitted but not yet in the conversation, queued
for the end of the running turn, delivery unconfirmed, or refused with its
reason. The shells render that block and its state; they decide nothing. The
Apple shell additionally follows the latest row when a record first appears, so
the reader's own message is on screen even if they had scrolled up.

A record is a reason to keep showing text and to say what is known about it. It
is never delivery evidence, and never permission to send anything again.

## Alternatives

Trusting the applied outcome and clearing the text is what loses it. Clearing a
record when the provider's queue stops reporting the row treats an unreadable
queue as acceptance. Extending ADR 0046's durable record to live until the
transcript accounts for the text would keep its read-only recovery worker and
its unconfirmed refusal running long after the outcome is known, changing that
decision rather than complementing it. Matching by message identity is not
possible: the provider assigns the identity, and the client never learns which
identity its text became. A new proto message for a durable list was not added:
`protoc` and `protoc-gen-swift` are not available in this environment, the
generated Swift is checked in and CI verifies it, so the projection reuses
existing `ChatBlock` fields and the records stay in memory.

## Consequences

Matching by text means a message the provider rewrites before recording it is
not recognized, and the record then remains visible as "submitted but not yet
in the conversation" until the user releases it. That is the honest failure
direction: it over-reports rather than losing the text. Records do not survive
an app restart; ADR 0046's durable record and its read-only recovery still
cover that case. The scan runs on every streamed event while a send is
unaccounted for, so it is one pass over the transcript and costs nothing once
every record is retired. At most eight unaccounted records are kept per
conversation; reaching that bound means the conversation has stopped accounting
for sends at all, which the oldest record's state already says.

## Verification

Cover admission, an applied outcome that does not retire a record, a queued row
including a clipped prompt, the queue row disappearing, retirement by the
transcript, a repeated prompt, an unknown outcome, a refusal, isolation between
conversations, and release. Record source tests separately from builds,
signing, installation and physical acceptance.

### Source evidence

The complete `muster-app-core` library suite passed 106 tests with one existing
ignored test, including the two new reducer regressions above. The complete
workspace suite passed 783 tests across 78 binaries, with 12 existing ignored
tests. Formatting, strict Clippy for the crate and all its targets, the
17-crate layer check and the public-content check passed.

Builds, signing, installation and physical acceptance of an inserted message on
a Mac and on an iPhone are not claimed by this source checkpoint. In
particular, no physical case has yet been run in which the provider holds a
queued message and the record is observed through to the transcript.

### Release 1.0.93 (84)

Released together with ADR 0047 as paired macOS and iOS build 1.0.93 (84). The
acceptance case this needs is a message inserted while the agent is working: it
must appear in the conversation at once with its state, survive leaving and
reopening the conversation, and be retired only when the provider's own
transcript carries it. ADR 0047 records the release evidence for both.

### Subsequent decision

[ADR 0061](0061-send-receipt-format-and-manual-resolution.md) supersedes exact-text-only matching for Codex URL formatting and adds separately recorded user confirmation without replay.
