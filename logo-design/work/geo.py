"""Path helpers for the notchbase logo sketches. Coordinates are SVG user units (y down)."""
import math


def f(v):
    s = f"{v:.2f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


def pill(x, y, w, h):
    r = min(w, h) / 2
    return rrect(x, y, w, h, r)


def rrect(x, y, w, h, r):
    r = min(r, w / 2, h / 2)
    return (
        f"M{f(x + r)} {f(y)}H{f(x + w - r)}A{f(r)} {f(r)} 0 0 1 {f(x + w)} {f(y + r)}"
        f"V{f(y + h - r)}A{f(r)} {f(r)} 0 0 1 {f(x + w - r)} {f(y + h)}"
        f"H{f(x + r)}A{f(r)} {f(r)} 0 0 1 {f(x)} {f(y + h - r)}"
        f"V{f(y + r)}A{f(r)} {f(r)} 0 0 1 {f(x + r)} {f(y)}Z"
    )


def squircle(x, y, w, h, r, ext=1.52, k=0.34):
    """Rounded rectangle with continuous-curvature corners (one cubic per corner)."""
    e = min(r * ext, w / 2, h / 2)
    c = e * k
    x1, y1 = x + w, y + h
    return (
        f"M{f(x + e)} {f(y)}H{f(x1 - e)}C{f(x1 - c)} {f(y)} {f(x1)} {f(y + c)} {f(x1)} {f(y + e)}"
        f"V{f(y1 - e)}C{f(x1)} {f(y1 - c)} {f(x1 - c)} {f(y1)} {f(x1 - e)} {f(y1)}"
        f"H{f(x + e)}C{f(x + c)} {f(y1)} {f(x)} {f(y1 - c)} {f(x)} {f(y1 - e)}"
        f"V{f(y + e)}C{f(x)} {f(y + c)} {f(x + c)} {f(y)} {f(x + e)} {f(y)}Z"
    )


def goo_below(y0, cx, cy, rd, rf):
    """Fillets joining the line y=y0 (liquid below it) to a drop circle hanging under it.

    Returns (x_right, x_left, right_tangent, left_tangent): where the fillets leave the line and
    where they meet the drop."""
    fy = y0 + rf
    dx = math.sqrt((rf + rd) ** 2 - (cy - fy) ** 2)
    t = rd / (rf + rd)
    right = (cx + dx * t, cy + (fy - cy) * t)
    left = (cx - dx * t, cy + (fy - cy) * t)
    return cx + dx, cx - dx, right, left


def island_with_drop(x, y, w, h, cx, cy, rd, rf):
    """A horizontal pill with a drop fused to its underside."""
    r = h / 2
    y0 = y + h
    xr, xl, tr, tl = goo_below(y0, cx, cy, rd, rf)
    large = 1 if tr[1] < cy else 0
    return (
        f"M{f(x + r)} {f(y)}H{f(x + w - r)}A{f(r)} {f(r)} 0 0 1 {f(x + w - r)} {f(y0)}"
        f"H{f(xr)}A{f(rf)} {f(rf)} 0 0 0 {f(tr[0])} {f(tr[1])}"
        f"A{f(rd)} {f(rd)} 0 {large} 1 {f(tl[0])} {f(tl[1])}"
        f"A{f(rf)} {f(rf)} 0 0 0 {f(xl)} {f(y0)}"
        f"H{f(x + r)}A{f(r)} {f(r)} 0 0 1 {f(x + r)} {f(y)}Z"
    )


def pendant(top, cx, flare, neck, r1, cap_y, cap_w, cap_h, r2):
    """Liquid hanging from the line y=top: a meniscus, a neck and a capsule.

    flare: half width on the ceiling is neck/2 + r1. cap_y is the capsule's top."""
    n = neck / 2
    cr = cap_h / 2
    xl, xr = cx - cap_w / 2, cx + cap_w / 2
    return (
        f"M{f(cx - n - r1)} {f(top)}H{f(cx + n + r1)}"
        f"A{f(r1)} {f(r1)} 0 0 0 {f(cx + n)} {f(top + r1)}"
        f"V{f(cap_y - r2)}A{f(r2)} {f(r2)} 0 0 0 {f(cx + n + r2)} {f(cap_y)}"
        f"H{f(xr - cr)}A{f(cr)} {f(cr)} 0 0 1 {f(xr - cr)} {f(cap_y + cap_h)}"
        f"H{f(xl + cr)}A{f(cr)} {f(cr)} 0 0 1 {f(xl + cr)} {f(cap_y)}"
        f"H{f(cx - n - r2)}A{f(r2)} {f(r2)} 0 0 0 {f(cx - n)} {f(cap_y - r2)}"
        f"V{f(top + r1)}A{f(r1)} {f(r1)} 0 0 0 {f(cx - n - r1)} {f(top)}Z"
    )


def svg(body, w=256, h=256, title="notchbase sketch"):
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" role="img">'
        f"<title>{title}</title>{body}</svg>\n"
    )
