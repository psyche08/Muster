"""Exercise the built private macOS host without launching a stream or sending input."""
import argparse
import os
from pathlib import Path
import secrets
import select
import socket
import ssl
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
import xml.etree.ElementTree as ET


def issue(directory, name):
    cert = directory / (name + ".pem")
    key = directory / (name + ".key")
    subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes",
                    "-subj", "/CN=localhost", "-days", "1", "-keyout", key, "-out", cert,
                    "-addext", "subjectAltName=IP:127.0.0.1"],
                   check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return cert, key


def run(executable, directory):
    server, key = issue(directory, "host")
    viewer, viewer_key = issue(directory, "viewer")
    other, other_key = issue(directory, "other")
    port = 40000 + secrets.randbelow(5000)
    config = directory / "sunshine.conf"
    (directory / "apps.json").write_text('{"apps":[],"env":{}}')
    config.write_text(f"port = {port}\npkey = {key}\ncert = {server}\nfile_apps = {directory}/apps.json\n")
    env = dict(os.environ, MUSTER_DESKTOP_SESSION_DIR=str(directory))
    with (directory / "host.log").open("wb") as log:
        child = subprocess.Popen([executable, "--muster-managed", config], cwd=directory, stdin=subprocess.PIPE,
                                 stdout=subprocess.PIPE, stderr=log, env=env)
        try:
            pem = viewer.read_bytes()
            child.stdin.write(b"MST1" + bytes([0]) + len(pem).to_bytes(2, "big") + pem)
            child.stdin.flush()

            def https(path, cert, private_key):
                context = ssl.create_default_context(cafile=str(server))
                context.load_cert_chain(cert, private_key)
                url = f"https://127.0.0.1:{port - 5}/{path}"
                with urllib.request.urlopen(url, context=context, timeout=2) as response:
                    return ET.fromstring(response.read(65536))

            deadline = time.monotonic() + 35
            while True:
                try:
                    apps = https("applist", viewer, viewer_key)
                    break
                except (OSError, urllib.error.URLError):
                    if child.poll() is not None or time.monotonic() > deadline:
                        raise
                    time.sleep(0.25)
            assert apps.get("status_code") == "200"
            assert [(app.findtext("AppTitle"), app.findtext("ID")) for app in apps.findall("App")] == [("Desktop", "0")]
            try:
                unauthorized = https("applist", other, other_key)
                assert unauthorized.get("status_code") == "401"
            except (ssl.SSLError, ConnectionError, urllib.error.URLError):
                pass
            args = "appid=1&rikeyid=1&rikey=" + "00" * 16 + "&localAudioPlayMode=0"
            assert https("launch?" + args, viewer, viewer_key).get("status_code") == "403"
            for url in (f"http://127.0.0.1:{port}/pair", f"https://127.0.0.1:{port - 5}/pair"):
                context = ssl.create_default_context(cafile=str(server))
                context.load_cert_chain(viewer, viewer_key)
                try:
                    with urllib.request.urlopen(url, context=context, timeout=2) as response:
                        assert ET.fromstring(response.read(65536)).get("status_code") == "404"
                except urllib.error.HTTPError as error:
                    assert error.code == 404
            child.stdin.write(b"P")
            child.stdin.flush()
            assert select.select([child.stdout], [], [], 10)[0], "host did not park"
            assert child.stdout.read(1) == b"I"
            with socket.socket() as probe:
                assert probe.connect_ex(("127.0.0.1", port - 5)) != 0, "idle host still listening"
            pem = other.read_bytes()
            child.stdin.write(b"RMST1" + bytes([0]) + len(pem).to_bytes(2, "big") + pem)
            child.stdin.flush()
            deadline = time.monotonic() + 35
            while True:
                try:
                    assert https("applist", other, other_key).get("status_code") == "200"
                    break
                except (OSError, urllib.error.URLError):
                    if child.poll() is not None or time.monotonic() > deadline:
                        raise
                    time.sleep(0.25)
            try:
                assert https("applist", viewer, viewer_key).get("status_code") == "401"
            except (ssl.SSLError, ConnectionError, urllib.error.URLError):
                pass
            child.stdin.close()
            assert child.wait(timeout=10) == 0
            logs = (directory / "host.log").read_bytes()
            if b"No screen capture permission!" in logs:
                print("Muster Screen Recording permission is required for live capture.")
            print("PASS: pinned host, selected viewer only, desktop only, idle reset, fresh viewer certificate, owner EOF")
        finally:
            if not child.stdin.closed:
                child.stdin.close()
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("host", type=Path)
    parser.add_argument("--expect-bundle-id", help="check the signed app layout and enclosing identity")
    args = parser.parse_args()
    host = args.host.resolve(strict=True)
    if args.expect_bundle_id:
        app = host.parents[2]
        assert host == app / "Contents/MacOS/muster-desktop-host"
        assert not list(app.rglob("Sunshine.app"))
        assert subprocess.check_output([host, "--muster-bundle-id"], text=True).strip() == args.expect_bundle_id
        subprocess.run(["codesign", "--verify", "--strict", "--deep", app], check=True)
    with tempfile.TemporaryDirectory(prefix="muster-host-test-") as root:
        run(host, Path(root))
