# Agent Development Rules

These rules apply to the entire repository.

The target architecture is `docs/ARCHITECTURE.md`; its §4 says which existing
code is kept, ported or deleted, and §5 gives the migration phases. Do not add
capabilities to code on the delete list. Superseded documents live in
`docs/history/` and are records, not requirements.

## Cross-platform architecture

- Put all platform-independent business logic in Shared Rust. This includes product state machines, protocol and authorization decisions, validation, provider/session semantics, network orchestration, durable transition rules, and security/trust-critical behavior.
- Keep every platform App thin: it may render UI, collect user input, bridge application lifecycle, call Shared Rust through the supported ABI, and adapt APIs that only the operating system can provide (for example local authentication, hardware-backed keys, secure storage, camera, notifications, and native terminal integration).
- Platform code must not duplicate, reconstruct, or independently infer Shared Rust business state from JSON, booleans, HTTP success, cached metadata, or other serializable projections. Trust-bearing state and transitions must remain Rust-owned and use opaque handles or narrowly scoped platform callbacks.
- When a product capability is missing from the Shared Rust ABI, extend Shared Rust and its ABI first. Do not implement a platform-specific business-logic fallback.

## Architecture Decision Records (ADR)

- Every future repository change must add or update a numbered ADR in `docs/adr/` in the same pull request. This includes behavior, protocol, UI, tests, build/release, configuration, and documentation changes; there is no small-change exemption. Read-only investigation with no repository changes needs no ADR.
- Before implementing a change, read the relevant ADRs and record its context, decision, alternatives, consequences, and verification plan. One ADR may cover a coherent group of changes; keep implementation and device acceptance evidence distinct.
- Update an existing ADR only when continuing the same decision or adding evidence. A changed decision needs a new ADR that links and supersedes the old one; do not rewrite accepted decision history.
- Update `docs/adr/README.md` when adding an ADR, link the ADR in the PR description, and record unresolved validation or manual prerequisites honestly. Use `docs/adr/template.md`.
- Run `scripts/check-adr.sh origin/main` before landing. `scripts/finish-session.sh` enforces this requirement; an index/template-only edit does not satisfy it.

## Turn completion and commits

- At the end of every development turn that changes repository files, create a Git commit for the changes owned by that turn before sending the final response.
- Stage files explicitly. Do not include unrelated pre-existing changes, another agent's work in progress, generated build products, secrets, credentials, or local runtime state.
- Run checks proportional to the change before committing. Report code, tests, signing/notarization, installation, runtime, and end-to-end verification as separate states.
- A failed test, signing step, or deployment does not justify leaving completed source edits uncommitted; commit the coherent source change and report the failed verification truthfully. If the change itself is incomplete or unsafe to commit, do not claim the turn is complete.
- Read-only investigation turns and turns with no repository changes do not require an empty commit.

## Branches, worktrees, and landing work

- Keep the primary checkout on `main`. Do each session's development in its own linked worktree on a new branch started from `origin/main`; do not switch the primary checkout to a session branch.
- When the work the user asked for is complete and its verification is reported, land it in the same turn: open a pull request with an English title and description, then run `scripts/finish-session.sh` from the session's worktree. It pushes, waits for CI, merges with a merge commit, deletes the remote branch, and runs `scripts/worktree-gc.sh`.
- If landing fails (CI failure, merge conflict, missing credentials), fix it, or report the failure and the pull request link in the final response. Do not leave finished work unmerged without saying so.
- Do not remove worktrees or branches by hand. `scripts/worktree-gc.sh` removes only worktrees that are clean, landed on `origin/main`, not in use, and idle, and deletes their branches; it never discards uncommitted or unmerged work. Run it with `--dry-run` to see what it would do.

## Security

- Never run or cause any process, script, SSH session, or remote host to run `security unlock-keychain` (including `/usr/bin/security unlock-keychain`). Never request or handle a Keychain password. If signing requires an unlocked Keychain, ask the user to unlock it interactively in Keychain Access and wait for confirmation.

## Release coordination

- Publish an iOS build whenever publishing a macOS update, unless the user explicitly requests otherwise. Keep both release results visible in the delivery report.
- Publish both platforms using the operator-provided private SSH destination, port and release root (`MUSTER_RELEASE_HOST`, `MUSTER_RELEASE_SSH_PORT`, `MUSTER_RELEASE_ROOT`), preserving the desktop and iOS directories documented in `docs/DESKTOP_UPDATES.md`. Do not commit private deployment access details; see `docs/PUBLIC_CONTENT.md`.

## Public content

- Use synthetic identities, tasks and device records in examples and fixtures. Follow `docs/PUBLIC_CONTENT.md` and run `python3 scripts/check-public-content.py` for source/documentation changes.
