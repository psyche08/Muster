import hashlib
import importlib.util
import json
from pathlib import Path
import plistlib
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location('prune', Path(__file__).parents[1] / 'prune-releases.py')
prune = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prune)


class PruneTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.ios, self.mac = self.root / 'ios', self.root / 'desktop'
        self.ios.mkdir(); self.mac.mkdir()
        with zipfile.ZipFile(self.ios / 'Muster.ipa', 'w') as z:
            z.writestr('Payload/Muster.app/Info.plist', plistlib.dumps({'CFBundleVersion': '30'}))
        data = (self.ios / 'Muster.ipa').read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        self.name = f'Muster-{digest}.ipa'
        (self.ios / self.name).write_bytes(data)
        manifest = {'items': [{'assets': [{'kind': 'software-package', 'url': 'https://linsheng.dev:8443/muster/ios/' + self.name}], 'metadata': {'bundle-version': '30'}}]}
        (self.ios / 'manifest.plist').write_bytes(plistlib.dumps(manifest))
        (self.ios / f'manifest-{digest}.plist').write_bytes(plistlib.dumps(manifest))
        (self.ios / f'install-{digest}.plist').write_bytes(plistlib.dumps(manifest))
        with zipfile.ZipFile(self.mac / 'Muster-osx-arm64-Portable.zip', 'w') as z:
            z.writestr('Muster.app/Contents/Info.plist', plistlib.dumps({'CFBundleShortVersionString': '1.0.40'}))

    def test_removes_old_packages_and_feed_without_touching_current_or_unrelated_files(self):
        old = [self.ios / ('Muster-' + 'a'*64 + '.ipa'), self.ios / ('manifest-' + 'a'*64 + '.plist'), self.ios / ('install-' + 'a'*64 + '.plist'), self.mac / 'Muster-1.0.28-osx-arm64-full.nupkg', self.mac / 'releases.osx-arm64.json', self.mac / 'releases.osx-arm64.json.sig']
        for p in old: p.write_text('old')
        old[-2].write_text(json.dumps({'Assets': [{'Version': '1.0.28'}]}))
        (self.mac / 'index.html').write_text('page')
        self.assertEqual(set(prune.obsolete_files(self.root)), set(old))
        self.assertTrue(all(p.exists() for p in old))  # dry-run is read-only

    def test_only_superseded_standalone_release_files_are_pruned(self):
        old = self.root / '1.0.39'
        current = self.root / '1.0.40'
        old.mkdir(); current.mkdir()
        files = ['index.html', 'install.plist', 'SHA256SUMS', 'Muster-1.0.39-macos-arm64.zip']
        for name in files:
            (old / name).write_text('old')
        (current / 'index.html').write_text('current')
        (old / 'unrelated.txt').write_text('keep')
        self.assertEqual(set(prune.obsolete_files(self.root)), {old / n for n in files})

    def test_refuses_cleanup_when_latest_alias_is_inconsistent(self):
        (self.ios / 'Muster.ipa').write_bytes(b'stale')
        with self.assertRaisesRegex(ValueError, 'alias/hash mismatch'):
            prune.obsolete_files(self.root)

    def test_keeps_current_feed_and_verified_package(self):
        p = self.mac / 'Muster-1.0.40-osx-arm64-full.nupkg'
        p.write_bytes(b'current')
        (self.mac / 'releases.osx-arm64.json').write_text(json.dumps({'Assets': [{'Version': '1.0.40', 'FileName': p.name, 'Size': p.stat().st_size, 'SHA256': hashlib.sha256(p.read_bytes()).hexdigest()}]}))
        self.assertEqual(prune.obsolete_files(self.root), [])


if __name__ == '__main__':
    unittest.main()
