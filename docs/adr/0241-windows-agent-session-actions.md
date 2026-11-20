# ADR 0241. Expose session creation and catalog actions on Windows

- Date: 2026-11-20
- Status: In development
- Related: ADR 0150, ADR 0173, ADR 0212 and ADR 0240

## Context

The Windows Agents screen opens and searches sessions. The Apple client also
creates CLI and native desktop sessions, changes device filters, organizes a
row and opens its parent. The shared projection currently treats every
reachable desktop as a potential native Mac session host, including a Windows
machine whose platform is already known.

## Decision

Collect inputs in native Windows controls using the hosts, providers, pending
states and refused draft supplied by Shared Rust. Dispatch the same typed
intents as the Apple client. Show the first-message action explicitly. Resolve
context actions against the selected Rust-issued row and its current flags.
Use known platform information in the shared core to omit incompatible hosts.
Keep desktops with an unknown platform eligible for the existing live service
preflight. The target daemon retains control of permissions and execution.

## Alternatives

A Windows-only provider list or platform heuristic would diverge from the
shared model. Requiring cached device information for every target would hide
older peers and freshly connected Macs before the first detail poll.

## Consequences

A Windows user can request a Code session on a capable Mac and a CLI session
on a capable member. Creating a native Code session on Windows itself still
requires a Windows provider adapter. A refused request can be corrected from
the draft. Unknown delivery follows the shared recovery rules.

## Verification

Exercise projected choices, pending states, keyboard cancellation, draft
restoration, typed requests and row capability flags with synthetic records.
Run the shared target selection regression and build the x64 portable package.
Verify the actual client against the existing daemon. Keep provider sends and
changes to real catalog records distinct from synthetic UI tests.

The Windows control fixture exercised modal Escape, typed CLI and native
requests, refusal recovery, an unavailable draft target, row actions and
pending-state controls. The Rust regression exercised known local and remote
Windows hosts alongside an online desktop with unknown platform. The phone
fixture now reuses the synthetic signer from the send-recovery suite, allowing
these tests to run beside the live Windows daemon. All eleven recovery cases
and the target-selection regression passed. No provider interaction is needed
for these tests.

A capture of the shipped form at 192 DPI exposed a layout regression in the
initial implementation. The prompt was one pixel tall and the help label
overlapped the directory field. Use a 96-DPI design baseline, reserve a
readable multiline editor and scroll the form body when the viewport is small.
Validate non-overlapping visible controls as well as the resulting request.

The control fixture references the desktop assembly and accepts a screenshot
output directory. The Windows agent guide documents capture and byte matching
against the ZIP. A repeat capture at 192 DPI showed the prompt, directory,
explanation and submit controls separately.
