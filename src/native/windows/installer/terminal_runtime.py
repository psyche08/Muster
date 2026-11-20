"""Pinned MSYS2 files for the portable Windows conversation terminal."""
from hashlib import sha256
import os
from pathlib import Path
import subprocess
from urllib.request import urlopen

REPO = "https://repo.msys2.org/"
BASE = "msys2-base-x86_64-20260927.tar.zst"
TMUX = "tmux-3.8-1-x86_64.pkg.tar.zst"
EVENT = "libevent-2.1.13-2-x86_64.pkg.tar.zst"
BINARIES = {
    BASE: ("distrib/x86_64/", "8a2095647afbf799d113d0cae35f7544589d059819042635504e75bd2179bd02"),
    TMUX: ("msys/x86_64/", "8ccd83d29f0f29aa5ed283f00c4770e55731a0f36e488ed05d7a5226034c2560"),
    EVENT: ("msys/x86_64/", "77a1a6fc33d6b3742e6556bbfce4ee2198fe858a1a3138f9dea3984275b9ce01"),
}
SOURCES = {
    "msys2-runtime-3.6.10-5": "c05a1d70e1e1c94b02f294071ae53f05280ec94ae4d41873d2af3898beae782a",
    "bash-5.3.020-1": "92c9edc67b962374cb79868fefa4ef26d8327f679cd0d5460834651c5939ee2d",
    "ncurses-6.6-2": "f5df12bd297e6b6930e2aa68e8014347b2193592645d93acce6774e05cbf15e6",
    "tmux-3.8-1": "63903b645e0c13a49b2723bcea6c897335bd0c794ea5b8cba9dace5419c7a162",
    "libevent-2.1.13-2": "c2bee9465277f7cc14dd1b432fd8e817a5497dcdc4ef5cc65b2c380a4ed697fe",
    "coreutils-8.32-5": "2b8689413bc50367a9995adf50ea84cd3caaff0ec68a8f32d8328842c3f83835",
    "gettext-0.22.5-1": "e07a0b3e6716aae86dc0edd2dcdc8864731d56051345592b53659a3840ccfbd8",
    "libiconv-1.19-1": "5a8bcd3242e2be94fffd9f03fadcda8d9fd64047430050c5cbd214cd3bf7f0a7",
}
TOOLS = ("bash sh basename cat chmod cp cut cygpath date dirname env head ls mkdir mv "
         "pwd rm rmdir sleep tail touch tr uname wc").split()
BASE_FILES = (
    *(f"usr/bin/{name}.exe" for name in TOOLS),
    "usr/bin/msys-2.0.dll", "usr/bin/msys-ncursesw6.dll",
    "usr/bin/msys-iconv-2.dll", "usr/bin/msys-intl-8.dll",
    "usr/share/terminfo/78/xterm-256color", "usr/share/terminfo/74/tmux-256color",
    "usr/share/terminfo/73/screen", "usr/share/licenses/ncurses/LICENSE",
    "usr/share/doc/Cygwin/COPYING", "usr/share/doc/Cygwin/CYGWIN_LICENSE",
    "usr/share/doc/mpfr/COPYING.LESSER",
)


def verified_input(work, name, directory, digest):
    target = work / name
    cache = os.environ.get("MUSTER_MSYS2_ARCHIVES")
    source = (Path(cache) / name).open("rb") if cache else urlopen(REPO + directory + name, timeout=90)
    with source, target.open("xb") as output:
        checksum = sha256()
        while chunk := source.read(1024 * 1024):
            checksum.update(chunk)
            output.write(chunk)
    if checksum.hexdigest() != digest:
        raise ValueError(f"MSYS2 input checksum mismatch: {name}")
    return target


def stage_runtime(work):
    """Extract only named members to a private scratch directory."""
    root = work / "msys64"
    tar = Path(os.environ["SystemRoot"]) / "System32/tar.exe" if os.name == "nt" else "tar"
    files = []
    for name, members in (
        (BASE, BASE_FILES),
        (TMUX, ("usr/bin/tmux.exe", "usr/share/licenses/tmux/COPYING")),
        (EVENT, ("usr/bin/msys-event_core-2-1-7.dll", "usr/share/licenses/libevent/LICENSE")),
    ):
        archive = verified_input(work, name, *BINARIES[name])
        paths = [f"msys64/{member}" for member in members] if name == BASE else members
        subprocess.run([tar, "-xf", archive, "-C", work if name == BASE else root, *paths], check=True)
        files.extend(f"msys64/{member}" for member in members)
    profile = root / "etc/profile"
    profile.parent.mkdir()
    profile.write_bytes(b'export PATH="/usr/bin:$PATH"\n')
    files.append("msys64/etc/profile")

    sources = []
    notices = ["Windows conversation terminal", "",
               "Corresponding sources and MSYS2 build recipes are included in Terminal-Sources/",
               "in the distribution ZIP. The unchanged upstream source packages are listed below.",
               "License texts and copyright notices accompany the libraries and sources.", ""]
    for package, digest in SOURCES.items():
        name = package + ".src.tar.zst"
        sources.append(verified_input(work, name, "msys/sources/", digest))
        notices.append(f"{name}  SHA-256 {digest}")
        notices.append(REPO + "msys/sources/" + name)
    (work / "Terminal-Runtime.txt").write_text("\n".join(notices) + "\n", encoding="utf-8")
    files.append("Terminal-Runtime.txt")
    return files, sources
