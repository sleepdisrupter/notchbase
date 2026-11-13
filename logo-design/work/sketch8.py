"""Round 4: hybrids of A (liquid island and drop) and C (strict geometry, the wordmark)."""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from build import INK, PEARL, TILE
from geo import f, island_with_drop, pill, svg
from type import ASC, GAPS, S as STEM, wordmark

OUT = os.path.join(os.path.dirname(__file__), "sketches")


def path(d, fill=INK):
    return f'<path fill="{fill}" d="{d}"/>'


def teardrop(cx, cy, r, h=None):
    """A circle with two tangents meeting at a point above it. h = 2r gives a 60 degree tip."""
    h = h or 2 * r
    th = math.acos(r / h)
    tx, ty = r * math.sin(th), r * math.cos(th)
    return (
        f"M{f(cx)} {f(cy - h)}L{f(cx + tx)} {f(cy - ty)}"
        f"A{f(r)} {f(r)} 0 1 1 {f(cx - tx)} {f(cy - ty)}Z"
    )


def circle(cx, cy, r):
    return f"M{f(cx - r)} {f(cy)}a{f(r)} {f(r)} 0 1 0 {f(2 * r)} 0a{f(r)} {f(r)} 0 1 0 {f(-2 * r)} 0Z"


def n_at(cx, base, k):
    d, _ = wordmark(cx - 88 * k / 2, base, k, island=False, text="n")
    return d


def tile(d):
    return path(TILE) + path(d, PEARL)


S = {}
S["h0-A-as-shown"] = tile(island_with_drop(42, 72, 172, 54, 128, 163, 21, 11))
S["h1-teardrop"] = tile(pill(42, 64, 172, 52) + teardrop(128, 174, 21))
S["h2-round"] = tile(pill(42, 66, 172, 52) + circle(128, 166, 21))
S["h3-smooth"] = tile(island_with_drop(42, 68, 172, 52, 128, 166, 19, 22))
S["h4-airy"] = tile(pill(72, 36, 112, 30) + teardrop(128, 150, 17))
S["h5-airy-round"] = tile(pill(60, 40, 136, 38) + circle(128, 132, 17))
# the n hangs from the island by a thread of liquid
isl = island_with_drop(72, 32, 112, 30, 128, 86, 9, 8)
S["h6-n-thread"] = tile(isl + n_at(128, 214, 1.18))
# island, drop, and the drop lands in the n
S["h7-n-drop"] = tile(pill(72, 30, 112, 28) + teardrop(128, 92, 9) + n_at(128, 218, 1.12))

for name, body in S.items():
    with open(os.path.join(OUT, name + ".svg"), "w") as fh:
        fh.write(svg(body, title=name))

cols, cw, ch = 4, 300, 360
names = list(S)
rows = (len(names) + cols - 1) // cols
W = cols * cw
H = rows * ch + 300
parts = [f'<rect width="{W}" height="{H}" fill="#ffffff"/>']
for i, name in enumerate(names):
    cx, cy = (i % cols) * cw, (i // cols) * ch
    for dx, dy, sc in ((22, 14, 1), (60, 284, 48 / 256), (130, 292, 32 / 256), (184, 300, 16 / 256)):
        parts.append(f'<g transform="translate({cx + dx},{cy + dy}) scale({sc})">{S[name]}</g>')
    parts.append(f'<text x="{cx + 22}" y="{cy + 350}" font-family="Helvetica" font-size="13" fill="#888">{name}</text>')

# the wordmark's island drips too
y0 = rows * ch
k = 1.0
d, w = wordmark(60, y0 + 190, k)
hx = 60 + sum((88, GAPS["no"], 103, GAPS["ot"], 20, GAPS["tc"])) * k
parts.append(path(d))
d2, _ = wordmark(700, y0 + 250, 0.3)
parts.append(path(d2))
with open(os.path.join(OUT, "_sheet8.svg"), "w") as fh:
    fh.write(svg("".join(parts), W, H, "round 4"))
print(W, H)
