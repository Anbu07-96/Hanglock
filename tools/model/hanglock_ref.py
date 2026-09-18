"""Hanglock reference model — the rope solver and the painter, in pure Python.

This is NOT the product. It is the design tool for it:

*   ``crates/hanglock-core``'s solver and ``crates/hanglock-render``'s painter are transcriptions
    of this file, constant for constant, so what is validated here is what ships;
*   it runs where the product cannot. This build box has no Rust toolchain and no crates.io, so
    ``cargo test`` cannot run here. The reference is how the physics *feel* and the look were
    checked instead: real renders to review, and real numbers for settle time, overshoot, cord
    stretch, refresh-rate equivalence and drag tracking;
*   it writes ``tests/golden/trace_*.json``, which ``crates/hanglock-core/tests/golden_trace.rs``
    asserts against. So a divergence between the two implementations fails on the maintainer's
    machine rather than silently shipping.

    python3 tools/model/hanglock_ref.py sheet    [out.png]  # the face, big
    python3 tools/model/hanglock_ref.py render   [dir]      # stills: rest/drag/release/swing/settled
    python3 tools/model/hanglock_ref.py metrics             # the numbers
    python3 tools/model/hanglock_ref.py trace    [path]     # golden traces for cargo test
"""
from __future__ import annotations
import json, math, os, struct, sys, zlib

TAU = math.tau

# ----------------------------------------------------------------------------------------
# config — mirrored by crates/hanglock-core/src/rope/config.rs and -src/card.rs
# ----------------------------------------------------------------------------------------
SEGMENTS       = 16
GRAVITY        = 2400.0      # logical px/s^2, +y is down
DAMPING        = 0.9997      # per fixed step: a whisper of air drag, for jitter only
FRICTION_ACC   = 215.0       # px/s^2 of dry (Coulomb) friction at the ring and the bracket
FIXED_DT       = 1.0 / 240.0
MAX_FRAME      = 0.10        # accumulator clamp: a stall cannot burst
MAX_STRETCH    = 1.02        # one-sided ceiling: 2% of a 9.4 px link is 0.2 px
RELAX_TOL      = 0.02        # px; relaxation exits early below this
RELAX_CAP      = 8           # passes cap == 8 * nodes (must exceed node count)
SLEEP_MAX_MOVE = 0.25        # px any corner may move per window; 0.25 px/0.1 s = 2.5 px/s
BRAKE_ON_MOVE  = 1.6         # px/window: below this the settle brake engages (16 px/s)
BRAKE_OFF_MOVE = 3.2         # px/window: above this it lets go again (32 px/s)
BRAKE_STEP     = 0.90        # carried-displacement multiplier while braking
RELEVEL        = 0.014       # per step: ease the hanging line back under the anchor
SETTLE_TIME    = 0.30        # s of unchanged picture, then sleep
MAX_SPEED      = 5200.0      # px/s divergence rail
REACH_RATIO    = 0.985       # drag target clamped inside this fraction of rope length
SWEEP_DEG      = 52.0        # half-angle of the sector the card may occupy, in swing too
STOP_REST      = 0.22        # restitution at that stop: a thunk, not a bounce
MASS_CARD      = 1.00        # the bob
CORD_MASS      = 0.020       # per interior node: a light cord, so energy stays in the swing
INITIAL_ANGLE  = 0.30        # rad off vertical at first appearance

CARD_W, CARD_H, BRACKET, CORNER, HANG = 252.0, 96.0, 9.0, 17.0, 150.0

# card attitude follower: (stiffness, damping, max angle). Posture is just this table.
POSTURE = {
    "natural": (120.0, 9.0,  math.radians(26.0)),
    "plate":   (420.0, 24.0, math.radians(9.0)),
    "mounted": (1400.0, 46.0, math.radians(2.5)),
    "locked":  (4000.0, 90.0, 0.0),
}
ATTITUDE_GAIN = 0.55         # how much of the cord's lean the plate takes up

TIME_CAP, SUFFIX_CAP, TRACKING, STROKE = 48.0, 16.0, 0.055, 0.125
WIN_W, WIN_H = 560, 300

# palette — Hanglock's own. A dark instrument plate, a light-catching top rim, one accent.
PLATE_TOP, PLATE_BOT = (0.150, 0.166, 0.188, 0.94), (0.072, 0.079, 0.092, 0.965)
RIM_TOP, RIM_BOT = (1.0, 1.0, 1.0, 0.17), (0.0, 0.0, 0.0, 0.40)
INK, ACCENT = (0.965, 0.973, 0.985, 1.0), (0.44, 0.74, 0.99, 1.0)
# A mid-tone cord with a lit edge and a dark edge, because a near-black cord vanishes on a dark
# wallpaper and a light cord vanishes on a white document. Neither can be the answer; both passes
# together read on any desktop, and neither is a per-frame filter.
CORD, CORD_LIT, CORD_SHADOW = (0.30, 0.325, 0.37, 0.95), (0.82, 0.86, 0.92, 0.34), (0.0, 0.0, 0.0, 0.38)
RIM_ALL = (1.0, 1.0, 1.0, 0.20)          # 1 px all round: the plate separates on dark walls
RIM_W = 0.9
SHADOW_ALPHA, SHADOW_BLUR, SHADOW_DROP = 0.30, 7.5, 7.0

# ----------------------------------------------------------------------------------------
# geometry
# ----------------------------------------------------------------------------------------
def clamp(v, lo, hi): return lo if v < lo else (hi if v > hi else v)
def smooth(e0, e1, x):
    t = clamp((x - e0) / (e1 - e0), 0.0, 1.0); return t * t * (3 - 2 * t)

def dist_seg(px, py, ax, ay, bx, by):
    vx, vy = bx - ax, by - ay
    l2 = vx * vx + vy * vy
    if l2 <= 1e-12: return math.hypot(px - ax, py - ay)
    t = clamp(((px - ax) * vx + (py - ay) * vy) / l2, 0.0, 1.0)
    return math.hypot(px - (ax + t * vx), py - (ay + t * vy))

def sdf_box(lx, ly, hw, hh, r):
    """Exact outside distance to a rounded box centred on the origin (Minkowski form)."""
    qx = max(abs(lx) - (hw - r), 0.0)
    qy = max(abs(ly) - (hh - r), 0.0)
    return math.hypot(qx, qy) - r

