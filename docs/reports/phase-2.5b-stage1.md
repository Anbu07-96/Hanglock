# Phase 2.5B, Stage 1 — diagnosis and three concepts

Stage 1 is diagnosis and preview only. No production Rust changed, no gate changed, no solver
touched. What follows is (1) what is provably wrong, with the arithmetic and the file:line, (2)
what is probably wrong functionally, with the code path, and (3) three original concepts to choose
from. Stage 2 begins when a concept is picked.

## 0. The evidence I actually have

**The screenshot did not arrive.** It was described in the request, but no image file reached this
workspace and nothing was attached to the message, so nothing below is a reading of what appeared
on the desktop. Rather than guess at it, everything here comes from two sources that are both
reproducible in this checkout:

- **the source of record** — defaults, geometry and painter constants, quoted with file:line and
  computed out, and
- **the deterministic renderer** — `tools/model/hanglock_ref.py`, which shares the palette, the
  painter logic and the solver constants with the Rust crates, and which is therefore able to show
  what the shipped app paints. Its own `00_rest` frame and the committed
  `docs/previews/04-seconds.png` turned out to be evidence in my hands (§2, §3).

Where the desktop could still contradict me, I say so. If the screenshot and the output of
`.\hanglock.exe --diag` (see `docs/testing/windows-desktop-validation.md` §0) are re-sent, §3 can
be replaced by a confirmation instead of a prediction.

## 1. The iteration loop that was missing

A look cannot be refined by editing Rust and re-downloading a CI artifact, so the loop now exists
as a separate renderer:

    python3 tools/model/hanglock_concepts.py all docs/previews/phase-2.5
    python3 tools/model/hanglock_concepts.py one /tmp/x.png slate|glass|thread
    python3 tools/model/hanglock_concepts.py ladder /tmp/x.png slate
    python3 tools/model/hanglock_concepts.py footprint /tmp/x.png

`hanglock_concepts.py` **imports** `hanglock_ref.py` (canvas, anti-aliasing, palette, `render`,
`new_sim`) and never rebinds a constant there. That is deliberate: the reference module's
`RopeConfig` values are pinned by `tests/golden/trace_drag_settle.txt`, and `face_data.rs` is
pinned by the generated output of `tools/model/gen_face.py`, so a design sketch must be unable to
reach either. It draws 18 previews, all listed file by file in `assets/SOURCES.md`, and every number
it uses is also quoted in this document so that a preview and a constant can be checked against
each other.

## 2. Visual diagnosis — what is wrong, measured

Today, at the shipped defaults, at 100 % scale and 100 % DPI:

