"""Builds the app icon (Thread) at every size the app and the website ship.

The mark is drawn in the 256 design grid, where the tile spans 12..244. Three cuts of it exist: the regular one,
a middle one for 24 to 32 px and a bold one for 16 to 20 px, each with a thicker thread and a larger drop.
Output goes to ../final; install.py copies the files into place.
"""
import os
import struct
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.expanduser(
    "~/.claude/plugins/cache/logo-design-skill/logo-design/1.4.4/skills/logo-design/scripts"))
import render_png  # noqa: E402
from build2 import thread  # noqa: E402
from geo import f  # noqa: E402

OUT = os.path.join(HERE, "..", "final")
PNG = os.path.join(OUT, "png")
PEARL = "#F4F3F0"  # crates/paint/src/color.rs

REGULAR = thread(46, 54, 164, 50, 128, 180, 19.5, 9, 21, 15)
# the island of the two small cuts runs from y 55.5 to 113.5, which lands on whole pixels at 16 and at 32 px
MIDDLE = thread(42, 55.5, 172, 58, 128, 182, 22, 13, 17, 12)
BOLD = thread(34, 55.5, 188, 58, 128, 183, 25, 18, 13, 9)
SPAN = {REGULAR: (54, 199.5), MIDDLE: (55.5, 204), BOLD: (55.5, 208)}


def cut_for(tile_px):
    if tile_px <= 22:
        return BOLD
    if tile_px <= 34:
        return MIDDLE
    return REGULAR


def squircle(x, y, s, inset=0.0):
    x, y, s = x + inset, y + inset, s - 2 * inset
    r = s * 52 / 232
    e = r * 1.52
    c = e * 0.34
    x1, y1 = x + s, y + s
    return (
        f"M{f(x + e)} {f(y)}H{f(x1 - e)}C{f(x1 - c)} {f(y)} {f(x1)} {f(y + c)} {f(x1)} {f(y + e)}"
        f"V{f(y1 - e)}C{f(x1)} {f(y1 - c)} {f(x1 - c)} {f(y1)} {f(x1 - e)} {f(y1)}"
        f"H{f(x + e)}C{f(x + c)} {f(y1)} {f(x)} {f(y1 - c)} {f(x)} {f(y1 - e)}"
        f"V{f(y + e)}C{f(x)} {f(y + c)} {f(x + c)} {f(y)} {f(x + e)} {f(y)}Z"
    )


def icon(canvas, tile, shadow=False, square=False):
    """One icon as SVG: a `tile` px tile centred on a `canvas` px square. `square` fills the whole canvas instead."""
    x = (canvas - tile) / 2
    k = tile / 232
    mark = cut_for(tile)
    top, bottom = SPAN[mark]
    small = tile <= 34
    rim = max(1.0, tile * 0.006)
    body = f'<rect width="{canvas}" height="{canvas}"/>' if square else f'<path d="{squircle(x, x, tile)}"/>'
    edge = "" if square else (
        f'<path d="{squircle(x, x, tile, rim / 2)}" fill="none" stroke="url(#rim)" stroke-width="{f(rim)}"/>'
    )
    glow = "" if small else f'<g fill="url(#glow)">{body}</g>'
    # the shadow is its own blurred shape under the tile, so the tile's dark tones never pass through a filter
    drop = canvas * 0.0117
    blur = (
        '<filter id="cast" x="-20%" y="-20%" width="140%" height="150%" color-interpolation-filters="sRGB">'
        f'<feGaussianBlur stdDeviation="{f(drop)}"/></filter>'
    )
    shade = f'<path d="{squircle(x, x + drop, tile)}" fill="#000" fill-opacity="0.3" filter="url(#cast)"/>'
    if small:
        tile_stops = '<stop offset="0" stop-color="#19191B"/><stop offset="1" stop-color="#101012"/>'
        rim_top, rim_bottom = 0.2, 0.1
        ink = PEARL
    else:
        tile_stops = (
            '<stop offset="0" stop-color="#1C1C1F"/><stop offset="0.5" stop-color="#0E0E10"/>'
            '<stop offset="1" stop-color="#060607"/>'
        )
        rim_top, rim_bottom = 0.22, 0.05
        ink = "url(#pearl)"
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {canvas} {canvas}" width="{canvas}" height="{canvas}">'
        "<defs>"
        f'<linearGradient id="tile" gradientUnits="userSpaceOnUse" x1="0" y1="{f(x)}" x2="0" y2="{f(x + tile)}">'
        f"{tile_stops}</linearGradient>"
        f'<radialGradient id="glow" gradientUnits="userSpaceOnUse" cx="{f(canvas / 2)}" cy="{f(x)}" r="{f(tile * 0.7)}">'
        '<stop offset="0" stop-color="#fff" stop-opacity="0.07"/><stop offset="1" stop-color="#fff" stop-opacity="0"/>'
        "</radialGradient>"
        f'<linearGradient id="rim" gradientUnits="userSpaceOnUse" x1="0" y1="{f(x)}" x2="0" y2="{f(x + tile)}">'
        f'<stop offset="0" stop-color="#fff" stop-opacity="{rim_top}"/>'
        f'<stop offset="1" stop-color="#fff" stop-opacity="{rim_bottom}"/></linearGradient>'
        f'<linearGradient id="pearl" gradientUnits="userSpaceOnUse" x1="0" y1="{top}" x2="0" y2="{bottom}">'
        '<stop offset="0" stop-color="#FBFAF8"/><stop offset="1" stop-color="#E9E6DF"/></linearGradient>'
        f'{blur if shadow else ""}</defs>'
        f'{shade if shadow else ""}<g fill="url(#tile)">{body}</g>{glow}{edge}'
        f'<path transform="translate({f(x)} {f(x)}) scale({k:.5f}) translate(-12 -12)" fill="{ink}" d="{mark}"/>'
        "</svg>\n"
    )