def arc3(p0, p1, p2, seg=24):
    """Polyline of the circle through three points, taken along the branch that passes p1.
    Authoring curves as "start, bulge, end" is far more forgiving than listing angles."""
    (ax, ay), (bx, by), (cx, cy) = p0, p1, p2
    d = 2.0 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by))
    if abs(d) < 1e-9:
        return [(ax, ay), (bx, by), (cx, cy)]
    a2, b2, c2 = ax * ax + ay * ay, bx * bx + by * by, cx * cx + cy * cy
    ux = (a2 * (by - cy) + b2 * (cy - ay) + c2 * (ay - by)) / d
    uy = (a2 * (cx - bx) + b2 * (ax - cx) + c2 * (bx - ax)) / d
    r = math.hypot(ax - ux, ay - uy)
    ang = lambda x, y: math.atan2(y - uy, x - ux) % TAU
    a0, a1, a2a = ang(ax, ay), ang(bx, by), ang(cx, cy)
    span = (a2a - a0) % TAU
    delta = span if (a1 - a0) % TAU < span else -(TAU - span)   # the branch holding the bulge
    return [(ux + r * math.cos(a0 + delta * i / seg), uy + r * math.sin(a0 + delta * i / seg))
            for i in range(seg + 1)]

# ----------------------------------------------------------------------------------------
# the face. Strokes only, so one analytic-capsule primitive draws type and cord alike, with
# the same anti-aliasing and the same round joins. Original geometry: a geometric stroke
# face with tabular figures by construction (every advance is identical).
# Units: cap height 1.0, y down, digit width GW.
# ----------------------------------------------------------------------------------------
GW, ADV = 0.46, 0.62
L = lambda *p: list(p)
S = lambda *p: [list(q) for q in p]                 # one stroke: a polyline of points
def A(a, b, c, n=15): return arc3(list(a), list(b), list(c), n)   # one arc stroke, 3-point form

# Each glyph is a list of strokes. Cap height is 1.0, y grows down, width GW.
FACE_GLYPHS = {
    "0": [A((0, .26), (GW / 2, -.03), (GW, .26)) + [[GW, .74]],
          A((GW, .74), (GW / 2, 1.03), (0, .74)) + [[0, .26]]],
    "1": [S((GW * .10, .20), (GW / 2, .02), (GW / 2, 1.0))],
    "2": [A((GW * .02, .28), (GW / 2, -.03), (GW, .30)) + [[GW, .45], [0, 1.0], [GW, 1.0]]],
    "3": [A((GW * .06, .17), (GW * .66, -.03), (GW, .32)) + [[GW, .39], [GW * .48, .52]],
          A((GW * .48, .52), (GW * 1.06, .73), (GW * .60, 1.0)) + [[GW * .16, .94]]],
    "4": [S((GW * .78, .02), (GW * .78, 1.0)),
          S((GW * .78, .02), (0, .71)), S((0, .71), (GW, .71))],
    "5": [S((GW, .03), (GW * .06, .05), (GW * .05, .46), (GW * .56, .48)),
          A((GW * .56, .48), (GW * 1.08, .72), (GW * .58, 1.0)) + [[GW * .14, .92]]],
    "6": [A((GW * .90, .06), (0, .45), (GW * .46, .60)),
          A((GW * .46, .60), (GW * 1.10, .84), (GW * .46, 1.02)) + [[0, .80], [GW * .46, .60]]],
    "7": [S((0, .03), (GW, .03), (GW * .26, 1.0))],
    "8": [A((GW * .50, .48), (-GW * .04, .25), (GW * .50, .02)) + [[GW, .25], [GW * .50, .48]],
          A((GW * .50, .52), (GW * 1.06, .76), (GW * .50, 1.00)) + [[-GW * .04, .76], [GW * .50, .52]]],
    "9": [A((GW * .54, .40), (GW * -.10, .17), (GW * .54, -.02)) + [[GW, .20], [GW * .54, .40]],
          A((GW, .20), (GW * .60, 1.04), (GW * .02, .70))],
    ":": [("dot", (GW * .5, .28)), ("dot", (GW * .5, .74))],
    ".": [("dot", (GW * .5, .93))],
    "A": [S((0, 1.0), (GW * .5, .02), (GW, 1.0)), S((GW * .16, .66), (GW * .84, .66))],
    "M": [S((0, 1.0), (0, .02), (GW * .5, .60), (GW, .02), (GW, 1.0))],
    "P": [S((0, 1.0), (0, .02)), A((0, .02), (GW * 1.12, .27), (0, .55), 17)],
    "S": [A((GW * .98, .16), (GW * .46, -.03), (0, .20), 14) + [[0, .34], [GW * .5, .50], [GW, .66]],
          A((GW, .66), (GW * .58, 1.05), (0, .85), 14)],
    "T": [S((0, .02), (GW, .02)), S((GW / 2, .02), (GW / 2, 1.0))],
    "-": [S((GW * .08, .52), (GW * .92, .52))],
    " ": [],
}
assert all(isinstance(g, list) for g in FACE_GLYPHS.values())


def glyph(ch):
    lines, dots = [], []
    for it in FACE_GLYPHS.get(ch, []):
        if isinstance(it, tuple) and it[0] == "dot":
            x, y = it[1]; dots.append((x, y, 0.098))
        else:
            lines.append(it)
    return lines, dots

def layout(text, cap, tracking=TRACKING):
    """-> [(lines, dots)] in px, plus the total advance."""
    adv = cap * ADV + cap * tracking
    out, x = [], 0.0
    for ch in text:
        ln, dt = glyph(ch)
        out.append(([[L(x + px * cap, py * cap) for px, py in l] for l in ln],
                    [L(x + cx * cap, cy * cap, r * cap) for cx, cy, r in dt]))
        x += adv
    return out, (x - cap * tracking if text else 0.0)

