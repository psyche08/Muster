"""Collect notices from the exact .NET runtime packs resolved by restore."""
import json
import sys
from pathlib import Path

assets = json.loads(Path(sys.argv[1]).read_text())
folders = [Path(folder) for folder in assets["packageFolders"]]
framework, = assets["project"]["frameworks"].values()
notices = []
for dep in framework["downloadDependencies"]:
    name = dep["name"]
    if name not in {"Microsoft.NETCore.App.Runtime.win-x64", "Microsoft.WindowsDesktop.App.Runtime.win-x64"}:
        continue
    version = dep["version"].lstrip("[").split(",")[0]
    directory = next(path for root in folders if (path := root / name.lower() / version).is_dir())
    files = [path for path in sorted(directory.iterdir())
             if path.name.lower() in {"license", "license.txt", "third-party-notices.txt"}]
    if not files:
        raise ValueError(f"Missing license for {name}")
    notices.append(name + " " + version + "\n\n" + "\n\n".join(file.read_text(encoding="utf-8-sig") for file in files))
if len(notices) != 2:
    raise ValueError("Missing resolved runtime pack notices")
notices.append("Google.Protobuf\n\n" + Path(__file__).with_name("Protobuf-LICENSE.txt").read_text())
Path(sys.argv[2]).write_text("\n\n".join(notices), encoding="utf-8")
