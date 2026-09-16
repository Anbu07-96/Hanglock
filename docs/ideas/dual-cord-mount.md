# Idea: the dual-cord mount — why a hanging clock should not be a hanging charm

Status: design note, targeting Phase 4 (post-MVP). This is the one physics idea that is distinctly
Hanglock's, and it is why we should not simply port Hangly's solver and call it done.

## 1. The problem the reference project doesn't have

A charm is roughly a lump: a point mass at the end of a cord, oriented by the last link's direction.
Rolling it about the cord axis costs nothing and looks fine, because there is no legible surface.

A clock is a **wide plate with text on it**, hanging from near its top edge. On a single strand it
behaves like a pendulum with a free pivot at the knot: any horizontal impulse gives it angular
momentum about that pivot and nothing takes it away, so the plate rolls to ±20–30° and keeps rolling.
For an ornament that is charming. For a time readout it is a bug you can't read past — and the
usual workaround (clamp the angle, over-damp the rotation) makes it stop behaving like a hanging
thing and start behaving like a window with a spring on it. We would rather not ship the trade-off.

## 2. The fix is geometric, not artificial

Hang the plate from **two strands** that meet at one ring at the top of the screen — a `V`, the way a
framed plaque, a hanging mirror, or a gallery label actually hangs.

Ring at the origin, plate centre at distance `L`, the two attachment points `P₁, P₂` separated by the
bracket width `w`. If the plate rotates by `θ` about its centre, one attachment moves toward the ring
by approximately `(w/2)·sin θ` and the other moves away by the same. Two near-inextensible cords cannot
allow that: one would have to stretch. The solver's own distance constraints therefore **resist
rotation**, with a restoring stiffness that is:

* ∝ `w` (bracket width) — a wide frame is level, a narrow one swings like a sign;
* ∝ `1/L` — a long cord lets the plate roll more, a short cord clamps it down;
* ∝ `1/` (plate's moment of inertia) — a heavier, denser plate lags and settles slower.

All three are physically true and all three are *design controls we already want*. That is the whole
argument: legibility stops being an angle clamp fighting the simulation and becomes a property of the
mount, which the user can see and choose.

## 3. Implementation sketch (cheap, and it reuses the MVP solver)

Model the plate as a **rigid triangle of three Verlet particles** — `P₁`, `P₂` (top corners, where the
cords attach) and `P₃` (bottom centre) — joined by three distance constraints, all solved in the same
Gauss-Seidel pass as the cord links.

```
per fixed step:
  pin ring
  integrate            # plate nodes carry mass/3 each, cord nodes their own
  drive held node(s)   # dragging grabs the plate, so both P₁ and P₂ follow the pointer
  relax:  cords (2 × N links) + triangle (3 links)      # one pass list, one tolerance, one early exit
  project over-stretch (one-sided, shared corrections)   # same ceiling as the cord
  read pose: angle = atan2(P₂-P₁), centre = mean(P₁,P₂,P₃)·weights
```

No separate rigid-body integrator, no torque, no quaternion, no angular ODE — orientation falls out of
positions, exactly the way the cord's shape does. Cost is 3 extra constraints and one `atan2` per
frame; the solver structure, the fixed 240 Hz timestep, the accumulator clamp and the sleep rule are
unchanged. `CardRig` (MVP, single strand + `θ_max` clamp) and `VCardRig` (this) then share the
`RopeSim` core and differ only in the constraint list — which is why the MVP is allowed to be the
simple one: the seam is the constraint list, so this is additive.

Rendering gains from the same structure: the two cords are two `Cord` objects (already one type), the
plate is drawn at `angle`, the ring is a small ellipse at the top, and the bracket is two short bars
from `P₁`/`P₂` into the plate — so the joint where the physics attaches is *also* the joint that is
drawn, which is what makes a hanging object read as hung rather than as pasted on.

## 4. What it buys the product

| Setting (user-visible) | Physical parameter | Feel |
|---|---|---|
| Posture: `Sign` | `w` small (cord rings close together) | Swings and rolls a lot; playful; you flick it |
| Posture: `Plate` *(default)* | `w` ≈ 60 % of card width | Tilts to ~4° in a hard swing, back to level in under a second |
| Posture: `Mounted` | `w` = card width, `L` short | Barely rotates; reads as bolted on a rail; best for a wall with white paper under it |
| Rope length | `L` | Longer hangs lower and rolls more; ties directly to the existing wheel control |
| Card mass | `mass` | Heavy plate = slow, dead swing; light = lively, longer settle |

Two free side benefits:

* **Asymmetric anchors become possible.** `P₁` on the top-left of the screen, `P₂` on the top-right of
  a wide plate = the object can be strung across the whole top edge. That is a whole visual identity
  (a banner clock) we get for free from the same constraint set.
* **Rope styles get a second axis.** Chain/ribbon/wire change `w`-effective stiffness (a chain holds
  the plate level, a ribbon twists) — so Phase 4's cosmetic styles become behavioural again, which
  was already the point of coupling material to mass.

## 5. Costs and open questions

* Two cords double the cord draw cost — still trivial (a few strokes), but the swept-area window
  sizing logic has to consider two curves, not one.
* `L`-long plus wide plate means the drag reach region is a lens, not a disc; `hit.rs` needs the
  plate's oriented rect (already needed for the MVP card) rather than a radius.
* Two cords can tangle visually when the plate spins hard. Real ones do too; but if it reads as a
  bug rather than physics, the fix is a small out-of-plane damping on `θ` — decide by eye, with a
  `--record`/`--replay` clip committed as the test.
* Grab semantics change: which cord do you pull? Proposal — grabbing the plate drives both `P₁` and
  `P₂` (you are holding the object); grabbing a cord drags that cord's node only, which makes a
  satisfying "yank one corner" interaction the MVP never has.
* **Do not do this before Phase 4.** One strand, one plate, real drag physics, real click-through,
  shipped and measured first: a hanging clock that is *almost* right and public beats a perfect mount
  that is private. The triangle trick is why adding it later is a day, not a rewrite.