# ----------------------------------------------------------------------------------------
# solver
# ----------------------------------------------------------------------------------------
class Sim:
    """A cord of `SEGMENTS` links, pinned at the anchor, weighted at the card's centre of
    mass, plus a damped attitude follower for the plate. Step order matters and is pinned:
    anchor -> integrate -> drive held -> relax -> project stretch -> card."""

    def __init__(self, anchor=(0.0, 0.0), scale=1.0, hang=HANG, posture="plate", host=None):
        self.anchor = anchor
        self.scale = scale
        self.host = host
        self.hang = hang * scale
        self.seg_len = self.hang / SEGMENTS
        self.card_w, self.card_h = CARD_W * scale, CARD_H * scale
        self.com_dy = (self.card_h / 2.0 + BRACKET * scale)
        self.inv_mass = ([0.0] + [1.0 / CORD_MASS] * (SEGMENTS - 1) + [1.0 / MASS_CARD])
        self.pk, self.cc, self.amax = POSTURE[posture]
        self.pts, self.prev, self.theta, self.omega = [], [], 0.0, 0.0
        self.acc, self.sleeping, self.still = 0.0, False, 0
        self.ref, self.nstep, self.braking = None, 0, False
        self.drag, self.drag_target, self.drag_v = None, (0.0, 0.0), (0.0, 0.0)
        self.reset()

    def reset(self, angle=INITIAL_ANGLE):
        self.pts = [[self.anchor[0] + math.sin(angle) * i * self.seg_len,
                     self.anchor[1] + math.cos(angle) * i * self.seg_len] for i in range(SEGMENTS + 1)]
        self.prev = [list(p) for p in self.pts]
        self.acc, self.theta, self.omega = 0.0, 0.0, 0.0
        self.drag = None
        self.ref, self.nstep, self.braking = None, 0, False
        self.wake()

    # -- solver -----------------------------------------------------------------------
    def _inv(self, i): return 0.0 if i == self.drag else self.inv_mass[i]

    def step(self, dt):
        if self.sleeping: return False
        self.acc = min(self.acc + dt, MAX_FRAME)
        n = 0
        while self.acc >= FIXED_DT:
            self._advance(FIXED_DT)
            self._sleep_check()          # per fixed step, never per frame: a check on the frame
            self.acc -= FIXED_DT         # clock would make settling (and the brake that feeds
            n += 1                       # it) depend on the display's refresh rate
            if self.sleeping:
                break                    # no work after the picture has stopped
        return n > 0

    def _advance(self, h):
        g = GRAVITY * self.scale
        self.pts[0][0], self.pts[0][1] = self.anchor[0], self.anchor[1]
        self.prev[0][0], self.prev[0][1] = self.anchor[0], self.anchor[1]
        lim = MAX_SPEED * h
        # Coulomb, not viscous, is what makes the two feel targets compatible. A velocity-
        # proportional drag that settles a clock in ~2 s also erases a throw; dry friction
        # takes a fixed bite per step, so a fast swing keeps most of its energy while the
        # slow tail is cut off in a bounded time instead of decaying for ever.
        dv = FRICTION_ACC * self.scale * h * h
        for i in range(1, len(self.pts)):
            if i == self.drag: continue
            p, q = self.pts[i], self.prev[i]
            b = BRAKE_STEP if self.braking else 1.0
            vx, vy = (p[0] - q[0]) * DAMPING * b, (p[1] - q[1]) * DAMPING * b
            m = math.hypot(vx, vy)
            if m > 1e-12:
                k = max(0.0, m - dv) / m
                vx *= k; vy *= k
            if m > lim:
                vx *= lim / m; vy *= lim / m
            q[0], q[1] = p[0], p[1]
            p[0] += vx
            p[1] += vy + g * h * h
        if self.drag is not None:
            # The held node chases the cursor at a finite rate. At human pointer speeds the
            # limit is never reached, so tracking is exact; past it the card lags instead of
            # the rope tearing, which is both the honest model and the stable one.
            p, q = self.pts[self.drag], self.prev[self.drag]
            gx, gy = self.drag_target
            dx, dy = gx - p[0], gy - p[1]
            step_lim = MAX_SPEED * h
            m = math.hypot(dx, dy)
            if m > step_lim: dx, dy = dx * step_lim / m, dy * step_lim / m
            p[0] += dx; p[1] += dy
            q[0] = p[0] - self.drag_v[0] * h
            q[1] = p[1] - self.drag_v[1] * h
        self._relax()
        self._project()
        if self.braking:
            # A cord under tension hangs straight; a solver with friction can park a few degrees
            # off it, and a clock frozen crooked reads as broken rather than as at rest. So while
            # the brake is on, ease the hanging line back under the anchor. Never during a swing
            # (braking is only engaged below BRAKE_ON_MOVE), so it cannot touch the physics that
            # are actually being felt.
            ax = self.anchor[0]
            for i in range(1, len(self.pts)):
                q = self.pts[i]
                q[0] += (ax - q[0]) * RELEVEL
            self.theta *= (1.0 - 8.0 * h)
        self._stop(h)
        self._card(h)

    def _relax(self):
        rest, cap = self.seg_len, RELAX_CAP * (SEGMENTS + 1)
        for _ in range(cap):
            worst = 0.0
            for i in range(SEGMENTS):
                ia, ib = self._inv(i), self._inv(i + 1)
                tot = ia + ib
                if tot <= 0.0: continue
                a, b = self.pts[i], self.pts[i + 1]
                dx, dy = b[0] - a[0], b[1] - a[1]
                d = math.hypot(dx, dy)
                if d <= 1e-9: continue
                c = (d - rest) / d / tot
                cx, cy = dx * c, dy * c
                a[0] += cx * ia; a[1] += cy * ia
                b[0] -= cx * ib; b[1] -= cy * ib
                worst = max(worst, abs(cx * ia), abs(cy * ia), abs(cx * ib), abs(cy * ib))
            if worst < RELAX_TOL: break

    def _project(self):
        """One-sided ceiling: links inside the limit are untouched, so this converges where a
        two-sided snap would oscillate."""
        limit = self.seg_len * MAX_STRETCH
        for _ in range(40):
            moved = False
            for i in range(SEGMENTS):
                ia, ib = self._inv(i), self._inv(i + 1)
                tot = ia + ib
                if tot <= 0.0: continue
                a, b = self.pts[i], self.pts[i + 1]
                dx, dy = b[0] - a[0], b[1] - a[1]
                d = math.hypot(dx, dy)
                if d <= limit or d <= 1e-9: continue
                c = (d - limit) / d / tot
                a[0] += dx * c * ia; a[1] += dy * c * ia
                b[0] -= dx * c * ib; b[1] -= dy * c * ib
                moved = True
            if not moved: break

    def _stop(self, h):
        """The cord cannot leave the sector, swinging or dragged. Not a nicety: an unbounded
        clock on a 150 px cord reaches ~100 degrees of swing, which puts the plate off the
        monitor and behind the taskbar, and it is the stop that lets the overlay window be a
        fixed sector-shaped box instead of a full circle. Radial motion is already handled by
        the inextensibility constraints; this clamps the angle, keeps the tangential part of
        the velocity, and takes a bite out of it so hitting the stop reads as a thunk."""
        ax, ay = self.anchor
        i = len(self.pts) - 1
        p, q = self.pts[i], self.prev[i]
        ang = math.atan2(p[0] - ax, p[1] - ay)
        lim = math.radians(SWEEP_DEG)
        if -lim <= ang <= lim:
            return
        side = 1.0 if ang > 0 else -1.0
        r = math.hypot(p[0] - ax, p[1] - ay)
        ca, sa = math.cos(lim), math.sin(lim)
        p[0], p[1] = ax + side * r * sa, ay + r * ca
        # reflect the angular part of the implied velocity, keep the rest
        va = (p[0] - q[0]) * side * ca - (p[1] - q[1]) * sa
        q[0] = p[0] - ((p[0] - q[0]) - va * side * ca * (1.0 + STOP_REST))
        q[1] = p[1] - ((p[1] - q[1]) + va * sa * (1.0 + STOP_REST))

    def _card(self, h):
        before, tail = self.pts[-2], self.pts[-1]
        lean = math.atan2(tail[0] - before[0], tail[1] - before[1])
        target = clamp(ATTITUDE_GAIN * lean * (1.0 + 0.25 * MASS_CARD), -self.amax, self.amax)
        self.omega += (self.pk * (target - self.theta) - self.cc * self.omega) * h
        self.theta += self.omega * h
        if abs(self.theta) > self.amax:
            self.theta = math.copysign(self.amax, self.theta)
            self.omega *= -0.15

    STILL_WINDOW = 24        # fixed steps between samples: 0.1 s at 240 Hz. The thresholds
                         # below are px *per window*, so this number is what turns
                         # them into speeds — change it and re-derive all three.

    def _sleep_check(self):
        """Sleep when the *picture* has stopped changing. That is the product requirement, and
        it is why the test is not "every node is slower than X": two such tests failed here. A
        per-step speed test with a threshold under RELAX_TOL asks relaxation to converge finer
        than its own tolerance and therefore never sleeps (a settled clock stayed awake 11 s),
        and a test on the plate alone slept while the cord above it still crept. So: sample the
        whole chain every STILL_WINDOW steps and sleep once no node has moved further than half
        a pixel and the plate has not turned, for SETTLE_TIME running. Sub-pixel per 50 ms is
        not something a person can see, and the check is one pass over the chain per step."""
        if self.drag is not None:
            self.still = 0
            self.ref = None
            return
        self.nstep += 1
        if self.nstep % self.STILL_WINDOW:
            return
        now = self.corners()
        if self.ref is None:
            # No baseline: see the note in the Rust port. Never infer "still" from a missing sample.
            self.ref = now
            return
        moved = max(math.hypot(b[0] - a[0], b[1] - a[1]) for a, b in zip(self.ref, now))
        self.ref = now
        if moved > BRAKE_OFF_MOVE * self.scale:
            self.braking = False
        elif moved < BRAKE_ON_MOVE * self.scale:
            self.braking = True
        if moved > SLEEP_MAX_MOVE * self.scale:
            self.still = 0
            return
        self.still += self.STILL_WINDOW
        if self.still * FIXED_DT >= SETTLE_TIME:
            self.sleeping = True

    def corners(self):
        """The plate's four corners: what a person can actually see move. Testing the drawn
        silhouette rather than a node index is what makes the sleep rule honest."""
        cx, cy = self.pts[-1]
        hw, hh = self.card_w / 2.0, self.card_h / 2.0
        ct, st = math.cos(self.theta), math.sin(self.theta)
        return [(cx + x * ct - y * st, cy + x * st + y * ct)
                for x, y in ((-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh))]

    def wake(self): self.sleeping = False; self.still = 0

    # -- metrics used by tests --------------------------------------------------------
    def max_stretch(self):
        w = 1.0
        for i in range(SEGMENTS):
            a, b = self.pts[i], self.pts[i + 1]
            w = max(w, math.hypot(b[0] - a[0], b[1] - a[1]) / self.seg_len)
        return w

    # -- interaction ------------------------------------------------------------------
    def in_card(self, x, y, pad=0.0):
        cx, cy = self.pts[-1]
        ct, st = math.cos(-self.theta), math.sin(-self.theta)
        px, py = x - cx, y - cy
        return (abs(px * ct - py * st) <= self.card_w / 2 + pad
                and abs(px * st + py * ct) <= self.card_h / 2 + pad)

    def reachable(self, x, y):
        """Taut instead of torn: inside the reach circle, inside the sweep sector, inside the
        host rect (and never above the anchor, since a cord cannot push)."""
        ax, ay = self.anchor
        dx, dy = x - ax, y - ay
        r = math.hypot(dx, dy)
        reach = self.hang * REACH_RATIO
        if r > reach: dx, dy = dx * reach / r, dy * reach / r; r = reach
        if r > 1e-9:
            ang = clamp(math.atan2(dx, dy), -math.radians(SWEEP_DEG), math.radians(SWEEP_DEG))
            dx, dy = math.sin(ang) * r, math.cos(ang) * r
        x, y = ax + dx, ay + dy
        if self.host:
            hx0, hy0, hx1, hy1 = self.host
            x = clamp(x, hx0, hx1); y = clamp(y, ay + 0.05 * self.hang, hy1)
        return x, y

    def begin_drag(self, x, y, pad=10.0):
        if not self.in_card(x, y, pad * self.scale): return False
        self.drag = SEGMENTS
        self.drag_target = self.reachable(x, y)
        self.drag_v = (0.0, 0.0)
        self.wake(); return True

    def move_drag(self, x, y, vel):
        if self.drag is None: return
        self.drag_target = self.reachable(x, y)
        vx, vy = vel
        m = math.hypot(vx, vy)
        if m > MAX_SPEED: vx, vy = vx * MAX_SPEED / m, vy * MAX_SPEED / m
        self.drag_v = (vx, vy)

    def end_drag(self):
        self.drag = None
        self.drag_v = (0.0, 0.0)
        self.wake()

