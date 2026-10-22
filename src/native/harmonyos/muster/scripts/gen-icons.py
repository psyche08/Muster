#!/usr/bin/env python3
"""Renders the HarmonyOS app icons from the Apple apps' icon source.

The mark comes from src/native/apple/MusterApp/Design/AppIcon.svg (an ink
square with the parchment arms and clay core), so every platform shows the
same icon. A HarmonyOS layered icon is the ink background and the mark as
foreground at the Apple icon's own proportions; the launcher masks the
whole canvas to its shape. The start window shows a plain image, so it
gets the tile with MusterMark's corner radius (229/1024).

Needs rsvg-convert (brew install librsvg). Usage: scripts/gen-icons.py
"""
import pathlib
import re
import subprocess
import tempfile

HERE = pathlib.Path(__file__).resolve().parent.parent
SOURCE = HERE.parents[2] / 'native/apple/MusterApp/Design/AppIcon.svg'
INK = '#2A2825'
MEDIA = [HERE / 'AppScope/resources/base/media', HERE / 'entry/src/main/resources/base/media']
LAYERED = '''{
  "layered-image": {
    "background": "$media:background",
    "foreground": "$media:foreground"
  }
}
'''


def svg(body: str, size: int = 1024) -> str:
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" '
            f'viewBox="0 0 1024 1024">{body}</svg>')


def render(markup: str, out: pathlib.Path, size: int) -> None:
    with tempfile.NamedTemporaryFile('w', suffix='.svg', delete=False) as f:
        f.write(markup)
    subprocess.run(['rsvg-convert', '-w', str(size), '-h', str(size), '-o', str(out), f.name], check=True)
    pathlib.Path(f.name).unlink()


def main() -> None:
    source = SOURCE.read_text()
    match = re.search(r'<g transform="translate\(512,512\)">(.*)</g></svg>', source, re.S)
    if match is None:
        raise SystemExit(f'{SOURCE}: the mark group was not found')
    mark = match.group(1)
    foreground = svg(f'<g transform="translate(512,512)">{mark}</g>')
    background = svg(f'<rect width="1024" height="1024" fill="{INK}"/>')
    start = svg(f'<rect width="1024" height="1024" rx="229" fill="{INK}"/>'
                f'<g transform="translate(512,512)">{mark}</g>', 288)
    for media in MEDIA:
        render(foreground, media / 'foreground.png', 1024)
        render(background, media / 'background.png', 1024)
        (media / 'layered_image.json').write_text(LAYERED)
    render(start, MEDIA[1] / 'startIcon.png', 288)


if __name__ == '__main__':
    main()
