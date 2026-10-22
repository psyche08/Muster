# Claude Desktop cards

The Shared Rust adapter handles the card formats observed in Claude Desktop
2.42304.0 on 2026-10-16. Its input is the main session's complete JSONL history
and the desktop session registry. Platform UI renders Rust projections; actions
use the existing guarded native Accessibility path.

| Card | Compatibility |
| --- | --- |
| `wrap_up` | `recap`/`outcome`, `ask`/`next_ask`/`next_brief`, `actions`/`next_actions`, `blocked` boolean or string, and legacy `needs_user` |
| Replies | Label-only replies, explicit messages, two-button limit, UTF-16 truncation, NFC normalization, command/mention/invisible-character rejection |
| Native actions | Commit, create PR, merge, fix CI, address comments, resolve conflicts, verify, archive; bound to visible controls inside the matching card |
| Blocked without buttons | Required interaction and background notification; free text uses the guarded composer and provider send receipt |
| Recap only | Visible status card without a fabricated pending question |
| Task proposal | `start_session` own-initiative and legacy `spawn_task`; independent proposals survive unrelated turns |
| Session offer | Required new-session/do-it-here choice or ordinary composer reply; direct user-requested starts are displayed as results |
| Task lifetime | Successful start/dismiss records, desktop registry started/dismissed/expired states, and seven-day expiry retire proposals |
| Plan | Full item lists, done/start updates, boolean/string advance, and successful wrap-up completion |
| Chapter | Title and summary remain in the transcript reading flow |
| File delivery | Caption and confirmed attachment links; only a successful matching `SendUserFile` result with a validated path enables preview |
| Artifact | Successful `https://claude.ai/code/artifact/…` delivery links |

A successful wrap-up result means the card was shown, not that its question was
answered. Later human input, a replacement wrap-up, a display failure, or an
explicit no-box result retires it without manufacturing an answer receipt.
Internal notifications and sidechain records do not retire human questions.
A complete transcript rewrite retires stale UI, while a temporarily unreadable
source preserves it and prevents writes that cannot be revalidated.

Native option submission requires an unchanged source interaction, verified
session and enabled, unique controls. The adapter supports separate question text
and the current combined recap-plus-question Accessibility node. It refuses
ambiguous cards or controls outside that card. An action name never invokes a
shell command. Composer answers retain input-generation guards, draft echo,
one final press, and provider receipt reconciliation.

Plan updates replace previous plan snapshots within a turn. Successful cards
are outside collapsed tool activity; failures remain ordinary tool errors.
File links carry an opaque call/index key. The service rechecks that key against
the same session's successful provider result before reading a regular file;
arbitrary absolute paths and symlink attachments are not accepted through this
route. Existing preview byte limits still apply.

## Verification

- Regression fixtures cover wrap-up variants, blocking notifications, native
  form scoping, concurrent proposals, registry-only changes, expiry, partial
  JSONL writes, source truncation, plan replay, and attachment provenance.
- Read-only replay of the 100 most recently modified local transcripts covered
  44 wrap-ups, 99 plan calls, 7 starts, 10 legacy proposals, 5 dismissals,
  22 chapters, 84 file deliveries and 2 Artifacts. Successful results became
  cards; failed deliveries/invalid plan calls remained tool errors. Completion
  snapshots account for plan-card counts exceeding plan-call counts.
- The installed UI supplied the combined recap/question Accessibility layout
  and delivered-file structure used by the tests. No action was submitted to
  a real work conversation during this verification.
- Rust tests, Clippy, layer checks and Apple shared-UI compilation are separate
  from signing, installation and device acceptance. This source change does not
  publish or install a release; real device button submission remains an E2E gate.