| # | Symptom | Measured cause | Where |
|---|---|---|---|
| V1 | The face breaks as soon as seconds are on | the run is centred on the card without ever being measured against its width: `10:42:07` + gap + `PM` = `8·48·0.675 − 2.64 + 10 + 20.8` = **287.4 device px on a 252 px plate**, so ~14 % of the run is painted off the plate — the leading `1` almost entirely, the meridiem entirely. It is size-invariant: `card_w`, `time_cap` and `suffix_gap` all scale together, so Bigger/Smaller cannot rescue it, and a 24-hour `23:45:59 PM` is worse. `docs/previews/04-seconds.png` shows it. | `hanglock-render/src/paint.rs:170,186-194,160-167`; `face_data.rs:14,19` |
| V2 | "The rope dominates the desktop" | `swept_box` gives a **520.4 × 269** window for a **252 × 96** card: the transparent, interactive canvas is 2.07× the card's width and 2.8× its height, so at rest ~78 % of the window is empty space whose only content is a line. The same numbers also decide the presented surface (≈560 KB of BGRA per full repaint). | `hanglock-core/src/placement.rs`; matches CI's own `frame 252,0 520x269` |
| V3 | The *first* frame is the worst frame | `initial_angle: 0.30` rad = **17.2°** off vertical, i.e. the card appears 44 px sideways of its anchor at hang 150, tilted, with the cord diagonal. Any `anchor_ratio` / `anchor_drop` saved while following the checklist's Alt-drag test then persists a lean. | `rope/config.rs:159`, `rope/mod.rs:85`, `settings.rs:86-94` |
| V4 | The cord reads as a cable / a strip of tape | `w = (1.7·scale).max(card_h·0.026)` = **2.5 px half-width = a 5 px line** on a 96 px card, drawn with a blurred twin at 1.6× (≈8 px) underneath and a highlight on top. With 16 segments and round joins, the beads show. That halo, not the length, is what makes it loud. | `paint.rs:469`, `hanglock_ref.py:616-632` |
| V5 | The mount looks crude, and more so the bigger the clock gets | `Settings::card()` scales `width` and `height` and passes `bracket: 9.0` and `corner: 17.0` **through unscaled**, so at scale 1.75 the hardware and the radius fall behind the plate, and at 0.75 they are oversized. The joint is therefore never right; there is no eyelet/hook concept, only a bar. | `hanglock-core/src/settings.rs:119-129`, `rope/config.rs:164-174` |
| V6 | The shadow is muddy and floats the card | `shadow_alpha 0.30`, `shadow_blur 7.5`, `shadow_drop 7.0`: blur + offset ≈ 15 % of the card's height, in a single soft mass, and it is unrelated to tilt. Tuning, not a bug — but it is the loudest ambient in the composition. | `theme.rs:62-64` |
| V7 | The digits look handwritten | every glyph is a polyline of **capsules** at `STROKE_RATIO 0.12500` (weight ≈ 25 % of cap), `ADVANCE 0.62000`, `TRACKING 0.05500`, sharing the cord's coverage function by design. Round caps, monoline, heavy weight, closed bowls: `0` is a stadium and `8` is a snowman. Metrics are fine (`GW 0.46` in an advance of 0.62 leaves a real sidebearing, so glyphs do not collide) — the *construction* is the problem. | `face_data.rs:14-19`, `tools/model/gen_face.py:41-48`, `hanglock_ref.py:69-70` |
| V8 | The meridiem is a sticker | `suffix_cap 0.167` (16 px on a 96 px card), fixed `suffix_gap 10.0·scale`, and the colour `accent = rgb(0.44, 0.74, 0.99)` — a saturated blue, drawn with the same marker capsules as V7, at the end of the run rather than on the plate's grid. There is no minimum size, so at scale 0.75 it is ~8 px tall and unreadable while still being the only colour on screen. | `theme.rs:55,65-67`, `paint.rs:187-194,338` |

**Process finding.** `docs/previews/01-settled-light.png`, committed in Phase 1 and generated by
this same tool, is the 17° frame of V3 — the composition problem was visible in the repo for two
phases and nobody looked at it. The loop only counts if the output is reviewed; the concept command
above now prints the frame numbers next to each image so a mismatch cannot hide.

## 3. Functional diagnosis — code paths, not invented symptoms

No behavior is asserted as "what you saw". Each item below is a defect in a path that only executes
on a real desktop, which is exactly why 111 Windows-side unit tests, clippy and the golden trace are
all green while something on the screen misbehaves.

**F1 — pointer position is converted twice, so clicks land in the wrong place.**
`WM_NCHITTEST`'s `lParam` is a **screen** coordinate and the code converts it correctly:
`local = screen − frame.x0/y0` (`hanglock-win/src/window.rs:805-813`). `WM_MOUSEMOVE`,
`WM_LBUTTONDOWN` and `WM_LBUTTONUP` deliver **client** coordinates, and those same three lines
subtract the frame origin from them anyway (`:824`, `:851`, `:870`) — `host.frame` is
`model.layout_frame`, the window rect in screen device px (`app.rs:118`, `model.rs:264`). So every
press, move and release reaching the model is displaced by the window's own position: with the
default placement (`x0 ≈ 700`) a click on the centre of the plate arrives 700 px left of the plate,
so it hits nothing. Where a press *does* register — `click_through = "solid"`, where
`whole_window` answers `HTCLIENT` everywhere, or a window whose `x0` happens to be small — the drag
then tracks a point hundreds of pixels from the cursor: the held node slams toward the far side of
the swept box, the cord goes taut diagonally and the card ends up beside its own anchor instead of
under it. Predicted symptoms, to confirm: grabbing the plate does nothing in most positions; a drag
in Solid mode throws the clock sideways; a long diagonal rope across the screen; no snap back to
vertical on release because the release point is wrong too. One conversion, taken from
`GetCursorPos` minus the frame origin for down/move/up (the code already reads `GetCursorPos` for
velocity precisely because `lParam` under capture is untrustworthy) makes position and velocity agree
and is immune to the 16-bit wrap the existing comment worries about. `WM_NCHITTEST` keeps its
`lParam`. `docs/testing/windows-desktop-validation.md` section D covers this; its results would be
the confirmation.

