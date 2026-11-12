# ADR 0193: Verify the identity of one terminal host through Chrome

- Date: 2026-11-12
- Status: Proposed
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

Exercise the bridge, prompt boundaries, stale output, navigation, cleanup and ambiguous writes with synthetic fixtures. Separately check the local CLI against the operator's signed-in profile, including a fresh target prompt, delimited output and a return to the first hop. Device acceptance is pending.

The local Rust bridge suite exercises command order, an incomplete hostname, a missing output line, a mid-command Native Messaging disconnect, and authentication handoff. The JavaScript adapter fixture distinguishes a prior command transcript and the terminal input value from a fresh output row. The matching host and CLI built locally. Live terminal acceptance requires loading the unpacked extension in the signed-in profile.
