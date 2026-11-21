# ADR 0261. Copy the Chrome extension management address on Windows

- Date: 2026-11-21
- Status: Accepted
- Related: [ADR 0260](0260-windows-chrome-extension-setup.md), architecture §3.6.2

## Context

On Windows, starting Chrome 158 with the extensions URL opened a new tab.
Sending the same URL to the already running test profile also left it on a
new tab. The previous form test only checked the process arguments.

## Decision

Replace the Windows launch shortcut from ADR 0260 with a local-only action
that copies the fixed extensions address. Show where to paste it and how to
load the folder reported by the node. Chrome owns navigation and approval.

## Alternatives

Driving the personal browser's address bar would depend on focus and profile
selection. Users can also reach the page through Chrome's extensions menu.

## Consequences

The user chooses the Chrome profile and navigates explicitly. Copying replaces
the clipboard on demand. The displayed address also works when the clipboard
is busy. Shared browser status and requests keep their existing semantics.

## Verification

Check the copied value, clipboard failure and retry, and visibility on local
and remote targets in the Windows form test. Check actual navigation in a
temporary Chrome profile by reading the resulting page title and address.
Record the portable upgrade and the extension ping separately.

On 2026-11-21, Chrome 158 and Chrome for Testing each loaded the unpacked
extension from the retained 1.1.55 directory using a fresh Windows test
profile. Each profile connected to the registered 1.1.94 native host and
returned `agenthalo.browser` version `0.4.0` to a ping. After closing the
browser, the isolated node reported disconnection. Starting the same profile
again without extension-loading or debugging switches returned the same ping.
Chrome retained the original unpacked directory. Acceptance with changed
extension content requires checking the loaded files as well as the native host.
