"""Round 0: rough thumbnails of the longlist, to pick three directions by eye."""
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from geo import f, goo_below, island_with_drop, pendant, pill, rrect, squircle, svg

INK = "#0b0b0c"
PEARL = "#f1efe9"
OUT = os.path.join(os.path.dirname(__file__), "sketches")
os.makedirs(OUT, exist_ok=True)

TILE = squircle(12, 12, 232, 232, 52)


def path(d, fill=INK, rule=None):
    extra = f' fill-rule="{rule}"' if rule else ""
    return f'<path fill="{fill}"{extra} d="{d}"/>'


sketches = {}

# s01 the current idea, flat: island with a centred drop
sketches["s01-drip"] = path(island_with_drop(36, 62, 184, 60, 128, 176, 30, 20))

# s02 the drop pulled off centre
sketches["s02-drip-lean"] = path(island_with_drop(36, 62, 184, 60, 164, 176, 30, 20))

# s03 pearl tile, black liquid hanging from the top edge into the island
sketches["s03-pendant"] = path(TILE, PEARL) + path(pendant(12, 128, 0, 30, 34, 76, 156, 56, 14))

# s04 black tile, pearl liquid
sketches["s04-pendant-dark"] = path(TILE, INK) + path(pendant(12, 128, 0, 30, 34, 76, 156, 56, 14), PEARL)

# s05 black tile as bezel, pearl screen, black liquid from the bezel
screen = squircle(34, 34, 188, 188, 34)
sketches["s05-pendant-bezel"] = (
    path(TILE, INK) + path(screen, PEARL) + path(pendant(34, 128, 0, 26, 26, 82, 128, 46, 12))
)

# s06 the island drips into an n: stem, island, drop
stem = rrect(52, 56, 52, 152, 26)
isl = island_with_drop(52, 56, 152, 52, 176, 166, 30, 18)
sketches["s06-n-drip"] = path(stem + isl)

# s07 docked island: a base with a pill recess and the pill lifted out of it
base = rrect(36, 116, 184, 108, 40)
bite = pill(64, 92, 128, 56)
lift = pill(72, 40, 112, 44)
sketches["s07-docked"] = (
    f'<path fill="{INK}" d="{base}"/><path fill="#ffffff" d="{bite}"/><path fill="{INK}" d="{lift}"/>'
)

# s08 island over a base slab
sketches["s08-over-base"] = path(pill(76, 60, 104, 40)) + path(rrect(36, 124, 184, 76, 30))

# s09 a base with a notch bitten out of its top
notch = (
    f"M{f(68)} {f(0)}H{f(188)}V{f(12)}A10 10 0 0 0 {f(178)} {f(22)}V{f(44)}"
    f"A20 20 0 0 1 {f(158)} {f(64)}H{f(98)}A20 20 0 0 1 {f(78)} {f(44)}V{f(22)}A10 10 0 0 0 {f(68)} {f(12)}Z"
)
sketches["s09-bite"] = path(TILE, INK) + path(notch, "#ffffff")

# s10 the three states nested: idle, peek, open
sketches["s10-nested"] = (
    path(TILE, INK)
    + path(rrect(40, 12, 176, 148, 40), "#3a3a3d")
    + path(rrect(72, 12, 112, 84, 30), "#8e8c88")
    + path(pill(92, 28, 72, 26), PEARL)
)

# s11 two posts and the island between them
sketches["s11-posts"] = (
    path(rrect(44, 44, 40, 168, 0)) + path(rrect(172, 44, 40, 168, 0)) + path(pill(96, 44, 64, 40))
)

# s12 island with a liquid ear budding off its side
r = 30
ear = (
    f"M{f(40 + r)} {f(98)}H{f(150)}A{f(r)} {f(r)} 0 0 1 {f(150)} {f(158)}H{f(40 + r)}"
    f"A{f(r)} {f(r)} 0 0 1 {f(40 + r)} {f(98)}Z"
)
sketches["s12-ear"] = path(ear) + f'<circle cx="204" cy="128" r="24" fill="{INK}"/>'

for name, body in sketches.items():
    with open(os.path.join(OUT, name + ".svg"), "w") as fh:
        fh.write(svg(body, title=name))

# contact sheet
cols = 4
cell_w, cell_h = 300, 360
names = list(sketches)
rows = (len(names) + cols - 1) // cols
W, H = cols * cell_w, rows * cell_h
parts = [f'<rect width="{W}" height="{H}" fill="#ffffff"/>']
for i, name in enumerate(names):
    cx, cy = (i % cols) * cell_w, (i // cols) * cell_h
    body = sketches[name]
    parts.append(f'<g transform="translate({cx + 22},{cy + 14})">{body}</g>')
    parts.append(f'<g transform="translate({cx + 60},{cy + 284}) scale({48 / 256})">{body}</g>')
    parts.append(f'<g transform="translate({cx + 130},{cy + 292}) scale({32 / 256})">{body}</g>')
    parts.append(f'<g transform="translate({cx + 184},{cy + 300}) scale({16 / 256})">{body}</g>')
    parts.append(
        f'<text x="{cx + 22}" y="{cy + 350}" font-family="Helvetica" font-size="13" fill="#666">{name}</text>'
    )
    parts.append(f'<rect x="{cx}" y="{cy}" width="{cell_w}" height="{cell_h}" fill="none" stroke="#eee"/>')
with open(os.path.join(OUT, "_sheet.svg"), "w") as fh:
    fh.write(svg("".join(parts), W, H, "round 0 sketches"))
print(W, H)
