# ADR 0024: Make Cloud external testing opt-in

- Date: 2026-10-22
- Status: Accepted; hosted configuration pending
- Related: [Xcode Cloud](../XCODE_CLOUD.md); [ADR 0016](0016-suspend-hosted-actions-budget.md); [ADR 0023](0023-bootstrap-sanitized-repository.md)

## Context

The owner requests that default Cloud builds do not enable external testing.
Several historical runs have successful archive actions but remain RUNNING in
TestFlight External Testing. A run-level concurrency guard treats those
post-actions as active compilation and prevents a requested new build. Neither
that status nor a successful archive proves external distribution or installation.

## Decision

The default main-branch workflow archives the MusteriOS Release scheme for
App Store eligible distribution without a TestFlight External Testing
post-action. External beta submission and distribution require an explicit
operator request for a selected build; a request to build does not authorize
external distribution. Do not add an automatic internal-testing replacement.
Keep the archive action, signing configuration and main trigger unchanged.

Apply the default in the hosted workflow's Post-Actions settings by removing
its external-testing distribution action. Repository documentation alone does
not change the Apple-hosted workflow. Read back the saved configuration and
verify a later explicitly requested run has no external-testing action.

Canceling historical runs is a separate, explicitly authorized operation.
Removing a post-action from future runs does not establish cancellation of old
runs. Keep GitHub Actions disabled under ADR 0016. Do not alter existing public
beta groups, invitations or installed builds as part of this default change.

## Alternatives

Leaving external distribution automatic contradicts the owner's request and
couples ordinary build completion to the external beta process. Disabling the
entire Cloud workflow would also prevent ordinary archives. Canceling old runs
alone would not change future defaults. Removing metadata files would not
remove a server-side post-action.

## Consequences

External beta releases become intentional and separate from source validation.
Cloud build/archive status, external distribution and physical-device acceptance
must be reported separately. Operators need authenticated access to the hosted
workflow settings; the public API helper currently provides no post-action edit
or build-cancel operation. This decision does not claim to fix that helper's
run-level concurrency guard.

## Verification

Check the current-tree public-content policy, Markdown diff and ADR gate before
landing this documentation change. Hosted application is pending: the browser
redirects to an authentication-failed login page, so removal of the post-action
and cancellation of the old runs have not been performed. The repository contains
no deployable Cloud post-action configuration to apply this setting locally.

After authentication, save the default without external testing, read it back,
and inspect the actions of the next explicitly authorized build. Do not treat
these source checks as proof of a hosted setting change, successful Cloud build,
signing, publication, installation or device acceptance.
