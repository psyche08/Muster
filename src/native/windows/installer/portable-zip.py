"""Assemble and read back the full Windows package on native and cross-build hosts."""
from hashlib import sha256
import json
import os
from pathlib import Path
import re
import sys
from tempfile import TemporaryDirectory
from zipfile import ZIP_DEFLATED, ZIP_STORED, ZipFile

from terminal_runtime import stage_runtime

PAYLOAD = (
    "Muster.exe", "musterd.exe", "muster-stream-worker.exe", "muster-tun.exe", "muster-vnet.exe",
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
        work = Path(work)
        runtime, sources = stage_runtime(work)
        viewer_dir = Path(os.environ["MUSTER_WINDOWS_VIEWER"])
        viewer_payload = ("muster-moonlight.exe", "muster-stream-host.exe", "Viewer-Notices.txt")
        host_root = viewer_dir / "Sunshine"
        if not (host_root / "muster-stream-host.exe").is_file() or not (host_root / "assets/shaders").is_dir():
            raise ValueError("Missing private Sunshine host or its shader assets")
        viewer_payload += tuple(p.relative_to(viewer_dir).as_posix() for p in sorted(host_root.rglob("*")) if p.is_file())
        archive = work / name
        checksums = {}
        sizes = {}
        with ZipFile(archive, "w", compression=ZIP_DEFLATED) as package:
            for item in (*PAYLOAD, *runtime, *viewer_payload):
                directory = stage if item in PAYLOAD else work
                if item in viewer_payload:
                    directory = viewer_dir
                data = (directory / item).read_bytes()
                if item.endswith((".dll", ".exe")):
                    offset = int.from_bytes(data[60:64], "little")
                    if data[:2] != b"MZ" or data[offset:offset + 6] != bytes.fromhex("504500006486"):
                        raise ValueError(f"Expected x64 PE image in {item}")
                checksums[item] = sha256(data).hexdigest()
                sizes[item] = len(data)
                package.writestr("payload/" + item, data)
            package.write(viewer_dir / "Host-Sources.zip", "Host-Sources.zip", compress_type=ZIP_STORED)
            viewer_source = viewer_dir / "Viewer-Sources.zip"
            with ZipFile(viewer_source) as source_zip:
                if source_zip.testzip() or "remote-desktop/viewer/windows.cpp" not in source_zip.namelist():
                    raise ValueError("Incomplete Windows viewer source package")
            package.write(viewer_source, viewer_source.name, compress_type=ZIP_STORED)
            for source_package in sources:
                package.write(source_package, "Terminal-Sources/" + source_package.name, compress_type=ZIP_STORED)
            manifest = json.dumps({"version": version, "files": checksums, "sizes": sizes}, indent=2).encode()
            package.write(source / "Apply-Update.cmd", "Apply-Update.cmd")
            updater = (source / "Apply-Update.ps1").read_bytes()
            if updater.count(b"@MUSTER_RELEASE_SHA256@") != 1:
                raise ValueError("Missing unique release manifest pin in the Windows updater")
            package.writestr("Apply-Update.ps1", updater.replace(
                b"@MUSTER_RELEASE_SHA256@", sha256(manifest).hexdigest().encode("ascii")))
            package.write(source.parent / "README.md", "README.md")
            package.writestr("release.json", manifest)
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
