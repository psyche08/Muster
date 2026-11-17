# Muster for Windows x64

Launch **Muster** from the Start menu. The MSI installs a self-contained
WinForms application, the Rust application core, daemon, CLI, browser native
messaging host, standalone virtual network node, Wintun and its adapter
helper. Installation does not launch Muster or change OpenSSH.

The desktop uses the signed-in user's TPM identity. On a computer already
running the standalone node, close that node before launching the full desktop
for the same account. An exclusive owner lock protects the identity and roster
against concurrent use. To keep running only the network node, leave the
desktop closed and keep the existing node running. The MSI leaves user data
in place during repair or uninstall.

The full daemon runs in the interactive session. Windows requests elevation
for the Wintun helper when the virtual network starts. The helper handles IP
packets only. Agent and terminal sharing are off for a new Windows policy.
An administrator on the Muster network can review a joining device's
fingerprint and choose its scopes before accepting it.

The Chats page can control a local Claude or ChatGPT desktop app in an
unlocked interactive session. Keep the provider signed in. ChatGPT reads the
same typed local catalog used by the Mac client. The adapter checks the
provider identity and acts on individual UI Automation controls.

Choose **Presence key** to register a separate TPM-backed P-256 candidate.
Complete the native Windows key-protection prompt. An existing network
administrator can then enroll the public candidate as described in
[Presence enrollment](../../../docs/PRESENCE_ENROLLMENT.md). Every protected
signature requires the key provider's interactive authorization. Windows Hello
local confirmation is independent of this key.

To use the Chrome integration, load the
installed AgentHaloBrowser folder as an unpacked extension in Chrome.

## Build

From the repository root, run the following on a macOS or Linux cross-build
host with the .NET 10 SDK, protoc, Rust's x86_64-pc-windows-gnu target,
mingw-w64, bash, curl, unzip, shasum, Python 3, wixl and msiextract.
CARGO_TARGET_DIR is the shell's existing Cargo build directory.

    src/native/windows/installer/build-msi.sh 1.1.49 /absolute/output/directory

WINTUN_ARCHIVE can point to a local copy of the pinned Wintun ZIP. The build
checks its SHA-256 and extracts the completed MSI to compare every installed
file with its input. This produces a development package. Install, launch,
upgrade and uninstall should also be exercised on the target Windows version.
