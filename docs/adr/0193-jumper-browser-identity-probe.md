# ADR 0193: Verify the identity of one terminal host through Chrome

- Date: 2026-11-12
- Status: Accepted; local Chrome terminal verified
- Related: architecture §3.6, ADR 0049, Agent Halo Browser

## Context

A signed-in operator needs an end-to-end identity check on one locally designated bastion target. Login can require an interactive challenge. Prior manual observations cannot establish the result of a new operation.

## Decision

Expose one argument-free local browser operation. The Rust native host reads the site and target from local configuration, opens an independent terminal, sequences the fixed connect, whoami and exit actions, and checks receipts in order. Chrome owns all site requests and terminal transport. The adapter confines input and output to the bound document and terminal. Only bounded evidence crosses Native Messaging. Use fresh rendered terminal output following the submitted command and require the first-hop prompt to return after exit. Release the adapter on every outcome and retain the page for authentication. Report the stage on failure and never replay input.

## Alternatives

Forwarding arbitrary fetches cannot drive a page-owned terminal, and forwarding a socket or shell would expand the operation beyond the identity check. A backend HTTP or SSH client would require a separate authentication and transport path.

## Consequences

The user must load the extension in the signed-in Chrome profile and register its matching native host. The page must expose a supported terminal and recognizable prompts. Unsupported rendering or ambiguous results stop the operation. Local configuration and live identities stay outside the repository.

## Verification

The local Rust bridge suite exercises command order, an incomplete hostname, a missing output line, a mid-command Native Messaging disconnect, and authentication handoff. The JavaScript adapter fixture distinguishes a prior command transcript and the terminal input value from a fresh output row. These failure paths were exercised with fixtures.

On 2026-11-12, Chrome loaded the unpacked 0.4.0 extension in the signed-in profile and the matching locally built CLI received its native bridge ping. A fresh local CLI probe observed the configured target prompt, obtained the account from the rendered output after the command echo, then observed the exit echo and first-hop prompt. The operation returned `status: closed` and `pageRetained: false`. Operator-specific identities are omitted here. This run used the development host and CLI; it did not publish a signed application update.