# ----------------------------------------------------------------------------------------
# painter
# ----------------------------------------------------------------------------------------
class Canvas:
    """Straight (not premultiplied) float RGBA, because that is what a source-over painter
    wants; the conversion to premultiplied BGRA happens once, on present, which is exactly
    what UpdateLayeredWindow demands."""

    def __init__(self, w, h):
        self.w, self.h = w, h
        n = w * h
        self.r = [0.0] * n; self.g = [0.0] * n; self.b = [0.0] * n; self.a = [0.0] * n
        self.bounds = [w, h, -1, -1]

    def touch(self, x, y):
        b = self.bounds
        b[0] = min(b[0], x); b[1] = min(b[1], y); b[2] = max(b[2], x); b[3] = max(b[3], y)

    def over(self, i, r, g, b, a):
        da = self.a[i]
        na = a + da * (1.0 - a)
        if na <= 1e-6: return
        inv = 1.0 - a
        self.r[i] = (r * a + self.r[i] * inv) / na
        self.g[i] = (g * a + self.g[i] * inv) / na
        self.b[i] = (b * a + self.b[i] * inv) / na
        self.a[i] = na

    def premul_bgra(self):
        out = bytearray(self.w * self.h * 4)
        for i in range(self.w * self.h):
            al = clamp(self.a[i], 0.0, 1.0); j = i * 4
            out[j] = int(round(clamp(self.b[i], 0.0, 1.0) * al * 255.0))
            out[j + 1] = int(round(clamp(self.g[i], 0.0, 1.0) * al * 255.0))
            out[j + 2] = int(round(clamp(self.r[i], 0.0, 1.0) * al * 255.0))
            out[j + 3] = int(round(al * 255.0))
        return bytes(out)

    def over_solid(self, rgb):
        """Flatten onto an opaque backdrop, for previews a human can look at."""
        n = self.w * self.h
        out = bytearray(n * 3)
        br, bg, bb = rgb
        for i in range(n):
            a = clamp(self.a[i], 0.0, 1.0); j = i * 3
            out[j] = int(round(clamp(self.r[i], 0.0, 1.0) * 255 * a + br * (1 - a)))
            out[j + 1] = int(round(clamp(self.g[i], 0.0, 1.0) * 255 * a + bg * (1 - a)))
            out[j + 2] = int(round(clamp(self.b[i], 0.0, 1.0) * 255 * a + bb * (1 - a)))
        return out

