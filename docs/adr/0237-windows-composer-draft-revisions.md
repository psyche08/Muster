# ADR 0237. Bind Windows chat actions to the active editor revision

- Date: 2026-11-20
- Status: In development
- Related: ADR 0068, ADR 0235 and the shared ComposerAction ABI

## Context

The core initializes a new chat with an opaque draft key and an empty revision.
It rejects a send or navigation snapshot whose revision is empty. The Windows
shell copied that empty revision, and used text equality to decide whether a
receipt should clear the editor. Editing away and back to identical text can
therefore lose a later edit. Windows also omitted the core's unconfirmed-send
notice, retry and dismissal actions.

## Decision

Initialize a native editor revision when opening a draft, advance it on local
edits, and capture the buffer only when dispatching a user action. Scope it to
the key supplied by Rust. Apply a receipt to its submitted revision. Render
the core's unconfirmed-send message and offer explicit retry and dismissal
through the existing intents. Use the existing completion intent to let users
read and insert a suggestion. Rust retains ownership of persistence, delivery
evidence, retry admission and confirmation.

## Alternatives

Comparing text loses edit identity. Sending every keystroke changes the
cross-platform persistence contract. A Windows-specific retry would duplicate
the core's operation journal.

## Consequences

Expired receipts can arrive after a user has edited or switched conversations.
The native editor tracks the visible buffer and its outstanding submission.
Retry can send the current draft and must be an explicit user action.

## Verification

Exercise the Windows editor with synthetic drafts, identical retyped text,
out-of-order receipts, navigation and a reopened draft. Inspect the retry and
dismissal intents without sending to a provider. Run shared draft tests and
build the desktop client using the standard-user development toolchain.

On Windows the recovery fixture requested the platform identity; eight tests
could not start alongside the installed daemon. Give each test shell a fresh
in-memory signing key through its existing device callbacks. This exercises
the real embedded backend and leaves the production identity lock in place.

The Windows editor regression executable passed with a real WinForms text
control. The three Rust draft tests and all eleven send-recovery tests passed
with the installed daemon online. A standard-user release build attached to
that daemon and exposed its network home controls. No provider send, MSI
upgrade or hardware signing was exercised for this client-only change.
