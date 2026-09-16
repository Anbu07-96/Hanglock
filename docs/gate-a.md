# Gate A — measured, and what is still owed

Gate A is the performance/trust gate for Phase 1 (`docs/mvp.md` §4). It is **partially measured**: the
model, physics and cost geometry are measured and reproducible here; the numbers that need a Windows
desktop cannot be measured in this environment, and are stated as owed rather than invented.

## Environment of this build

* Sandbox: Linux container, **no Rust toolchain and no crates.io access** (see
  [ADR-0002](decisions/0002-zero-dependencies.md)). Nothing in this phase could be compiled, so
  nothing here is a measured Rust number. The reference model in `tools/model/` is a transcription of
  the same algorithms, and every physics number below is measured by running it.
* The visual design was reviewed by rendering frames with the same painter logic and looking at them.
* Consequence for review: **`cargo test` and `cargo build` have not been run.** Expect first-compile
  fixes; CI runs both. The syntax of all 39 files is verified with a Rust parser, which catches
  structural errors but not types or trait mismatches.

## Physics, measured (`tools/model/hanglock_ref.py metrics`)

| Quantity | Measured | Target | Where it is asserted in Rust |
|---|---|---|---|
| Settle, release from 0.30 rad | **2.1 s** | 2–3 s | `tests/rope.rs::it_sleeps_hanging_straight` |
| Settle after a hard throw | 4.1 s | < 8 s | same (upper bound) |
| Throw carry, 425 px/s release | **86.5°** of arc vs **83.1°** ideal undamped pendulum (104 %) | ≥ 90 % of ideal | `a_release_keeps_its_momentum` |
| Residual motion at the freeze | 0.013° | invisible | `it_sleeps_hanging_straight` (< 0.25 px off vertical) |
| Worst link stretch, free swing | **1.0028×** rest | ≤ 1.02× | `the_cord_never_stretches_under_free_swing` |
| Worst link stretch, 3600 px/s reversing flick | 1.0248× | bounded | `a_hard_flick_stays_within_tolerance` (≤ 1.03) |
| Worst link stretch, ~15 000 px/s synthetic | 1.036×, recovers, sleeps | bounded, no NaN | `torture_input_stays_bounded_and_recovers` |
| Span over rest length, ever | **1.0000×** | ≤ 1.0 (never taut-longer) | `the_drag_target_cannot_leave_the_reachable_sector` |
| 60 Hz vs 120 Hz divergence | **0.0 px** (exact) | 0 | `behaviour_is_identical_at_60_and_120_hz`, `tests/golden_trace.rs` |
| Drag tracking error at 5 000 px/s | **0.0 px** | 0 | `drag_tracking_is_exact_at_human_speeds` |
| 60 s stall, advance in one call | clamped (< 12 px) | no teleport | `a_stall_cannot_burst_into_catchup_steps` |
| Plate tilt by posture | natural ≤ 26°, plate ≤ 9°, mounted ≤ 2.5°, locked 0° | as set | `posture_clamps_tilt_and_lock_holds_it_level` |

Three of those rows were **bugs the numbers caught**, and the fixes are in both implementations:

1. A per-step stillness test below `relax_tol` never sleeps — the settled state stayed awake 11 s.
   Now: whole-chain stillness measured over a 24-step window, on the plate's corners.
2. A `braking` flag inferred from a *missing* reference sample engaged the settle brake for the
   first 0.1 s of every throw, costing 4 % of the momentum (82.9° → 86.5° once fixed).
3. A near-massless-vs-heavy mismatch: cord nodes at mass 1 against a plate at 1.55 dumped 93 % of a
   throw into swinging the cord. Cord mass is now 0.02 per node — the number that makes "throw it"
   feel like throwing an object rather than tugging a chain.

## Cost geometry (exact, from the same formulas as the code)

Swept box at the default settings (`hang = 150`, card `252×96`, sweep ±52°, margin 16):

| Display scale | Window, logical px | Window, device px | Full present | 1 Hz digits present |
|---|---|---|---|---|
| 100 % | 520 × 270 | 520 × 270 | 547 KiB | 61 KiB |
| 125 % | 520 × 270 | 651 × 338 | 859 KiB | 95 KiB |
| 150 % | 520 × 270 | 781 × 405 | 1.20 MiB | 137 KiB |
| 200 % | 520 × 270 | 1041 × 540 | 2.14 MiB | 244 KiB |

Derived claims, which the measurement below must confirm or refute:

* **Settled steady state:** one present of ~137 KiB per second at 150 % (≈ 8 KB/s of copies), and no
  physics work at all. That is the ~11× ratio between the digits' rect and the full window, and it is
  why `present(rect)` exists on the surface rather than `present()`.
* **Swinging at 60 Hz:** 60 × 1.2 MiB ≈ 72 MB/s of copies at 150 % — the number ADR-0001 §4 warned
  about, and the reason the window is sized to the swept sector instead of Hangly's fixed 740×420 at
  120 Hz (≈ 250 MB/s). If the measurement shows this dominating, the fix is the recorded fallback: a
  DirectComposition presenter inside `surface.rs`, no other file changes.
* **Idle:** the process should be inside `GetMessageW` with one 1 Hz timer and no threads of ours.

## Owed: the hardware table

Run `pwsh scripts/gate-a.ps1 -Seconds 60 -WithDragSeconds 20`. Fill in and paste into the release
notes; `budget` is from `docs/architecture.md` §8 and a miss with a written reason does not block a
release (0.1.0 ships with the numbers and an honest limitation, per the reference project's move).

| Metric | Value | Budget | Note |
|---|---|---|---|
| Idle CPU, % of one core | _measure_ | ≤ 0.05 | 60 s, untouched |
| CPU while swinging, % | _measure_ | ≤ 3 | dragged for 20 s |
| Working set, MB | _measure_ | ≤ 20, fail > 30 | |
| Private bytes, MB | _measure_ | ≤ 35 | |
| Cold start, ms | _measure_ | ≤ 150 | |
| `--bench` paint, µs/frame | _measure_ | ≤ 400 | the only row measurable without a desktop |
| Executable, KB | _measure_ | ≤ 2048 | |
| Installer, KB | _measure_ | ≤ 4096 | `scripts/installer.iss` |
| Frame loop stops when settled | _observe_ | yes | `--diag` counters: `ticks` must not grow while idle beyond the 1 Hz timer |
| Click-through over the desktop | _observe_ | yes | click a desktop icon through the overlay's empty area |
| Focus never stolen | _observe_ | yes | caret in Notepad while dragging the clock; then open both menus |
| DPI change | _observe_ | re-anchors | 100 % → 200 % while running, no reset of the swing |
| Monitor unplug/replug | _observe_ | stays sane | including the display it was on disappearing |
| Sleep/resume | _observe_ | resyncs | no catch-up burst, correct time on wake |

## What Gate A's fallback ladder is, if a row fails

`docs/mvp.md` §4: (i) keep Rust, swap only `surface.rs` for a DirectComposition + DXGI presenter;
(ii) split a click-through paint window from a small hit window if hit testing is the sole problem;
(iii) invoke Plan B (C#/WPF), which keeps `hanglock-core` by translation and replaces the two
platform-shaped crates. Step (iii) is a decision to make with these numbers in hand, not before.