def render(name, svg, px):
    os.makedirs(PNG, exist_ok=True)
    src = os.path.join(PNG, name + ".svg")
    out = os.path.join(PNG, name + ".png")
    with open(src, "w") as fh:
        fh.write(svg)
    render_png.render(src, out, px, px, backend="chrome")
    os.remove(src)
    return out


def write_ico(pngs, path):
    blobs = [(px, open(p, "rb").read()) for px, p in pngs]
    head = struct.pack("<HHH", 0, 1, len(blobs))
    offset = 6 + 16 * len(blobs)
    entries, data = b"", b""
    for px, blob in blobs:
        entries += struct.pack("<BBBBHHII", px % 256, px % 256, 0, 0, 1, 32, len(blob), offset)
        offset += len(blob)
        data += blob
    with open(path, "wb") as fh:
        fh.write(head + entries + data)


def main():
    os.makedirs(OUT, exist_ok=True)
    # masters: the macOS grid (an 824 tile with its shadow) and the full-bleed tile
    render("macos-1024", icon(1024, 824, shadow=True), 1024)
    render("full-1024", icon(1024, 968), 1024)
    with open(os.path.join(OUT, "notchbase-macos.svg"), "w") as fh:
        fh.write(icon(1024, 824, shadow=True))
    with open(os.path.join(OUT, "notchbase.svg"), "w") as fh:
        fh.write(icon(1024, 968))

    # Windows: ten sizes, the tile nearly filling the square
    ico = []
    for px in (16, 20, 24, 32, 40, 48, 64, 96, 128, 256):
        tile = px if px <= 32 else round(px * 0.97 / 2) * 2
        ico.append((px, render(f"ico-{px}", icon(px, tile), px)))
    write_ico(ico, os.path.join(OUT, "notchbase.ico"))

    # macOS: the iconset, on the grid, a little fuller at the two smallest sizes
    iconset = os.path.join(OUT, "notchbase.iconset")
    os.makedirs(iconset, exist_ok=True)
    grid = {16: 14, 32: 28, 64: 52, 128: 103, 256: 206, 512: 412, 1024: 824}
    for px, tile in grid.items():
        render(f"mac-{px}", icon(px, tile, shadow=px >= 64), px)
    names = {
        "icon_16x16": 16, "icon_16x16@2x": 32, "icon_32x32": 32, "icon_32x32@2x": 64, "icon_128x128": 128,
        "icon_128x128@2x": 256, "icon_256x256": 256, "icon_256x256@2x": 512, "icon_512x512": 512,
        "icon_512x512@2x": 1024,
    }
    for name, px in names.items():
        with open(os.path.join(PNG, f"mac-{px}.png"), "rb") as src:
            with open(os.path.join(iconset, name + ".png"), "wb") as dst:
                dst.write(src.read())
    subprocess.run(["iconutil", "-c", "icns", iconset, "-o", os.path.join(OUT, "notchbase.icns")], check=True)

    # the website's touch icon: a full square, since iOS cuts its own corners
    render("apple-icon-180", icon(180, 180, square=True), 180)
    print("built")


if __name__ == "__main__":
    main()
