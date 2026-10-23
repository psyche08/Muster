# ADR 0043: Preserve terminal input order across RPCs

- Date: 2026-10-23
- Status: Accepted
- Related: [ADR 0035](0035-native-terminal-dev.md), [ADR 0036](0036-terminal-simulation-coverage.md), ARCHITECTURE §3.6.3

## Context

A physical phone's rapid keyboard input reached a native terminal out of order.
The shared client launched an independent asynchronous RPC for each input intent,
so transport and adapter scheduling could reorder characters, including Return.

## Decision

Shared Rust owns one ordered input queue for each attached remote terminal view.
It starts one input RPC at a time and batches bytes accumulated while that RPC
is awaiting acknowledgement. Keyboard, text and paste use the same queue.
Only Applied or Duplicate acknowledgements release the next batch. Failed,
unknown or rejected input stops the queue and discards its remainder, requiring
the user to reopen the view before continuing. There is no automatic replay.

Bound pending input to 64 KiB. Overflow also stops input rather than allowing a
later Return to execute a partial command. Exact device, session and operation
identity fence late acknowledgements from replacement views. Leaving the view
or entering the background discards queued input; foreground reattaches the
remote terminal. An already dispatched input may have reached the host and
cannot be undone. Existing server authorization and Ghostty lease rules remain
authoritative, including on reattach.

## Alternatives

A mutex inside independent tasks cannot guarantee acquisition follows intent
order. A platform-specific queue would duplicate cross-platform semantics.
Sending every key after an extra timer would add avoidable first-key latency.
A new streaming protocol may improve sustained throughput later but is not
necessary to establish ordering with the current acknowledged RPC protocol.

## Consequences

The first key dispatches immediately. Burst input uses fewer RPCs and preserves
byte order, at the cost of waiting for one acknowledgement before its next
batch. Network failures visibly stop input; users must inspect the terminal
before reopening. Multiple independent clients remain independent writers.

## Verification

Planned: a controlled RPC fixture holds the first acknowledgement while mixed
text, Unicode, paste and Return arrive; verify ordered batches, bounded input,
failure/unknown handling, navigation/background discard and stale replies.
Build and sign the physical-phone client, then test both native hosts through
Device Hub, including locked target sessions. Record source/test, signing,
installation and physical-device evidence separately; none is implied by this
decision. No update-feed publication is part of this change.
