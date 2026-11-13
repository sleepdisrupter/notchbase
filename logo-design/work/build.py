"""Builds the three concept files into ../concepts (version suffix from argv, default v2).

A  Drip    the current idea, drawn flat: an island with a drop
B  Let go  the island is the drop: black liquid leaving the top edge of a pearl tile
C  Pocket  the island sits between the h and the b; the icon is the n under the island
"""
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from geo import f, island_with_drop, pill
from type import wordmark

VER = sys.argv[1] if len(sys.argv) > 1 else "v2"
INK = "#0B0B0C"
PEARL = "#EFECE5"
OUT = os.path.join(os.path.dirname(__file__), "..", "concepts")
os.makedirs(OUT, exist_ok=True)

# the tile: a 232 square with continuous corners, 12 units of air around it
L, T, R, B = 12.0, 12.0, 244.0, 244.0
E = 52 * 1.52  # how far a corner reaches along each side
C = E * 0.34  # distance of its control points from the corner
TL = [(L, T + E), (L, T + C), (L + C, T), (L + E, T)]
TR = [(R - E, T), (R - C, T), (R, T + C), (R, T + E)]
TILE = (
    f"M{f(L + E)} {f(T)}H{f(R - E)}C{f(R - C)} {f(T)} {f(R)} {f(T + C)} {f(R)} {f(T + E)}"
    f"V{f(B - E)}C{f(R)} {f(B - C)} {f(R - C)} {f(B)} {f(R - E)} {f(B)}"
    f"H{f(L + E)}C{f(L + C)} {f(B)} {f(L)} {f(B - C)} {f(L)} {f(B - E)}"
    f"V{f(T + E)}C{f(L)} {f(T + C)} {f(L + C)} {f(T)} {f(L + E)} {f(T)}Z"
)


def lerp(a, b, t):
    return (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t)


def split(p, t):
    a1, a2, a3 = lerp(p[0], p[1], t), lerp(p[1], p[2], t), lerp(p[2], p[3], t)
    b1, b2 = lerp(a1, a2, t), lerp(a2, a3, t)
    c = lerp(b1, b2, t)
    return [p[0], a1, b1, c], [c, b2, a3, p[3]]


def t_at_x(p, x):
    lo, hi = 0.0, 1.0
    for _ in range(60):
        mid = (lo + hi) / 2
        left, _ = split(p, mid)
        if left[3][0] < x:
            lo = mid
        else:
            hi = mid
    return (lo + hi) / 2


def pts(*ps):
    return " ".join(f"{f(x)} {f(y)}" for x, y in ps)


def let_go(cx, half_top, waist, cap_y, cap_w, cap_h, half_bot):
    """Black liquid that leaves the tile's top edge, narrows to a waist and swells into the island.

    Its upper edge follows the tile's own outline, so the two share one silhouette."""
    _, tl = split(TL, t_at_x(TL, cx - half_top))
    tr, _ = split(TR, t_at_x(TR, cx + half_top))
    y_edge = tl[0][1]
    cr = cap_h / 2
    xl, xr = cx - cap_w / 2, cx + cap_w / 2
    mid = (T + cap_y) / 2
    w = waist / 2
    up, down = (mid - T) * 0.5, (cap_y - mid) * 0.5
    return (
        f"M{pts(tl[0])}C{pts(tl[1], tl[2], tl[3])}H{f(R - E)}C{pts(tr[1], tr[2], tr[3])}"
        f"C{f(cx + half_top * 0.45)} {f(y_edge)} {f(cx + w)} {f(mid - up)} {f(cx + w)} {f(mid)}"
        f"C{f(cx + w)} {f(mid + down)} {f(cx + half_bot * 0.45)} {f(cap_y)} {f(cx + half_bot)} {f(cap_y)}"
        f"H{f(xr - cr)}A{f(cr)} {f(cr)} 0 0 1 {f(xr - cr)} {f(cap_y + cap_h)}"
        f"H{f(xl + cr)}A{f(cr)} {f(cr)} 0 0 1 {f(xl + cr)} {f(cap_y)}H{f(cx - half_bot)}"
        f"C{f(cx - half_bot * 0.45)} {f(cap_y)} {f(cx - w)} {f(mid + down)} {f(cx - w)} {f(mid)}"
        f"C{f(cx - w)} {f(mid - up)} {f(cx - half_top * 0.45)} {f(y_edge)} {pts(tl[0])}Z"
    )


def doc(body, w, h, title):
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {f(w)} {f(h)}" width="{f(w)}" height="{f(h)}" '
        f'role="img" aria-labelledby="title"><title id="title">{title}</title>{body}</svg>\n'
    )


def path(d, fill=INK, gid=None):
    tag = f'<path fill="{fill}" d="{d}"/>'
    return f'<g id="{gid}">{tag}</g>' if gid else tag


def save(name, body, w, h, title):
    with open(os.path.join(OUT, f"{name}-{VER}.svg"), "w") as fh:
        fh.write(doc(body, w, h, title))


def lockup(name, symbol_body, title, island=False, k=0.8, gap=66):
    x0 = R + gap
    d, w = wordmark(x0, 128 + 50 * k, k, island=island)
    total = round(x0 + w + 20)
    save(name, f'<g id="symbol">{symbol_body}</g>' + path(d, INK, "wordmark"), total, 256, title)


def main():
    # A - Drip
    a_symbol = path(TILE) + path(island_with_drop(42, 72, 172, 54, 128, 163, 21, 11), PEARL)
    save("a-drip-symbol", a_symbol, 256, 256, "notchbase, concept A: Drip")
    save("a-drip-mark", path(island_with_drop(22, 62, 212, 66, 128, 174, 26, 13.5)), 256, 256, "notchbase, concept A: Drip, bare mark")
    lockup("a-drip-lockup", a_symbol, "notchbase, concept A: Drip, lockup")

    # B - Let go
    liquid = let_go(128, 56, 15, 50, 168, 56, 38)
    b_symbol = path(TILE, PEARL) + path(liquid)
    save("b-letgo-symbol", b_symbol, 256, 256, "notchbase, concept B: Let go")
    save("b-letgo-reversed", path(TILE) + path(liquid, PEARL), 256, 256, "notchbase, concept B: Let go, reversed")
    lockup("b-letgo-lockup", b_symbol, "notchbase, concept B: Let go, lockup")

    # C - Pocket
    nk = 1.18
    n_d, _ = wordmark(128 - 88 * nk / 2, 214, nk, island=False, text="n")
    isl = pill(72, 32, 112, 30)
    c_symbol = path(TILE) + path(isl + n_d, PEARL)
    save("c-pocket-symbol", c_symbol, 256, 256, "notchbase, concept C: Pocket")
    save("c-pocket-mark", path(isl + n_d), 256, 256, "notchbase, concept C: Pocket, bare mark")
    d, w = wordmark(24, 206, 1.0)
    save("c-pocket-wordmark", path(d, INK, "wordmark"), round(w + 48), 256, "notchbase, concept C: Pocket, wordmark")
    d, w = wordmark(24, 206, 1.0, island=False)
    save("plain-wordmark", path(d, INK, "wordmark"), round(w + 48), 256, "notchbase wordmark, plain")
    print("built", VER)


if __name__ == "__main__":
    main()
