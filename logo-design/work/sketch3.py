"""Round 2: variants of the three chosen directions."""
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from geo import f, island_with_drop, pill, squircle, svg
from type import wordmark

INK = "#0b0b0c"
PEARL = "#f1efe9"
GREY = "#8f8d88"
OUT = os.path.join(os.path.dirname(__file__), "sketches")
TILE = squircle(12, 12, 232, 232, 52)


def path(d, fill=INK):
    return f'<path fill="{fill}" d="{d}"/>'


def bite(cx, top, w, depth, r_in, r_lip, up=40):
    x0, x1 = cx - w / 2, cx + w / 2
    return (
        f"M{f(x0 - r_lip)} {f(top - up)}H{f(x1 + r_lip)}V{f(top)}"
        f"A{r_lip} {r_lip} 0 0 0 {f(x1)} {f(top + r_lip)}V{f(top + depth - r_in)}"
        f"A{r_in} {r_in} 0 0 1 {f(x1 - r_in)} {f(top + depth)}H{f(x0 + r_in)}"
        f"A{r_in} {r_in} 0 0 1 {f(x0)} {f(top + depth - r_in)}V{f(top + r_lip)}"
        f"A{r_lip} {r_lip} 0 0 0 {f(x0 - r_lip)} {f(top)}Z"
    )


def n_at(cx, base, k):
    d, w = wordmark(cx - 88 * k / 2, base, k, island=False, text="n")
    return d


S = {}

# C: n under the island
S["c-a-narrow"] = path(TILE) + path(n_at(128, 206, 1.12), PEARL) + path(pill(88, 48, 80, 24), PEARL)
S["c-b-grey"] = path(TILE) + path(n_at(128, 206, 1.12), PEARL) + path(pill(88, 48, 80, 24), GREY)
S["c-c-wide"] = path(TILE) + path(n_at(128, 210, 1.0), PEARL) + path(pill(70, 40, 116, 30), PEARL)
# the pocket itself: two posts (h and b) with the island between, the arch below
k = 1.0
hx = 128 - (88 + 30 + 20) * k / 2
d_h, _ = wordmark(hx, 214, k, island=True, text="hb")
S["c-d-pocket"] = path(TILE) + f'<clipPath id="t"><path d="{squircle(12, 12, 232, 232, 52)}"/></clipPath>' + (
    f'<g clip-path="url(#t)"><path fill="{PEARL}" d="{d_h}"/></g>'
)

# B: a base with a notch
S["b-a-104"] = path(TILE) + path(bite(128, 12, 104, 40, 16, 8), "#fff")
S["b-b-88"] = path(TILE) + path(bite(128, 12, 88, 34, 14, 7), "#fff")
S["b-c-pill"] = path(TILE) + path(bite(128, 12, 120, 54, 27, 8), "#fff") + path(pill(82, 12, 92, 38))
# ears like the real notch: the tile grows flared lips into the bite
S["b-d-deep"] = path(TILE) + path(bite(128, 12, 96, 56, 22, 10), "#fff")

# A: island with a drop
S["a-a"] = path(TILE) + path(island_with_drop(42, 74, 172, 54, 128, 164, 21, 11), PEARL)
S["a-b"] = path(island_with_drop(20, 60, 216, 68, 128, 172, 26, 14))
S["a-c"] = path(TILE) + path(island_with_drop(42, 74, 172, 54, 128, 150, 24, 12), PEARL)
S["a-d"] = path(TILE) + path(pill(42, 70, 172, 54) + pill(107, 146, 42, 42), PEARL)

for name, body in S.items():
    with open(os.path.join(OUT, name + ".svg"), "w") as fh:
        fh.write(svg(body, title=name))

cols = 4
cell_w, cell_h = 300, 360
names = list(S)
rows = (len(names) + cols - 1) // cols
W = cols * cell_w
H = rows * cell_h + 330
parts = [f'<rect width="{W}" height="{H}" fill="#ffffff"/>']
for i, name in enumerate(names):
    cx, cy = (i % cols) * cell_w, (i // cols) * cell_h
    body = S[name].replace('id="t"', f'id="t{i}"').replace("url(#t)", f"url(#t{i})")
    for j, (dx, dy, sc) in enumerate(((22, 14, 1), (60, 284, 48 / 256), (130, 292, 32 / 256), (184, 300, 16 / 256))):
        b = body.replace(f't{i}"', f't{i}_{j}"').replace(f"#t{i})", f"#t{i}_{j})")
        parts.append(f'<g transform="translate({cx + dx},{cy + dy}) scale({sc})">{b}</g>')
    parts.append(f'<text x="{cx + 22}" y="{cy + 350}" font-family="Helvetica" font-size="13" fill="#666">{name}</text>')

y0 = rows * cell_h
d, w = wordmark(60, y0 + 190, 1.0)
parts.append(path(d))
d, w = wordmark(60, y0 + 290, 0.3)
parts.append(path(d))
d, w = wordmark(400, y0 + 290, 0.16)
parts.append(path(d))
d, w = wordmark(620, y0 + 290, 0.5, island=False)
parts.append(path(d))

with open(os.path.join(OUT, "_sheet3.svg"), "w") as fh:
    fh.write(svg("".join(parts), W, H, "round 2 sketches"))
print(W, H)
