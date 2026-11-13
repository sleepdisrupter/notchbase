"""Round 1: the shortlisted directions, a little more carefully, plus the wordmark."""
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from geo import f, island_with_drop, pill, rrect, squircle, svg
from type import wordmark

INK = "#0b0b0c"
PEARL = "#f1efe9"
OUT = os.path.join(os.path.dirname(__file__), "sketches")
TILE = squircle(12, 12, 232, 232, 52)


def path(d, fill=INK, rule=None):
    extra = f' fill-rule="{rule}"' if rule else ""
    return f'<path fill="{fill}"{extra} d="{d}"/>'


def on_tile(d, extra=""):
    return path(TILE) + path(d, PEARL) + extra


S = {}

# a1 island with a small drop on a thin thread
a1 = island_with_drop(44, 78, 168, 52, 128, 164, 20, 9)
S["a1-drop-thread"] = on_tile(a1)

# a2 island and a detached round drop
a2 = pill(44, 84, 168, 52) + f"M{f(128 - 19)} {f(170)}a19 19 0 1 0 38 0a19 19 0 1 0 -38 0Z"
S["a2-drop-free"] = on_tile(a2)

# a3 island and a detached teardrop, off the centre line
tx, ty, tr = 150, 172, 19
tear = (
    f"M{f(tx)} {f(ty - 34)}C{f(tx + 6)} {f(ty - 22)} {f(tx + tr)} {f(ty - 12)} {f(tx + tr)} {f(ty)}"
    f"A{tr} {tr} 0 0 1 {f(tx - tr)} {f(ty)}C{f(tx - tr)} {f(ty - 12)} {f(tx - 6)} {f(ty - 22)} {f(tx)} {f(ty - 34)}Z"
)
S["a3-teardrop"] = on_tile(pill(44, 74, 168, 52) + tear)

# b1 a base with a wide, shallow notch (rounded lips)
def bite(cx, top, w, depth, r_in, r_lip):
    x0, x1 = cx - w / 2, cx + w / 2
    return (
        f"M{f(x0 - r_lip)} {f(top - 40)}H{f(x1 + r_lip)}V{f(top)}"
        f"A{r_lip} {r_lip} 0 0 0 {f(x1)} {f(top + r_lip)}V{f(top + depth - r_in)}"
        f"A{r_in} {r_in} 0 0 1 {f(x1 - r_in)} {f(top + depth)}H{f(x0 + r_in)}"
        f"A{r_in} {r_in} 0 0 1 {f(x0)} {f(top + depth - r_in)}V{f(top + r_lip)}"
        f"A{r_lip} {r_lip} 0 0 0 {f(x0 - r_lip)} {f(top)}Z"
    )


S["b1-bite"] = path(TILE) + path(bite(128, 12, 116, 46, 20, 10), "#ffffff")

# b2 notch and base as two pieces of one tile
plug = (
    f"M{f(84)} {f(12)}H{f(172)}V{f(38)}A16 16 0 0 1 {f(156)} {f(54)}H{f(100)}A16 16 0 0 1 {f(84)} {f(38)}Z"
)
S["b2-split"] = path(TILE) + path(bite(128, 12, 116, 58, 28, 8), "#ffffff") + path(plug)

# b3 the notch is the light piece
S["b3-light-notch"] = path(TILE) + path(bite(128, 12, 116, 46, 20, 10), PEARL)

# b4 pearl tile, black notch that has let an island go
S["b4-let-go"] = (
    path(TILE, PEARL)
    + path(bite(128, 12, 84, 14, 7, 7))
    + path(pill(56, 52, 144, 50))
)

# c1 n under the island
n_d, n_w = wordmark(0, 0, 1, island=False, text="n")
n_d, n_w = wordmark(128 - 88 * 1.18 / 2, 212, 1.18, island=False, text="n")
S["c1-n-island"] = path(TILE) + path(n_d, PEARL) + path(pill(88, 44, 80, 26), PEARL)

for name, body in S.items():
    with open(os.path.join(OUT, name + ".svg"), "w") as fh:
        fh.write(svg(body, title=name))

cols = 4
cell_w, cell_h = 300, 360
names = list(S)
rows = (len(names) + cols - 1) // cols
W = cols * cell_w
H = rows * cell_h + 620
parts = [f'<rect width="{W}" height="{H}" fill="#ffffff"/>']
for i, name in enumerate(names):
    cx, cy = (i % cols) * cell_w, (i // cols) * cell_h
    body = S[name]
    parts.append(f'<g transform="translate({cx + 22},{cy + 14})">{body}</g>')
    parts.append(f'<g transform="translate({cx + 60},{cy + 284}) scale({48 / 256})">{body}</g>')
    parts.append(f'<g transform="translate({cx + 130},{cy + 292}) scale({32 / 256})">{body}</g>')
    parts.append(f'<g transform="translate({cx + 184},{cy + 300}) scale({16 / 256})">{body}</g>')
    parts.append(f'<text x="{cx + 22}" y="{cy + 350}" font-family="Helvetica" font-size="13" fill="#666">{name}</text>')

y0 = rows * cell_h
# wordmark with the island in the h-b pocket
d, w = wordmark(60, y0 + 190, 1.0)
parts.append(path(d))
d, w = wordmark(60, y0 + 290, 0.3)
parts.append(path(d))
d, w = wordmark(400, y0 + 290, 0.16)
parts.append(path(d))
# plain
d, w = wordmark(60, y0 + 480, 0.7, island=False)
parts.append(path(d))
# emblem: the name inside the island
d, w = wordmark(0, 0, 0.42, island=False)
ex, ey = 700, y0 + 400
parts.append(path(pill(ex, ey, w + 96, 124)))
d, w = wordmark(ex + 48, ey + 84, 0.42, island=False)
parts.append(path(d, PEARL))

with open(os.path.join(OUT, "_sheet2.svg"), "w") as fh:
    fh.write(svg("".join(parts), W, H, "round 1 sketches"))
print(W, H)
