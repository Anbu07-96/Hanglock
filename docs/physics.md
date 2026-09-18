# The rope

What `crates/hanglock-core/src/rope/` does, and the reason for each part. The numbers are in
[`gate-a.md`](gate-a.md); the tunables live in one place, `rope/config.rs`.

## Shape of it

Sixteen links, seventeen nodes. Node 0 is the anchor and immovable. The last node is the plate's
**centre of mass**, not its top edge — so gravity acts where the mass is, and the joint the cord
visually ends at sits `height/2 + bracket` above that node. The painter covers the cord's last
 stretch, which is why the joint never separates: the cord is trimmed by "am I inside the plate", not
by a fixed inset.

```
anchor (pinned)                                  node 0, inv_mass = 0
  │
  │  15 interior nodes, inv_mass = 1/0.02
  │
  ●  plate centre of mass, inv_mass = 1/1.0     ← the drag grabs this
  │  ↕ height/2 + bracket
[────]  the plate is drawn here; the cord stops at its top edge
```

## One step

```
pin anchor → integrate → drive held node → relax → project stretch
           → (relevel, while braking) → sector stop → plate attitude → sleep check
```

**Verlet**, position and previous position and no velocity array, for three reasons that are all load
bearing: momentum on release costs nothing (stop writing the position; the gap *is* the velocity),
constraints are positional (move nodes, no stiffness term to tune into instability), and it stays
stable at the pass counts a taut cord needs.

**Fixed 1/240 s**, with the accumulator clamped to 0.1 s. The display only decides how often `step`
is called. Two consequences worth stating: behaviour is *identical* at 60 and 120 Hz (asserted to
0.0 px, and the sleep transition happens on the same frame), and a stall or a resume from sleep
cannot fire a burst of catch-up steps — which would look like the clock teleporting.

**The sleep check runs inside the fixed-step loop, not per frame.** It once ran per frame, and the
brake's window then meant "24 frames" at 60 Hz and "24 frames" at 120 Hz but "12 steps" in physical
time at neither: settling became display-dependent and the 60-vs-120 test failed by 18 px. That bug
is the clearest argument in the repo for the fixed timestep.

## Keeping it taut

Three mechanisms, weakest first:

1. **Relaxation.** Gauss-Seidel over every link toward its rest length, exiting early when no
   correction in a pass exceeds 0.02 px. The pass cap is `8 × nodes` because corrections travel about
   one link per pass: below the node count, yanking the bottom end leaves the top end unaware and the
   links in between absorb the difference by stretching. The cap costs nothing when the early-out
   fires, which it does in one or two passes for a settled cord.
2. **One-sided projection** of any link still over `1.02×` its rest length. One-sided matters: links
   inside the limit must not be touched, or the pass fights the relaxation instead of finishing it.
   And the correction is *shared* between both ends — snapping the offending node onto the limit
   oscillates on a chain pinned at both ends, because each sweep undoes the last one's work.
3. **Clamping the input.** The drag target is limited to the reach circle (0.985 of rest length), the
   sweep sector (±52°), and the window rect; and the held node *follows* the target at
   `max_speed × dt` rather than teleporting to it. Measured: tracking error is exactly 0 px at 5 000
   px/s (no hand can exceed that), and a superhuman 15 000 px/s input stays bounded and recovers
   instead of tearing.

## Why it settles, and why a throw still carries

Air drag alone cannot do both. A viscous `damping` strong enough to stop a clock in two seconds erases
a throw; one weak enough to let a throw swing leaves the object drifting for ten. So the dissipation is
mostly **dry (Coulomb) friction** — `friction_acc`, a fixed bite per step, applied to each node's
carried displacement — which takes a constant bite regardless of speed: a fast swing keeps most of its
energy while the slow tail is cut off in bounded time rather than decaying forever. Measured: 104 % of
an undamped pendulum's arc for a 425 px/s throw, and 2.1 s to rest from the initial release.

Two additions, both product decisions wearing physics clothes:

* **The settle brake** (`brake_on_move`/`brake_off_move`, with hysteresis so it cannot chatter) engages
  below ~16 px/s and damps hard. It exists because pivot friction alone parks the plate a few degrees
  off vertical, and a clock frozen crooked reads as broken. While it is engaged, **relevel** eases the
  hanging line back under the anchor; the object is asleep within 0.25 px of vertical.
* **The sector stop** bounds the swing to ±52° even in free motion. Unbounded, a plate on a 150 px
  cord reaches ~100° and goes behind the taskbar and off the monitor; bounded, the swept area is a
  sector, which is what lets the window be sized once and cheaply.

## Sleeping

Stillness is measured on **the four plate corners**, sampled every 24 fixed steps: if nothing drawn
moved more than 0.25 px in 0.1 s, for 0.3 s, the solver stops. `step()` then returns false, no scene
is built, nothing is painted and nothing is presented. Per-node speed thresholds were the first two
attempts and both are wrong: a threshold under the relaxation tolerance asks the solver to converge
finer than its own tolerance and so never sleeps at all (measured: 11 s awake), and a threshold on the
plate alone sleeps while the cord above it still creeps. The drawn silhouette is the thing that must
stop changing, so it is the thing that is measured.

## Not a port

The algorithm family — Verlet with positional constraints, fixed timestep, arc-length-flattened
spline for the cord — is standard numerical practice, and the shape of the constraint list here was
chosen for a *plate*, not a charm: centre-of-mass terminal node, an attitude follower with a legibility
clamp instead of "orient along the last link", a mass ratio tuned so a throw works, a sector stop
sized to the window, and a stillness test that measures the rendered object. `docs/research/ip-boundaries.md`
governs what may be taken from the reference project; this file describes what was designed here.