def capsule(cv, ax, ay, bx, by, rad, col, aa=1.05):
    x0 = max(0, int(math.floor(min(ax, bx) - rad - 1))); x1 = min(cv.w - 1, int(math.ceil(max(ax, bx) + rad + 1)))
    y0 = max(0, int(math.floor(min(ay, by) - rad - 1))); y1 = min(cv.h - 1, int(math.ceil(max(ay, by) + rad + 1)))
    r, g, b, a = col
    for py in range(y0, y1 + 1):
        row = py * cv.w
        for px in range(x0, x1 + 1):
            d = dist_seg(px + .5, py + .5, ax, ay, bx, by) - rad
            cov = smooth(aa * .5, -.5 * aa, d) * a
            if cov <= .002: continue
            cv.over(row + px, r, g, b, cov)
            cv.touch(px, py)

def _fill_round(cv, x0, y0, x1, y1, rad, col):
    hw, hh = (x1 - x0) / 2, (y1 - y0) / 2
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    r, g, b, a = col
    for py in range(max(0, int(y0) - 1), min(cv.h, int(y1) + 2)):
        for px in range(max(0, int(x0) - 1), min(cv.w, int(x1) + 2)):
            d = sdf_box(px + .5 - cx, py + .5 - cy, hw, hh, min(rad, hh, hw))
            cov = smooth(.55, -.55, d) * a
            if cov <= .002: continue
            cv.over(py * cv.w + px, r, g, b, cov); cv.touch(px, py)

def _stroke_round(cv, cx, cy, rad, w, col):
    """Annulus: difference of two circles. The ring the cord passes through."""
    r, g, b, a = col
    for py in range(max(0, int(cy - rad - w)), min(cv.h, int(cy + rad + w) + 1)):
        for px in range(max(0, int(cx - rad - w)), min(cv.w, int(cx + rad + w)) + 1):
            d = abs(math.hypot(px + .5 - cx, py + .5 - cy) - rad) - w * .5
            cov = smooth(.55, -.55, d) * a
            if cov <= .002: continue
            cv.over(py * cv.w + px, r, g, b, cov); cv.touch(px, py)

def soft_shadow(cv, cx, cy, hw, hh, rad, rot, drop, blur, alpha):
    """Analytic: coverage of the plate SDF pushed down and dilated by blur. One pass, no
    filter, and therefore nothing that could keep the app awake."""
    x0 = max(0, int(cx - hw - blur * 3)); x1 = min(cv.w - 1, int(cx + hw + blur * 3))
    y0 = max(0, int(cy - hh - blur * 3)); y1 = min(cv.h - 1, int(cy + hh + drop + blur * 3))
    ct, st = math.cos(-rot), math.sin(-rot)
    for py in range(y0, y1 + 1):
        row = py * cv.w
        for px in range(x0, x1 + 1):
            lx = (px + .5 - cx) * ct - (py + .5 - cy - drop) * st
            ly = (px + .5 - cx) * st + (py + .5 - cy - drop) * ct
            d = sdf_box(lx, ly, hw, hh, rad)
            cov = smooth(blur, -.4 * blur, d) * alpha
            if cov <= .002: continue
            cv.over(row + px, 0.0, 0.0, 0.0, cov)
            cv.touch(px, py)

