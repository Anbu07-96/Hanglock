# Phase 1 report — the first runnable Hanglock prototype

* Scope: the MVP vertical slice from [`docs/mvp.md`](../mvp.md) — one hanging digital clock, real rope
  physics, drag/swing/throw, always-on-top, tray, persistence. No Timer, Stopwatch, faces, themes, rope
  styles, V-mount, plugins.
* Date: 2026-09-16
* Architecture: unchanged from [`docs/architecture.md`](../architecture.md). Two recorded deviations, both
  environmental and both documented as decisions rather than silently absorbed
  ([ADR-0002](../decisions/0002-zero-dependencies.md)).

---

## 1. The thing that must be read first

**The prototype has not been compiled or run.** This build box has no Rust toolchain and crates.io is
unreachable (only `github.com` answers), so `cargo build`, `cargo test` and the Windows overlay itself
could not be executed here. What *was* executed, repeatedly, is a runnable reference model of the same
algorithms and the same painter (§4), which is where the physics was designed, measured and tuned; the
Rust implementation is a transcription of it, and a test locks the two together.

Consequences to accept before opening it:

* Expect first-build fixes: type errors, borrow-checker friction, unused imports. Every file was parsed
  with a real Rust grammar (`tree-sitter-rust`) so the syntax and structure are sound, but a grammar is
  not a type checker.
* The `unsafe` Win32 layer is the highest-risk area, precisely because it is where a compiler would
  normally catch a wrong signature. Mitigations built in: `size_of` assertions on every `#[repr(C)]`
  struct, a bounded `unsafe` surface (four files), `GetProcAddress` for the two post-2016 imports so a
  missing export degrades instead of failing to load, and `docs/windows-overlay-notes.md` carrying a
  seven-item verify list ordered by what a failure would look like.
* Nothing in §5's table is a measured Rust number. Do not treat a row as a result until
  `scripts/gate-a.ps1` has run on a Windows machine.

## 2. What was implemented

**Crates** (`workspace`, zero third-party dependencies, `#![forbid(unsafe_code)]` in three of four):

| Crate | Contents |
|---|---|
| `hanglock-core` | Verlet cord: 16 links, fixed 240 Hz step with a clamped accumulator, Gauss-Seidel relaxation with an early exit, one-sided stretch projection at 1.02×, reach + sector + host clamping of the drag target, rate-limited held node, dry-friction settling, a settle brake with hysteresis and a relevel-to-vertical step, a sector stop with restitution, plate attitude follower, and a stillness test on the plate's four corners. Plus: civil-time formatting (12/24 h, seconds, meridiem, tabular), the settings document with its tolerant codec, `placement` (swept-box sizing, monitor anchoring, DPI conversion), and `Scene` as the one immutable handoff to the renderer. |
| `hanglock-render` | A software compositor: analytic signed-distance coverage for capsules, rounded boxes and annuli; a gradient plate with a 1 px rim; a filterless distance-field soft shadow; the cord as a lit/shadowed stroke stack trimmed where the plate covers it; the generated face; premultiplied-BGRA output with dirty-rect tracking; and a stdlib-only PNG writer so the frame can be looked at headless. |
| `hanglock-platform` | Traits and small enums: `Command`, `Input`, `SystemEvent`, `Presenter`, `OverlayHost`, `FrameClock`, `Tray`, `DisplaySource`, `HitTest`. Nothing Windows-shaped, nothing that does anything. |
| `hanglock-win` | The `unsafe`: ~40 hand-declared imports and their `#[repr(C)]` layouts; the layered window (`WS_POPUP` + `LAYERED·TOOLWINDOW·NOACTIVATE·TOPMOST`); `UpdateLayeredWindowIndirect` with a dirty rect; `WM_NCHITTEST` per-pixel click-through from the painted plate and ring; two timers (`TICK` at the fps cap while swinging, `SECOND` always) and no thread of our own; `WM_DPICHANGED` / `WM_DISPLAYCHANGE` / `WM_POWERBROADCAST` handling; monitor enumeration with per-monitor scale and work area; the shell tray icon with a real popup menu; `CreateIcon` from a distance-field description; `QueryPerformanceCounter` deltas; autostart via `HKCU\...\Run` with a read-back. |
| `apps/hanglock` | `model.rs`, the state machine (`Hidden`/`Settled`/`Swinging`), where every decision lives and where every test that is about behaviour lives; `store.rs`, atomic save with corrupt-quarantine; `app.rs`, the ~150-line adapter that turns `Action`s into window calls; `main.rs`, `--dump-scene` / `--bench` / `--diag`. |

