#!/usr/bin/env python3
"""Sign a Velopack feed with an offline P-256 release key; never copy the key."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument('feed', type=Path)
parser.add_argument('--key', type=Path, required=True)
args = parser.parse_args()
if args.feed.name not in {f'releases.{p}.json' for p in ('osx-arm64', 'osx-x64', 'win-arm64', 'win-x64')}:
    parser.error('expected a platform/architecture release feed')
data = args.feed.read_bytes()
feed = json.loads(data)
# Only full packages are allowed by the updater. Publish packages before feed.
import hashlib
for asset in feed['Assets']:
    name = asset['FileName']
    if Path(name).name != name or '/' in name or '\\' in name or asset['PackageId'] != 'Muster' or asset['Type'] != 'Full':
        parser.error('unexpected update asset')
    package = args.feed.parent / name
    if package.stat().st_size != asset['Size'] or hashlib.file_digest(package.open('rb'), 'sha256').hexdigest().lower() != asset['SHA256'].lower():
        parser.error('package size/checksum mismatch')
with tempfile.TemporaryDirectory(prefix='muster-feed-') as directory:
    payload = Path(directory) / 'payload'
    payload.write_bytes(args.feed.name.encode() + b'\n' + data)
    subprocess.run(['openssl', 'dgst', '-sha256', '-sign', str(args.key), '-out', str(args.feed) + '.sig', str(payload)], check=True)
    public_key = Path(__file__).resolve().parents[2] / 'crates/muster-app-core/src/desktop_update/release-public.pem'
    subprocess.run(['openssl', 'dgst', '-sha256', '-verify', str(public_key), '-signature', str(args.feed) + '.sig', str(payload)], check=True)
print(f'Signed {args.feed.name}: {len(feed["Assets"])} full packages')
