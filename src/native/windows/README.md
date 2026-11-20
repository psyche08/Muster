# Muster for Windows x64

## Install or update

Verify the downloaded x64 ZIP against its published SHA-256. For a trusted downloaded ZIP, use Properties > Unblock if Windows marked it
as downloaded. Extract it to a temporary directory. Open an ordinary shell
there and run:

    .\Apply-Update.cmd

The launcher uses Windows PowerShell with RemoteSigned for that process.
Organizational group policy still applies. The script prints the fixed shortcut at
`%LOCALAPPDATA%\Programs\Muster\Muster.lnk`. Pin this shortcut or copy a
shortcut to it. To use a different private directory, pass `-Destination`
with the same path on each update. Choose a location where only your account
and administrators can replace executables or rename their parent directories.
Use the same procedure for each new ZIP. A single-file WinForms app, Rust
core, daemon, CLI, Chrome host and extension, network node, Wintun DLL,
packet helper and license notices are included. PowerShell verifies the file
hashes and switches the shortcut after staging the new release.

The desktop opens maximized with its normal title bar. Reopening the shortcut
activates the current window. Close the GUI and reopen the shortcut to pick
up new UI code. The daemon and tunnel can continue running. A daemon update
takes effect the next time the full node starts. Earlier release directories
can stay in place while processes still use them. For rollback, reapply a
previous verified ZIP. Updates leave the user identity, roster, SSH settings
and application data in place.

The desktop uses the signed-in user's TPM identity. On a computer already
running the standalone node, close that node before launching the full desktop
for the same account. An exclusive owner lock protects the identity and roster
against concurrent use. To keep running only the network node, leave the
desktop closed and keep the existing node running.

The full daemon runs in the interactive session. Windows requests elevation
for the Wintun helper when the virtual network starts. An existing protected
helper from the desktop MSI can continue serving a compatible packet protocol.
On a fresh machine an administrator must first copy the verified package's
`payload\muster-tun.exe` and `payload\wintun.dll` into the system
`Program Files\Muster` directory using Explorer and approve that copy. Use
the same separate step when a release changes the helper protocol or driver.
Provision these files again if removing a previous MSI removes them. Keep
the default administrator-owned Program Files permissions. Start the desktop
from the per-user shortcut. The helper handles IP
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
active release's AgentHaloBrowser folder as an unpacked extension in Chrome.
After an extension update, load the folder shown in the browser setup screen.

## Develop as a standard user

For repeatable GUI input, screenshots and high-DPI checks, see the
[Windows agent development and debugging guide](../../../docs/WINDOWS_AGENT_DEVELOPMENT.md).

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
Starting a real Wintun adapter requires Windows administrator approval.

## Build

On Windows x64, use PowerShell 7.4 or later, x64 MSVC Rust, Visual C++ build
tools, the .NET 10 SDK, protoc and Python 3. Build from the repository root:

    ./src/native/windows/installer/build-portable.ps1 -Version 1.1.55 -OutputDirectory .\artifacts

On a Unix cross-build host, use the .NET 10 SDK, protoc, Rust's
x86_64-pc-windows-gnu target, mingw-w64, bash, curl, unzip, shasum and Python
3. Set CARGO_TARGET_DIR to the Cargo build directory, then run:

    src/native/windows/installer/build-portable.sh 1.1.55 /absolute/output/directory

WINTUN_ARCHIVE can point to a local copy of the pinned Wintun ZIP. Both
builds verify the driver archive, validate all executable images as x64 and
read back the ZIP to compare its payload hashes. A development ZIP is unsigned.
