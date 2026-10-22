#!/usr/bin/env python3
"""Package an already-signed iOS App plus its HTTPS OTA manifest and page.
Does not upload files, modify the signed App, or expand provisioning rights.
"""
import argparse
import re
from datetime import datetime, timezone
import hashlib
import html
from pathlib import Path
import plistlib
import shutil
import subprocess
import tempfile
import zipfile
from urllib.parse import quote, urlparse, urljoin


def render_page(info, profile, base, published_at, manifest_name, ipa_name, mac_version=None):
    """Render a fixed release instant in the visiting device's local timezone."""
    install = "itms-services://?action=download-manifest&url=" + quote(base + "/manifest.plist?build=" + str(info["CFBundleVersion"]), safe="")
    retry_manifest = manifest_name.replace("manifest-", "install-", 1)
    retry_install = "itms-services://?action=download-manifest&url=" + quote(base + "/" + retry_manifest, safe="")
    expiry = profile["ExpirationDate"].strftime("%Y-%m-%d")
    version = html.escape(str(info.get("CFBundleShortVersionString", "")))
    build = html.escape(str(info["CFBundleVersion"]))
    published_iso = published_at.astimezone(timezone.utc).isoformat(timespec="seconds")
    published_fallback = published_at.astimezone().strftime("%Y-%m-%d %H:%M:%S %Z")
    development = profile.get("Entitlements", {}).get("get-task-allow", False)
    note = "首次运行需在 iPhone 设置中开启开发者模式。" if development else ""
    mac_subtitle = "适用于 macOS 14 或更高版本"
    if mac_version:
        mac_subtitle = f"版本 {html.escape(mac_version)} · {mac_subtitle}"
    desktop = urljoin(base + "/", "../desktop/")
    mac_download = html.escape(desktop + "Muster-osx-arm64-Portable.zip" + ("?version=" + quote(mac_version, safe="") if mac_version else ""), quote=True)
    ipa_download = html.escape(base + "/Muster.ipa?build=" + quote(str(info["CFBundleVersion"]), safe=""), quote=True)
    checksums = html.escape(base + "/SHA256SUMS", quote=True)
    return f'''<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="color-scheme" content="light dark">
<meta name="description" content="下载最新版 Muster，适用于 Mac 和已登记的 iPhone、iPad。">
<meta http-equiv="Cache-Control" content="no-store">
<title>下载最新版 · Muster</title>
<style>
:root{{color-scheme:light dark;--bg:#f5f2ec;--card:#fffdf9;--ink:#272722;--muted:#64645d;--line:#ddd9d0;--accent:#8d4b32;--soft:#ebe6dc}}
*{{box-sizing:border-box}}
body{{margin:0;background:var(--bg);color:var(--ink);font:16px/1.65 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}}
a{{color:var(--accent);text-underline-offset:4px}}
a:focus-visible{{outline:3px solid var(--accent);outline-offset:5px}}
main{{max-width:980px;margin:auto;padding:48px 28px 32px}}
.brand{{display:flex;align-items:center;gap:12px;font-size:20px;font-weight:650;letter-spacing:-.5px}}
.mark{{display:grid;place-items:center;width:38px;height:38px;border-radius:12px;background:var(--accent);color:white;font-size:24px}}
header{{padding:60px 0 34px}}
.eyebrow{{font-size:12px;letter-spacing:2px;color:var(--muted);text-transform:uppercase}}
h1{{font-size:clamp(34px,6vw,54px);line-height:1.2;letter-spacing:-2px;margin:14px 0 18px;font-weight:650}}
header p{{color:var(--muted);font-size:18px;margin:0}}
.platforms{{display:grid;grid-template-columns:1fr 1fr;gap:20px}}
.card{{background:var(--card);border:1px solid var(--line);border-radius:22px;padding:30px;display:flex;flex-direction:column}}
.card-top{{display:flex;align-items:center;justify-content:space-between;gap:12px}}
.tag{{font-size:12px;color:var(--muted);background:var(--soft);padding:4px 10px;border-radius:20px;white-space:nowrap}}
h2{{font-size:27px;letter-spacing:-.7px;margin:0}}
.subtitle{{color:var(--muted);font-size:14px;margin:9px 0 24px}}
.install{{display:block;text-align:center;border-radius:12px;padding:13px 16px;background:transparent;border:1px solid var(--line);color:var(--ink);text-decoration:none;font-weight:600;min-height:50px}}
.install:hover{{filter:brightness(1.12)}}
.instructions{{color:var(--muted);font-size:14px;margin:20px 0 0}}
.details{{margin-top:auto;padding-top:22px;font-size:13px;color:var(--muted)}}
.details p{{margin:5px 0}}
footer{{padding:25px 2px 0;font-size:12px;color:var(--muted)}}
@media(max-width:640px){{main{{padding:28px 20px}}header{{padding:40px 0 28px}}.platforms{{grid-template-columns:1fr}}.card{{padding:25px}}}}
@media(prefers-color-scheme:dark){{:root{{--bg:#1e201e;--card:#272a27;--ink:#f4f2ec;--muted:#b4b7ad;--line:#3f443e;--accent:#c78969;--soft:#363b34}}.mark{{color:#1b201b}}}}
</style>
</head>
<body>
<main>
<div class="brand"><span class="mark" aria-hidden="true">M</span>Muster</div>
<header><div class="eyebrow">MUSTER / DOWNLOAD</div>
<h1>下载最新版 Muster</h1>
<p>下载入口始终提供当前最新发布版本。</p></header>
<div class="platforms">
<section class="card" aria-labelledby="mac-title">
<div class="card-top"><h2 id="mac-title">Mac</h2><span class="tag">Apple 芯片</span></div>
<p class="subtitle">{mac_subtitle}</p>
<a class="install" href="{mac_download}">下载 Mac 版</a>
<p class="instructions">下载后解压，将 Muster 拖入「应用程序」文件夹，再打开应用。</p>
<div class="details"><p>支持 M 系列芯片的 Mac。</p><p>已安装 Muster？下载后替换「应用程序」中的旧版本。</p></div>
</section>
<section class="card" aria-labelledby="ios-title">
<div class="card-top"><h2 id="ios-title">iPhone / iPad</h2><span class="tag">iOS 17+</span></div>
<p class="subtitle">开发测试版 · {version} · Build {build}</p>
<a class="install" href="{html.escape(install, quote=True)}">安装到此设备</a>
<p class="instructions">请用 Safari 打开。此入口仅支持已登记的测试设备。{note}</p>
<div class="details"><p>安装后仍是旧版？<a href="{html.escape(retry_install, quote=True)}">重新安装 {version}（{build}）</a>，完成后重新打开 Muster。</p>
<p>设备安装签名有效期至 {expiry}</p>
<p><a href="{ipa_download}">下载 IPA</a> · <a href="{checksums}">校验值</a></p></div>
</section>
</div>
<footer>iOS 版本发布时间：<time id="published-at" datetime="{published_iso}">{published_fallback}</time></footer>
</main>
<script>
const published = document.getElementById('published-at');
const instant = new Date(published.dateTime);
if (!Number.isNaN(instant.getTime())) {{
  published.textContent = new Intl.DateTimeFormat(undefined, {{
    year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', second: '2-digit',
    hour12: false, timeZoneName: 'short'
  }}).format(instant);
}}
</script>
</body></html>'''