def plate_and_text(cv, sim, text, suffix):
    cx, cy = sim.pts[-1]
    hw, hh, th = sim.card_w / 2.0, sim.card_h / 2.0, sim.theta
    k = sim.scale
    ct, st = math.cos(-th), math.sin(-th)
    r1, g1, b1, a1 = PLATE_TOP
    r2, g2, b2, a2 = PLATE_BOT
    rad = CORNER * k
    R = math.hypot(hw, hh) + 2
    x0 = max(0, int(cx - R)); x1 = min(cv.w - 1, int(cx + R))
    y0 = max(0, int(cy - R)); y1 = min(cv.h - 1, int(cy + R))
    for py in range(y0, y1 + 1):
        row = py * cv.w
        for px in range(x0, x1 + 1):
            lx = (px + .5 - cx) * ct - (py + .5 - cy) * st
            ly = (px + .5 - cx) * st + (py + .5 - cy) * ct
            d = sdf_box(lx, ly, hw, hh, rad)
            cov = smooth(.55, -.55, d)
            if cov <= 0: continue
            t = clamp((ly + hh) / (2 * hh), 0.0, 1.0)
            r, g, b, a = r1 + (r2 - r1) * t, g1 + (g2 - g1) * t, b1 + (b2 - b1) * t, a1 + (a2 - a1) * t
            if abs(d) < RIM_W:                       # 1 px rim all round, so a dark plate on a
                r, g, b, a = RIM_ALL                 # dark desktop is still an object, not a hole
            elif abs(d) < 1.0:                       # and the top edge catches the light
                if ly < -hh + 1.6: r, g, b, a = RIM_TOP
                elif ly > hh - 2.0: r, g, b, a = RIM_BOT[0], RIM_BOT[1], RIM_BOT[2], max(a, RIM_BOT[3])
            cv.over(row + px, r, g, b, a * cov)
            cv.touch(px, py)

    def put(lines, dots, ox, oy, cap, col, wgt):
        """ox, oy is the text box origin in *card-local* px; the whole run rotates with the plate."""
        rr = cap * wgt * .5
        ct, st = math.cos(th), math.sin(th)
        def rot(px, py): return (ox + px * ct - py * st, oy + px * st + py * ct)
        for l in lines:
            rp = [rot(px, py) for px, py in l]
            for i in range(len(rp) - 1):
                capsule(cv, rp[i][0], rp[i][1], rp[i + 1][0], rp[i + 1][1], rr, col)
        for (dx, dy, dr) in dots:
            px, py = rot(dx, dy)
            capsule(cv, px, py, px, py, dr * .95, col)

    # The whole readout is one centred group: digits, a gap, then the meridiem riding high and
    # in the accent. It is centred rather than right-butted so the plate reads as an object with
    # a label on it, not a window with text in it.
    cap = TIME_CAP * k
    ln, tot = layout(text, cap)
    cap2 = SUFFIX_CAP * k
    gap = (10.0 * k) if suffix else 0.0
    ln2, tot2 = layout(suffix, cap2, 0.16) if suffix else ([], 0.0)
    # layout() already spaces the glyphs inside a run, so a run is drawn from ONE origin; the
    # previous version advanced per glyph on top of that and spread the time across the screen.
    ox = cx - (tot + gap + tot2) / 2.0
    oy = cy - hh + (2 * hh - cap) / 2.0
    for gl, gd in ln:
        put(gl, gd, ox, oy, cap, INK, STROKE)
    if suffix:
        for gl, gd in ln2:
            put(gl, gd, ox + tot + gap, oy - cap2 * 0.12, cap2, ACCENT, 0.17)

def render(sim, text, suffix, bg=(238, 236, 230)):
    cv = Canvas(WIN_W, WIN_H)
    # cord first, so the plate covers its end exactly where the object does
    poly = [list(p) for p in sim.pts]
    while len(poly) > 2 and sim.in_card(poly[-1][0], poly[-1][1], pad=1.5 * sim.scale):
        poly.pop()
    w = max(1.7 * sim.scale, sim.card_h * .026)
    for i in range(len(poly) - 1):
        ax, ay = poly[i]; bx, by = poly[i + 1]
        capsule(cv, ax + 1.3, ay + 1.3, bx + 1.3, by + 1.3, w * 1.6, CORD_SHADOW)
    for i in range(len(poly) - 1):
        ax, ay = poly[i]; bx, by = poly[i + 1]
        capsule(cv, ax, ay, bx, by, w, CORD)
    for i in range(len(poly) - 1):
        ax, ay = poly[i]; bx, by = poly[i + 1]
        capsule(cv, ax - w * .30, ay - w * .30, bx - w * .30, by - w * .30, w * .34, CORD_LIT)
    # the mount: a clamp hard against the top of the screen, and a ring the cord runs through
    ax, ay = sim.anchor
    k = sim.scale
    soft_shadow(cv, ax, ay + 1.5 * k, 21 * k, 4.2 * k, 3.0 * k, 0.0, 1.0 * k, 2.2 * k, .20)
    _fill_round(cv, ax - 21 * k, -2.0 * k, ax + 21 * k, ay + 3.4 * k, 3.0 * k, (0.085, 0.095, 0.115, 0.97))
    _stroke_round(cv, ax, ay + 8.6 * k, 3.5 * k, 1.25 * k, (0.085, 0.095, 0.115, 0.97))
    _stroke_round(cv, ax - w * .28, ay + 8.6 * k - w * .28, 3.5 * k, .5 * k, (0.62, 0.66, 0.72, .30))
    cx, cy = sim.pts[-1]
    soft_shadow(cv, cx, cy, sim.card_w / 2, sim.card_h / 2, CORNER * k, sim.theta,
                 SHADOW_DROP * k, SHADOW_BLUR * k, SHADOW_ALPHA)
    plate_and_text(cv, sim, text, suffix)
    return cv, bg

def write_png(path, w, h, rgb, channels):
    raw = bytearray()
    st = channels
    for y in range(h):
        raw.append(0)
        row = y * w * st
        raw += rgb[row:row + w * st]
    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    ct = 6 if st == 4 else 2
    png = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, ct, 0, 0, 0))
           + chunk(b"IDAT", zlib.compress(bytes(raw), 9)) + chunk(b"IEND", b""))
    d = os.path.dirname(path)
    if d: os.makedirs(d, exist_ok=True)
    open(path, "wb").write(png)

# ----------------------------------------------------------------------------------------
# drivers
# ----------------------------------------------------------------------------------------
def new_sim(scale=1.0, posture="plate", hang=HANG):
    return Sim(anchor=(WIN_W * scale / 2.0, 7.0 * scale), scale=scale, hang=hang, posture=posture,
               host=(0, 0, WIN_W * scale, WIN_H * scale))

def drag_and_release(steps=64, pull=150.0, depth=150.0):
    s = new_sim()
    out = []
    for i in range(steps):
        if i < 12:
            t = i / 11.0
            x = s.anchor[0] + pull * t
            y = s.anchor[1] + depth
            vx = (x - s.pts[-1][0]) * 60.0
            vy = (y - s.pts[-1][1]) * 60.0
            s.begin_drag(s.pts[-1][0], s.pts[-1][1]) if s.drag is None else None
            s.move_drag(x, y, (vx, vy))
        elif i == 12:
            s.end_drag()
        s.step(1 / 60.0)
        out.append((i, s))
    return out

def cmd_sheet(path):
    text, cap = "0123456789:AMPST-.", 96.0
    ln, tot = layout(text, cap)
    pad = cap * .8
    w = int(tot + pad * 2); h = int(cap * 2.6)
    cv = Canvas(w, h)
    x = pad
    for gl, gd in ln:
        for l in gl:
            rp = [(x + px, py + (h - cap) / 2) for px, py in l]
            for i in range(len(rp) - 1):
                capsule(cv, rp[i][0], rp[i][1], rp[i + 1][0], rp[i + 1][1], cap * STROKE * .5,
                        (0.06, 0.07, 0.08, 1.0))
        for (dx, dy, dr) in gd:
            capsule(cv, x + dx, dy + (h - cap) / 2, x + dx, dy + (h - cap) / 2, dr * .95, (0.06, 0.07, 0.08, 1.0))
        x += cap * ADV + cap * TRACKING
    write_png(path, w, h, cv.over_solid((250, 250, 248)), 3)
    print("sheet ->", path, f"{w}x{h}")

