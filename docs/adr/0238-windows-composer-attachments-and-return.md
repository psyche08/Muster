# ADR 0238. Adapt desktop chat attachments and Return to Windows

- Date: 2026-11-20
- Status: In development
- Related: ADRs 0069, 0237 and Architecture section 3.6

## Context

The Mac composer accepts files and clipboard images where the core advertises
attachment limits. Windows only submits text. Its multiline text box also
treats ordinary Return as a newline.

## Decision

Use a Windows file picker, file drop and clipboard image import. Keep selected
bytes in the visible editor, bound by the count and total byte limits in its
ChatView. Supply these bytes in the existing AgentSend intent. Scope picks to
the current conversation and advance the editor revision on attachment edits.
Let Shared Rust validate and deliver the request. Route plain Return in the
focused chat editor to its normal Send action, preserve native Shift+Return
editing and leave IME composition keys to the editor.

## Alternatives

Uploading files separately would introduce another delivery path. A global
Return shortcut could send from a different control or during composition.

## Consequences

Files are staged only for the active composer. The installed daemon and
provider do not need a protocol change. The core can disable attachments for
a particular chat. Decode previews with the Windows image decoder and scale
before allocating display pixels. OS interactions run with the signed-in user's privileges.

## Verification

Exercise the native editor with fixture files, an image, attachment-only
messages, changed/late receipts, advertised limits and switching chats.
Check ordinary Return, composition, modified Return and non-chat text input.
Build the desktop client and run shared payload validation. Report live IME
and provider acceptance independently from synthetic tests.

On Windows the native editor passed file-drop and bitmap data-object imports,
image-only submission, scaled preview, selection replacement, IME composition
messages, capability changes and late receipts. GDI+ can report success after
a failed stream write. The bounded PNG buffer records overflow; tests rejected
both a two-byte budget and a budget one byte below the encoded size with no
partial attachment staged. The shared attachment validation test passed.
The development launcher ran with a standard token alongside the installed
daemon. A physical IME candidate window and delivery to a provider remain
outside this synthetic acceptance run.
