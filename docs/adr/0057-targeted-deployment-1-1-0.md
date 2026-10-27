# ADR 0057: Targeted deployment of 1.1.0 (90) and device acceptance

- Date: 2026-10-27
- Status: Accepted
- Related: [0055](0055-clear-every-input-before-a-submission.md),
  [0056](0056-bounded-scans-skip-transcripts-and-retry-vanished-nodes.md),
  [0052](0052-targeted-multica-deployment.md) (the guarded targeted process),
  [deployment runbook](../DESKTOP_UPDATES.md)

## Context

ADRs 0055 and 0056 fixed the send failures found in the three Macs' daemon
logs: drafts merged instead of replaced, refusals over a draft in a question
field, `CoverLost` stops on merely-stale cover reports, a destroyed submit
control counted as ambiguous, a session scan that walked every transcript into
the node ceiling, and a control scan that gave up on one vanished node. Both
are source changes with unit coverage only. The owner asked for the version to
become 1.1.0 and for the change to be deployed and accepted on the devices.

The fleet is not uniform. The Mac Mini and M1Max run the published 1.0.94 (85).
M4Pro runs 1.0.98 (89), built from the unmerged branch
`codex/native-runtime-integration` (daemon stamp `92517da9`) — another line of
work under test there.

## Decision

Build 1.1.0 (90) from clean committed source containing ADRs 0055 and 0056,
with the guarded, Developer ID signed and notarized portable-bundle process of
ADR 0052, and install it on the owner's Macs. Build number 90 stays above every
build in the fleet, including M4Pro's 89, so no installation is a downgrade.

This is a targeted installation, not a publication: the public desktop feed and
the iOS OTA pair stay unchanged, and `released-commits.txt` records only
published versions. M4Pro is replaced only with the owner's explicit consent,
since that would remove the unmerged branch build under test there.

Device acceptance exercises what the fixes change, through real provider
conversations on an installed build: a send into a composer that already holds
a draft, a question answered while its field or the composer holds text, and
a send with several conversation views open.

## Alternatives

- **Reuse 1.0.97–1.0.99.** 1.0.97 and 1.0.98 are already taken by builds from
  another branch, and reusing a number for different source obscures
  provenance.
- **Publish the update feed.** Exceeds the request, and each macOS publication
  needs a paired iOS build.
- **Install over M4Pro unasked.** Would silently replace another branch's build.

## Consequences

The targeted Macs advance to 1.1.0 independently of the published 1.0.94 pair.
In-app updates on them keep offering nothing newer until a release is
published. No old application bundle is retained.

## Verification

The owner consented on 2026-10-27 to replacing M4Pro's branch build, to
acceptance sends only into dedicated conversations, and to landing after
acceptance.

### Build and installation on 2026-10-27

- **Source and gates:** clean source `7c02cea1ec1ac6fc3fe8c7f0db4e17ca166529f9`.
  `verify-release.sh` passed (stamp equals HEAD, clean tree, released ancestry,
  fingerprints) and Presence signing verified on the built and packed bundles.
- **Signing:** this Mac holds the team's older Developer ID certificate (valid
  to 2027-02) and its "Muster Developer ID Presence" profile; the "Presence G2"
  certificate used for 1.0.94 and 1.0.98 is held only on the other two Macs.
  Both profiles grant identical entitlements, and every designated requirement
  in the bundle is team-based (`certificate leaf[subject.OU] = "89LGY6BD53"`),
  so TCC grants, Locked Use trust and Secure Enclave keychain groups are
  unaffected. The gate checks that the embedded profile authorizes the signing
  certificate, which it does.
- **Package:** Velopack 1.2.158 with `package-macos.sh`'s gates and `vpk pack`
  arguments; the feed signature step was not run because nothing is published
  and the offline feed key is not on the build Mac. Apple notarization
  `49acb168-484b-4c79-a982-8df01bcd5935` Accepted for the Velopack-signed
  bundle; stapled; Gatekeeper "Notarized Developer ID". `--signDisableDeep`
  kept the daemon byte-identical to the build. Portable ZIP SHA-256
  `9616873a55857cce0a81e901c35b8657bf658e077060a8dffe9dc558c78d0bdc`, daemon
  SHA-256 `19321790a29e2521fc8af29a03efb59e1adfb1697bcaf3f153753bab70223260`.
- **Installation:** M4Pro (12:19 UTC, over its physical bridge address), M1Max
  (12:21, detached installer over its Vnet-carried tunnel, back after about a
  minute) and the Mac Mini (12:23, locally, last). Each run verified the package
  hash, signature, Gatekeeper, version and stamp, held the daemon's
  `DesktopUpdateLease` until `DesktopUpdateReady`, stopped only Muster's app and
  daemon, replaced only the bundle, waited ten seconds and launched once. No
  backup, no forced kill. On all three: unchanged device id and fingerprint,
  installed daemon equal to the package's, Locked Use plugin re-verified by the
  root helper, relay registered, Vnet up, every link `direct`, and all six
  virtual-IP ping directions 4/4.

### Device acceptance on 2026-10-27

- **Mac Mini, Codex, dedicated conversation:** a send's transcript user record
  is exactly the sent text and Codex replied as instructed; exact echo in 77 ms,
  provider receipt in 578 ms, ledger `applied`.
- **M4Pro, session scan (ADR 0056):** right after start its AX controller
  attached to five Codex conversation views at once — the condition that hit
  the node ceiling at 07:52 that morning. A later traced scan with nine views
  completed in 848 nodes, 43 ms.
- **Read-only surface diagnostic on M4Pro:** the Codex composer exposes
  `AXValue`, `AXSelectedText` and `AXSelectedTextRange` as settable, so both of
  ADR 0055's clearing mechanisms are available there.
- **M4Pro, Locked Use from the phone:** two sends at 13:07 and 13:08 unlocked
  under the cover and relocked correctly, but refused with "more than one view
  shows this session"; see ADR 0058, which 1.1.1 resolves.

Not accepted on 1.1.0: a send into a composer that already holds a draft and an
answer while the composer or the card's field holds text (they need the owner
to place text in an input box; computer-use was declined, and no other UI
automation was used instead); a `CoverLost`-prone cover reporting gap; any
check on M1Max after it left the network at 12:59 UTC (its physical interface
stopped answering; it had no conversation windows open since 08:20).
