# ADR 0270. Name Windows transcript and preview readers for accessibility

- Date: 2026-11-21
- Status: Accepted
- Related: [Windows conversation links](0269-windows-transcript-links.md), [session file previews](0243-windows-session-file-previews.md), architecture §3.7

## Context

With the installed 1.1.101 GUI on a synthetic session, an external UI Automation client found the linked transcript under the hidden welcome label, the file preview under the generic RichEdit name, and the plain transcript and Settings details under empty names. All four exposed their actual text. Looking at the WinForms AccessibleName property alone missed the native provider's behavior.

## Decision

Set the UI Automation name on each native RichEdit window as its handle is created using the [documented RichEdit message](https://github.com/MicrosoftDocs/win32/blob/docs/desktop-src/Controls/em-setuianame.md). Use the same conversation name on the plain and rich readers. Give the shared plain output a page-specific details name on other screens.

## Alternatives

Matching transcript text would require knowing a message in advance. Moving labels in the tab order would leave the control dependent on unrelated layout. Replacing the native accessibility provider would also require maintaining its text and selection support.

## Consequences

Assistive clients can identify a reader by its purpose while using the existing Edit or Document text patterns. The platform shell supplies these names from the projected screen.

## Verification

Check the actual external UI Automation names on an installed x64 GUI attached to a software node. Open linked and plain conversations, follow a file link, return and visit Settings. Independently check the native reader after its window handle is recreated, then verify portable update, GUI startup and node state.

On Windows x64 a separate UI Automation process queried the real transcript and file preview controls. Both returned their intended names, Document control type and TextPattern, before and after their native window handles were recreated.
