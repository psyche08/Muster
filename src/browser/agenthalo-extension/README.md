# Agent Halo Browser (JavaScript + Rust)

Chrome web accessibility is a local Agent Halo capability. JavaScript adapts `chrome.debugger`,
`chrome.tabs`, and Native Messaging; Shared Rust `muster-browser` owns request types,
validation, serial scheduling, each operation's execution deadline, AX tree pruning, and release after 15 seconds idle. There is no Python runtime.

The extension also makes one request on the browser's behalf for the web window
(ADR 0026): `fetch` reaches a site as this browser, with the session the person is
already signed in with, and opens no tab. It asks for no site access when installed
(`optional_host_permissions`, nothing required) and refuses until the person grants
that one origin in `chrome://extensions`. The caller may not send `cookie`, `host`,
`connection` or `content-length`, and `Set-Cookie` never travels back.

```text
muster browser → muster-browser (Rust client / private Unix socket)
               → muster-browser-host (Rust, Chrome launches it)
               → Native Messaging → sw.js → Chrome CDP Accessibility / DOM / Input
```

Use `muster browser` locally; remote calls use musterd's same encrypted PeerLink/RPC.
Requests can also be entered directly in Muster's Devices page → Chrome browser.
Both macOS and iOS Apps use Shared Rust state, validation, and user authentication flows.

Remote reads require `browser.view`, and writes require `browser.control`. These are new independent
scopes and are not inherited automatically from old Agent permissions. Targets remain governed by the local `offer_agent` policy.
Update Muster on each device first, then explicitly grant access on the network device permissions page; older versions do not recognize the new scopes.
While locked, requests also require `locked.control`, control while locked enabled on the target, and a Presence signature
over the complete request (including op_id / JSON). Browser CDP **keeps the system locked**;
it does not call desktop temporary-unlock, overlay, or brightness APIs. It is mutually exclusive with desktop Locked Use through the console write lock.

```sh
muster --target Mac-B browser '{"op":"tabs"}'
# Hand locked requests to local Muster for review; click Run & authenticate, and results appear in the App.
muster --target Mac-B browser --approve-in-app '{"op":"snapshot","tabId":123}'
```

`--approve-in-app` is an explicit macOS handoff. It neither prints a fabricated completed result nor automatically
clicks authentication. The URL only populates the target and request for review; sending occurs only after the user clicks Run.
Writes with the same op_id execute at most once; unknown outcomes remain in the ledger without automatic retry. Revocation is checked before cache hits.
Native browser adapters support macOS / Linux; the Windows named-pipe adapter is not implemented.

## Provenance

Ported from the mac-b Claude chat **Chrome 网页无障碍支持** (Chrome web accessibility support)
(`e14861ed-2879-5d1c-83c1-b81bdb980371`, 2026-09-28), specifically its working directory's
`muster/extension/{manifest.json,sw.js}`. The public manifest key is retained; the extension ID is
`ngfbfehgemldmajkjhjahbechooakidc`. This is a public key, not a device or hardware identity.
The Python host / CLI / test launcher were not imported into the project.

The prototype chat recorded successful Chrome 153/154 temporary-profile tests; **this is not device acceptance of the migrated version**.
The Rust host / protocol integration tests and JavaScript API mock tests in this round are reported separately.

## Build and Installation

The production macOS App bundles the extension and signed native host. In **Settings → Chrome extension →
Set up Agent Halo Browser**, proceed in order: set up the native host, reveal the extension directory, open Chrome's
extensions page, enable Developer mode and Load unpacked, then Check connection.
Installation registration is restricted to the local user; remote pages instruct the user to finish on the target Mac. Connection status comes from an actual Rust
ping; copying files or writing a manifest does not establish a successful connection.

Developers can also run from the repository root:

```sh
cargo build --manifest-path src/muster/Cargo.toml --release -p muster-browser -p muster-cli
# macOS: register the absolute path of the current release binary (register again after moving the directory)
src/muster/target/release/muster-browser-host install \
  "$HOME/Library/Application Support/Google/Chrome/NativeMessagingHosts"
# For Linux Chrome, pass "$HOME/.config/google-chrome/NativeMessagingHosts"
```

Enable Developer mode at Chrome's `chrome://extensions`, select "Load unpacked",
and open this directory. If the prototype is already loaded, replace its old directory with this one and reload the extension.
The registration name is now `dev.linsheng.muster.browser`; it does not overwrite the prototype's `com.muster.host`.
Registration and browser permissions are explicitly installed and enabled by the user; builds do not modify the everyday profile.
Chrome displays its debugger notification bar; this project does not hide it.

