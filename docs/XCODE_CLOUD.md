# Muster Xcode Cloud and opt-in public TestFlight

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

App Store Connect app: Muster Agents (`6817180898`), bundle `dev.linsheng.muster`,
team `89LGY6BD53`.

## Source and workflow

- Project: `src/native/apple/MusterApp/Muster.xcodeproj`.
- Shared scheme: `MusteriOS`; archive Release for iOS, App Store distribution.
- Post-clone: `src/native/apple/MusterApp/ci_scripts/ci_post_clone.sh` builds
  `muster-ffi` with pinned Rust 1.98.0 and Cargo.lock. Swift dependency versions
  are also committed in the project workspace.
- Generate project configuration with `MusterApp/scripts/generate-project.sh`
  using XcodeGen 2.46.0. Review and commit the generated project and plists.
- Intended trigger: changes on `main`; archive `MusteriOS` without automatic
  TestFlight external testing or distribution. External beta distribution is
  opt-in for an explicitly selected build; see [ADR 0024](adr/0024-cloud-external-testing-opt-in.md).
  Do not run automatic distribution for pull requests or archive `MusterMac`.
- Initial Cloud version: 1.0.44, starting at Cloud build 1. Previous manual
  uploads are in the 1.0.43 train. Always inspect existing builds before upload.

Apple requires first-time product onboarding in Xcode. Select Muster iOS and
the project publisher, configure the archive and trigger, and connect only psyche08/Muster.
After onboarding, workflows and build runs can be read and managed through the
App Store Connect API. No local signing credentials belong in Cloud variables.

## Default distribution policy

In the hosted workflow's Post-Actions settings, remove `TestFlight External
Testing - iOS` from the default workflow. Keep the archive action, signing and
main trigger unchanged. Do not automatically substitute an internal-testing
post-action. A request to build authorizes an archive, not external beta
submission or distribution.

After saving, read back the workflow and inspect the next explicitly requested
build's actions to confirm external testing is absent. Changing this document
or removing TestFlight metadata files does not apply the hosted setting. Old
RUNNING records require separate cancellation; changing future post-actions does
not cancel them. Keep GitHub Actions disabled independently.

Application status (2026-10-22): this policy is accepted, but the hosted change
and old-run cancellations remain pending App Store Connect browser login.
No new Cloud build has verified this default yet.

## Public beta metadata (explicit opt-in)

The public privacy and support pages are maintained in `deploy/public/` and
published as `${MUSTER_RELEASE_ROOT}/{privacy,support}.html` through
`${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}`. Apple metadata uses `https://www.linsheng.dev/muster/` (standard HTTPS).
The existing download origin at port 8443 remains available.
The privacy text distinguishes encrypted service traffic from signed membership
records stored in the relay mailbox; do not describe all relay data as ciphertext.

External testers need a separate external group, a valid build, test information
and Beta App Review. Submission is not approval. A public invitation link is
usable only when the group has an approved build; verify the build's external
state before advertising availability. Physical iPhone installation remains a
separate acceptance step.

## Initial setup evidence (2026-10-08)

- Product: `c1c1e8da-3592-5aac-bb1a-c0fee8ffc5a9` (MusteriOS).
- Workflow: `a5255dea-b8b2-5755-a21c-c874aa7c50fc` (Muster iOS TestFlight).
- Source repository: `psyche08/Muster`; container path is the committed project
  listed above. The initial Xcode 27 onboarding incorrectly asked for access to
  public Swift packages. A temporary local project registered the same app;
  it was never built or pushed. The workflow was then updated to the full project
  before its first run. No third-party repository permission was granted.
- `ci_post_clone.sh` successfully built the iOS Rust archive locally. The full
  `MusteriOS` Release archive passed with `CODE_SIGNING_ALLOWED=NO` on Xcode 27.
  This is compilation evidence only, not Cloud signing or device acceptance.
- Manual build 1.0.43 (33), build ID
  `a8592d5b-b1ec-553e-8321-55bff7bbdd2f`, was submitted to Beta App Review and
  read back as `WAITING_FOR_REVIEW` at 2026-10-08 11:29 UTC.
- External group Public Beta: `c84b9aab-0988-50ea-989e-aa0bc22e4b8e`.
  Public invitation: <https://testflight.apple.com/join/Fh6ZkP8a>.
  It cannot admit testers until Apple approves a build.
- `TestFlight/WhatToTest.en-US.txt` and `WhatToTest.zh-Hans.txt` provide Cloud
  uploads with version-independent test instructions. Keep them current.

The PR #90 landing script merged successfully (`6711f018`), with no GitHub checks
reported, then its worktree-GC step failed with "not a git repository". No worktree
or branch was removed manually. This does not invalidate the merged commit.
