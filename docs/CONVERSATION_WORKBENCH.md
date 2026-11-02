# Mobile conversation workbench

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Mobile conversation pages use three primary destinations: Chat, Activity, and
Terminal. Activity contains execution traces, subagents, and related sessions.
The iOS shell retains native navigation bars and confirmation/result sheets.

A complete top-level `sh`, `bash`, `shell`, or `zsh` Markdown fence in a user or
assistant text message can be reviewed for execution. Rust projects the original
code, language and an opaque content handle. Console transcripts, unlabelled code,
reasoning and incomplete streaming fences are display-only. Read-only subagents
cannot execute commands. Opening a preview does not execute anything.

The user confirms the exact code, device and working directory. Rust revalidates
the displayed content and the server revalidates the current directory. The
command runs in a separate window of the conversation's tmux session, leaving
existing interactive programs alone. The terminal tab provides the live terminal
and recent execution records; Start, End and Restart retain their existing scope
of the whole conversation terminal, including command windows.

Each execution has a durable record and a unique operation ID. A repeated or
ambiguous request cannot execute the command a second time. Each record retains
up to 32 KiB / 2,000 terminal history lines of output, an explicit truncation flag,
and a known exit code only after tmux reports completion. The retained pane
does not add tmux exit notices to captured command output. Commands may be
interactive; their window remains available in the terminal tab. Lost panes have
an unavailable outcome and cannot automatically send a fabricated result.

After completion, the initiating client waits for fresh provider idle evidence
(the final text can precede turn completion), then returns code, directory, exit code and
output to the original conversation through the existing AgentSend/Locked Use
path. Navigation cannot change the destination. Native authorization remains in
force. The server records the authenticated initiating device for automatic
return. While the phone is backgrounded, execution continues; return resumes
when Muster is foregrounded. After restarting the app, reopening the original
conversation restores the return intent from the server. A different device can
inspect the result and explicitly request its return, but cannot inherit the
original device's automatic-return consent.

The return operation ID is stable across clients and restarts. Pending/unknown
receipts are not automatically replayed. A late confirmed receipt repairs the
status without clearing or changing the currently open conversation's draft.
Local authentication cancellation keeps the result and displays an unconfirmed
return. Users can copy the output and inspect the original conversation.

Up to 32 records are retained per conversation. Starting another command can
rotate the oldest completed record only when its provider receipt is confirmed.
Unreturned/unknown records are preserved; a full set must be resolved before
another command starts. tmux pane identity is checked before reading or closing
it, including after a tmux server restart.

## Validation

The isolated real-tmux test covers an occupied original shell, literal working
directory characters, nonzero exit status, recorded output after reconnect/end,
duplicate prevention, directory-change rejection, initiating-device persistence,
and bounded large output. Reducer tests cover original-conversation delivery and
unknown outcomes. Handler tests cover terminal control/read access and routing.
Device installation, provider receipt, physical mobile UI and OTA verification
are separate release gates; compilation alone does not establish them.

Mac-C 1.0.72 acceptance exercised the same Shared Rust intents against an
installed daemon and a dedicated independent Codex CLI conversation. Both normal
completion and a deliberately still-generating provider returned exactly one
result and received an acknowledgement; the latter visibly waited before sending.
See [release evidence](validation/DESKTOP-RELEASES-2026-09-29-TO-10-26.md#2026-10-20-mobile-conversation-workbench-and-mac-c-acceptance-1072--build-63)
for installation, publication, and untested native/physical-device boundaries.