```sh
src/muster/target/release/muster browser '{"op":"ping"}'
src/muster/target/release/muster browser '{"op":"tabs"}'
src/muster/target/release/muster browser '{"op":"snapshot","tabId":123}'
src/muster/target/release/muster browser '{"op":"find","tabId":123,"role":"button","name":"Submit"}'
```

Snapshot/find returns `{tabId, document, nodes}`. Use the actual returned `document` and node `ref`
for operations; the numbers below are examples only:

```sh
src/muster/target/release/muster browser '{"op":"type","tabId":123,"document":50,"ref":8,"text":"Hello","clear":true}'
src/muster/target/release/muster browser '{"op":"click","tabId":123,"document":50,"ref":9}'
src/muster/target/release/muster browser '{"op":"press","tabId":123,"document":50,"ref":8,"key":"Enter"}'
src/muster/target/release/muster browser '{"op":"sessions"}'
src/muster/target/release/muster browser '{"op":"detach","tabId":123}'
```

```sh
# The web window's own operation; the reply is the site's bytes, base64.
src/muster/target/release/muster browser \
  '{"op":"fetch","url":"https://example.test/issues","method":"GET","headers":[["accept","text/html"]],"body":""}'
```

`open {url}` and `close {tabId}` are also supported. Open returns the new tab ID without waiting for page load;
a subsequent snapshot queries the actual page state. Every request involving an existing tab must specify tabId;
input must specify a node and does not rely on current focus. Input from old snapshots is rejected after the document changes.
This does not freeze the page: the DOM can still change within the same document, so read fresh state before every operation.

## Boundaries and Failure Semantics

- Local socket: `$MUSTER_DIR/browser/bridge.sock`, using Muster's standard state directory by default.
  Directory mode 0700, socket mode 0600; a file lock limits use to one Chrome profile. A second host does not take over an old instance.
- The socket returns ready before receiving one request. The Rust client does not queue writes in advance while the host is busy.
  Native Messaging and the local socket both use a native-endian u32 length followed by UTF-8 JSON.
  Requests are ≤256 KiB (a `fetch`, which carries a body, ≤768 KiB — Chrome caps one message to the extension at 1 MB)
  and responses ≤8 MiB, with a `fetch` reply's body ≤5 MiB; Rust checks lengths before allocation. Unknown fields and arbitrary CDP/eval are rejected.
- Connection/host execution deadline: 3 seconds, and 20 for a `fetch`, which is network-bound rather than local. A complete CLI call has one further second.
  Execution timeout or loss of the native channel closes the host without replay; writes return outcome unknown.
  The extension only restores the Native Messaging connection, never resends executed requests, and releases the debugger after disconnection.
- After 15 seconds without requests, Rust sends detachAll, releasing only tabs attached by this extension.
  Requests attach as needed; they do not enable system-wide macOS AX or independently unlock, overlay, or change brightness.
- HTTP(S) pages only; Chrome internal pages, extension pages, file pages, cross-origin iframes, and native select are unsupported.
  Chrome's own network stack handles page requests; the extension does not change Chrome DNS/DoH settings.
- Malicious same-UID programs remain within architecture threat model D3; the local socket is not a remote authorization credential.
  The extension has no content script or externally_connectable; web pages cannot directly request the native host.

## Verification

```sh
cargo test --manifest-path src/muster/Cargo.toml -p muster-browser
cargo clippy --manifest-path src/muster/Cargo.toml -p muster-browser -p muster-cli --all-targets -- -D warnings
node --test src/browser/agenthalo-extension/adapter.test.cjs
```

Rust tests launch a real host subprocess and use stdio Native Messaging and a private Unix socket
to verify requests/responses, no replay after timeout, incorrect origins, profile exclusion, and EOF cleanup. Chrome-side tests use
API mocks covering document invalidation, blocking further input during disconnection, idle detach, keys, and the extension ID derived from the public key.
New musterd RPC tests cover independent scopes, lock-screen policy, signature binding, duplicate requests, and revocation.
Browser automation boundary tests use simulated Chrome/socket environments and do not replace complete E2E with the user's Chrome and Face ID/Touch ID.

API references: [Chrome Native Messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging),
[Chrome debugger](https://developer.chrome.com/docs/extensions/reference/api/debugger)。