def cmd_render(dirpath):
    want = {0: "00_rest", 11: "11_drag", 13: "13_release", 18: "18_swing", 30: "30_return", 63: "63_settled"}
    for i, s in drag_and_release(64):
        if i in want:
            cv, bg = render(s, "10:42", "PM")
            write_png(f"{dirpath}/{want[i]}.png", WIN_W, WIN_H, cv.over_solid(bg), 3)
    # scale + posture checks, same code path
    s = new_sim(scale=1.5, posture="plate"); s.step(1 / 60.0)
    cv, bg = render(s, "09:07", "AM")
    write_png(f"{dirpath}/scale150.png", WIN_W, WIN_H, cv.over_solid(bg), 3)
    for mode in ("natural", "mounted", "locked"):
        s = new_sim(posture=mode)
        for i, _ in enumerate(drag_and_release(20)):
            pass
        s2 = Sim(anchor=s.anchor, scale=1.0, host=s.host, posture=mode)
        s2.begin_drag(s2.pts[-1][0], s2.pts[-1][1]); s2.move_drag(s2.anchor[0] + 120, s2.anchor[1] + 140, (900.0, 0.0))
        for _ in range(14): s2.step(1 / 60.0)
        s2.end_drag()
        for _ in range(10): s2.step(1 / 60.0)
        cv, bg = render(s2, "10:42", "PM")
        write_png(f"{dirpath}/posture_{mode}.png", WIN_W, WIN_H, cv.over_solid(bg), 3)
    # dark wallpaper, to prove it reads on both
    s = new_sim(); [s.step(1 / 60.0) for _ in range(240)]
    cv, _ = render(s, "10:42", "PM")
    write_png(f"{dirpath}/wallpaper_dark.png", WIN_W, WIN_H, cv.over_solid((26, 28, 33)), 3)
    print("renders ->", dirpath)

def cmd_metrics():
    # 1. free swing from the initial release: settle time and worst stretch
    s = new_sim(); t = 0.0; settle = None; mx = 0.0; amax = 0.0
    while t < 30.0:
        s.step(1 / 60.0); t += 1 / 60.0
        mx = max(mx, s.max_stretch()); amax = max(amax, abs(s.theta))
        if s.sleeping and settle is None: settle = t
    # 2a. a hard but humanly reachable flick: ~2600 px/s, reversing, dragged past reach
    s = new_sim(); st = 0.0; tot = 1.0
    s.begin_drag(s.pts[-1][0], s.pts[-1][1])
    for i in range(120):
        x = s.anchor[0] + 240 * math.sin(i / 4.0)
        s.move_drag(x, s.anchor[1] + 150, (240 * math.cos(i / 4.0) / 4.0 * 60, 0.0))
        s.step(1 / 60.0)
        st = max(st, s.max_stretch())
        span = math.hypot(s.pts[-1][0] - s.anchor[0], s.pts[-1][1] - s.anchor[1]) / s.hang
        tot = max(tot, span)
    # 2b. synthetic torture no pointer can produce: ~15000 px/s, teleporting above the anchor
    s3 = new_sim(); s3.begin_drag(s3.pts[-1][0], s3.pts[-1][1]); st3 = 0.0
    for i in range(120):
        s3.move_drag(s3.anchor[0] + 900 * math.sin(i / 2.5), s3.anchor[1] - 400,
                     (900 * math.cos(i / 2.5) * 240, -3000.0))
        s3.step(1 / 60.0); st3 = max(st3, s3.max_stretch())
    s.end_drag()
    st_release = 0.0
    for _ in range(60):
        s.step(1 / 60.0); st_release = max(st_release, s.max_stretch())
    # 3. refresh-rate equivalence
    a, b = new_sim(), new_sim(); d = 0.0
    for _ in range(600):
        a.step(1 / 60.0); b.step(1 / 120.0); b.step(1 / 120.0)
        d = max(d, max(math.hypot(p[0] - q[0], p[1] - q[1]) for p, q in zip(a.pts, b.pts)))
    # 4. a thrown clock: bring it up to speed along the arc, release near vertical, and ask
    #    how far it carries, whether it respects the stop, and how long until it sleeps
    s = new_sim(); s.reset(angle=-0.62)
    px, py = s.pts[-1]; s.begin_drag(px, py)
    for i in range(26):
        a = -0.62 + 0.048 * i
        r = s.hang * REACH_RATIO
        nx, ny = s.anchor[0] + math.sin(a) * r, s.anchor[1] + math.cos(a) * r
        s.move_drag(nx, ny, ((nx - px) * 60.0, (ny - py) * 60.0)); px, py = nx, ny
        s.step(1 / 60.0)
    v = math.hypot(s.pts[-1][0] - s.prev[-1][0], s.pts[-1][1] - s.prev[-1][1]) / FIXED_DT
    a0 = math.atan2(s.pts[-1][0] - s.anchor[0], s.pts[-1][1] - s.anchor[1])
    s.end_drag()
    lo = hi = a0; t2 = 1 / 60.0; slept = None
    tail = []
    while t2 < 20.0:
        s.step(1 / 60.0); t2 += 1 / 60.0
        a = math.atan2(s.pts[-1][0] - s.anchor[0], s.pts[-1][1] - s.anchor[1])
        lo, hi = min(lo, a), max(hi, a)
        tail.append(a)
        if s.sleeping: slept = t2; break
    last = tail[-24:] if len(tail) >= 24 else tail
    residual = (max(last) - min(last)) if last else 0.0
    swing = math.degrees(hi - lo)
    want = math.degrees(math.acos(clamp(1 - v * v / (2 * GRAVITY * s.hang), -1.0, 1.0))) * 2.0
    stop_ok = abs(math.degrees(hi)) <= SWEEP_DEG + 0.6 and abs(math.degrees(lo)) <= SWEEP_DEG + 0.6
    # 5. tracking during drag is exact
    s = new_sim(); s.begin_drag(s.pts[-1][0], s.pts[-1][1])
    err = 0.0
    for i in range(20):
        x = s.anchor[0] + i * 3.0
        s.move_drag(x, s.anchor[1] + 130, (180.0, 0.0)); s.step(1 / 60.0)
        err = max(err, math.hypot(s.pts[-1][0] - s.drag_target[0], s.pts[-1][1] - s.drag_target[1]))
    # 6. posture models, settle and peak tilt
    post = {}
    for m in POSTURE:
        s = Sim(anchor=(WIN_W / 2, 7.0), host=(0, 0, WIN_W, WIN_H), posture=m)
        s.begin_drag(s.pts[-1][0], s.pts[-1][1]); s.move_drag(s.anchor[0] + 120, s.anchor[1] + 140, (1200.0, 0.0))
        for _ in range(12): s.step(1 / 60.0)
        s.end_drag()
        t2 = 0.0; sl = None; pk = 0.0
        while t2 < 20.0:
            s.step(1 / 60.0); t2 += 1 / 60.0; pk = max(pk, abs(s.theta))
            if s.sleeping and sl is None: sl = t2
        post[m] = {"peak_deg": round(math.degrees(pk), 2), "settle_s": round(sl or -1, 2)}
    # 7. hang sweep: how long a full-surface present is, in painted pixels (cost model input)
    # 8. free-swing amplitude envelope, for the decay shape
    s4 = new_sim(); env = []; t4 = 0.0; pk = 0.0
    while t4 < 12.0:
        s4.step(1 / 60.0); t4 += 1 / 60.0
        a = abs(math.degrees(math.atan2(s4.pts[-1][0] - s4.anchor[0], s4.pts[-1][1] - s4.anchor[1])))
        pk = max(pk, a)
        if s4.sleeping: break
        if int(t4 * 4) != int((t4 - 1 / 60.0) * 4): env.append((round(t4, 2), round(pk, 2))); pk = 0.0
    print(json.dumps({
        "settle_free_swing_s": round(settle or -1, 3),
        "peak_tilt_deg_free_swing": round(math.degrees(amax), 2),
        "max_stretch_free_swing": round(mx, 5),
        "max_stretch_human_flick": round(st, 5),
        "max_stretch_synthetic_torture": round(st3, 5),
        "max_span_over_rest_length": round(tot, 5),
        "max_stretch_after_release": round(st_release, 5),
        "rate_independence_px": f"{d:.2e}",
        "throw_speed_px_s": round(v, 1),
        "throw_swing_arc_deg": round(swing, 1),
        "throw_swing_arc_deg_undamped_pendulum": round(want, 1),
        "throw_within_stop": stop_ok,
        "throw_settle_after_release_s": round(slept or -1, 2),
        "residual_motion_at_sleep_deg": round(math.degrees(residual), 3),
        "drag_tracking_error_px": round(err, 6),
        "amplitude_envelope_deg": env,
        "posture": post,
    }, indent=2))

