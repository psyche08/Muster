#!/usr/bin/env python3
"""Checks that every crate of the workspace depends only on crates the layer
map allows (docs/MODULES.md §2). Dependency direction is architecture: the
foundation never reaches up, services never reach the OS adapters, the OS
adapters never reach the daemon or the client core. Run from src/muster:

    scripts/check-layers.py            # exit 1 on a violation
    scripts/check-layers.py --print    # also print the graph it read

Only `[dependencies]` and `[target.*.dependencies]` count: tests may use any
crate (dev-dependencies are integration fixtures, e.g. the relay).
"""

import re
import sys
from pathlib import Path

# The crates each crate may depend on. A crate missing here, or a dependency
# missing from its list, fails the check: extend the map (and MODULES.md)
# deliberately, not by accident.
ALLOWED = {
    # Foundation: no I/O, no runtime, no platform code.
    "muster-core": set(),
    "muster-proto": set(),
    "muster-paths": set(),
    # Infrastructure: storage, transport, relay, push.
    "muster-store": {"muster-core"},
    "muster-rpc": {"muster-proto", "muster-core"},
    "muster-net": {"muster-core"},
    "muster-relay": {"muster-core", "muster-net"},
    "muster-apns": set(),
    # Services (the integration layer): Agent Halo, terminals, the browser host.
    "muster-agent": {"muster-proto", "muster-core"},
    "muster-terminal": {"muster-proto", "muster-core"},
    "muster-browser": {"muster-paths"},
    # The web window serves one foreign origin on a loopback port; it knows
    # nothing of Muster's own types.
    "muster-web": set(),
    # OS adapters: what only the operating system provides, behind traits.
    "muster-platform": {"muster-proto", "muster-agent", "muster-store", "muster-core"},
    # Composition: the daemon and its command line.
    "musterd": {
        "muster-acp",  # Native scheduler framing composed by the daemon bridge.
        "muster-browser",
        "muster-web",
        "muster-core",
        "muster-net",
        "muster-proto",
        "muster-store",
        "muster-rpc",
        "muster-terminal",
        "muster-agent",
        "muster-platform",
        "muster-paths",
    },
    # A client of the daemon's socket, like the command line: it serves an
    # external task scheduler's protocol and hands every turn to musterd.
    "muster-acp": {"muster-proto", "muster-rpc"},
    "muster-cli": {"muster-proto", "muster-rpc", "muster-paths", "muster-browser", "muster-acp"},
    # Clients: the shells' core and its C ABI. app-core embeds the daemon on
    # phones (§3.3), so it may depend on musterd.
    "muster-app-core": {
        "muster-agent",
        "muster-proto",
        "muster-rpc",
        "musterd",
        "muster-terminal",
        "muster-core",
        "muster-net",
    },
    "muster-ffi": {"muster-app-core", "muster-terminal"},
}

SECTION = re.compile(r"^\[(?P<name>[^\]]+)\]\s*$")
PATH_DEP = re.compile(r'^\s*(?P<dep>[A-Za-z0-9_-]+)\s*=\s*\{[^}]*path\s*=\s*"\.\./(?P<crate>[^"/]+)"')


def dependencies(manifest: Path) -> set[str]:
    """Workspace crates named in the manifest's runtime dependency tables."""
    found = set()
    counting = False
    for line in manifest.read_text().splitlines():
        section = SECTION.match(line)
        if section:
            name = section.group("name").strip()
            counting = name == "dependencies" or (
                name.startswith("target.") and name.endswith(".dependencies")
            )
            continue
        if not counting:
            continue
        dep = PATH_DEP.match(line)
        if dep:
            found.add(dep.group("crate"))
    return found


def main() -> int:
    root = Path(__file__).resolve().parent.parent / "crates"
    graph = {}
    for manifest in sorted(root.glob("*/Cargo.toml")):
        graph[manifest.parent.name] = dependencies(manifest)
    problems = []
    for crate, deps in graph.items():
        if crate not in ALLOWED:
            problems.append(f"{crate}: not in the layer map (scripts/check-layers.py)")
            continue
        for dep in sorted(deps - ALLOWED[crate]):
            problems.append(f"{crate} -> {dep}: not allowed by the layer map")
    for crate in sorted(set(ALLOWED) - set(graph)):
        problems.append(f"{crate}: in the layer map but not in crates/")
    if "--print" in sys.argv[1:]:
        for crate, deps in graph.items():
            print(f"{crate}: {', '.join(sorted(deps)) or '-'}")
    for problem in problems:
        print(f"layers: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"layers: {len(graph)} crates follow the layer map")
    return 0


if __name__ == "__main__":
    sys.exit(main())
