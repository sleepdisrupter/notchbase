"""Round 3: candidates for the third (abstract) direction."""
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from geo import f, pendant, pill, rrect, squircle, svg

INK = "#0b0b0c"
PEARL = "#f1efe9"
OUT = os.path.join(os.path.dirname(__file__), "sketches")
TILE = squircle(12, 12, 232, 232, 52)


def path(d, fill=INK):
    return f'<path fill="{fill}" d="{d}"/>'


def bridge(top, cx, half_top, waist, cap_y, cap_w, cap_h, half_bot):
    """Liquid between the ceiling and a capsule: wide at the ceiling, a waist, wide again on the capsule."""
    cr = cap_h / 2
    xl, xr = cx - cap_w / 2, cx + cap_w / 2
    mid = (top + cap_y) / 2
    w = waist / 2
    return (
        f"M{f(cx - half_top)} {f(top)}H{f(cx + half_top)}"
        f"C{f(cx + half_top * 0.45)} {f(top)} {f(cx + w)} {f(mid - (mid - top) * 0.5)} {f(cx + w)} {f(mid)}"
        f"C{f(cx + w)} {f(mid + (cap_y - mid) * 0.5)} {f(cx + half_bot * 0.45)} {f(cap_y)} {f(cx + half_bot)} {f(cap_y)}"
        f"H{f(xr - cr)}A{f(cr)} {f(cr)} 0 0 1 {f(xr - cr)} {f(cap_y + cap_h)}"
        f"H{f(xl + cr)}A{f(cr)} {f(cr)} 0 0 1 {f(xl + cr)} {f(cap_y)}H{f(cx - half_bot)}"
        f"C{f(cx - half_bot * 0.45)} {f(cap_y)} {f(cx - w)} {f(mid + (cap_y - mid) * 0.5)} {f(cx - w)} {f(mid)}"
        f"C{f(cx - w)} {f(mid - (mid - top) * 0.5)} {f(cx - half_top * 0.45)} {f(top)} {f(cx - half_top)} {f(top)}Z"
    )


def bump(top, cx, half, h):
    return (
        f"M{f(cx - half)} {f(top)}H{f(cx + half)}"
        f"C{f(cx + half * 0.55)} {f(top)} {f(cx + half * 0.5)} {f(top + h)} {f(cx)} {f(top + h)}"
        f"C{f(cx - half * 0.5)} {f(top + h)} {f(cx - half * 0.55)} {f(top)} {f(cx - half)} {f(top)}Z"
    )


S = {}
S["g1-bridge"] = path(TILE, PEARL) + path(bridge(12, 128, 60, 16, 46, 160, 54, 40))
S["g2-bridge-short"] = path(TILE, PEARL) + path(bridge(12, 128, 52, 22, 36, 160, 54, 36))
S["g3-let-go"] = path(TILE, PEARL) + path(bump(12, 128, 54, 14)) + path(pill(48, 44, 160, 54))
S["g4-bridge-dark"] = path(TILE) + path(bridge(12, 128, 60, 16, 46, 160, 54, 40), PEARL)
# nested states
S["g5-grow"] = (
    path(TILE)
    + path(rrect(36, 12, 184, 150, 44) , "#2c2c2f")
    + path(rrect(64, 12, 128, 92, 34), "#6f6d69")
    + path(pill(86, 30, 84, 28), PEARL)
)
S["g6-grow-float"] = (
    path(TILE)
    + path(rrect(34, 34, 188, 150, 46), "#2c2c2f")
    + path(rrect(60, 34, 136, 92, 38), "#6f6d69")
    + path(pill(84, 48, 88, 30), PEARL)
)
S["g7-grow-pearl"] = (
    path(TILE, PEARL)
    + path(rrect(34, 12, 188, 156, 46), "#bdbbb6")
    + path(rrect(62, 12, 132, 96, 36), "#55534f")
    + path(pill(84, 28, 88, 30), INK)
)
S["g8-bridge-bare"] = path(rrect(20, 40, 216, 22, 0)) + path(bridge(61, 128, 60, 16, 104, 176, 60, 40))

for name, body in S.items():
    with open(os.path.join(OUT, name + ".svg"), "w") as fh:
        fh.write(svg(body, title=name))

cols = 4
cell_w, cell_h = 300, 360
names = list(S)
rows = (len(names) + cols - 1) // cols
W, H = cols * cell_w, rows * cell_h
parts = [f'<rect width="{W}" height="{H}" fill="#d9d9d9"/>']
for i, name in enumerate(names):
    cx, cy = (i % cols) * cell_w, (i // cols) * cell_h
    body = S[name]
    for dx, dy, sc in ((22, 14, 1), (60, 284, 48 / 256), (130, 292, 32 / 256), (184, 300, 16 / 256)):
        parts.append(f'<g transform="translate({cx + dx},{cy + dy}) scale({sc})">{body}</g>')
    parts.append(f'<text x="{cx + 22}" y="{cy + 350}" font-family="Helvetica" font-size="13" fill="#444">{name}</text>')
with open(os.path.join(OUT, "_sheet4.svg"), "w") as fh:
    fh.write(svg("".join(parts), W, H, "round 3 sketches"))
print(W, H)