def cmd_trace(path):
    s = new_sim()
    doc = {"config": {"segments": SEGMENTS, "gravity": GRAVITY, "damping": DAMPING,
                      "fixed_dt": FIXED_DT, "max_stretch": MAX_STRETCH, "hang": s.hang,
                      "anchor": list(s.anchor), "mass_card": MASS_CARD, "relax_tol": RELAX_TOL,
                      "sleep_max_move": SLEEP_MAX_MOVE, "settle_time": SETTLE_TIME, "cord_mass": CORD_MASS,
                      "friction_acc": FRICTION_ACC, "viscous": DAMPING,
                      "posture": POSTURE["plate"], "attitude_gain": ATTITUDE_GAIN,
                      "card": [CARD_W, CARD_H, BRACKET], "reach_ratio": REACH_RATIO, "cord_mass": CORD_MASS,
                      "friction_acc": FRICTION_ACC, "viscous": DAMPING,
                      "sweep_deg": SWEEP_DEG, "max_speed": MAX_SPEED}, "frames": []}
    for i in range(150):
        if i == 0: s.begin_drag(s.pts[-1][0], s.pts[-1][1])
        if i < 20: s.move_drag(s.anchor[0] + 7.0 * i, s.anchor[1] + 140.0, (7.0 * 60, 0.0))
        elif i == 20: s.end_drag()
        s.step(1 / 60.0)
        doc["frames"].append({"p": [[round(v, 7) for v in q] for q in s.pts],
                              "th": round(s.theta, 9), "om": round(s.omega, 9),
                              "asleep": s.sleeping, "stretch": round(s.max_stretch(), 9)})
    # a second trace: pure free swing from the initial release angle, no input at all
    s2 = new_sim(); free = []
    for _ in range(140):
        s2.step(1 / 60.0)
        free.append({"p": [[round(v, 7) for v in q] for q in s2.pts], "th": round(s2.theta, 9),
                              "om": round(s2.omega, 9), "asleep": s2.sleeping,
                              "stretch": round(s2.max_stretch(), 9)})
    doc["free_swing"] = free
    open(path, "w").write(json.dumps(doc))
    # A text form for the Rust test: no JSON parser in a dependency-free test suite, and one line
    # per frame with a fixed field order, so a divergence points at a specific frame and node.
    txt = os.path.splitext(path)[0] + ".txt"
    with open(txt, "w") as f:
        f.write("segments %d\n" % SEGMENTS)
        f.write("anchor %.9f %.9f\n" % (s.anchor[0], s.anchor[1]))
        f.write("hang %.9f\n" % s.hang)
        f.write("scale 1\n")
        f.write("frames %d\n" % len(doc["frames"]))
        for i, fr in enumerate(doc["frames"]):
            nums = " ".join("%.9f %.9f" % (p[0], p[1]) for p in fr["p"])
            f.write("F %d %s %.9f %.9f %d %.9f\n" % (i, nums, fr["th"], fr["om"], 1 if fr["asleep"] else 0, fr["stretch"]))
        f.write("free %d\n" % len(free))
        for i, fr in enumerate(free):
            nums = " ".join("%.9f %.9f" % (p[0], p[1]) for p in fr["p"])
            f.write("S %d %s %.9f %.9f %d %.9f\n" % (i, nums, fr["th"], fr["om"], 1 if fr["asleep"] else 0, fr["stretch"]))
    print("wrote", path, "and", txt)

if __name__ == "__main__":
    a = sys.argv[1] if len(sys.argv) > 1 else "render"
    if a == "sheet":   cmd_sheet(sys.argv[2] if len(sys.argv) > 2 else "/tmp/preview/sheet.png")
    elif a == "metrics": cmd_metrics()
    elif a == "trace": cmd_trace(sys.argv[2] if len(sys.argv) > 2 else "/tmp/trace.json")
    else: cmd_render(sys.argv[2] if len(sys.argv) > 2 else "/tmp/preview")
