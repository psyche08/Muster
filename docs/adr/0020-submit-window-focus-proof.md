# ADR 0020: Verify Submit focus within the addressed application window

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

- Date: 2026-10-22
- Status: Accepted
- Related: Supersedes ADR 0017's mandatory application focus-element equality;
  retains its one-key activation policy and ADR 0019's ordering/diagnostics

## Context

The first, unpublished 1.0.82 candidate (source 9f484f2e) was notarized and
installed only on Mac-A. During the owner-confirmed quiet window, exact Submit
was enabled=true and focused=true, but the application's AXFocusedUIElement
read produced no element. No key was delivered. Moving focus acquisition after
traversal therefore did not resolve the observed failure.

Apple's installed AXAttributeConstants.h defines AXFocused=true as the element
having keyboard focus within its window. The older guard additionally assumes
that every application supplies AXFocusedUIElement. An optional read erases
whether this attribute is absent or an IPC/proxy read failed. This is a confirmed
observability/admission mismatch, not proof that every absent read is safe.

## Decision

Shared Rust owns a typed application focus observation: matching, different,
not supplied, or failed. Require Submit enabled=true and AXFocused=true in every
case. A matching application focus remains sufficient. Only an explicit native
NoValue or AttributeUnsupported result may instead use a matching window proof:
Submit's AXWindow must equal the application's AXFocusedWindow. Both must be
readable AX elements in the already verified application's session surface.
A contradictory application focus, malformed value, timeout, invalid proxy,
missing window or different window rejects before any key.

The macOS adapter preserves AX read errors, gathers these facts, applies the
Rust decision, rechecks authorization/input and sends one app-directed Space
pair. Never use global keys, click fallbacks, focus loops or post-dispatch retry.
Logs record native error, focus source and window equality without card text.

## Alternatives

Ignoring all absent application-focus reads would admit IPC failure and an
inactive window. Requiring the optional attribute unconditionally reproduces
the refusal despite explicit control focus. Global activation could interfere
with the owner's desktop or Locked Use cover. Both remain excluded.

## Consequences

This admits only an independently focused control in the application's actual
focused window when its application-level element attribute is not supplied.
It retains exact session/form/control checks and all existing local-input and
Locked Use checks. A provider with neither focus proof remains unavailable.
The unpublished candidate is not acceptance and is not uploaded as a release.

## Verification

Exercise every enabled/control-focus/application-focus/window-match combination,
including malformed/failed reads, conflicting application focus and another
window. Run platform regressions, strict Clippy, architecture and ADR gates.
Rebuild both Apple artifacts from a clean commit and notarize before replacing
the unpublished candidate. Require Mac-B-to-Mac-A provider answer and continuation
before public publication. Separate device results from source tests and signing.

### Local validation

221 agent tests (4 pre-existing ignored), 34 platform tests and 51 daemon tests
passed on Mac-A, including the expanded focus evidence matrix and native error
classification regression. Strict all-target Clippy and the 17-crate layer gate
passed. This candidate retains the unpublished 1.0.82 (73) version; the first
candidate was installed only on Mac-A and never published to OTA/downloads.

### Second candidate observation and bounded focus settling

The second unpublished candidate (4cc96b7a) returned native NoValue (-25212),
matched Submit's window to AXFocusedWindow, but the immediate Submit AXFocused
sample was false. It delivered no key. A subsequent read-only question snapshot
showed the same pending form and its Submit as the focused element, with no
answer receipt. Focus acquisition therefore cannot rely on one immediate read.

Refine readiness with a Shared Rust bounded policy: at most six samples within
250 ms after the one focus request, 25 ms between pending samples. Only an
enabled control reporting false focus with an otherwise valid application/window
proof may wait. Contradictory/missing focus proof, read failure or deadline expiry
rejects. Authorization and input generation are checked at every sample and
before dispatch. No second focus write or activation retry is introduced.

The bounded readiness update passed 223 agent tests (4 existing ignored),
34 platform tests and 51 daemon tests, strict all-target Clippy and the layer
gate. The first two candidates both stopped before any activation and were not
published. Rebuild and notarize a third candidate before continuing acceptance.


### Final installed acceptance (2026-10-22, 15:55 CST)

On final Mac-A 1.0.82 (73), source 366a199fa346e6288578023f79d96c0dc365730b,
Mac-B answered the original pending card C once with operation
MUSTER82-R4-QUESTION-C-answer-once. Native logs recorded NotSupplied,
same_window=true, samples=2, elapsed_ms=26 before one focused-Space dispatch.
The provider recorded question toolu_01JzdETQnshjx6CDHNwKUoGZ as answered with
"收到", then emitted the final reply "MUSTER81-QUESTION-1022-C 收到".
This is an actual answer plus continuation acceptance, not only API success.
The earlier failed candidates had dispatched no activation key.

All three Macs now run the same notarized final daemon; paired iOS OTA build
73 and public artifacts were downloaded and verified. Startup deployment
findings and exact acceptance limits are recorded in ADR 0021. No new chat
prompt, duplicate answer, or new permission grant was needed for this check.