**Product surface:** the clock hangs from the top edge of a chosen monitor; grab the plate to drag it,
release to throw it; grab the ring to slide it along the top; wheel over it for hang length, `Ctrl`-free
for now; right-click (card or tray) for the menu with Always on Top, Show/Hide, Seconds, 12-hour, four
postures, hang/size steps, Reset position, Quit; settings persist to `%APPDATA%\Hanglock\settings.toml`.

**Not implemented, by scope:** double-click to toggle seconds (needs a debounce decision), keyboard
shortcuts, `Alt`+drag (needs key state through FFI), first-run autostart defaulting, the settings
window, reduced-motion.

## 3. Visual design — original, and how to see it

`docs/previews/*.png`, rendered by the reference model with the same constants and the same painter logic
as `hanglock-render`. **These are not screenshots of the app**; they are what the shipped painter will
draw, which was the only way to review the look without a desktop.

| | |
|---|---|
| `01-settled-light.png` | The steady state on a light desktop: clamp, ring, twisted cord, plate, `10:42` `PM` |
| `02-mid-swing.png` | Mid-throw: the cord whips, the plate stays near level (`plate` posture) |
| `03-settled-dark.png` | Same frame on a dark wallpaper — the rim is what keeps it an object instead of a hole |
| `04-seconds.png` | `10:42:07`, the widest run the MVP face draws |
| `05-long-hang-natural.png` | 230 px hang, `natural` posture: livelier, still bounded |
| `06-held-locked.png` | Held under the cursor, `locked` posture: level, for reduced-motion users |

Decisions worth reviewing: dark plate + white rim rather than glass or blur (a blur is the per-frame
filter cost we ruled out, and it would be ClearType-hungry text on a translucent panel); grayscale-AA
digits at a 48 px cap with a monoline 12.5 % stroke (thin strokes turn to mush without subpixel
rendering); the meridiem in the single accent colour; the cord drawn as a three-pass stroke stack; no
controls, borders, title or resize affordance visible on the object.

Known visual nits, not yet fixed: the cord's lit-edge offset reads slightly like a zipper at 100 %; the
1 px rim is derived from a distance band and can break up on near-horizontal edges (the Rust version
measures along the normal, which should fix it); the face has only the 20 glyphs needed for a clock.

## 4. Gate A — measured here, and owed on hardware

Full procedure, the geometry table and the exact fallback ladder: [`docs/gate-a.md`](../gate-a.md).

Physics, measured from the reference model (every row has a Rust test that asserts it):

| Quantity | Measured | Target |
|---|---|---|
| Settle after release (0.30 rad start) | 2.10 s | 2–3 s ✓ |
| Settle after a hard throw | 4.1 s | < 8 s ✓ |
| Throw carry, 425 px/s | **86.5° arc vs 83.1° ideal** (104 %) | ≥ 90 % ✓ |
| Residual motion at the freeze | 0.013° / < 0.25 px off vertical | invisible ✓ |
| Worst link stretch: free swing / human flick / synthetic | 1.0028 / 1.0248 / 1.036 (bounded, recovers) | ≤ 1.02 / bounded ✓ |
| Span over rest length | 1.0000 (never longer) | ≤ 1.0 ✓ |
| 60 Hz vs 120 Hz | **0.0 px** difference | exact ✓ |
| Drag tracking error at 5 000 px/s | **0.0 px** | exact ✓ |
| 60 s stall in one `step()` call | clamped, no teleport | ✓ |

Cost geometry, exact from the same formulas as the code, at the default settings:

| Scale | Window (logical) | Window (device) | Full present | 1 Hz digits present |
|---|---|---|---|---|
| 100 % | 520 × 270 | 520 × 270 | 547 KiB | 61 KiB |
| 150 % | 520 × 270 | 781 × 405 | 1.20 MiB | 137 KiB |
| 200 % | 520 × 270 | 1041 × 540 | 2.14 MiB | 244 KiB |

Which is the design's central bet in one line: **settled, it copies ~8 KB a second; swinging at 60 Hz it
copies ~72 MB/s** (the reference project measured that presenting a transparent window, not the solver,
is what costs, so the window is sized to the swept sector and the 1 Hz path is a digit rect). Idle CPU
should be ~0.0 % because the process sits in `GetMessageW` with one 1 Hz timer and no threads of ours —
structurally better than the reference implementation's 30 Hz poll-while-settled.

