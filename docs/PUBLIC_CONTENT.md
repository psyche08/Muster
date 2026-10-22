# Public source content policy

The current tree uses anonymized examples for personal operational evidence.
Historical device aliases, home paths, LAN addresses, session/operation IDs and
internal task references were replaced under [ADR 0022](adr/0022-sanitize-current-public-tree.md).
Mac-A, Mac-B and Mac-C denote distinct test hosts, not a public inventory. Identifiers
in these records are illustrative and must not be used to contact real devices
or retrieve private tasks. Dates, software versions, technical observations and
acceptance limits remain historical. This cleanup is not a new acceptance run.

## Intentionally public compatibility inputs

The existing `dev.linsheng.*` identifiers, Apple signing team `89LGY6BD53`,
GitHub project `psyche08/Muster`, and public `linsheng.dev` download/relay URLs
remain. They are visible in shipped products and required by existing identity,
update, entitlement or trust contracts. Public verification keys and source/
artifact hashes and public notarization request IDs also remain. Product Vnet
subnets and generic container bridge defaults are retained as protocol examples. This is an explicit boundary of this cleanup, not
an assertion that these identifiers cannot be linked to the publisher.
Third-party application bundle/team identifiers remain where needed to identify
supported providers. No private keys or access tokens belong in source control.

## Local deployment configuration

Release operators obtain SSH destinations, ports, credentials and remote paths
from their private deployment configuration. In documentation, `MUSTER_RELEASE_HOST`
is the SSH destination (including account or an SSH config alias),
`MUSTER_RELEASE_SSH_PORT` its port, and `MUSTER_RELEASE_ROOT` the existing web root.
`MUSTER_APPLE_CREDENTIALS_FILE` names a local environment file containing signing
API configuration. These names are documentation inputs, not an automatic loader.
Existing public download URLs and desktop/iOS directory separation stay unchanged.
Do not invent a new production destination from an example.

Website source templates omit personal contact information. Before deploying
`deploy/public/privacy.html`, replace its contact placeholder with the operator's
private support channel and check the policy's publisher details. Public issue
trackers are for non-sensitive bug reports only, not personal-data requests.
No website was deployed by this cleanup.

## Checks and remaining publication work

Run `python3 scripts/check-public-content.py` before committing source or docs.
Use synthetic session titles, reserved example domains and documentation addresses
in fixtures; do not paste real account, task or device records into them.
The check reports file/line and rule names without printing candidate secrets.
It checks tracked UTF-8 content only, not Git history, images or remote services.

The previous repository and its historical commits/PRs remain in a private
archive. The new repository is bootstrapped from tracked sanitized files as one
commit under [ADR 0023](adr/0023-bootstrap-sanitized-repository.md); old Git objects,
authors, PRs and hosted logs are not imported. Historical commit hashes and PR
numbers in documentation refer to that private record. They are not evidence
that the old repository is safe to publish. Keep the archive private.

The new repository also starts private, with GitHub Actions disabled. Review its
single root commit, all refs and hosted surfaces before changing visibility.
Existing clones remain attached to the archive; use a fresh clone for the new
repository and never force-push an old branch into it.
