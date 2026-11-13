"""A constructed geometric lowercase for the notchbase wordmark.

Every glyph is rectangles plus ring sectors, all wound the same way so one path unions them.
Glyph space: baseline y=0, y up, x-height 100. Pen flips into SVG space."""
import math

from geo import f

X = 100.0  # x-height
ASC = 144.0  # ascender
S = 20.0  # stem
BAR = 17.0  # horizontal strokes, thinner so they look as heavy as the stems
OV = 1.5  # overshoot of round forms
RO = X / 2 + OV  # outer radius of round letters
SIDE = 21.0  # round strokes at their sides
TOP = 18.0  # round strokes at top and bottom
NW = 88.0  # width of n and h
NR = NW / 2


class Pen:
    def __init__(self, ox, base, scale):
        self.ox, self.base, self.k = ox, base, scale
        self.d = []

    def pt(self, x, y):
        return f(self.ox + x * self.k), f(self.base - y * self.k)

    def rect(self, x0, y0, x1, y1):
        a, b = self.pt(x0, y1)
        c, d = self.pt(x1, y0)
        self.d.append(f"M{a} {b}H{c}V{d}H{a}Z")

    def ring(self, cx, cy, rxo, ryo, rxi, ryi):
        k = self.k
        for rx, ry, sweep in ((rxo, ryo, 1), (rxi, ryi, 0)):
            x0, y0 = self.pt(cx - rx, cy)
            x1, _ = self.pt(cx + rx, cy)
            self.d.append(
                f"M{x0} {y0}A{f(rx * k)} {f(ry * k)} 0 1 {sweep} {x1} {y0}"
                f"A{f(rx * k)} {f(ry * k)} 0 1 {sweep} {x0} {y0}Z"
            )

    def sector(self, cx, cy, rxo, ryo, rxi, ryi, a0, a1):
        """Ring sector between parametric angles a0 and a1 (degrees, counter-clockwise, y up)."""
        k = self.k

        def p(rx, ry, a):
            return self.pt(cx + rx * math.cos(math.radians(a)), cy + ry * math.sin(math.radians(a)))

        large = 1 if (a1 - a0) > 180 else 0
        o0, o1, i0, i1 = p(rxo, ryo, a0), p(rxo, ryo, a1), p(rxi, ryi, a0), p(rxi, ryi, a1)
        self.d.append(
            f"M{o1[0]} {o1[1]}A{f(rxo * k)} {f(ryo * k)} 0 {large} 1 {o0[0]} {o0[1]}"
            f"L{i0[0]} {i0[1]}A{f(rxi * k)} {f(ryi * k)} 0 {large} 0 {i1[0]} {i1[1]}Z"
        )

    def pill(self, x0, y0, x1, y1):
        r = (y1 - y0) / 2
        a, top = self.pt(x0 + r, y1)
        b, bot = self.pt(x1 - r, y0)
        rr = f(r * self.k)
        self.d.append(f"M{a} {top}H{b}A{rr} {rr} 0 0 1 {b} {bot}H{a}A{rr} {rr} 0 0 1 {a} {top}Z")


def bowl(pen, x, a0=None, a1=None):
    args = (x + RO, X / 2, RO, RO, RO - SIDE, RO - TOP)
    if a0 is None:
        pen.ring(*args)
    else:
        pen.sector(*args, a0, a1)


def g_n(pen, x, top=X):
    cy = X + OV - NR
    pen.rect(x, 0, x + S, top)
    pen.sector(x + NR, cy, NR, NR, NR - S, NR - TOP, 0, 180)
    pen.rect(x + NW - S, 0, x + NW, cy)
    return NW


def g_h(pen, x):
    return g_n(pen, x, ASC)


def g_o(pen, x):
    bowl(pen, x)
    return 2 * RO


def g_c(pen, x):
    bowl(pen, x, 42, 318)
    return RO + RO * math.cos(math.radians(42))


def g_e(pen, x):
    lo = X / 2 - BAR / 2
    cut = -math.degrees(math.asin((X / 2 - lo) / RO))
    edge = RO * math.cos(math.radians(cut))
    bowl(pen, x, cut, 318)
    pen.rect(x + SIDE * 0.6, lo, x + RO + edge, lo + BAR)
    return 2 * RO


def g_b(pen, x):
    pen.rect(x, 0, x + S, ASC)
    bowl(pen, x)
    return 2 * RO


def g_a(pen, x):
    bowl(pen, x)
    pen.rect(x + 2 * RO - S, 0, x + 2 * RO, X)
    return 2 * RO


def g_t(pen, x):
    pen.rect(x, 0, x + S, 130)
    pen.rect(x - 22, X - BAR, x + S + 24, X)
    return S


def g_s(pen, x):
    # two stacked bowls, the upper one a little smaller so the letter stands firmly
    total = X + 2 * OV
    up, lo = 0.485, 0.515
    hu, hl = (total + TOP) * up, (total + TOP) * lo
    rxl, rxu = 36.0, 33.5
    cx = x + rxl
    top_c, bot_c = X + OV - hu / 2, hl / 2 - OV
    pen.sector(cx, top_c, rxu, hu / 2, rxu - SIDE, hu / 2 - TOP, 26, 270)
    pen.sector(cx, bot_c, rxl, hl / 2, rxl - SIDE, hl / 2 - TOP, 206, 450)
    return 2 * rxl


GLYPHS = {"n": g_n, "o": g_o, "t": g_t, "c": g_c, "h": g_h, "b": g_b, "a": g_a, "s": g_s, "e": g_e}
# space after each letter of "notchbase", set by eye
GAPS = {"no": 19, "ot": 35, "tc": 39, "ch": 15, "hb": 30, "ba": 14, "as": 20, "se": 14}
ISLAND_PAD = 13.0


def wordmark(ox, base, scale, island=True, text="notchbase"):
    """Returns (path data, width in SVG units)."""
    pen = Pen(ox, base, scale)
    x = 0.0
    spots = {}
    for i, ch in enumerate(text):
        spots[i] = x
        x += GLYPHS[ch](pen, x)
        if i + 1 < len(text):
            x += GAPS[text[i : i + 2]]
    if island and "hb" in text:
        i = text.index("hb")
        pen.pill(spots[i] + S + ISLAND_PAD, ASC - S, spots[i + 1] - ISLAND_PAD, ASC)
    return "".join(pen.d), x * scale


if __name__ == "__main__":
    d, w = wordmark(0, 0, 1)
    print(round(w, 1))