def verify_ipa(ipa):
    """Verify ZIP contents without ditto silently consuming AppleDouble files."""
    with zipfile.ZipFile(ipa) as archive:
        metadata = [name for name in archive.namelist()
                    if any(part.startswith("._") or part == "__MACOSX"
                           for part in Path(name).parts)]
        if metadata:
            raise ValueError("IPA contains macOS metadata: " + ", ".join(metadata))
    with tempfile.TemporaryDirectory(prefix="muster-ota-verify-") as temporary:
        subprocess.run(["/usr/bin/unzip", "-q", str(ipa.resolve()), "-d", temporary], check=True)
        subprocess.run(["/usr/bin/codesign", "--verify", "--deep", "--strict",
                        str(Path(temporary) / "Payload" / "Muster.app")], check=True)


def package_signed_app(app, ipa):
    with tempfile.TemporaryDirectory(prefix="muster-ota-") as temporary:
        payload = Path(temporary) / "Payload"
        payload.mkdir()
        shutil.copytree(app, payload / "Muster.app", symlinks=True)
        # Match Xcode's IPA ZIP export: resource forks/xattrs must not become
        # unsigned ._* files inside the signed bundle on non-Mac extraction.
        subprocess.run(["/usr/bin/ditto", "-c", "-k", "--norsrc", "--keepParent",
                        str(payload), str(ipa.resolve())], check=True)
    verify_ipa(ipa)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("app", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("base_url", nargs="?", default="https://linsheng.dev:8443/muster/ios/",
                        help="HTTPS directory (default: the fixed Muster iOS release URL)")
    parser.add_argument("--mac-version", help="the Mac download's version, shown on the page (e.g. 1.0.36)")
    parser.add_argument("--published-at", type=datetime.fromisoformat,
                        help="the iOS release instant (ISO 8601) when rebuilding the page of an already published IPA")
    args = parser.parse_args()
    if args.mac_version and not re.fullmatch(r"\d+(\.\d+){1,3}", args.mac_version):
        parser.error("--mac-version must look like 1.0.36")
    if args.published_at and args.published_at.tzinfo is None:
        parser.error("--published-at needs a time zone, e.g. 2026-10-03T15:50:03+00:00")
    url = urlparse(args.base_url)
    if url.scheme != "https" or not url.netloc or url.query or url.fragment or url.username:
        parser.error("base_url must be a plain HTTPS directory URL")
    base = args.base_url.rstrip("/")
    app = args.app.resolve()
    subprocess.run(["/usr/bin/codesign", "--verify", "--deep", "--strict", str(app)], check=True)
    info = plistlib.loads((app / "Info.plist").read_bytes())
    profile = plistlib.loads(subprocess.check_output([
        "/usr/bin/security", "cms", "-D", "-i", str(app / "embedded.mobileprovision")
    ]))
    if not profile.get("ProvisionedDevices"):
        parser.error("this packager requires a registered-device provisioning profile")
    if info.get("CFBundleIdentifier") != "dev.linsheng.muster":
        parser.error("expected the Muster iOS App")
    args.output.mkdir(parents=True, exist_ok=False)
    ipa = args.output / "Muster.ipa"
    package_signed_app(app, ipa)
    # Fixed URLs can retain an older download across OTA install attempts.
    # Bind manifest assets to the exact IPA bytes. The page uses the latest
    # manifest alias; nginx redirects historical manifest links to that alias.
    checksum = hashlib.sha256(ipa.read_bytes()).hexdigest()
    ipa_name = f"Muster-{checksum}.ipa"
    manifest_name = f"manifest-{checksum}.plist"
    shutil.copyfile(ipa, args.output / ipa_name)
    manifest = {"items": [{
        "assets": [{"kind": "software-package", "url": base + "/" + ipa_name}],
        "metadata": {"bundle-identifier": info["CFBundleIdentifier"],
                     "bundle-version": info["CFBundleVersion"],
                     "kind": "software", "title": "Muster"},
    }]}
    manifest_data = plistlib.dumps(manifest)
    (args.output / manifest_name).write_bytes(manifest_data)
    (args.output / "manifest.plist").write_bytes(manifest_data)
    # A separate content-addressed path avoids the latest-alias redirect used
    # by historical manifest-* links, for retrying an unchanged installation.
    (args.output / f"install-{checksum}.plist").write_bytes(manifest_data)
    published_at = args.published_at or datetime.now(timezone.utc)
    page = render_page(info, profile, base, published_at, manifest_name, ipa_name, args.mac_version)
    (args.output / "index.html").write_text(page)
    (args.output / "SHA256SUMS").write_text(f"{checksum}  {ipa_name}\n{checksum}  Muster.ipa\n")
    expiry = profile["ExpirationDate"].strftime("%Y-%m-%d")
    print(f"Prepared {args.output}; {len(profile['ProvisionedDevices'])} registered devices; profile expires {expiry}")


if __name__ == "__main__":
    main()
