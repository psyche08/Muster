import importlib.util
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
import zipfile

spec = importlib.util.spec_from_file_location('package_ota', Path(__file__).parents[1] / 'package-ota.py')
ota = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ota)


class ArchiveTests(unittest.TestCase):
    def test_rejects_appledouble_before_extraction(self):
        with tempfile.TemporaryDirectory() as directory:
            ipa = Path(directory) / 'bad.ipa'
            with zipfile.ZipFile(ipa, 'w') as archive:
                archive.writestr('Payload/Muster.app/._Info.plist', b'metadata')
            with self.assertRaisesRegex(ValueError, 'macOS metadata'):
                ota.verify_ipa(ipa)

    @unittest.skipUnless(sys.platform == 'darwin', 'requires macOS codesign and ditto')
    def test_signed_app_with_xattrs_survives_plain_zip_extraction(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            app = root / 'Muster.app'
            contents = app / 'Contents'
            (contents / 'MacOS').mkdir(parents=True)
            (contents / 'Resources').mkdir()
            shutil.copyfile('/usr/bin/true', contents / 'MacOS' / 'Fixture')
            os.chmod(contents / 'MacOS' / 'Fixture', 0o755)
            (contents / 'Info.plist').write_bytes(plistlib.dumps({
                'CFBundleIdentifier': 'dev.linsheng.muster.ota-test',
                'CFBundleExecutable': 'Fixture', 'CFBundlePackageType': 'APPL'}))
            resource = contents / 'Resources' / 'test.txt'
            resource.write_text('signed resource')
            subprocess.run(['/usr/bin/codesign', '--force', '--sign', '-', str(app)], check=True,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            subprocess.run(['/usr/bin/xattr', '-w', 'com.muster.ota-test',
                            'archive regression', str(resource)], check=True)
            # The old ditto invocation must reproduce the defect with this
            # fixture, otherwise the passing clean-archive check is vacuous.
            run = subprocess.run
            def legacy_ditto(args, **kwargs):
                return run([arg for arg in args if arg != '--norsrc'], **kwargs)
            with mock.patch.object(ota.subprocess, 'run', side_effect=legacy_ditto):
                with self.assertRaisesRegex(ValueError, 'macOS metadata'):
                    ota.package_signed_app(app, root / 'legacy.ipa')
            ipa = root / 'Muster.ipa'
            ota.package_signed_app(app, ipa)
            with zipfile.ZipFile(ipa) as archive:
                self.assertEqual(archive.read('Payload/Muster.app/Contents/Resources/test.txt'),
                                 b'signed resource')
                self.assertFalse(any(Path(n).name.startswith('._') for n in archive.namelist()))


if __name__ == '__main__':
    unittest.main()
