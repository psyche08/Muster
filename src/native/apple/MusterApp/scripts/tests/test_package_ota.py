import importlib.util
from datetime import datetime, timezone
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('package_ota', Path(__file__).parents[1] / 'package-ota.py')
ota = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ota)

INFO = {'CFBundleShortVersionString': '1.0.36', 'CFBundleVersion': '26'}
PROFILE = {'ExpirationDate': datetime(2027, 9, 30, tzinfo=timezone.utc), 'Entitlements': {'get-task-allow': True}}
AT = datetime(2026, 10, 3, 15, 50, 3, tzinfo=timezone.utc)


def page(mac_version=None):
    return ota.render_page(INFO, PROFILE, 'https://linsheng.dev:8443/muster/ios', AT,
                           'manifest-abc.plist', 'Muster-abc.ipa', mac_version)


class PageTests(unittest.TestCase):
    def test_both_cards_show_their_versions(self):
        html = page('1.0.36')
        self.assertIn('版本 1.0.36 · 适用于 macOS 14 或更高版本', html)
        self.assertIn('开发测试版 · 1.0.36 · Build 26', html)
        self.assertIn('2026-10-03T15:50:03+00:00', html)

    def test_install_links_always_resolve_latest_aliases(self):
        from html import unescape
        from urllib.parse import parse_qs, urlparse
        import re
        links = [unescape(x) for x in re.findall(r'href="([^"]+)"', page('1.0.40'))]
        install = next(x for x in links if x.startswith('itms-services:'))
        self.assertEqual(parse_qs(urlparse(install).query)['url'],
                         ['https://linsheng.dev:8443/muster/ios/manifest.plist?build=26'])
        self.assertIn('https://linsheng.dev:8443/muster/ios/Muster.ipa?build=26', links)
        self.assertIn('https://linsheng.dev:8443/muster/desktop/Muster-osx-arm64-Portable.zip?version=1.0.40', links)
        self.assertNotIn('manifest-abc.plist', page())
        self.assertNotIn('Muster-abc.ipa', page())
        self.assertNotIn('TestFlight', page())

    def test_retry_uses_a_distinct_content_addressed_manifest(self):
        from html import unescape
        from urllib.parse import parse_qs, urlparse
        import re
        links = [unescape(x) for x in re.findall(r'href="([^"]+)"', page())]
        installs = [parse_qs(urlparse(x).query)['url'][0]
                    for x in links if x.startswith('itms-services:')]
        self.assertEqual(installs, [
            'https://linsheng.dev:8443/muster/ios/manifest.plist?build=26',
            'https://linsheng.dev:8443/muster/ios/install-abc.plist'])
        self.assertIn('重新安装 1.0.36（26）', page())

    def test_without_a_mac_version_the_mac_card_is_unchanged(self):
        html = page()
        self.assertIn('<p class="subtitle">适用于 macOS 14 或更高版本</p>', html)
        self.assertNotIn('版本 ', html)


if __name__ == '__main__':
    unittest.main()
