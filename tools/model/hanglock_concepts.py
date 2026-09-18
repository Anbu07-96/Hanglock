"""Phase 2.5B concept renderer: original proposals for the desktop composition and the face.

Why this file exists
--------------------
`hanglock_ref.py` is the *shipped* model: its solver constants are pinned against
`tests/golden/trace_drag_settle.txt`, and its `render()` draws what `hanglock-render` draws today.
Both must stay exactly as they are, because the golden trace and the six Phase 1 previews are the
record of the design that is in the product.

Refining a look therefore needs a second renderer that (a) reuses the shared canvas, its
anti-aliasing and its palette, and (b) changes only *drawing*. That is this file: it imports the
painter from `hanglock_ref`, never edits a constant there, and composes candidate designs from its
own local parameters. Nothing here is production code. It is the visual-iteration loop the Phase
2.5B brief asks for, so that a concept can be accepted or rejected from a picture instead of from
an argument about one.

What is on the table
--------------------
Three compositions (Slate, Glass, Thread) and one face decision shared by all three: replace the
heavy round-capped monoline strokes with a chamfered segment face whose verticals carry slightly
more ink than its horizontals.

    python3 tools/model/hanglock_concepts.py all docs/previews/phase-2.5
    python3 tools/model/hanglock_concepts.py one /tmp/x.png slate
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from hanglock_ref import Canvas, capsule, clamp, smooth, write_png  # shared; nothing is rebound

LIGHT_BG = (238, 236, 230)
DARK_BG = (26, 28, 33)
# The room the swept box leaves above the anchor: the clamp's own height, the anchor inset and the
# margin, which is exactly how `placement::swept_box` builds its top edge. 9 + 14 + 16 = 39.
ANCHOR_HEAD = 39.0


# ---------------------------------------------------------------------------------------------
# primitives the shipped painter does not have
# ---------------------------------------------------------------------------------------------
def bar(cv, ax, ay, bx, by, hw, col, end=0.18, aa=1.05):
    """A stroke segment: a rounded rectangle along a-b, half-width `hw`, corner rounding
    `end * hw`. At `end = 0.5` this is a capsule, which is what the cord and every glyph in the
    shipped face are; a small value gives a flat terminal with a hair of softness on it. One
    primitive for both means the join style of a face and the join style of a rope cannot drift
    apart, which is the thing that makes a hanging object look designed.
    """
    dx, dy = bx - ax, by - ay
    ln = math.hypot(dx, dy)
    if ln < 1e-9 or hw <= 0:
        return
    r, g, b, a = col
    if a <= 0.002:
        return
    ux, uy = dx / ln, dy / ln
    nx, ny = -uy, ux
    rad = hw * min(1.0, max(0.0, end))
    half = ln * 0.5
    cx, cy = (ax + bx) / 2, (ay + by) / 2
    pad = hw + 2
    x0 = max(0, int(math.floor(min(ax, bx) - pad)))
    x1 = min(cv.w - 1, int(math.ceil(max(ax, bx) + pad)))
    y0 = max(0, int(math.floor(min(ay, by) - pad)))
    y1 = min(cv.h - 1, int(math.ceil(max(ay, by) + pad)))
    for py in range(y0, y1 + 1):
        row = py * cv.w
        for px in range(x0, x1 + 1):
            vx, vy = px + 0.5 - cx, py + 0.5 - cy
            lu = abs(vx * ux + vy * uy) - half
            lv = abs(vx * nx + vy * ny) - hw
            d = math.hypot(max(lu, 0.0), max(lv, 0.0)) + min(max(lu, lv), 0.0) - rad
            cov = smooth(aa * 0.5, -0.5 * aa, d) * a
            if cov <= 0.002:
                continue
            cv.over(row + px, r, g, b, cov)
            cv.touch(px, py)


def round_rect(cv, cx, cy, hw, hh, rad, col, aa=1.05):
    r, g, b, a = col
    if a <= 0.002 or hw <= 0 or hh <= 0:
        return
    rad = min(rad, hw, hh)
    x0 = max(0, int(math.floor(cx - hw - 2)))
    x1 = min(cv.w - 1, int(math.ceil(cx + hw + 2)))
    y0 = max(0, int(math.floor(cy - hh - 2)))
    y1 = min(cv.h - 1, int(math.ceil(cy + hh + 2)))
    for py in range(y0, y1 + 1):
        row = py * cv.w
        for px in range(x0, x1 + 1):
            qx = max(abs(px + 0.5 - cx) - (hw - rad), 0.0)
            qy = max(abs(py + 0.5 - cy) - (hh - rad), 0.0)
            d = math.hypot(qx, qy) + min(max(qx, qy), 0.0) - rad
            cov = smooth(aa * 0.5, -0.5 * aa, d) * a
            if cov <= 0.002:
                continue
            cv.over(row + px, r, g, b, cov)
            cv.touch(px, py)


def soft_shadow(cv, cx, cy, hw, hh, rad, blur, drop, alpha, steps=7):
    """A shadow that falls off instead of stacking: `steps` concentric rounded rects whose coverage
    is the smooth ramp of a distance field, so the result has an edge rather than a ledge. Blur is
    expressed in the same unit as the card, which is what keeps a shadow from growing fatter than
    the object that casts it."""
    if alpha <= 0.002:
        return
    for i in range(steps):
        t = i / float(steps - 1)
        grow = blur * (0.35 + 1.0 * t)
        a = alpha * (1.0 - t) * (1.0 - t) * 0.55
        round_rect(cv, cx, cy + drop, hw + grow, hh + grow, rad + grow, (0.0, 0.0, 0.0, a))


def blit(dst, src, x0, y0):
    for y in range(src.h):
        for x in range(src.w):
            i = y * src.w + x
            a = src.a[i]
            if a <= 0.004:
                continue
            px, py = x0 + x, y0 + y
            if px < 0 or py < 0 or px >= dst.w or py >= dst.h:
                continue
            dst.over(py * dst.w + px, src.r[i], src.g[i], src.b[i], a)
            dst.touch(px, py)


def swept_half_width(hang, card_w, sweep_deg, margin):
    """`hanglock_core::placement::swept_box`'s horizontal extent, from the same formula, so a
    footprint in a preview is the footprint on the desktop and not a drawing of one."""
    return hang * math.sin(math.radians(sweep_deg)) + card_w / 2.0 + margin


# ---------------------------------------------------------------------------------------------
# the face: a chamfered segment set, in a unit box (y = 0 cap line, y = 1 baseline)
# ---------------------------------------------------------------------------------------------
# Each segment is defined by the two corners it runs between, not by a pre-trimmed span: the slits
# are cut at draw time, per glyph, only where a neighbour is actually lit. That is what a well-made
# segment display does, and it is the difference between a face that looks engineered and one that
# looks broken - a "1" made of two separated dashes reads as a colon, while a "1" whose halves
# merge when nothing crosses them reads as a one.
SEG_DEF = {
    "a": ((0.055, 0.020), (0.945, 0.020)),
    "b": ((0.945, 0.020), (0.945, 0.500)),
    "c": ((0.945, 0.500), (0.945, 0.980)),
    "d": ((0.055, 0.980), (0.945, 0.980)),
    "e": ((0.055, 0.500), (0.055, 0.980)),
    "f": ((0.055, 0.020), (0.055, 0.500)),
    "g": ((0.055, 0.500), (0.945, 0.500)),
}
# Which segments meet each end. An end touching a lit neighbour is pulled back, so the joint shows
# a slit; an end touching a dark neighbour runs on, so a pair like b + c becomes one stroke.
NEIGH = {
    "a": (["f"], ["b"]),
    "b": (["a"], ["g"]),
    "c": (["g"], ["d"]),
    "d": (["e"], ["c"]),
    "e": (["g"], ["d"]),
    "f": (["a"], ["g"]),
    "g": (["f", "e"], ["b", "c"]),
}
TRIM = 0.055
DIGITS = {
    "0": "abcdef",
    "1": "bc",
    "2": "abged",
    "3": "abgcd",
    "4": "fbgc",
    "5": "afgcd",
    "6": "afgedc",
    "7": "abc",
    "8": "abcdefg",
    "9": "abfcgd",
}
# Each letter is a list of polylines, in the same unit box and with the same chamfered terminals as
# the digits, because a suffix set in a different idiom is what reads as a sticker.
LETTERS = {
    "A": [[(0.08, 0.985), (0.08, 0.34), (0.24, 0.05), (0.76, 0.05), (0.92, 0.34), (0.92, 0.985)],
          [(0.08, 0.62), (0.92, 0.62)]],
    "M": [[(0.06, 0.985), (0.06, 0.03), (0.50, 0.60), (0.94, 0.03), (0.94, 0.985)]],
    "P": [[(0.12, 0.985), (0.12, 0.03), (0.70, 0.03), (0.90, 0.23), (0.90, 0.44), (0.70, 0.64), (0.12, 0.64)]],
    "T": [[(0.08, 0.03), (0.92, 0.03)], [(0.50, 0.03), (0.50, 0.985)]],
    "H": [[(0.08, 0.03), (0.08, 0.985)], [(0.92, 0.03), (0.92, 0.985)], [(0.08, 0.50), (0.92, 0.50)]],
}
W_H = 0.86  # a horizontal carries less ink than the vertical beside it, or the face looks top-heavy
# Advance in cap heights, fixed. The ink box spans x = 0.055 to 0.945, so 1.04 leaves 7 % of a cap of
# sidebearing on each side: enough that "11" does not fuse, few enough that "12:34" stays tight. A
# proportional advance is what makes a clock shuffle as it ticks, so this is tabular by construction.
ADV = 1.04
TRACK = 0.00
W_V = 0.080


def line_width(text, cap):
    if not text:
        return 0.0
    return (len(text) - 1) * cap * (ADV + TRACK) + cap * ADV


def draw_text(cv, text, x, y_top, cap, col, weight=1.0, end=0.16):
    """Draw `text` with its cap line at `y_top`; returns the right edge of the last glyph."""
    hw_v = cap * W_V * weight
    hw_h = hw_v * W_H
    for ch in text:
        if ch == ":":
            for fy in (0.31, 0.69):
                round_rect(cv, x + cap * (ADV * 0.5 + 0.005), y_top + cap * fy, hw_v * 1.25,
                           hw_v * 1.25, hw_v * 0.32, col)
        elif ch in DIGITS:
            lit = set(DIGITS[ch])
            for seg in DIGITS[ch]:
                (px, py), (qx, qy) = SEG_DEF[seg]
                lo, hi = NEIGH[seg]
                t0 = TRIM if any(n in lit for n in lo) else 0.0
                t1 = TRIM if any(n in lit for n in hi) else 0.0
                horiz = abs(qy - py) < 1e-9
                if horiz:
                    ax, ay = (px + t0) * cap, py * cap
                    bx, by = (qx - t1) * cap, qy * cap
                else:
                    ax, ay = px * cap, (py + t0) * cap
                    bx, by = qx * cap, (qy - t1) * cap
                bar(cv, x + ax, y_top + ay, x + bx, y_top + by, hw_h if horiz else hw_v, col, end)
        elif ch in LETTERS:
            for pts in LETTERS[ch]:
                for i in range(len(pts) - 1):
                    horiz = abs(pts[i + 1][1] - pts[i][1]) < 1e-9
                    bar(cv, x + pts[i][0] * cap, y_top + pts[i][1] * cap,
                        x + pts[i + 1][0] * cap, y_top + pts[i + 1][1] * cap,
                        (hw_h if horiz else hw_v) * weight, col, end)
        x += cap * (ADV + TRACK)
    return x - cap * TRACK


def block_width(text, cap, gap=0.0):
    return line_width(text, cap) + gap


# ---------------------------------------------------------------------------------------------
# the three compositions
#
# Each card is stated as a size in device px at 100 % scale, exactly like `CardSpec`, and everything
# else is derived from it: the corner radius, the mount, the cord weight and the cap height all grow
# with the card, which is the property `Settings::card()` does not have today (it scales width and
# height and passes `bracket` and `corner` through unchanged, so hardware and radius fall behind as
# the clock gets bigger). The cap also *fits*: the run of digits is measured and shrunk if it would
# exceed the plate, which is what `text_bounds` does not do today.
# ---------------------------------------------------------------------------------------------
CONCEPTS = {
    # A, the recommendation. The same dark instrument plate, a quarter of the cord's visual weight,
    # a mount the plate grows out of instead of a clamp bolted to it, a contact shadow sized to the
    # card, and no accent colour on the meridiem.
    "slate": dict(
        card=(240.0, 66.0), cap_ratio=0.52, corner=8.0, hang=96.0, scale=0.85,
        rail=(30.0, 4.0), neck=(15.0, 5.0), eyelet=3.2,
        plate_top=(0.128, 0.142, 0.162), plate_bot=(0.060, 0.067, 0.079), plate_a=(0.945, 0.965),
        rim_all=0.13, rim_top=0.22, rim_bot=0.16,
        ink=(0.955, 0.962, 0.978), ink_suffix=(0.735, 0.765, 0.805), suffix_a=0.90,
        suffix=0.34, suffix_gap=0.22,
        cord=0.030, cord_lit=0.30, cord_col=(0.34, 0.36, 0.40, 0.92),
        cord_lit_col=(0.86, 0.89, 0.94, 0.22),
        mount_col=(0.098, 0.110, 0.132, 0.95),
        shadow=(0.15, 0.045, 0.035), weight=0.95, topband=0.0,
    ),
    # B. A thinner, brighter object: the plate turns toward glass - lower alpha, a lit top band, a
    # wide very faint ambient shadow instead of one dark blob, and a hairline cord.
    "glass": dict(
        card=(252.0, 58.0), cap_ratio=0.56, corner=12.0, hang=112.0, scale=0.90,
        rail=(24.0, 3.0), neck=(11.0, 3.5), eyelet=3.4,
        plate_top=(0.145, 0.160, 0.185), plate_bot=(0.070, 0.080, 0.096), plate_a=(0.855, 0.900),
        rim_all=0.22, rim_top=0.44, rim_bot=0.12,
        ink=(0.980, 0.984, 0.992), ink_suffix=(0.820, 0.845, 0.880), suffix_a=0.86,
        suffix=0.32, suffix_gap=0.26,
        cord=0.024, cord_lit=0.26, cord_col=(0.40, 0.43, 0.48, 0.82),
        cord_lit_col=(0.90, 0.93, 0.97, 0.26),
        mount_col=(0.26, 0.29, 0.34, 0.62),
        shadow=(0.09, 0.130, 0.030), weight=0.90, topband=0.055,
    ),
    # C. The smallest thing that is still a clock: no mount hardware at all, the cord passing
    # through an eyelet punched in the plate's own top edge, and a 1 px thread for a rope.
    "thread": dict(
        card=(184.0, 50.0), cap_ratio=0.58, corner=6.0, hang=72.0, scale=0.95,
        rail=(0.0, 0.0), neck=(0.0, 0.0), eyelet=3.0,
        plate_top=(0.102, 0.112, 0.130), plate_bot=(0.042, 0.047, 0.055), plate_a=(0.960, 0.975),
        rim_all=0.10, rim_top=0.17, rim_bot=0.20,
        ink=(0.960, 0.968, 0.980), ink_suffix=(0.680, 0.715, 0.755), suffix_a=0.92,
        suffix=0.36, suffix_gap=0.18,
        cord=0.016, cord_lit=0.20, cord_col=(0.38, 0.40, 0.44, 0.90),
        cord_lit_col=(0.88, 0.91, 0.95, 0.18),
        mount_col=(0.0, 0.0, 0.0, 0.0),
        shadow=(0.10, 0.035, 0.028), weight=1.0, topband=0.0,
    ),
}


def run_caps(k, chars, with_suffix):
    """The width of the whole run - digits, gap, meridiem - measured in cap heights."""
    w = chars * (ADV + TRACK) - TRACK
    if with_suffix:
        w += k["suffix_gap"] + 2 * k["suffix"] * (ADV + TRACK)
    return w


def cap_for(k, card_h, card_w, chars, with_suffix):
    """Cap height: the design ratio, or smaller if the run would not fit inside the plate.

    This is the rule `hanglock-render` lacks. Today `time_cap` is a flat fraction of the card's
    height and the run is centred on the card without ever being measured against its width, so
    `10:42:07 PM` at 8 glyphs is 287 device px wide on a 252 px plate: the leading 1 and the whole
    meridiem are painted off the plate, onto the desktop, at every size.
    """
    inset = card_w * 0.10
    fit = (card_w - 2 * inset) / max(1.0, run_caps(k, chars, with_suffix))
    return min(card_h * k["cap_ratio"], fit)


def draw_concept(cv, name, text, suffix, scale=1.0, theta=0.0, anchor=None, hang=None, dpi=1.0):
    """Paint one concept. `theta` tilts the object about its mount, which is how a hanging plate
    really moves; every preview here is drawn at rest, because a first frame that opens 17 degrees
    off vertical is itself one of the things being fixed."""
    k = CONCEPTS[name]
    s = scale * dpi
    card_w, card_h = k["card"][0] * s, k["card"][1] * s
    corner = k["corner"] * s
    cap = cap_for(k, card_h, card_w, len(text), bool(suffix))
    hang = (k["hang"] * dpi) if hang is None else hang * dpi
    ax, ay = anchor if anchor else (cv.w * 0.5, 8.0 * dpi)
    neck_h = k["neck"][1] * s
    top = ay + hang
    cx, cy = ax, top + card_h / 2.0
    sa, ca = math.sin(theta), math.cos(theta)

    def rot(px, py):
        dx, dy = px - ax, py - ay
        return ax + dx * ca - dy * sa, ay + dx * sa + dy * ca

    # cord: one body plus one lit edge. `paint.rs` draws three passes - a blurred twin at 1.6x the
    # width underneath, the body, and a highlight - which on a 66 px plate is a 5 px cable with an
    # 8 px halo, and the halo is what makes a hanging line the loudest thing on the screen.
    w = card_h * k["cord"]
    ex, ey = rot(cx, top + card_h * 0.04)
    bx, by = rot(ax, ay + k["rail"][1] * s * 0.5)
    if w > 0.16:
        capsule(cv, bx, by, ex, ey, w, k["cord_col"], aa=1.05)
        capsule(cv, bx - w * 0.32, by - w * 0.32, ex - w * 0.32, ey - w * 0.32,
                w * k["cord_lit"], k["cord_lit_col"], aa=1.05)
    # the rail the cord is tied to: a flat bar against the screen edge, not a block over it
    rw, rh = k["rail"][0] * s, k["rail"][1] * s
    if rw > 0.6:
        round_rect(cv, ax, ay + rh * 0.4, rw * 0.5, rh * 0.5, rh * 0.5, k["mount_col"])
        bar(cv, ax - rw * 0.5 + rh, ay + rh * 0.05, ax + rw * 0.5 - rh, ay + rh * 0.05,
            max(0.45, 0.4 * s), (1, 1, 1, 0.16), end=0.0)
    # neck: the plate tapers into the mount, so the joint is a part of the object
    nw, nh = k["neck"][0] * s, neck_h
    if nw > 0.6:
        hx, hy = rot(cx, top + nh * 0.45)
        round_rect(cv, hx, hy, nw * 0.5, nh * 0.75, min(nw, nh) * 0.42, k["mount_col"])
    # shadow, then plate
    sh_a, sh_blur, sh_drop = k["shadow"]
    soft_shadow(cv, cx, cy, card_w / 2.0, card_h / 2.0, corner, cap * sh_blur, cap * sh_drop, sh_a)
    pt, pb = k["plate_top"], k["plate_bot"]
    at, ab = k["plate_a"]
    for i in range(int(math.ceil(card_h)) + 1):
        y = top + i + 0.5
        t = clamp(i / max(1.0, card_h - 1.0), 0.0, 1.0)
        col = tuple(pt[j] + (pb[j] - pt[j]) * t for j in range(3)) + (at + (ab - at) * t,)
        inset = 0.0
        if corner > 0.5:
            for edge in (i, card_h - i):
                if 0 <= edge < corner:
                    inset = max(inset, corner - math.sqrt(max(0.0, corner * corner - (corner - edge) ** 2)))
        bar(cv, cx - card_w / 2.0 + inset, y, cx + card_w / 2.0 - inset, y, 0.55, col, end=0.0)
    if k["topband"] > 0:
        band = card_h * 0.26
        for i in range(int(band)):
            a = k["topband"] * (1.0 - i / band)
            yy = top + 1.0 + i
            inset = 0.0
            if corner > 0.5 and i < corner:
                inset = corner - math.sqrt(max(0.0, corner * corner - (corner - i) ** 2))
            bar(cv, cx - card_w / 2 + inset, yy, cx + card_w / 2 - inset, yy, 0.55,
                (1, 1, 1, a), end=0.0)
    # rim: a hair of light so the plate stays an object on a dark wallpaper, and a seat line under
    # it so it does not float on a light one
    bar(cv, cx - card_w / 2 + corner, top + 0.62 * s, cx + card_w / 2 - corner, top + 0.62 * s,
        0.62 * s, (1, 1, 1, k["rim_top"]), end=0.0)
    bar(cv, cx - card_w / 2 + 0.55 * s, top + corner, cx - card_w / 2 + 0.55 * s, top + card_h - corner,
        0.55 * s, (1, 1, 1, k["rim_all"]), end=0.0)
    bar(cv, cx + card_w / 2 - 0.55 * s, top + corner, cx + card_w / 2 - 0.55 * s, top + card_h - corner,
        0.55 * s, (1, 1, 1, k["rim_all"]), end=0.0)
    bar(cv, cx - card_w / 2 + corner, top + card_h - 0.55 * s, cx + card_w / 2 - corner,
        top + card_h - 0.55 * s, 0.55 * s, (0, 0, 0, k["rim_bot"]), end=0.0)
    # the eyelet: a ring the cord actually terminates in, so the joint is deliberate
    er = k["eyelet"] * s
    ix, iy = rot(cx, top + er * 0.55)
    round_rect(cv, ix, iy, er * 1.55, er * 1.35, er, (0.62, 0.67, 0.75, 0.42))
    round_rect(cv, ix, iy, er * 0.80, er * 0.62, er * 0.42, (0.015, 0.02, 0.03, 0.90))
    # text: the digit block is centred on the plate, the meridiem rides the cap line in the same
    # ink at lower alpha, and the gap is a fraction of the cap so it survives Bigger and Smaller
    s_cap = cap * k["suffix"]
    gap = cap * k["suffix_gap"]
    total = line_width(text, cap) + (gap + line_width(suffix, s_cap) if suffix else 0.0)
    x = cx - total / 2.0
    y_top = cy - cap * 0.48
    xr = draw_text(cv, text, x, y_top, cap, tuple(k["ink"]) + (1.0,), weight=k["weight"], end=0.16)
    if suffix:
        draw_text(cv, suffix, xr + gap, y_top + (cap - s_cap) * 0.06, s_cap,
                  tuple(k["ink_suffix"]) + (k["suffix_a"],), weight=k["weight"], end=0.22)
    return card_w, card_h


def flatten(cv, bg):
    return cv.over_solid(bg)


def canvas_for(name, text, suffix, scale=1.0, hang=None, dpi=1.0):
    k = CONCEPTS[name]
    s = scale * dpi
    w = k["card"][0] * s
    h = k["card"][1] * s
    hang = (k["hang"] * dpi) if hang is None else hang * dpi
    return w + 170 * dpi, hang + h + 130 * dpi, w, h


def single(path, name, text="14:53", suffix="PM", bg=LIGHT_BG, scale=None):
    k = CONCEPTS[name]
    sc = k["scale"] if scale is None else scale
    cw, ch, _w, _h = canvas_for(name, text, suffix, sc)
    cv = Canvas(int(cw), int(ch))
    draw_concept(cv, name, text, suffix, scale=sc, anchor=(cw * 0.5, 14.0))
    write_png(path, int(cw), int(ch), flatten(cv, bg), 3)
    return int(cw), int(ch)


def stack(path, names, text="14:53", suffix="PM", bg=LIGHT_BG):
    w = 760
    rows = []
    for n in names:
        cw, ch, _a, _b = canvas_for(n, text, suffix)
        rows.append((n, ch))
    row_h = int(max(r[1] for r in rows) + 34)
    h = row_h * len(rows) + 24
    cv = Canvas(w, h)
    for y in range(h):
        t = y / max(1, h - 1)
        col = tuple((bg[j] / 255.0) * (1.0 - 0.12 * t) for j in range(3))
        for x in range(w):
            cv.over(y * w + x, col[0], col[1], col[2], 1.0)
    for idx, (n, _ch) in enumerate(rows):
        draw_concept(cv, n, text, suffix, anchor=(w * 0.5, idx * row_h + 26.0))
    write_png(path, w, h, flatten(cv, bg), 3)


def ladder(path, name="slate"):
    """The same card at the three sizes the slider reaches, to show that the corner radius, the
    mount, the eyelet and the cap all grow with it. `Settings::card()` scales only width and height
    today, which is why the hardware looks like it fell off the plate at 1.75."""
    scales = (0.75, 1.00, 1.40, 1.75)
    sizes = [canvas_for(name, "14:53", "PM", sc) for sc in scales]
    w = int(sum(x[0] for x in sizes))
    h = int(max(x[1] for x in sizes))
    cv = Canvas(w, h)
    for y in range(h):
        t = y / max(1, h - 1)
        col = tuple((LIGHT_BG[j] / 255.0) * (1.0 - 0.10 * t) for j in range(3))
        for x in range(w):
            cv.over(y * w + x, col[0], col[1], col[2], 1.0)
    x0 = 0.0
    for sc, (cw, ch, _a, _b) in zip(scales, sizes):
        draw_concept(cv, name, "14:53", "PM", scale=sc, anchor=(x0 + cw * 0.5, 16.0))
        x0 += cw
    write_png(path, w, h, flatten(cv, LIGHT_BG), 3)


def footprint(path):
    """Today's swept window and each concept's, at the same scale, on a desktop with a taskbar.

    "The rope dominates the desktop" is not only about the cord: the interactive window *is* the
    swept box, and the shipped defaults make that box 520x269 device px around a 252x96 card. Both
    halves of that number come from the same formula `placement::swept_box` uses, so the red box in
    this image is the real footprint, not a sketch of it.
    """
    W, H = 1240, 400
    cv = Canvas(W, H)
    for y in range(H):
        t = y / (H - 1)
        for x in range(W):
            s = 0.05 * math.sin(x * 0.019 + y * 0.011)
            cv.over(y * W + x,
                    (LIGHT_BG[0] / 255.0) * (1.0 + s), (LIGHT_BG[1] / 255.0) * (1.0 + s * 0.8),
                    (LIGHT_BG[2] / 255.0) * (1.0 + s * 0.5), 1.0)
    for y in range(H - 30, H):
        for x in range(W):
            cv.over(y * W + x, 0.15, 0.16, 0.185, 0.97)

    def box(x0, y0, w, h, a):
        x0, y0 = int(round(x0)), int(round(y0))
        x1, y1 = x0 + int(round(w)), y0 + int(round(h))
        for x in range(x0, x1):
            for y, al in ((y0, a), (y1 - 1, a * 0.55)):
                if 0 <= y < H:
                    cv.over(y * W + x, 0.85, 0.20, 0.16, al)
        for y in range(y0, y1):
            for x, al in ((x0, a * 0.8), (x1 - 1, a * 0.8)):
                if 0 <= x < W:
                    cv.over(y * W + x, 0.85, 0.20, 0.16, al)

    sweep, margin = 52.0, 16.0
    hang_now, card_w_now, card_h_now = 150.0, 252.0, 96.0
    half_now = swept_half_width(hang_now, card_w_now, sweep, margin)
    h_now = hang_now + card_h_now / 2.0 + 2 * margin + ANCHOR_HEAD
    ax_now = 300
    box(ax_now - half_now, 0, half_now * 2, h_now, 0.72)
    from hanglock_ref import new_sim, render  # the shipped composition, unaltered, for the comparison
    sim = new_sim()
    for _ in range(220):
        sim.step(1 / 60.0)
    cv_ref, _bg = render(sim, "14:53", "PM")
    blit(cv, cv_ref, int(ax_now - 280), 0)
    for i, n in enumerate(("slate", "glass", "thread")):
        k = CONCEPTS[n]
        sc = k["scale"]
        cw, chh = k["card"][0] * sc, k["card"][1] * sc
        hang = k["hang"]
        half = swept_half_width(hang, cw, sweep, margin)
        hh = hang + chh / 2.0 + 2 * margin + ANCHOR_HEAD
        axx = int(900 + i * 170)
        box(axx - half, 0, half * 2, hh, 0.32)
        draw_concept(cv, n, "14:53", "PM", anchor=(axx, 16.0))
    write_png(path, W, H, flatten(cv, LIGHT_BG), 3)


def face_sheet(path):
    """The proposed face at the sizes it will actually be seen at, plus the fit rule in action."""
    W, H = 640, 320
    cv = Canvas(W, H)
    for y in range(H):
        for x in range(W):
            cv.over(y * W + x, 0.980, 0.980, 0.973, 1.0)
    k = CONCEPTS["slate"]
    ink = (0.06, 0.07, 0.08, 1.0)
    draw_text(cv, "0123456789", 40, 34, 40, ink)
    draw_text(cv, "14:53", 40, 104, 62, ink)
    for row, (txt, sfx, chars) in enumerate((("14:53", "PM", 5), ("14:53:07", "PM", 8), ("23:45", "", 5))):
        cap = cap_for(k, 66.0, 240.0, chars, bool(sfx))
        w = line_width(txt, cap) + (cap * k["suffix_gap"] + line_width(sfx, cap * k["suffix"]) if sfx else 0.0)
        round_rect(cv, 40 + w / 2 + 6, 210 + row * 34, w / 2 + 12, 15, 5, (0.11, 0.12, 0.14, 1.0))
        draw_text(cv, txt, 40 + 6, 210 + row * 34 - cap * 0.48, cap, (0.96, 0.97, 0.98, 1.0))
        if sfx:
            sc = cap * k["suffix"]
            draw_text(cv, sfx, 40 + 6 + line_width(txt, cap) + cap * k["suffix_gap"],
                      210 + row * 34 - cap * 0.48 + (cap - sc) * 0.06, sc, (0.74, 0.77, 0.81, 0.9))
    write_png(path, W, H, flatten(cv, (250, 250, 248)), 3)


def today_first_frame(path):
    """The frame the app actually paints first, from the shipped renderer, unaltered.

    Committed here because it is the evidence for the launch-composition finding: `hanglock_ref.py
    render` writes this as `00_rest`, and it is the one frame nobody reviewed - `01-settled-light.png`
    in Phase 1 is the settled, flattering one. At the shipped `initial_angle` of 0.30 rad the card
    appears 17.2 degrees off vertical, ~44 px sideways of its anchor, with the cord a diagonal.
    """
    from hanglock_ref import new_sim, render
    cv, bg = render(new_sim(), "10:42", "PM")
    write_png(path, cv.w, cv.h, cv.over_solid(bg), 3)


def cmd_all(out):
    os.makedirs(out, exist_ok=True)
    names = ("slate", "glass", "thread")
    for n in names:
        for tag, bg in (("light", LIGHT_BG), ("dark", DARK_BG)):
            single(f"{out}/concept-{n}-{tag}.png", n, "14:53", "PM", bg)
        single(f"{out}/concept-{n}-seconds.png", n, "14:53:07", "PM", DARK_BG)
        single(f"{out}/concept-{n}-rest.png", n, "23:45", "", LIGHT_BG, scale=CONCEPTS[n]["scale"])
    stack(f"{out}/concepts-light.png", names, "14:53", "PM", LIGHT_BG)
    stack(f"{out}/concepts-dark.png", names, "14:53", "PM", DARK_BG)
    stack(f"{out}/concepts-seconds.png", names, "14:53:07", "PM", DARK_BG)
    ladder(f"{out}/slate-scale-ladder.png", "slate")
    face_sheet(f"{out}/face-concepts.png")
    today_first_frame(f"{out}/today-launch-frame.png")
    footprint(f"{out}/footprint.png")
    print("concepts ->", out)


if __name__ == "__main__":
    if len(sys.argv) < 3:
        print(__doc__)
        raise SystemExit(2)
    if sys.argv[1] == "all":
        cmd_all(sys.argv[2])
    elif sys.argv[1] == "one":
        print(single(sys.argv[2], sys.argv[3] if len(sys.argv) > 3 else "slate"))
    elif sys.argv[1] == "footprint":
        footprint(sys.argv[2])
    elif sys.argv[1] == "ladder":
        ladder(sys.argv[2], sys.argv[3] if len(sys.argv) > 3 else "slate")
    else:
        raise SystemExit(f"unknown command {sys.argv[1]!r}")
