"""Round two: A's liquid island and drop, drawn with C's geometry, beside C's wordmark.

D  Released  the island has let one drop go: a pill and a teardrop, nothing else
E  Thread    the moment before: the drop still hangs on its thread (arcs and straight lines only)
F  Ink       E in black on a pearl tile
"""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from build import INK, PEARL, R, TILE, doc, path
from geo import f, pill
from type import wordmark

VER = sys.argv[1] if len(sys.argv) > 1 else "v4"
OUT = os.path.join(os.path.dirname(__file__), "..", "concepts")


def teardrop(cx, cy, r, tip=0.0):
    """A circle and its two tangents from a point 2r above the centre: a 60 degree tip, rounded by `tip`."""
    h = 2 * r
    th = math.acos(r / h)
    tx, ty = r * math.sin(th), r * math.cos(th)
    half = math.pi / 2 - th
    reach = tip / math.tan(half)
    sx, sy = reach * math.sin(half), reach * math.cos(half)
    top = cy - h
    start = f"M{f(cx - sx)} {f(top + sy)}A{f(tip)} {f(tip)} 0 0 1 {f(cx + sx)} {f(top + sy)}" if tip else f"M{f(cx)} {f(top)}"
    return f"{start}L{f(cx + tx)} {f(cy - ty)}A{f(r)} {f(r)} 0 1 1 {f(cx - tx)} {f(cy - ty)}Z"


def thread(x, y, w, h, cx, cy, r, tw, rf, rj):
    """Island with a drop still on its thread: flare, thread, 60 degree shoulders, round bulb."""
    pr = h / 2
    y0 = y + h
    n = tw / 2
    jy = cy - 2 * r + n / math.tan(math.radians(30))
    t = rj * math.tan(math.radians(15))
    sx, sy = t * 0.5, t * math.cos(math.radians(30))
    tx, ty = r * math.sin(math.radians(60)), r * 0.5
    return (
        f"M{f(x + pr)} {f(y)}H{f(x + w - pr)}A{f(pr)} {f(pr)} 0 0 1 {f(x + w - pr)} {f(y0)}"
        f"H{f(cx + n + rf)}A{f(rf)} {f(rf)} 0 0 0 {f(cx + n)} {f(y0 + rf)}"
        f"V{f(jy - t)}A{f(rj)} {f(rj)} 0 0 0 {f(cx + n + sx)} {f(jy + sy)}"
        f"L{f(cx + tx)} {f(cy - ty)}A{f(r)} {f(r)} 0 1 1 {f(cx - tx)} {f(cy - ty)}"
        f"L{f(cx - n - sx)} {f(jy + sy)}A{f(rj)} {f(rj)} 0 0 0 {f(cx - n)} {f(jy - t)}"
        f"V{f(y0 + rf)}A{f(rf)} {f(rf)} 0 0 0 {f(cx - n - rf)} {f(y0)}"
        f"H{f(x + pr)}A{f(pr)} {f(pr)} 0 0 1 {f(x + pr)} {f(y)}Z"
    )


def save(name, body, w, h, title):
    with open(os.path.join(OUT, f"{name}-{VER}.svg"), "w") as fh:
        fh.write(doc(body, w, h, title))


def lockup(name, symbol_body, title, island, k=0.8, gap=66):
    x0 = R + gap
    d, w = wordmark(x0, 128 + 50 * k, k, island=island)
    save(name, f'<g id="symbol">{symbol_body}</g>' + path(d, INK, "wordmark"), round(x0 + w + 20), 256, title)


def main():
    released = pill(46, 62, 164, 50) + teardrop(128, 170, 20, 3)
    hanging = thread(46, 54, 164, 50, 128, 180, 19.5, 9, 21, 15)

    d_symbol = path(TILE) + path(released, PEARL)
    save("d-released-symbol", d_symbol, 256, 256, "notchbase, concept D: Released")
    save("d-released-mark", path(pill(24, 52, 208, 63) + teardrop(128, 189, 25.5, 3.8)), 256, 256, "notchbase, concept D: Released, bare mark")
    lockup("d-released-lockup", d_symbol, "notchbase, concept D: Released, lockup", True)
    lockup("d-released-lockup-plain", d_symbol, "notchbase, concept D: Released, lockup with the plain wordmark", False)

    e_symbol = path(TILE) + path(hanging, PEARL)
    save("e-thread-symbol", e_symbol, 256, 256, "notchbase, concept E: Thread")
    save("e-thread-mark", path(thread(24, 36, 208, 63, 128, 196, 24.7, 11.4, 26.6, 19)), 256, 256, "notchbase, concept E: Thread, bare mark")
    lockup("e-thread-lockup", e_symbol, "notchbase, concept E: Thread, lockup", True)
    lockup("e-thread-lockup-plain", e_symbol, "notchbase, concept E: Thread, lockup with the plain wordmark", False)

    f_symbol = path(TILE, PEARL) + path(hanging)
    save("f-ink-symbol", f_symbol, 256, 256, "notchbase, concept F: Ink")
    lockup("f-ink-lockup", f_symbol, "notchbase, concept F: Ink, lockup", True)
    print("built", VER)


if __name__ == "__main__":
    main()