Owed, on hardware: idle/swing CPU %, working set, private bytes, cold start, µs/frame from `--bench`,
executable and installer size, handle/thread counts, plus the five manual observations
(click-through, focus, frame-loop-stopped, DPI change, unplug/replug, resume). `pwsh
scripts/gate-a.ps1 -Seconds 60 -WithDragSeconds 20` produces the table with those rows blank.

Three bugs the measurement caught, worth naming because they are the argument for measuring:

1. **A stillness test that could never pass.** Per-step node speed against a threshold below the
   relaxation tolerance asks the solver to converge finer than its own tolerance; a settled clock stayed
   awake 11 s. Now: whole-silhouette stillness over a 24-step window. (`0.5 px` per 0.1 s ≈ 2.5 px/s.)
2. **The brake stealing the start of every throw.** Inferring "moved 0 px" from a *missing* reference
   sample engaged the settle brake for the first 0.1 s after release. Cost 4 % of the throw's momentum
   (82.9° → 86.5° once fixed). No--sample now means no-verdict.
3. **A cord too heavy for its object.** Interior nodes at mass 1.0 against a plate at 1.55 dumped 93 %
   of a throw into swinging the rope — "the flick does nothing" — and no amount of damping tuning could
   fix it, because it was a mass-distribution error, not a friction one. Cord mass is now 0.02 per node.

Two smaller ones: the sleep check ran per frame, making settling display-rate-dependent (broke the 60-vs-120
equivalence by 18 px); and the reach clamp at 0.985 of rest length is *inside* a 1.01× per-link stretch
ceiling, so the two constraints contradicted each other — the ceiling is now 1.02× and the relationship
is stated in a comment.

## 5. Tests, build and lint

| Gate | Status here |
|---|---|
| Rust syntax of all 39 `.rs` files, real Rust grammar | **0 errors** (`tree-sitter-rust`; the check also caught a dropped `impl Host {`, a tuple-pattern type error in the settings parser, a channel-order bug in the painter, and a mis-parenthesised destructuring) |
| `cargo fmt --check`, `clippy -D warnings`, `cargo test --workspace` | **not run — no toolchain.** CI runs all three on `ubuntu-latest` and `windows-latest` (x64 + arm64) |
| Test suite: 60 `#[test]` functions (13 solver, 11 settings, 8 placement, 7 painter, 13 model, 1 golden trace, 7 clock) | **written, not executed** |
| Face generator reproducibility | `gen_face.py` output is diffed in CI, so `face_data.rs` cannot drift from the reviewed geometry |
| Size gate | CI fails above 2 048 KB; `build.ps1` throws likewise |

Test philosophy, since one of them looks unusual: the golden trace replays 150 frames of a scripted
drag-and-release against `tests/golden/trace_drag_settle.txt` at 1e-4 px per node. Property tests cannot
notice a change that makes the swing subtly wrong — a golden trace of *positions* (not pixels, which
would be brittle for no gain) can, and it is the guard that keeps the reviewed feel and the shipped
solver from parting company.

## 6. Known issues

1. **Uncompiled.** The dominant risk; see §1. Budget a first-build session for type and borrow fixes.
2. **Gate A's hardware rows are empty by construction**, and one target may simply miss: if a full
   present at 150 %/60 Hz measures poorly on a weak iGPU, the answer is the recorded fallback (a
   DirectComposition presenter inside `surface.rs`), not an architecture change.
3. **The idle story is not literally zero.** A settled clock wakes once a second to change a digit. It
   does no physics, no scene build and no cord/plate repaint beyond the primitives' own bounds, and
   presents ~137 KiB. "Frame loop completely stops while settled" is asserted as: `step()` returns
   false, `Action::PresentFull` never appears, the `TICK` timer is killed.
4. **Popup menu focus dance.** `TrackPopupMenu` needs our window briefly foreground to dismiss on
   click-away; we restore the previous foreground window afterwards. The user's caret is unaffected, but
   this is the one place `SetForegroundWindow` is called, and it is the first thing to re-examine if a
   menu ever sticks or steals a focus change.
5. **`click_through = "always"` and drag are mutually exclusive** in v0.1: with the whole window
   `WS_EX_TRANSPARENT`, the clock cannot be grabbed or moved, and quitting from the card is impossible —
   the tray icon is the only way back. The menu exposes it; a future version should auto-restore
   interactivity while a drag is in progress.
