# ADR 0119: Ignore the credential file types the fleet actually keeps loose

- Date: 2026-11-04
- Status: Accepted; source verified
- Related: [ADR 0009](0009-task-contract-and-runtime-manifest.md) (the
  `/.multica` token rule this follows), [PUBLIC_CONTENT.md](../PUBLIC_CONTENT.md),
  [DESKTOP_UPDATES.md](../DESKTOP_UPDATES.md#app-store-connect-credentials-and-testflight)

## Context

`psyche08/Muster` is now the only repository this work is pushed to, and it is
public. Everything that protects it has to assume a credential may be sitting
in a working tree: `.gitignore` already carries a "Local signing, identity, and
enrollment material" block, and `/.multica` is there precisely because "a token
in the working tree is one `git add -A` from being published".

That block covers `*.key`, `*.crt`, `*.token`, `*.blob`, `*.sepblob`, `*.p12`,
`*.pfx`, `*.mobileprovision`, `*.xcent` and `*.xcent.der`. The credential files
the authorized Macs actually keep beside a checkout include types it misses:

- An APNs signing identity is a `.p12` with its password beside it in a
  `.password` file. The archive checkout on one Mac held
  `apns_20260929.p12` — ignored — next to `apns_20260929.password`, which
  `git status` listed as untracked.
- The same directory held `aps.cer` and a `.certSigningRequest`.
- HarmonyOS signing material is a `.p12` plus a `.p7b` profile; a DevEco
  `build-profile.json5` names both by absolute path.

The App Store Connect `.p8` key that
[DESKTOP_UPDATES.md](../DESKTOP_UPDATES.md#app-store-connect-credentials-and-testflight)
warns never to commit is already covered, by `*.p8` in the environment-secrets
block below this one.

A `/keys` rule exists, but it only helps while the files stay in that one
directory. Moving them one level up removes the protection, which is what
happened.

## Decision

Extend the existing credential block with the types above — `*.p7b`, `*.cer`,
`*.certSigningRequest`, `*.password` — so the material is ignored wherever in
the tree it sits, not only under `/keys`.

The patterns are extension-wide on purpose, as the rest of the block is: a
credential's safety should not depend on which directory someone dropped it in.
No tracked file matches any of them, so nothing already in the repository
changes state, and `.gitignore` never affects a tracked file anyway.

This is a guard, not a licence. Credentials still do not belong in a working
tree, `scripts/check-public-content.py` still runs before pushing, and the
rules do nothing about a file that is force-added or already tracked.

## Alternatives

- **Leave it to `/keys`.** It protects one directory. The incident that
  prompted this was a file moved out of that directory.
- **Name the exact files.** `apns_20260929.password` would be ignored and the
  next one, dated differently, would not.
- **Add a pre-commit hook that scans for credential shapes.** Hooks are not in
  the repository, are bypassed by `--no-verify`, and have to be reinstalled
  after every clone. A `.gitignore` rule travels with the repository.
- **`*.pem` and `*.der` as well.** Neither is in use on the fleet, and `.der`
  is already covered where it occurs (`*.xcent.der`). Adding patterns for
  material nobody keeps would make the block harder to reason about.

## Consequences

- A loose APNs password, certificate, signing request, App Store Connect key
  or HarmonyOS profile is ignored wherever it lands in a checkout.
- A legitimate `.cer` fixture would now need `git add -f` and a reviewer's
  attention, which is the right default for a file of that type.
- Nothing about the daemon, the apps or the protocol changes.

## Verification

Confirm no tracked file matches the new patterns, that a file of each new type
at the repository root is reported as ignored by the new rule, and that the
public-content and ADR checks pass.

## Implementation verification

- `git ls-files | grep -iE '\.(cer|certSigningRequest|p7b|password)$'`
  returns nothing, so no tracked file changes state.
- `git check-ignore -v` attributes a root-level file of each new type to its
  new line in `.gitignore`.
- `python3 scripts/check-public-content.py` reports 0 findings, and
  `scripts/check-adr.sh origin/main` passes.
- No build, installation or device acceptance is involved in this change.
