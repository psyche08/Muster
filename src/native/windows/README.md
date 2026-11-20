# Muster for Windows x64

Launch **Muster** from the Start menu. The desktop opens maximized with its
normal title bar. Reopening it activates the existing window. The MSI installs a self-contained
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

## Develop as a standard user

From a PowerShell 7.4 or later shell with Cargo, the MSVC tools, protoc and the
.NET 10 SDK, use the following from the repository root:

    ./src/native/windows/dev.ps1

This builds a desktop client and the Rust FFI into a user-writable Cargo
target directory and opens **Muster (development)**. Start the installed
Muster node once before using this mode. Close the current GUI before launching a different client build. Installed
and development clients share a single main window. Client restarts reuse
its owner-checked pipe and existing vnet. The development client keeps its
own logs and view cache. Actions in this window operate on the live node.

To exercise daemon changes with a synthetic identity and a fresh temporary
store, run the fixture and use a second shell to call its printed socket:

    ./src/native/windows/dev.ps1 -Mode Daemon
    muster --socket <printed-path> info

Press Enter in the fixture shell to shut it down. It binds to loopback and
starts with agent discovery, cloud access and vnet off. Use the built CLI
from the Cargo target directory or the installed CLI under Muster's cli
folder. Both development modes accept -Release to reuse a release build.
Installing an MSI or starting an actual Wintun adapter requires Windows
administrator approval.

## Build

On Windows x64, use PowerShell 7.4 or later, the x64 MSVC Rust toolchain,
Visual C++ build tools, .NET 10 SDK, protoc, Python 3 and WiX 3.14. Set
WIX_BIN to the directory containing candle.exe and light.exe when those
programs are outside PATH. The output directory must have room for the MSI.

    ./src/native/windows/installer/build-msi.ps1 -Version 1.1.51 -OutputDirectory C:\\build\\muster

From the repository root, run the following on a macOS or Linux cross-build
host with the .NET 10 SDK, protoc, Rust's x86_64-pc-windows-gnu target,
mingw-w64, bash, curl, unzip, shasum, Python 3, wixl and msiextract.
CARGO_TARGET_DIR is the shell's existing Cargo build directory.

    src/native/windows/installer/build-msi.sh 1.1.51 /absolute/output/directory

WINTUN_ARCHIVE can point to a local copy of the pinned Wintun ZIP. The build
checks its SHA-256 and extracts the completed MSI to compare every installed
file with its input. This produces a development package. Install, launch,
upgrade and uninstall should also be exercised on the target Windows version.