6. **No manifest**, so DPI awareness is set at runtime rather than before DLL init, and long paths are
   not enabled. Works; the tidy version (`/MANIFESTINPUT` in `build.ps1`) is a follow-up.
7. **Monitor enumeration** uses a 64 px `MonitorFromPoint` scan for non-current displays rather than
   `EnumDisplayMonitors`, to keep callbacks out of the FFI layer. Correct for any realistic layout, ugly
   in principle; replacing it is a contained change in `displays.rs`.
8. **`hang --help`-style discoverability is thin:** the ring-drag affordance for moving the clock is not
   labelled. A first-run tooltip or a balloon is Phase 3.
9. **Face covers 20 glyphs.** Anything else renders as nothing (asserted), which is the right failure for
   a clock but means Timer/Stopwatch labels in Phase 3 need geometry authored first.
10. **Licence/branding stance** (MIT for code, public-domain icon and wordmark — the opposite of the
    reference project's reserved artwork) is proposed in `LICENSE` and needs the owner's sign-off.

## 7. Files added or substantially changed

```
Cargo.toml rust-toolchain.toml .gitignore .github/workflows/ci.yml
LICENSE CONTRIBUTING.md README.md assets/SOURCES.md
crates/hanglock-core/          (5 modules + rope/{mod,config,node,constraints,drag,card}.rs
                                + clock/{mod,format}.rs + settings.rs + scene.rs + ids.rs + placement.rs)
  tests/{rope.rs, placement.rs, settings.rs, golden_trace.rs}
crates/hanglock-render/        src/{lib,canvas,paint,theme,png,face_data}.rs, tests/paint.rs
crates/hanglock-platform/      src/lib.rs
crates/hanglock-win/           src/{lib,sys,window,surface,tray,displays,time,icon,autostart}.rs
apps/hanglock/                 src/{main,model,store,app}.rs
tools/model/hanglock_ref.py    tools/model/gen_face.py
tests/golden/trace_drag_settle.{txt,json}
docs/architecture.md docs/mvp.md docs/roadmap.md docs/physics.md docs/windows-overlay-notes.md
docs/gate-a.md docs/index.md docs/reports/phase-1.md
docs/decisions/0002-zero-dependencies.md
docs/previews/*.png (6)
```

**Sizes.** 7 064 lines of Rust in 39 files: 6 367 in `src` plus 697 in `tests/`. `hanglock-win` is
2 100 of those, 631 being the `sys.rs` declarations; every line of `unsafe` sits in that crate, and
`hanglock-core`, `hanglock-render` and `hanglock-platform` carry `#![forbid(unsafe_code)]`. The Python
reference model the physics was tuned against is ~700 lines and is not shipped. 60 `#[test]` functions exist — 13 solver,
11 settings, 8 placement, 7 painter, 13 model/geometry, 1 golden trace, plus 7 clock-formatting tests
in-crate. **None have been executed here.**

## 8. Commit and push

| | |
|---|---|
| Branch | `arena/01a0a5bc-hanglock` |
| Commit | one squashed commit on the branch, *"feat: the first runnable Hanglock prototype, and the plan it came from"*, on top of `3d2fcb4`; its own hash cannot be written inside itself, so: `git log -1 --format='%h %s' arena/01a0a5bc-hanglock` |
| Tree | 78 files added/changed, working tree clean |
| Push | **failed** — `fatal: could not read Username for 'https://github.com'`: the GitHub token Arena injects for this session is expired, so `git push` and `gh` cannot authenticate. Same failure as Phase 0. Fix: reconnect GitHub in Arena, then `git push origin arena/01a0a5bc-hanglock`. Nothing has been pushed to `main`; the repo's only remote branch remains `3d2fcb4`. |

## 9. What to review, in the order that saves time

1. `docs/previews/` — is this the object you wanted hanging there? (Cheap to change now: the plate, the
   weight of the digits and the cord's treatment are all in `theme.rs` and `face_data` generation.)
2. The feel numbers in §4 — is 86.5° of carry with a 2.1 s settle the right temperament, or should it be
   livelier (lower `friction_acc`, `natural` posture) or more obedient (`mounted`)?
3. Whether to spend a session getting a Rust toolchain into this environment (or moving the build to a
   Windows box / a `cargo`-capable CI run) *before* Phase 2. The next phase's main risk is not design,
   it is that every line I cannot compile is a line the first reviewer has to fix.