**F2 — a stolen capture leaves the clock held forever.** `WM_LBUTTONDOWN` calls `SetCapture`
(`:852`) and `WM_LBUTTONUP` releases it (`:871`); there is no `WM_CAPTURECHANGED` arm. Anything that
breaks capture from outside — Alt+Tab, a shell menu, a lock screen, `SetForegroundWindow`, the app's
own modal `About` — ends the capture without the model ever seeing a release, so the held node keeps
following, `TICK` stays armed, and no relevel or brake ever runs. Cost: a stuck or permanently
swinging clock and a message loop that will not sleep. Fix: one arm that converts a lost capture
into `Input::Release { vel: ZERO }`.

**F3 — the hit slop that was designed in is switched off.** `HitShape::pad` is documented as "extra
reach, device px. Generous, because the tip of a finger on a trackpad is not a pixel"; `app.rs:353`
passes `pad: 0.0`. Hover-to-grab and drag therefore require contact with the painted plate, whose
last pixel row is anti-aliased, which is also the exact reason `Solid` mode exists. A few device px
of pad (scaled with DPI) is the intended behaviour, restored.

**F4 — two-finger scroll on a precision touchpad cannot resize.** `window.rs:895` computes
`((w >> 16) as u16 as i16) / 120` and drops the message when the result is 0. Precision touchpads
(inertial scrolls) report deltas far smaller than 120 per step, so on a modern laptop the gesture
that the settings row describes produces nothing at all. A physical wheel is unaffected, which is
why this survives any CI. Fix: treat any non-zero delta as one step, and accumulate the fraction.

Two further paths were reviewed and are **not** claimed as bugs: the Alt read at press time
(`:860`, deliberately not a modifier mask on later moves) is right, and the anchor/frame clamp order
in `placement.rs` needs a desktop check at long hang rather than a code change — at hang 260 with
`respect_taskbar` on, `y0` is clamped before the anchor, which can place the ring inside the visible
area, and only the real window manager can tell us whether that is ever seen.

## 4. Three concepts

All three keep: the physics, the solver, the swept-box model, the multi-monitor/DPI path, the tray,
the native Win32 surface, zero new dependencies, and a user-adjustable hang length. All three change
the *default* composition, and all three size every part of the object from the cap height so
`Bigger`/`Smaller` keeps the design intact. Numbers are device px at 100 % DPI.

| | **A · Slate** (recommended) | **B · Glass** | **C · Thread** |
|---|---|---|---|
| card | 204 × 56 (at scale 0.85) | 227 × 52 (at 0.90) | 175 × 48 (at 0.95) |
| cap height | 26.6 | 29.2 | 22.8, fitted |
| corner radius | 8, scales with the card | 12 | 6 |
| cord | 2 px, one lit edge, no blurred twin | 1.4 px hairline | 1.5 px thread |
| mount | 30 × 4 rail at the screen edge, 15 × 5 neck, 3.2 px eyelet | smaller rail, wider eyelet | none — the cord ends in an eyelet punched into the plate's top edge |
| shadow | α 0.15, blur 0.045·cap, drop 0.035·cap | α 0.09, blur 0.130·cap, wide ambient | α 0.10, blur 0.035·cap |
| plate | α 0.95, gradient 0.128 → 0.060, rim 0.13 / top 0.22 | α 0.86, lit top band 0.055, rim 0.22 | α 0.96, deepest plate, rim 0.10 |
| meridiem | 0.34·cap, gap 0.22·cap, plate ink at 90 % α | 0.32·cap, same ink family | 0.36·cap, gap 0.18·cap |
| default hang | 96 | 112 | 72 |
| swept window | **387 × 195** | 435 × 209 | **320 × 167** |
| window ÷ card width | 1.90 | 1.92 | 1.83 |
| plate area vs today | −53 % | −52 % | −67 % |

