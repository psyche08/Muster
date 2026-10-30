# ADR 0067: Integrate runtime recovery and publish Apple 1.1.4

- Date: 2026-10-30
- Status: Accepted; publication and device acceptance pending
- Related: ADR 0059, ADR 0060, ADR 0061–0066; PR 60–63

## Context

The owner requests a new paired Apple release and installation on all three
Macs. One desktop already runs the unpublished 1.1.3 (93) runtime-recovery
candidate. Publishing main alone would remove those runtime repairs. On
2026-10-30 the owner explicitly chose to retain them and merge before release,
after being told Codex creation and repeated-directory receipts remain unverified.
This supersedes only ADR 0060's physical-acceptance-before-landing gate and its
targeted-installation-only scope. The unresolved acceptance is not a pass.

The candidate allocated request field 72 to native session creation. Unpublished
main allocated the same field to manual send confirmation. Reusing the installed
field for a different operation would be incompatible. Candidate ADR numbers
0061–0063 also collide with accepted mainline records.

## Decision

Integrate PR 60 with the receipt, composer and queued-message fixes in PR 61–63.
Keep deployed native creation at field 72; allocate field 73 to the unpublished
manual confirmation request. Regenerate all checked-in bindings and add a wire
compatibility regression. Renumber the candidate records to 0064–0066 without
changing their decisions, and update their links. Record the existing daemon-to-ACP
composition edge in the layer map: the daemon bridge uses the pure scheduler
protocol adapter; ACP still does not depend on the daemon or OS adapters.

Merge the integration and version changes before building 1.1.4 (94). Build both
Apple platforms from a clean stamped commit containing the candidate and main
histories. Publish signed/notarized macOS artifacts and the paired iOS OTA and
App Store Connect build through the existing private publisher and GitHub.
Explicitly select this build for public TestFlight review; default Cloud external
distribution remains off. State Apple's actual processing/review status.

Install the identical verified Mac package on all three Macs with the Rust
update lease and preserved device identity. Use management paths independent of
Muster's virtual network. A temporary loopback-only reverse SSH route may bridge
the desktop whose SSH server is loopback-only; remove that temporary route after
acceptance. No existing provider message is replayed or used as a release test.

## Alternatives

Publishing main alone loses the installed transport repair. Reusing 1.1.3 makes
an unpublished candidate and public release indistinguishable. Relabeling field
72 breaks installed native clients. Claiming Codex acceptance from compilation,
Claude success or protocol initialization is not justified.

## Consequences

The known native Codex creation limitation remains visible. Signing, notarization,
publication, device startup and provider execution are separate acceptance steps.
If no feed signing key is available, the notarized portable download is published
without claiming an in-app update feed. Existing access policy, credentials,
Keychain state, application sessions and Windows publication remain unchanged.

## Verification

Run protocol, ACP, agent, app-core, platform and daemon regressions; generated
binding checks, public-content and ADR gates; both Apple builds, strict signing,
release ancestry/fingerprints, notarization, public download hashes, and per-device
version/source/hash/identity/Accessibility/relay/Vnet checks. Record results below,
including failed or unavailable steps. Do not infer physical iOS installation or
successful native Codex tasks from upload or startup.

### Integration verification

The integrated library suites pass 520 distinct tests with seven pre-existing
ignored cases (excluding a nested subprocess test duplicate). Strict Clippy
passes for the six affected crates and all targets. The 18-crate layer map,
seven release-ancestry tests and two runtime transport cleanup tests pass.
Swift and HarmonyOS bindings and the Apple project were regenerated. Device
execution, builds and publication remain separate stages below.
