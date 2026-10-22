# Session workspace file reads

Status: Shared backend/client, ABI 1.77, Apple/Harmony native adapters, Mac
project-files/conversation-file UI and Harmony file browser/preview UI are
implemented. No signed installation or real-device acceptance is claimed by
this source checkpoint.

## Protocol and authority

`GET /workspace_files`, `GET /workspace_file_preview` and
`GET /workspace_file_reference` take `provider_id`,
`session_id` and `path`. The existing authenticated MST2
`agenthalo.session.read` operation is required; write-only grants, wrong methods
and unknown routes do not authorize these reads. Normal current target/Grant
validation and provider read deadlines still apply. Local requests go through
the private local-admin socket. Neither endpoint accepts a caller-defined root.

The provider must advertise `workspace_files`. Shared resolves the exact session
against its provider-native catalog, using the live catalog only when there is
no native match. Wrong provider identity, duplicate matches and missing cwd are
errors. A match without a safe cwd is never replaced by another guess.

The server chooses that session's absolute cwd. `path` is relative to it; the
empty path lists the root directory. Parent components, absolute paths, empty
interior components, backslashes, controls and oversized components are rejected.
Percent escapes are decoded by the HTTP parser once and encoded by the typed
client once; filesystem operations never percent-decode a filename again.

The reference endpoint accepts an absolute conversation reference only inside
that serving session's actual cwd, using an exact component-delimited prefix;
sibling-prefix paths and parent traversal fail. Relative references keep their
exact path. The response echoes the exact reference along with its resolved
relative path; Shared validates both response identity and content before the
platform receives bytes. No consumer cwd, home expansion or URL fallback is used.

## OS adapter

The Mac/Linux adapter walks the root and child components with descriptor-relative
`openat`, `O_NOFOLLOW`, `O_DIRECTORY` where applicable, and close-on-exec ownership.
Symlink cwd ancestors fail instead of being silently canonicalized to another
location. File reads require a regular single-link file; symlinks, hardlinks,
sockets and FIFOs are not previewable. Nonblocking opens prevent FIFO waits.
Directory listings label unsupported entries `unavailable` without following
them. Other host platforms currently return unsupported.

File identity/size/mtime/ctime are compared on the same descriptor before and
after reading. Directory metadata is likewise checked around enumeration;
concurrent detected changes return conflict. File descriptors and directory
streams close on every return path. This is not a filesystem snapshot or a
guarantee that files will remain unchanged after a completed read.

## Bounds and response validation

Directories are complete or an error: at most 4096 entries and 3 MiB of encoded
entry metadata. No successful partial directory or implicit truncation is used.
Each entry has a name, exact relative child path, kind and size. The client
rejects duplicate names, foreign children, identity mismatch and malformed types.

File previews contain at most 2 MiB, base64-encoded, with the original file size,
a truncation flag and SHA-256 of exactly the returned bytes. The client checks
the byte bound, digest, original size/truncation consistency and exact requested
provider/session/path before returning data. The total HTTP response is bounded
to 4 MiB. The digest is content integrity metadata, not authorization evidence.

Consumers must not interpret remote paths as local URLs, execute content, treat
truncated image/PDF bytes as complete files, or infer access from cached display.

## Native and UI boundary

ABI 1.77 local file read operations are 0 directory, 1 relative preview and
2 conversation reference. Authenticated remote session reads add operations
3 directory, 4 relative preview and 5 conversation reference; `path` is required
only for those operations. Existing opaque clients still own Grant/target checks.
The new Shared file-route selector rejects unknown or stale IDs on *every*
device; it never uses the legacy operation selector's local fallback.

Mac project files and conversation-file clicks use these readers. The browser
captures the selected client/session, fences late navigation results, and closes
on session/client replacement or shutdown. Errors are visible rather than empty
successes. The old consumer-local URL/FileHandle preview was removed. Only bounded
UTF-8 or single-image bytes are rendered. Native ImageIO rejects truncated binary
previews, multi-frame images, dimensions over 8192 and more than 16,777,216 pixels;
thumbnails are bounded to 2048 pixels. Truncated UTF-8 may drop an incomplete final
code point. No file content is executed and no PDF rendering is currently offered.

Harmony's transport and actual async NAPI adapter forward the exact session/path
to the same Shared reads, retaining the existing connection-generation checks.
Its development fixture rejects file reads explicitly. The phone's conversation
header opens a dedicated project-files view with directory/back/refresh/close,
an explicit conversation-file path field, and bounded text/image previews.
Shared still resolves all file references; the phone does not open local paths.
The current conversation message DTO is plain text, so paths are pasted into
that field rather than inferred from arbitrary message text.

The Harmony controller fences superseded reads and late decoder completions.
Session changes, view exit, authorization/lifecycle notifications and negative
device-availability updates close the view and release PixelMaps; unchanged
usable peer updates do not. Error state is distinct from empty directories.
UTF-8 decoding is fatal on invalid data and may drop a partial final code point
only for truncated content. ImageKit uses bytes, never URIs, rejects animations,
unrecognized/non-raster MIME types, dimensions over 8192 or 16,777,216 pixels,
and requests a thumbnail with a longest edge of 2048. ImageSource and PixelMap
resources are released on failure, replacement and late completion as well as
normal close. PDF, animations and executable/vector rendering are unsupported.

## Evidence boundary

Tests use isolated temporary directories and a real private Unix socket with the
typed client, including Unicode/percent-encoded paths, full directory reads,
content hashing, truncated previews and unknown sessions. Negative tests cover
root/parent escapes, symlink ancestors and children, hardlinks, FIFO reads,
capacity errors, cross-provider/session responses and exact read-grant routing.
They do not prove real Relay, Secure Enclave/HUKS, or installed UI operation.
