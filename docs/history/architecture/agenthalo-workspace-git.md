# Session-scoped read-only Git history

Shared owns repository selection, selectors, process orchestration, authorization
and response validation. The Mac App renders the returned history and patch and
passes opaque selectors unchanged. It never runs Git or opens a remote cwd as
a local URL. This feature does not modify the index, worktree, refs or commits.

## Protocol and routing

- `GET /workspace_git_history`: `provider_id`, `session_id`, `cursor` (empty for
  the first page). Pages contain at most 50 commits and a nullable next cursor.
- `GET /workspace_git_diff`: `provider_id`, `session_id`, `revision` returned by
  a history row. The patch is plain UTF-8, at most 512 KiB; excess output fails
  rather than returning a truncated patch.
- Both routes require exact Shared `SessionRead` authorization. Unknown query
  fields are rejected. The backend obtains cwd from the actual provider session,
  checks `workspace_git` capability and opens that directory with the existing
  descriptor-relative no-follow adapter. Callers cannot supply a repository root.
- A cursor binds a canonical offset to the observed HEAD; a revision binds a
  commit OID to that HEAD. Only full lowercase SHA-1/SHA-256 OIDs are accepted,
  never revision expressions, paths or options. A changed HEAD requires refresh.
  A diff commit must be an ancestor of the selected HEAD.
- All log/show calls carry the session cwd pathspec. Sibling-only commits and
  sibling file changes are excluded. Patches have paths relative to that cwd.
  Merge patches compare against the first parent; they are not combined diffs.
- An unborn repository returns an empty history. A nonrepository, corrupt HEAD,
  unavailable object, malformed metadata, unsupported Git or read-budget failure
  returns an explicit error, never an empty-success substitute.

ABI 1.78 exports `agenthalo_local_workspace_git_read` (history 0 / diff 1).
Authenticated session-read operations 6 / 7 carry `gitSelection` and forbid
`path`. Existing file/session opaque routes reject unknown or stale IDs before
dispatch, including unknown local IDs; the old local-route fallback is not used.
Typed Rust clients validate response identities, selectors, bounded metadata,
duplicate commits, page progress and patch contents before crossing the ABI.

## Host process boundary

The Mac/Linux adapter invokes fixed `/usr/bin/git` with a cleared environment
and fixed arguments, anchoring cwd using the owned directory descriptor before
exec. Each read shares a three-second budget across commands. Captured stdout
is bounded to 1 MiB and stderr to 8 KiB. On deadline/overflow, only the owned
child's private process group is terminated and reaped. Error output is not
forwarded to the consumer.

Global/system config, inherited Git environment, pager, hooks, fsmonitor,
signature verification, external diff, textconv, replacement objects, grafts,
automatic rename following and submodule diff execution are disabled. Lazy
fetch, prompts and all transport protocols are disabled. Missing objects fail
closed instead of contacting a remote. Tests configure hostile helper commands
and a promisor remote and assert they never run. Repository-local metadata is
still processed by the installed Git builtin; this is not a separate Git parser
or a guarantee against vulnerabilities in that executable. Unsupported versions
fail explicitly; the exercised host is Apple Git 2.50.1.

Relevant primary references: [Git command/environment options](https://git-scm.com/docs/git)
and [Git log/diff options](https://git-scm.com/docs/git-log).

## UI and verification boundary

The Mac Git button opens history for the selected device/session. UI provides
previous/next page, commit patch, HEAD refresh, retry and close. Replaced sessions,
clients or Owners and shutdown stop old readers; late results cannot overwrite
the new view. File and Git sheets are mutually exclusive. UI selectors are
navigation data, never independent authorization decisions.

Tests exercise real temporary repositories and the actual backend Unix socket,
including subdirectory scope, pagination, changed HEAD, unborn/corrupt state,
unreachable commits, detached linked worktrees, first-parent merges, missing
objects, disabled helpers, oversized patches and a blocked process deadline.
Swift tests exercise the actual local ABI/socket plus native remote forwarding,
multidevice route rejection and UI lifecycle fencing.

Harmony's conversation header now opens the read-only history/patch browser.
Its transport selects the existing per-device authenticated client and sends
native operations 6/7 with `gitSelection`, never a path or caller-defined root.
The controller retains opaque cursors for previous/next navigation; HEAD refresh
resets navigation and errors remain distinct from an empty history. The patch
is selectable plain text, not HTML or an executable link. File/Git views are
mutually exclusive. Closing, background/session changes, authorization events
and device removal/unavailability stop old display updates. An unchanged usable
device directory update does not interrupt the view.

Production ArkTS adapter/controller/page fixtures cover the above routing and
navigation, changed-HEAD errors, retries, exact same-ID sessions on different
devices and late success/failure after replacement or closure. The installed
SDK compiles the real ArkUI builder; host C++ NAPI tests reject substituting an
owner anchor for an authenticated client on both Git operations. These tests
do not constitute physical layout, HUKS/Relay or native remote Git acceptance.
Neither a signed installed-App Git display nor real remote Git use on a phone
has been accepted. Compilation and fixtures do not establish those gates.
