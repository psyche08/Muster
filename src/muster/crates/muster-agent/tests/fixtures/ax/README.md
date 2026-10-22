# Recorded Accessibility trees

Element trees of the real desktop apps, used by `muster_agent::profile` tests
to prove that each app's profile finds its controls (ARCHITECTURE §3.6.4).

| File | App and state | Recorded |
|---|---|---|
| `codex-desktop.dump` | ChatGPT app in Codex mode, one Codex session, idle, empty composer | 2026-10-03, Mac-B, Muster 1.0.32 |
| `claude-code.dump` | Claude app in Code mode, one Claude Code session, idle | 2026-10-03, Mac-B, Muster 1.0.32 |
| `catdesk.dump` | CatDesk main window, one session, idle | 2026-10-03, Mac-B, Muster 1.0.32 |
| `claude-code-running.dump` | Claude app in Code mode, a turn running (Stop, no Send) | 2026-10-03, Mac-C, Muster 1.0.31 |
| `claude-code-running-draft.dump` | Claude app in Code mode, a turn running, a draft in the composer (Send, no Stop) | 2026-10-03, Mac-C, dev build of the dumper |
| `codex-desktop-running-draft.dump` | ChatGPT app in Codex mode, a turn running, a draft in the composer (Queue, no Stop) | 2026-10-03, Mac-C, dev build of the dumper |
| `claude-code-question.dump` | Claude app in Code mode, a two-question card on its first page | 2026-10-03, Mac-C, dev build of the dumper |
| `codex-desktop-running.dump` | ChatGPT app in Codex mode, a turn running, empty composer | 2026-10-03, Mac-C, dev build of the dumper |
| `chatgpt-chat-new.dump` | ChatGPT app in Chat mode, new-chat page | 2026-10-03, Mac-C, dev build of the dumper |
| `chatgpt-chat.dump` | ChatGPT app in Chat mode, an existing conversation | 2026-10-03, Mac-C, dev build of the dumper |
| `claude-chat-new.dump` | Claude app in Chat mode, new-chat page with its recents | 2026-10-03, Mac-C, dev build of the dumper |

Muster 1.0.31's `ax-dump` stopped at 1,500 nodes, before the ChatGPT app's
composer in a long conversation. The rows marked "dev build of the dumper"
ran the same listing code without that limit from a process Accessibility
already trusted; `ax-dump` now reads up to 20,000 nodes. No message was sent
for these recordings: the running turns were already running, the question
card was this repository's own agent asking, and Chat mode was entered and
left again through Muster's mode switch. The drafts were typed by the user
(Codex) or written through AX into this repository's own, empty Claude
composer and cleared again; nothing was pressed.

## Recording and redaction

On the Mac, with its console unlocked (a locked console exposes only the menu
bar) and the app showing the state to record:

```sh
/Applications/Muster.app/Contents/Helpers/muster debug ax-dump ChatGPT > raw.dump
```

The command only reads the tree. A raw dump holds conversation text, so it
never enters the repository. Redact it first, from `src/muster`:

```sh
scripts/redact-ax-dump.py crates raw.dump crates/muster-agent/tests/fixtures/ax/<name>.dump
```

The redactor keeps the tree, roles, subroles, DOM identifiers and every label
string that Muster's product code names; it replaces all other text with
stable placeholders (`text-N`, `value-N`). A label starting with vocabulary
keeps it and loses only the rest, through the same table ("More options for
text-17" beside the row titled "text-17", "Code, text-5" for a radio with a
status). The apps' own page URLs keep their paths with fake ids of the same
form; any other link becomes `https://link-N.invalid/`. Review the kept
strings and URLs before committing:

```sh
grep -oE '(title|desc|label|value)="[^"]*"' <name>.dump | grep -vE '="(text|value)-[0-9]+"' | sort -u
grep -oE 'url="[^"]*"' <name>.dump | sort -u
```