Previews: `concepts-light.png`, `concepts-dark.png`, `concepts-seconds.png`,
`concept-<name>-{light,dark,seconds,rest}.png`, `footprint.png` (today's real 520 × 269 red box with
the shipped render inside it, next to each concept's), `slate-scale-ladder.png` (A at 0.75 / 1.00 /
1.40 / 1.75, to show that hardware and radius now scale), `face-concepts.png` (the face, and the fit
rule at 5, 8 and 5 glyphs on one width).

**The face decision, shared by all three.** A chamfered *segment* set in the unit box, drawn from
`SEG_DEF` with slits cut at draw time only where a neighbour is lit — so `1`'s two halves merge into
one stroke, `0` keeps six corner slits, `8` reads as an open grid instead of two stacked rings.
Verticals carry 0.080·cap, horizontals 0.86× that. Terminals are flat with a 0.16 rounding, not
capsules: that single change is what turns "written" into "drawn". `ADVANCE 1.04` is fixed, so the
run is tabular by construction and a ticking second cannot shuffle the plate. The meridiem is set in
the same face at 0.34·cap in the plate's own ink. Two consequences to accept explicitly: it is a
deliberately *instrumental* look (close to an airport board, not to a calculator) and it is not
Hangly's; and A's `cap_ratio` still needs a floor for the meridiem, which the scale ladder shows
failing at 0.75 — Stage 2 adds `max(0.34·cap, 9 px)`.

**What Stage 2 would touch**, for A: a `bar` primitive in `hanglock-render` (oriented rounded box;
`capsule` becomes `bar` at `end = 0.5`, so one coverage function serves cord and type as before),
`tools/model/gen_face.py` regenerating `face_data.rs` with the new polylines, a `cap_for(text,
card)` fit rule beside `time_cap` (V1), theme numbers (V4, V6, V8), `Settings::card()` scaling
`bracket` and `corner` (V5), defaults `hang 96` and `initial_angle` (V3), and the four input fixes.
**Not** touched: `crates/hanglock-core/src/rope/`, the solver, the gates, the tolerance of any
comparison, the platform abstraction.

One caveat that has to be said in advance: `initial_angle` is in the state the reference trace starts
from, so changing it changes `tests/golden/trace_drag_settle.txt`. Re-pinning that fixture is a
deliberate act in the same commit as the config change, generated by the tool — never a widened
tolerance. If you would rather keep the launch swing (it is a nice detail once the composition is
right), Stage 2 can leave `initial_angle` alone and fix only `hang`, which already removes most of
V3.

## 5. To decide before Stage 2

1. **Which concept** — A, B or C (or A with C's mount, which is a two-line change).
2. **Seconds**: shrink the digits to fit (as previewed) or widen the card when seconds are on?
   The first keeps the plate constant and is what the previews show; the second keeps the type size
   and changes the swept box.
3. **Launch swing**: reduce `initial_angle` to near-vertical at first appearance (needs the golden
   trace re-pin) or keep it and only shorten the default hang?
4. **Confirm F1 on the desktop**, since it is a prediction: with the clock near the *left* edge of
   the primary monitor, does clicking the plate grab it, while in the middle of the screen it does
   not? And does two-finger scrolling do nothing while a wheel works (F4)? Those two answers turn §3
   from a code-path finding into a closed bug.
5. Re-send the screenshot and the `--diag` transcript (run from PowerShell) if they are still
   available.
