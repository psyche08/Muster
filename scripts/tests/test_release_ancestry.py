import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('ancestry', ROOT / 'src/muster/deploy/desktop/verify-release-ancestry.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class ReleaseAncestryTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.repo = Path(self.tmp.name)
        self.env = {k: v for k, v in os.environ.items() if not k.startswith('GIT_')}
        self.run_git('init', '-q', '-b', 'main')
        self.run_git('config', 'user.name', 'Example')
        self.run_git('config', 'user.email', 'developer@example.invalid')
        self.run_git('config', 'commit.gpgsign', 'false')

    def run_git(self, *args):
        return subprocess.check_output(['git', '-C', str(self.repo), *args], env=self.env,
                                       stderr=subprocess.DEVNULL).decode().strip()

    def put(self, name, value):
        path = self.repo / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(value)
        self.run_git('add', name)

    def commit(self):
        self.run_git('commit', '-qm', 'fixture')
        return self.run_git('rev-parse', 'HEAD')

    def snapshot(self, bad_digest=False):
        self.put('source.txt', 'reviewed source')
        self.put(m.LEDGER, '1.0.1 ' + 'a' * 40 + ' historical release\n')
        self.put(m.BASELINE, '{}')
        digest = m.tree_digest(self.repo, self.run_git('write-tree'))
        self.put(m.BASELINE, json.dumps({'schema': 1, 'archived_source': 'b' * 40,
                                        'root_tree_sha256': '0' * 64 if bad_digest else digest}))
        return self.commit()

    def test_bound_root_accepts_only_original_version_and_ref_pairs(self):
        head = self.snapshot()
        m.verify(self.repo, head, '1.0.1 ' + 'a' * 40)
        for row in ['1.0.2 ' + 'a' * 40, '1.0.1 ' + 'c' * 40]:
            with self.assertRaises(ValueError):
                m.verify(self.repo, head, row)

    def test_changed_root_digest_is_rejected(self):
        head = self.snapshot(bad_digest=True)
        with self.assertRaisesRegex(ValueError, 'baseline'):
            m.verify(self.repo, head, '1.0.1 ' + 'a' * 40)

    def test_future_release_requires_actual_ancestry(self):
        root = self.snapshot()
        self.put('later.txt', 'next source')
        later = self.commit()
        m.verify(self.repo, later, '1.0.1 ' + 'a' * 40 + '\n1.0.2 ' + root)
        with self.assertRaises(ValueError):
            m.verify(self.repo, root, '1.0.2 ' + later)

    def test_changing_current_manifest_cannot_expand_historical_allowance(self):
        self.snapshot()
        self.put(m.BASELINE, json.dumps({'schema': 1, 'archived_source': 'c' * 40}))
        self.put(m.LEDGER, '1.0.1 ' + 'a' * 40 + '\n1.0.2 ' + 'c' * 40)
        head = self.commit()
        with self.assertRaises(ValueError):
            m.verify(self.repo, head, (self.repo / m.LEDGER).read_text())

    def test_merging_old_history_is_rejected(self):
        self.snapshot()
        self.run_git('checkout', '--orphan', 'unrelated')
        self.run_git('rm', '-rf', '.')
        self.put('other.txt', 'unrelated history')
        other = self.commit()
        self.run_git('checkout', 'main')
        self.run_git('merge', '--allow-unrelated-histories', '-qm', 'merge fixture', other)
        with self.assertRaisesRegex(ValueError, 'one root'):
            m.verify(self.repo, 'HEAD', '1.0.1 ' + 'a' * 40)

    def test_legacy_repo_requires_ancestry_without_baseline(self):
        self.put('source.txt', 'original source')
        root = self.commit()
        m.verify(self.repo, root, '1.0.1 ' + root)
        with self.assertRaises(ValueError):
            m.verify(self.repo, root, '1.0.1 ' + 'a' * 40)

    def test_malformed_and_duplicate_ledgers_are_rejected(self):
        for value in ['', '1.0.1 not-a-ref', '1.0.1 abcdef0\n1.0.1 abcdef1']:
            with self.assertRaises(ValueError):
                m.releases(value)


if __name__ == '__main__':
    unittest.main()
