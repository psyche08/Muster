# ADR 0201: Locate a Claude approval card through its background sidebar

- Date: 2026-11-13
- Status: Accepted
- Related: [ADR 0099](0099-local-claude-identity-and-permission-cards.md), [ADR 0100](0100-locked-claude-permission-cards.md), [ADR 0190](0190-background-desktop-admission.md), [ADR 0196](0196-foreground-waits-and-provider-queue.md)

## Context

An uncovered AgentInspect on a Mac with another application in front was refused before session selection. The installed Claude sidebar exposed the requested conversation's title, but its AX row did not expose a session URL. Claude's live permission card is available through the session's accessibility tree.

## Decision

For a local Claude Code inspection, first read an existing exact session surface. With another application in front, navigate using the running Claude process's own AX sidebar. Require a unique active desktop UUID mapping before selection. Use the current registry's unique exact title only as a row locator when a row lacks an identity. Use bounded expansion and require a complete, unambiguous selection. Verify the destination web area's mapped CLI UUID before attaching or inspecting. This narrowly supersedes the sidebar restriction in ADR 0099 and the foreground requirement in ADR 0190/0196 for this read.

Do not invoke Launch Services or use global input in the background path. Preserve the existing console, authority, process, input-generation and deadline checks. Approval responses remain separate operations with fresh card validation.

## Alternatives

A background URL open permits a provider to activate itself. A transcript or a registry entry cannot describe the live permission choices. Neither provides the requested background card read.

## Consequences

Selecting the session can change the conversation displayed inside Claude. Missing mappings, ambiguous or unavailable rows, and a different destination fail the inspection without reading another conversation's card.

## Verification

The agent, platform and daemon library suites passed locally (620 passed, eight ignored). Navigation fixtures cover a background selection, missing identity, ambiguous and incomplete rows, a wrong destination and no URL dispatch. All three crates passed strict Clippy across all targets.

On a Mac with a different application in front, the opt-in device test selected two initially hidden local Claude sessions in succession using the final sidebar policy. The tests verified the destination UUID, completed an accessibility snapshot and checked that the same other application was frontmost at the end. They took 1.06 s and 0.92 s. Neither session had a live permission card during these reads. No signing, installation, paired-client display or distribution was tested.
