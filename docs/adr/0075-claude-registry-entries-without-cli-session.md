# ADR 0075: A Claude conversation without a CLI session hides no other

- Date: 2026-10-31
- Status: Accepted; fix not yet installed on the affected Mac
- Related: [ADR 0039](0039-exact-session-navigation.md),
  [ADR 0040](0040-desktop-navigation-identity.md), ARCHITECTURE §3.6.2

## Context

On the owner's Mac (Muster 1.1.5, daemon build `3ae46a2f`), every phone
send to one Claude Code session failed on 2026-10-31. There were nine
attempts between 13:20 and 14:37 UTC. Each was refused before the ledger
with `cannot uniquely select the session in the desktop sidebar (0
matches, 3 navigation areas, unique title=true, 0 expansions,
complete=true)`.

- The trace shows the cause. Muster searched Claude.app's 83 sidebar rows
  for `当前还不支持deepseek harness ， 帮我实现`, the session's first prompt
  as its transcript records it.
- The sidebar shows the title Claude.app generated, `DeepSeek harness 支持`.
  That title is in the app's registry
  (`~/Library/Application Support/Claude/claude-code-sessions`).
- ADR 0040's `desktop_navigation_title` exists to read that title, and
  `desktop_navigation_id` exists for the exact-session fallback. Both
  returned nothing.
- Of the registry's 35 conversation files, one active entry
  (created 2026-10-24) has a title but no `cliSessionId`: a desktop
  conversation with no CLI session behind it. Both functions read each
  entry's `cliSessionId` with `?`, so that one entry ended the whole scan
  as if the registry were unreadable.
- Since that entry appeared, every Claude session whose transcript title
  differs from its desktop title has been unselectable by title, and its
  exact-session fallback has been disabled too.

## Decision

An active registry entry without a valid `cliSessionId` is another sidebar
conversation, not an incomplete read:

- `desktop_navigation_id` skips it, because it cannot be the session
  sought.
- `desktop_navigation_title` keeps its title among the sidebar's titles.
  It still makes a same-titled target non-unique, but it is never a
  candidate. An entry without a title is skipped, since it collides with no
  title.

Everything ADR 0040 refuses still refuses: an unreadable or malformed file,
an oversized file, too many files, a target mapped by several entries or
accounts, and a `sessionId` that disagrees with its file name.

## Alternatives

- **Requiring every entry to name a CLI session** keeps one unrelated
  conversation able to disable navigation for all of them. That is the
  observed failure.
- **Ignoring such entries entirely** would let a same-titled conversation
  without a CLI session make an ambiguous title look unique.
- **Falling back to the first prompt** is what produced the wrong title.
  It stays the caller's last resort.

## Consequences

A send to a Claude session resolves its sidebar title and its desktop
identity again, whatever unrelated conversations the registry holds.
Uniqueness and every other check are unchanged, as is the final check that
the opened conversation is the exact CLI session. The affected Mac keeps
failing until a build with this fix is installed there.

## Verification

- `navigation_identity_tests::a_conversation_without_a_cli_session_hides_no_other`
  covers the following against an entry without `cliSessionId`: the
  identity and title resolve, a same title is not unique, an untitled entry
  collides with nothing, and a malformed entry still withholds both. The
  existing identity tests pass unchanged.
- Against the owner's real registry (read only), this build resolves the
  failing session to `("DeepSeek harness 支持", unique)` and to its desktop
  identity `local_9b086275…`. Both were `None` before the change.
- Not done: installing this build on the affected Mac and repeating a phone
  send.
