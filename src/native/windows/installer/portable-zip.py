"""Assemble and read back the full Windows package on native and cross-build hosts."""
from hashlib import sha256
import json
import os
from pathlib import Path
import re
import sys
from tempfile import TemporaryDirectory
from zipfile import ZIP_DEFLATED, ZipFile

PAYLOAD = (
    "Muster.exe", "musterd.exe", "muster-tun.exe", "muster-vnet.exe",
    "muster_ffi.dll", "cli/muster.exe", "wintun.dll", "Wintun-LICENSE.txt",
    "LICENSE.txt", "Runtime-Notices.txt", "muster-browser-host.exe",
    "dev.linsheng.muster.browser.json", "AgentHaloBrowser/manifest.json",
    "AgentHaloBrowser/sw.js", "AgentHaloBrowser/jumper.js",
)


def build(stage, version, output):
    if not re.fullmatch(r"[0-9]+[.][0-9]+[.][0-9]+", version):
        raise ValueError("Expected a three-part release version")
    source = Path(__file__).resolve().parent
    output.mkdir(parents=True, exist_ok=True)
    name = f"Muster-{version}-win-x64.zip"
    with TemporaryDirectory(dir=output) as work:
        archive = Path(work) / name
        checksums = {}
        with ZipFile(archive, "w", compression=ZIP_DEFLATED) as package:
            for item in PAYLOAD:
                data = (stage / item).read_bytes()
                if item.endswith((".dll", ".exe")):
                    offset = int.from_bytes(data[60:64], "little")
                    if data[:2] != b"MZ" or data[offset:offset + 6] != bytes.fromhex("504500006486"):
                        raise ValueError(f"Expected x64 PE image in {item}")
                checksums[item] = sha256(data).hexdigest()
                package.writestr("payload/" + item, data)
            package.write(source / "Apply-Update.ps1", "Apply-Update.ps1")
            package.write(source.parent / "README.md", "README.md")
            package.writestr("release.json", json.dumps({"version": version, "files": checksums}, indent=2))
        with ZipFile(archive) as package:
            if package.testzip():
                raise ValueError("ZIP failed CRC verification")
            for item, digest in checksums.items():
                if sha256(package.read("payload/" + item)).hexdigest() != digest:
                    raise ValueError(f"ZIP readback failed for {item}")
        # Publish only a verified archive. A released version cannot be overwritten.
        if os.name == "nt":
            archive.rename(output / name)
        else:
            os.link(archive, output / name)
    print(output / name)


if __name__ == "__main__":
    build(Path(sys.argv[1]), sys.argv[2], Path(sys.argv[3]))
