#!/usr/bin/env python3
"""Remove superseded public releases after validating the current aliases.
Dry-run by default. Run on the publisher after switching both platforms:
  python3 prune-releases.py "${MUSTER_RELEASE_ROOT:?set private release root}" --apply
"""
import argparse
import hashlib
import json
from pathlib import Path
import plistlib
import re
from urllib.parse import urlparse
import zipfile


def obsolete_files(root):
    ios, desktop = root / 'ios', root / 'desktop'
    manifest = plistlib.loads((ios / 'manifest.plist').read_bytes())
    item = manifest['items'][0]
    asset = next(a['url'] for a in item['assets'] if a['kind'] == 'software-package')
    name = Path(urlparse(asset).path).name
    if not re.fullmatch(r'Muster-[a-f0-9]{64}\.ipa', name):
        raise ValueError('latest manifest must reference a hash-named IPA')
    digest = hashlib.sha256((ios / name).read_bytes()).hexdigest()
    if name != f'Muster-{digest}.ipa' or hashlib.sha256((ios / 'Muster.ipa').read_bytes()).hexdigest() != digest:
        raise ValueError('latest IPA alias/hash mismatch; refusing cleanup')
    with zipfile.ZipFile(ios / name) as z:
        info = plistlib.loads(z.read('Payload/Muster.app/Info.plist'))
    if info['CFBundleVersion'] != item['metadata']['bundle-version']:
        raise ValueError('latest manifest/build mismatch')
    with zipfile.ZipFile(desktop / 'Muster-osx-arm64-Portable.zip') as z:
        mac = plistlib.loads(z.read('Muster.app/Contents/Info.plist'))
    version = mac['CFBundleShortVersionString']
    keep = {name, f'manifest-{digest}.plist', f'install-{digest}.plist'}
    obsolete = [p for p in ios.iterdir() if re.fullmatch(r'(Muster-[a-f0-9]{64}\.ipa|(?:manifest|install)-[a-f0-9]{64}\.plist)', p.name) and p.name not in keep]
    feed = desktop / 'releases.osx-arm64.json'
    packages = set()
    if feed.exists():
        assets = json.loads(feed.read_bytes())['Assets']
        if assets and all(a['Version'] == version for a in assets):
            for a in assets:
                filename = a['FileName']
                if Path(filename).name != filename:
                    raise ValueError('invalid feed filename')
                p = desktop / filename
                if p.stat().st_size != a['Size'] or hashlib.sha256(p.read_bytes()).hexdigest().lower() != a['SHA256'].lower():
                    raise ValueError('latest desktop feed/package mismatch')
                packages.add(filename)
    if not packages:
        obsolete += [desktop / n for n in ('releases.osx-arm64.json', 'releases.osx-arm64.json.sig', 'RELEASES-osx-arm64', 'assets.osx-arm64.json') if (desktop / n).exists()]
    obsolete += [p for p in desktop.glob('Muster-*-osx-arm64-full.nupkg') if p.name not in packages]
    # Standalone version pages are available only while that release is current.
    for directory in root.iterdir():
        if directory.is_symlink() or not directory.is_dir() or not re.fullmatch(r'\d+\.\d+\.\d+', directory.name):
            continue
        if directory.name != version:
            obsolete += [p for p in directory.iterdir() if p.is_file() and (
                p.name in {'index.html', 'install.plist', 'SHA256SUMS'}
                or re.fullmatch(r'Muster-[0-9.]+-macos-arm64\.zip', p.name))]
    return sorted(set(obsolete))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('site_root', type=Path)
    parser.add_argument('--apply', action='store_true')
    args = parser.parse_args()
    paths = obsolete_files(args.site_root)
    for path in paths:
        if args.apply:
            path.unlink()
        print(('removed ' if args.apply else 'would remove ') + str(path))
    print(f'{len(paths)} superseded files; latest aliases preserved')


if __name__ == '__main__':
    main()
