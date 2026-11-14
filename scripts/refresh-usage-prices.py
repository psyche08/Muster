#!/usr/bin/env python3
"""Refresh Muster's offline estimates from a reviewed ccusage Git revision."""

import argparse
import json
import re
import subprocess
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("checkout", type=Path, help="path to a ccusage Git checkout")
parser.add_argument("revision", help="ccusage tag or commit to import")
args = parser.parse_args()


def git(*arguments):
    return subprocess.check_output(
        ["git", "-C", str(args.checkout), *arguments], text=True
    ).strip()


commit = git("rev-parse", "--verify", "--end-of-options", args.revision + "^{commit}")
crates = "rust/crates"
catalog = f"{crates}/ccusage-pricing-snapshots/src/models-dev-pricing.json"
upstream = json.loads(git("show", f"{commit}:{catalog}"))
destination = Path(__file__).resolve().parents[1] / "src/muster/crates/muster-agent/src"
prices_path = destination / "usage-prices.json"
# Delisted model IDs still occur in saved transcripts. Retain their last entry.
prices = json.loads(prices_path.read_text())
for model, entry in upstream.items():
    if "/" not in model and re.match(r"^(gpt-|claude-|o[1-9](?:$|-))", model):
        prices[model] = entry

# The November catalog omits this official ID. Prefer upstream once it is added.
# https://platform.claude.com/docs/en/models/mythos-5-2/overview
if "claude-mythos-5-2" not in upstream:
    prices["claude-mythos-5-2"] = {
        "cost": {"input": 8, "output": 40, "cache_read": 0.2, "cache_write": 10},
        "limit": {"context": 1_000_000},
    }

speeds = {}
for speed in ("fast", "flex", "ultrafast"):
    path = f"{crates}/ccusage-core/src/{speed}-multiplier-overrides.json"
    speeds[speed] = json.loads(git("show", f"{commit}:{path}"))

# Claude publishes these exact fast-mode multipliers separately.
# https://platform.claude.com/docs/en/build-with-claude/fast-mode
for model, multiplier in {
    "claude-opus-5": 2,
    "claude-opus-5-5": 2,
    "claude-fable-5-2": 2,
    "claude-mythos-5-2": 2,
}.items():
    speeds["fast"]["exact"].setdefault(model, multiplier)
metadata = {
    "revision": commit,
    "date": git("show", "-s", "--format=%cs", commit),
    "speeds": speeds,
}
prices_path.write_text(json.dumps(prices, indent=2, sort_keys=True) + "\n")
(destination / "usage-price-tiers.json").write_text(json.dumps(metadata, indent=2) + "\n")
