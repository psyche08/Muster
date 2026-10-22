import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "check-public-content.py"
spec = importlib.util.spec_from_file_location("public_content", SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class PublicContentTests(unittest.TestCase):
    def test_examples_and_product_identity_are_allowed(self):
        text = '/Users/developer/project operator@example.invalid git@github.com icon_16x16@2x.png'
        text += ' dev.linsheng.muster 89LGY6BD53 https://linsheng.dev:8443/muster/'
        self.assertEqual(list(module.findings(text)), [])

    def test_identity_and_credential_candidates_are_reported_without_values(self):
        candidate = 'fake-person' + '@' + 'mail.test'
        text = '\n' + candidate + '\n/Users/' + 'private-user/project'
        text += '\n' + 'ghp_' + 'x' * 36
        self.assertEqual(list(module.findings(text)), [
            (4, 'access-token'), (3, 'personal-home-path'), (2, 'contact-email')])

    def test_key_marker_is_rejected_but_public_key_is_allowed(self):
        private = '-----BEGIN ' + 'PRIVATE KEY-----'
        self.assertEqual(list(module.findings(private)), [(1, 'private-key')])
        self.assertEqual(list(module.findings('-----BEGIN PUBLIC KEY-----')), [])

    def test_only_tracked_text_is_scanned_and_symlinks_are_not_followed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            subprocess.run(['git', 'init', '-q', tmp], check=True)
            secret = 'ghp_' + 'y' * 36
            (root / 'untracked.txt').write_text(secret)
            (root / 'local-link').symlink_to(root / 'untracked.txt')
            (root / 'binary').write_bytes(b'\0' + secret.encode())
            (root / 'tracked.txt').write_text('safe example')
            subprocess.run(['git', '-C', tmp, 'add', 'tracked.txt', 'local-link', 'binary'], check=True)
            env = {k: v for k, v in os.environ.items() if not k.startswith('GIT_')}
            result = subprocess.run(['python3', str(SCRIPT)], cwd=tmp, env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            (root / 'tracked.txt').write_text(secret)
            result = subprocess.run(['python3', str(SCRIPT)], cwd=tmp, env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 1)
            self.assertIn('tracked.txt:1: access-token', result.stdout)
            self.assertNotIn(secret, result.stdout + result.stderr)


if __name__ == '__main__':
    unittest.main()
