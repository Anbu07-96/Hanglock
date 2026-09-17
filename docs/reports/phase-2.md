# Phase 2 report — usable: the hang point, the mouse modes, the tray, the settings window

* Scope: the usability half of [`docs/mvp.md`](../mvp.md) §2 — interaction, recovery, settings,
  first-run behaviour, and the visual polish of the one face that already existed. Explicitly out, and
  still out: Timer, Stopwatch, analog, world clocks, extra faces, themes, custom ropes, V-mount,
  installer/signing work, macOS, auto-update, plugins.
* Date: 2026-09-17. Every CI number below is read out of run
  [35173251285](https://github.com/Anbu07-96/Hanglock/actions/runs/35173251285) (the push run for
  `dbf9cba`), whose three jobs — Linux, Windows x86_64, Windows aarch64 — are green step by step, and out
  of the `pull_request` run 35173254159 for the same commit, which says the same thing. Where a figure
  could not be measured it is marked as such rather than estimated (§16).
* Branch `arena/01a0a5bc-hanglock`: sixteen commits on top of Phase 1's `9dcf3b9`, 34 files,
  +5 376/−552. PR #1 stays open and unmerged, as the phase brief required; nothing here touched `main`.

---

## 1. The thing that must be read first

**Phase 1's prototype had never been *used*, and the difference between "renders a clock" and "a thing
you leave on your desktop for a week" turned out to be four small things, none of them physics.** Where
it hangs, and how to put it back. What the mouse is for, in words. A settings surface that can hold a
position — a menu item cannot. And when the app may write a file.

The rope solver was not rewritten, and that is a result rather than an omission: every change to how the
clock is placed or dragged routes through the existing `place()` and `Rope::refit`/`set_anchor`, so the
golden trace, the 1.03 stretch bound, the throw's carry and the sleep-when-settled behaviour are the same
code that passed in Phase 1. The one new physics-adjacent number is `ANCHOR_INSET = 14.0`, inside
`swept_box`, and it exists so the clamp drawn at the hang point is never cut off by the top of the screen.

Two things in this phase were decided against my first design, and both are better for it:

* The anchor is stored as `(anchor_ratio, anchor_drop)` — a fraction of the usable width and a distance
  below the hang line — rather than as a point, and **both numbers are quantised to the four decimals the
  settings writer emits, inside `Anchor::clamped`.** The first version stored the full `f64` and wrote
  `{:.4}`, which meant `from_toml(to_toml(s)) != s`: saving your position moved it, and the round-trip
  test failed by a float's last digits. Fixing that in the *type* rather than by loosening the test into a
  tolerance soup costs ≤ 0.1 px of drag precision, which is the right trade.
* The settings window is a renderer of rows and nothing else. The alternative — a window with a draft
  copy of the settings, edited then committed by Apply — would have put a second writer in front of the
  document, and every value in that document is range-checked *because* it might arrive from a hand-edited
  file. So there is no Apply button, no validation in the window, and no setting the window can put the app
  into a state the rest of it must special-case. The tray answers with the same `Command` values, which is
  the half of the arrangement that has to hold.

## 2. What was implemented

`hanglock-core`

* `src/anchor.rs` *(new)* — `Anchor { ratio, drop }`, `DEFAULT`, `clamped` (the quantising clamp),
  `from_overlay`, and `dragged(&Monitor, &Settings, &RopeConfig, anchor_device, delta_device)`, which
  turns a device-pixel drag into that pair. `dragged` measures from where the hang point *is on screen*
  rather than accumulating onto the stored pair, because `place()` pulls an over-the-edge frame back onto
  the display and the two legitimately differ by the clamp's inset; accumulating onto the stored number
  produced a 14 px dead band at the top, which is how the first draft failed its own test.
* `src/ids.rs` — `ClickThrough` grows from two values to three (`Solid`, `Hover` default, `Always`) with
  `label()`, `ALL`, `interactive()`, `takes_window()`, `ignores_input()`; `PostureKind` gains `label()`
  and `ALL`, so menu wording lives beside the enum and one bug cannot become two menus.
* `src/settings.rs` — `anchor_drop` (key, `limits::ANCHOR_DROP = (0.0, 4096.0)`, `sanitize` arm, both TOML
  arms) with `SCHEMA` still 1, because a missing key is a default and not a migration; `Settings::card()`,
  which is now the only place design size × `overlay.scale` becomes a `CardSpec` (the duplicate `card_for`
  in the app crate is gone, and with it the chance of two different answers); `Settings::anchor()`.
* `src/placement.rs` — `ANCHOR_INSET` in `swept_box`; `hang_line(m, respect_taskbar)`, so the anchor's band
  and the window's placement are measured from the same line by construction; `place()` gained `anchor_drop`
  after `anchor_ratio` and clamps the hang point into the display, so a hand-edited file cannot put the
  clock where it cannot be reached.

`hanglock-platform`

* `src/panel.rs` *(new, 371 lines)* — the settings form as data: `RowId`, `Step`, `Control`
  (`Check`/`Choice`/`Nudge`/`Push`), `Row`, `Group`, `HANG_STEPS`, `hang_index`, `form(&Settings,
  &[Monitor])` and `change(&Settings, RowId, Step) -> Option<Command>`. Two surfaces, one source.
* `src/lib.rs` — `Command` from 12 to 19 values (the seven new ones are `ToggleMeridiem`,
  `SetClickThrough`, `SetMonitor`, `SetAnchor`, `ToggleLaunchAtLogin`, `OpenSettings`, `About`), and it is
  no longer `Eq`: it carries an `Anchor` of `f64`, and deriving `Eq` on a float is a lie waiting to be
  relied on. `Input::Press` gained `alt`; `OverlayHost` gained `tray_present()` and
  `set_ignore_input(bool)` became `set_input_mode(ClickThrough)`; `Dialogs` is the window contract.

`hanglock-win`

* `src/panel.rs` *(new, 850 lines)* — the settings window: a registered class, `BUTTON`/`STATIC` children,
  one `unsafe fn` per Win32 edge, laid out in logical px at the window's own DPI, re-laid out on
  `WM_DPICHANGED`, `AdjustWindowRectEx` for the frame, `SPI_GETICONTITLELOGFONT` for the font,
  `IsDialogMessageW` from the message loop for Tab/arrows/Space/Esc, and a `Box<Panel>` in
  `GWLP_USERDATA` freed in `WM_NCDESTROY` after the window long is cleared.
* `src/window.rs` — `Host` implements `OverlayHost` and `Dialogs` for real (batch 1 defined the traits and
  nothing implemented them, which is the kind of seam that quietly rots); `set_input_mode` writes the two
  styles together on change only; Alt is sampled at `WM_LBUTTONDOWN`; `HitShape::whole_window` is the half
  of `Solid` that is not a style; the loop's dialog-manager call is gated on the panel's own message queue.
* `src/tray.rs` — the menu rebuilt as three groups with three submenus, `MenuState` as a snapshot carrying
  the refusal, and the tooltip carrying it too (compared before writing: `apply` runs every frame).
* `src/autostart.rs` — `StartupApproved\Run` read, cleared on enable, `approved_bytes` as the pure part;
  `apply()` answers with what a read after the write reports.
* `src/sys.rs` — the imports and constants the window needs, `id_menu` for the `hMenu`-as-control-id
  overload (the crate's one integer-to-pointer cast, allowed once with the reason beside it), and a
  `LOGFONTW` with its 92-byte `size_of` pin.
* `src/displays.rs` — `monitors()`, the windowless list `--diag` asks for.

`apps/hanglock`

* `src/model.rs` — `layout_anchor`, `tray_ok`, `notice` (+`TRAY_REQUIRED`), `Reposition`,
  `apply_monitor` with pure `placement(&Monitor)`, `anchor_device()`, `set_anchor(Anchor, rehanging)`, the
  Alt-press re-anchor gated on `click_through.interactive()`, `HitRegions.whole_window`, nine new
  `on_command` arms, and `on_ready(tray_present, launch_at_login)` replacing the old `menu_state()`.
* `src/store.rs` — rewritten: `load_at`/`save_at` for tests, an unexpanded `display_path()` for
  diagnostics, no write on first run, quarantine only for a document that looks like someone else's file,
  and `Outcome: Display` so the recovery sentence is one line on stderr and the same line `--diag` prints.
* `src/app.rs` — the wiring: `on_ready` before the first frame, the new actions folded in `apply`,
  `panel_command` for the window's clicks, the `MenuState` snapshot, a tooltip that drops a refusal when
  the gesture that made it false arrives.
* `src/main.rs` — `store::load()` with the one stderr line; `--diag` now prints the §7 fields.

`hanglock-render` — one deletion: a dead `padding_x`. Centring was measured, not assumed, and is correct.

## 3. New settings and options

| Key | Values | Notes |
|---|---|---|
| `overlay.anchor_ratio` | `0.0..1.0`, default `0.5` | Across the usable width. Quantised to 4 decimals at rest |
| `overlay.anchor_drop` | `0.0..4096.0`, default `0.0` | Logical px below the hang line. Absent in a Phase 1 file, which is why `SCHEMA` did not move |
| `overlay.click_through` | `"solid"` \| `"hover"` \| `"always"` | `always` is refused while the tray icon is absent |
| `face.meridiem` | bool, default `true` | Meaningful only with `hour12`; the menu item is greyed otherwise rather than silently inert |
| `general.launch_at_login` | bool, default `false` | Reconciled against `Run` **and** `StartupApproved\Run` at startup |
| `overlay.monitor_index` | u32 | Switchable from the tray and the window; an unplugged index falls back through primary → first |

`enabled`, `hang`, `scale`, `opacity`, `topmost`, `respect_taskbar`, `margin`, `face.hour12`,
`face.seconds`, `face.posture`, `general.fps_cap` are Phase 1's, unchanged — and unchanged *including*
their decode rules, which is what the backward-compatibility tests assert: a Phase 1 file loads, gains the
new keys at their defaults, and re-writes identically.

## 4. Tray menu structure

```
Show clock ✓
──────────
Always on top ✓
Mouse: Transparent areas click through  ▸  Interactive (whole window) / ✓ Transparent areas click through / Fully click-through
Show seconds ✓
12-hour time ✓
AM / PM ✓                      (greyed while 12-hour time is off)
How it swings: Plate           ▸  Natural / Plate / Mounted / Locked
Hang from this display         ▸  one row per display, greyed when there is only one
──────────
Hang longer · Hang shorter · Bigger · Smaller
Reset position
──────────
Settings…
Start with Windows ✓
──────────
About Hanglock
Exit Hanglock
```

Three groups, not a list of every setting: the two decisions the clock cannot show you, the two you change
for an evening, the one that gets you out of a corner, and then the window. `Timer`/`Stopwatch` items are
not here, by instruction. The menu is built from a `MenuState` snapshot rather than from live settings — a
menu is open for as long as the user reads it, and a menu that queried state would be rebuilt under the
cursor mid-track. If the tray icon could not be installed, a greyed first line says so, and the tooltip
says the same thing; that line is also where `Fully click-through` refuses itself.

## 5. Interaction changes

| Gesture | Now |
|---|---|
| Drag the plate | Unchanged from Phase 1, including release velocity and the throw |
| **Alt + drag, anywhere on the plate** | Moves the hang point: the window follows the pointer, the anchor becomes the `(ratio, drop)` pair, release commits and saves. The card settles under the new anchor on release rather than during the gesture, because a re-anchor goes through `set_anchor` + `wake`, not through a rebuild of the cord |
| Drag the hang ring, no modifier | The same re-anchor, for the pointer that was already aimed at it |
| Wheel | Six cord rungs, shared with the window's `Cord` row via `panel::HANG_STEPS`, so "one step longer" is one number everywhere |
| Tray `Reset position` | Anchor to top-centre, cord 150, size 1.0, un-hides; leaves the *display* choice alone, because moving the clock to another monitor was not what was asked |
| Settings window | Every row is one click = one command. A nudge's `Less`/`More` are disabled at the ends of their range rather than wrapping or clamping quietly |
| Esc / Enter / Tab / arrows in the window | Work, from one `IsDialogMessageW` call in the loop, gated so the overlay's messages never pass through a dialog manager |

Alt is read **at the press**, from `GetKeyState`, and latched for the gesture: a modifier sampled from move
messages would let someone release Alt mid-swing and teleport the hang point to the cursor.

## 6. Visual changes

No new face, no themes, no second style — the brief said polish the one clock, and that is what happened.

* The rope now meets the plate at the swept box's top edge with `ANCHOR_INSET` of clearance, so the clamp
  is drawn rather than cropped: on a display without a top-docked taskbar the first 14 px of `anchor_drop`
  are already spent, which is why `drop = 0` and `drop = 10` look identical and `drop = 20` does not. That
  is a deliberate "small values are one position", and it is written in the field's doc comment.
* `Hang from this display` and the mode names in the tray are in the same words as the settings window,
  because both read `label()` on the enum rather than their own strings.
* The card's own context menu and the tray's menu are the same list, so learning one is learning both.
* Digits, plate, rim light, shadow, AM/PM and seconds layout: untouched, and `--dump-scene`'s PNG is
  byte-identical to Phase 1's at the default settings, because `place()` with `anchor_drop = 0` reproduces
  the Phase 1 geometry exactly — which is also why the six existing placement tests were left with their
  assertions unchanged apart from the new argument: CI passing them *is* the pin.

## 7. Tests added

51 new `#[test]` functions, every Phase 1 test preserved (nothing was deleted, weakened or `#[ignore]`d):

| File | New | What they hold |
|---|---|---|
| `core/tests/anchor.rs` | 7 | The clamp band, the quantisation, a drag at 100 % and at 175 % agreeing in logical px, a drop that would leave the display, a ratio pushed past an edge, and the round-trip through the document |
| `core/tests/settings.rs` | +4 | `anchor_drop` survives a round trip; a Phase 1 file without it decodes to `0.0`; `card()` follows `scale`; `anchor()` agrees with the pair |
| `core/tests/placement.rs` | +3 | `drop = 0` is the old placement; a huge drop is pulled on-screen rather than off; a display shorter than the swept box still keeps the hang point visible (`f64::clamp` panics when `min > max`, which is why every band here carries a `.max(floor)` guard) |
| `platform/tests/panel.rs` | 7 | Every row exists once; each control produces the command the model accepts; `Cord` steps along `HANG_STEPS` and stops at the file's own limits; the nudge buttons grey out at the ends; `form` derives from the settings it was given, not from a copy |
| `hanglock-win/src/panel.rs` | 6 | Every part of every row has its own id and reads back as the control that was created (the one arithmetic in this window that could silently answer the wrong setting); every control inside the client rect at 0.5–3.0 scales; a choice ticks exactly one option; a label never answers with a step; the nudge greys the button that cannot move the value; rows read back in creation order |
| `hanglock-win/src/autostart.rs` | 1 | `approved_bytes`: only the known disabled marker disables, and a short or unknown blob does not |
| `apps/hanglock/src/store.rs` | 9 | No file on first run; a bad-TOML-but-not-really file is never renamed; only a document with no `[section]` and no `schema` is quarantined; a directory in the way is survivable; save→load round-trips; the recovered path is in the message; a truncated document keeps what survived; a value that is not a number leaves that key at its own default |
| `apps/hanglock/src/model.rs` | +14 | Alt+press re-anchor, ring press, the anchor surviving `relayout` across two monitors, `ResetPosition` un-hiding and leaving the monitor alone, `Fully click-through` refused and accepted, `SetAnchor` no-op guard, meridiem only mattering with `hour12`, 12/24 formatting and the seconds toggle at the format layer, `on_ready` reconciliation with the registry, `placement(&Monitor)` purity, and a drag that quantises |

## 8. Total test count

**111 `#[test]` functions** in the workspace (Phase 1: 60), and CI agrees with the tree on both sides of
the `cfg`: the Linux job's notice reads `unit 104 passed, 0 failed`, each Windows job's reads
`unit 111 passed, 0 failed` — the seven-test difference being `win/src/panel.rs`'s 6 and
`win/src/autostart.rs`'s 1, which only compile on Windows. Counted from the tree with
`git ls-files '*.rs' | xargs grep -c '^[[:space:]]*#\[test\]'`: model 27, `core/tests/settings.rs` 15,
`core/tests/rope.rs` 13, `core/tests/placement.rs` 11, `store.rs` 9, `platform/tests/panel.rs` 7,
`core/tests/anchor.rs` 7, `render/tests/paint.rs` 7, `core/src/clock/format.rs` 7, `win/src/panel.rs` 6,
`core/tests/golden_trace.rs` 1, `win/src/autostart.rs` 1.

**Doc tests: none, by design, and the earlier figure was wrong.** `doc 0 passed, 0 failed` on all three
jobs. There are two fenced blocks in the sources (`main.rs`'s command-line table, `rope/mod.rs`'s step
order) and both are fenced as `text`, which rustdoc does not compile. What this project runs as an executable
example is instead the three headless modes, and CI runs them on Windows. The "10 doc tests" figure that
`8374c63` wrote into `docs/index.md` and into §8 of this report had no command behind it, and it is in no
report — `docs/reports/phase-1.md` publishes 60 tests and says nothing about doc tests — which is exactly
the failure this repo's numbering rule exists to catch, in the one place the rule was not applied.
`docs/index.md` is corrected in the same commit as this report. The totals in `docs/gate-a.md` are left
alone on purpose — that sentence describes run 35160471461 at `2654554`, and 60 tests on all three targets
is what that run did.

## 9–12. CI: three jobs, and what each one measured

Run [35173251285](https://github.com/Anbu07-96/Hanglock/actions/runs/35173251285) at `dbf9cba`:
every step of every job succeeded. The `pull_request` run for the same commit (35173254159) agrees.

| Job | Verdict | Tests (from the job's own notice) | Artefacts |
|---|---|---|---|
| `Model + painter (Linux)` | green — Format, Clippy, Test, Golden trace is current, Bench, Asset provenance | `unit 104 passed, 0 failed \| doc 0 passed, 0 failed` | none built; the trace and the face data are checked against their generators instead |
| `Build + test (Windows x86_64-pc-windows-msvc)` | green — Clippy, Test, Build release, Smoke, Size gate, Upload artifacts | `unit 111 passed, 0 failed \| doc 0` | `hanglock.exe` **363 008 B = 355 KB** of a 2 048 KB budget (Phase 1: 318 976 B, so +44 032 = +13.8 %) |
| `Build + test (Windows aarch64-pc-windows-msvc)` | green, same six steps on a native ARM64 host | `unit 111 passed, 0 failed \| doc 0` | `hanglock.exe` **315 392 B = 308 KB** (Phase 1: 277 504 B, so +37 888 = +13.7 %) |

The two Windows sizes sit 5.8× and 6.6× inside the gate. The prediction in this section's earlier draft —
"a few tens of KB, not hundreds" — was right: `hanglock-win` went from 2 347 to 3 831 lines and the app
crate from 1 634 to 2 789 (+63 % and +71 % of code) for +13.8 % and +13.7 % of bytes, which is what a
zero-dependency release build with `lto = "fat"` and `panic = "abort"` costs. The whole workspace is now
13 568 lines of Rust (core 3 476, render 2 497, platform 975, win 3 831, app 2 789).

**Smoke.** Both ABIs run the built `release` binary headless, and both printed
`559897 bytes written by --dump-scene` — the same figure Phase 1's green run published, so the picture the
headless painter makes did not change during a phase about interaction. `--diag` runs on the real runner and
its transcript is quoted in §15; it is also where the first-run promise is visible on Windows:
`as read no file yet; defaults, none written`.

**Costs.** The bench line is published as a notice on every run, because a gate nobody can read is a
rumour. Linux, release, 300 frames: `paint 2700.4 us/frame (budget 400 us) … solve+paint/sec 162.0 ms of a
1000 ms budget at 60 Hz`. x64 smoke (200 frames): `4147.8 us/frame`, `248.9 ms`. ARM64: `3330.0 us/frame`,
`199.8 ms`. The budget figure is exceeded by the *synthetic* case, and §16 item 11 keeps that honest instead
of explaining it away.

**How these numbers were read.** The runners' log hosts refuse this environment, so the only channel is the
check-run annotations the workflow packs itself: `tools/ci/emit.py` writes a failing step's log as chunked
gzip, `tools/ci/decode.py` reads it back, and `tools/ci/notice.py` publishes one line from a step that
*passed* — the latter existed for the bench and the smoke lines before this phase and now, since `dbf9cba`,
for the test totals too. That last change is why §8 can quote "111 passed" as a CI measurement instead of a
grep: a green run now publishes what it ran.

```sh
python3 tools/ci/decode.py 35173251285                 # every annotation of a run
python3 tools/ci/decode.py 35173251285 --grep '^error' # only the failures
```

**One risk on the horizon, from the runner itself.** The ARM64 job carries an annotation: *"The
windows-11-arm label will migrate to use Visual Studio 2026 by default beginning September 2026-09-21, for
more information see https://github.com/actions/runner-images/issues/14602"* — four days from this report.
This repo pins Rust 1.77.2 and links whatever MSVC the image provides, so the ARM64 job should be re-run
after that switch and, if the toolchain and the pinned compiler disagree, the fix belongs in the image pin or
the toolchain — not in a relaxed gate.

## 13–14. Commits

| Commit | What |
|---|---|
| `d4bfc0b` | Phase 2 (1/4): the hang point, the mouse modes, the settings form. 15 files, +2 460/−201 |
| `374e1b4` | Phase 2 (2/4): the tray, the mouse modes and the settings window on Windows, plus `d4bfc0b`'s two CI fixes. 10 files, +1 671/−213 |
| `4af0713` | docs: what Phase 2 made true, and what it made false. 7 files, +228/−49 |
| `8374c63` | tools: `decode.py`, the reader for the only CI channel this box can use. 3 files, +513/−7 |
| `3a1421c` | Phase 2 (3/4): rustfmt's own diff transplanted (48 hunks) and the five functions `too_many_lines` refused. 13 files, +2 435/−464 |
| `c82e775` | Phase 2 (4/4): what the first Windows compile found — 9 more fmt hunks, 6 `-win` compile fixes, one accidentally-committed `.orig`. 8 files, +75/−1 836 |
| `f0d7552` | Phase 2 (4b): the twelve lints and two type errors behind the first clean Win32 compile. 5 files, +22/−19 |
| `c2b223c` | Phase 2 (4c): the `# Safety` sections two new `unsafe fn` owed, and one more by-value argument. 3 files, +15/−5 |
| `cd5c8e1` | Phase 2 (4d): the app crate on Windows — `store::file_in` replacing a duplicated path fn, `let-else`, two unused bindings. 3 files, +18/−15 |
| `83c397b` | Phase 2 (4e): the model carries `shown`, so a format command reformats the time it was given instead of re-reading its own digits; `current_fields_from` deleted. 2 files, +55/−27 |
| `9bdbd2e` | Phase 2 (4f): `hang_can_up`/`hang_can_down` — the cord's buttons grey on whether a press would move, not on which rung is nearby; two batch-1 tests corrected. 4 files, +75/−12 |
| `ee55826` | Phase 2 (4g): the cord test compiles (`FnMut`). 1 file, +2/−1 |
| `eb5ae6e` | Phase 2 (4h): settings-window child ids allocated by counting parts instead of by a fixed stride (`Panel::row_part_of`), the DPI/device-px bound in the layout test, `store::save`'s cfg. 2 files, +96/−26 |
| `1ff5592` | Phase 2 (4i): `row_part_of` answers `None` for an empty panel; `--diag`'s path asserted per platform instead of Windows-only. 2 files, +14/−1 |
| `efbd85a` | Phase 2 (4j): `let row = self.row_at(i)?` — `clippy::question_mark`, and the shorter form is the clearer one. 1 file, +5/−6 |
| `dbf9cba` | ci: publish how many tests ran on the runs that pass (the source of every total in §8 and §9–12). 1 file, +23/−1 |

Phase 2 in one line: 16 commits, 34 files, +5 376/−552, ending with all three jobs green. `d4bfc0b`'s
message understated the new model tests as "+6"; measured against its parent the delta is +14, and §7 is the
corrected account — the message was not amended, because rewriting pushed history to fix a sentence is a bad
trade.

`dbf9cba` is the last commit that changes code. The commit that publishes this report is the branch tip after
it — documentation only, verified by its own run, which is the state the phase ends on.

## 15. Screenshots and rendered previews

No new renders. `docs/previews/` still shows the shipped picture, and that is a claim rather than a
convenience: nothing in `hanglock-render` changed except the deletion of an unused constant, and the
placement maths is identical at the default `anchor_drop = 0.0`. The previews come from
`python3 tools/model/hanglock_ref.py render <dir>`, which the golden trace keeps honest to the Rust
painter; the Windows build still has no screenshot in this repo because CI has no desktop to take one on. To see the new behaviour
without a desktop: `cargo run -p hanglock -- --diag` prints the anchor as both numbers and as a screen
coordinate, the display it was placed on, that display's DPI, the clock format, the mouse mode and
always-on-top, and the settings path unexpanded (`%APPDATA%\Hanglock\settings.toml`) so a pasted
diagnostic contains no account name.

That transcript is now in CI's record rather than only in a log. Both Windows jobs run the built binary with
`--diag`, and both published the same text (one line, because `notice.py` joins the lines and collapses their
column padding — that is the packing, not the output):

```text
hanglock 0.1.0 /   settings file   %APPDATA%\Hanglock\settings.toml /   as read  no file yet; defaults,
none written /   displays  1 /     #0  1024x768 at 0,0  work 1024x720  100% (96 dpi), primary /   on
monitor   #0 of 1 /   frame (device)  252,0 520x269  clipped=false /   anchor  ratio 0.5000 -> x 512.0;
drop 0.0 logical; device 260.2,14.0 in the frame /   anchor (screen) 512.0,14.0 device px /   clock
12-hour, with AM/PM, seconds off; shows "10:42" /   mouse  Transparent areas click through (hover) /   on
top  on /   visible  yes /   autostart  off /   posture  plate (Swings a little) /   hang  150 logical px,
150.0 device px, 16 links /   size  100%, margin 16 /   state  Settled, sleeping=false /   canvas  520x269,
present_rect true /   fps cap  60 Hz, 60 frames per present at 60 Hz /   budgets  paint 400 us, window
520x269 device, buffer 546 KiB
```

Four things are worth reading out of that: `as read no file yet; defaults, none written` is the first-run
promise, executed on a real Windows desktop image and not on a mock; `anchor` gives the same position as a
ratio, as a logical drop and as a device point, which is the whole reason the pair is stored instead of a
point; `clipped=false` is the placement's own report that the swept box fits; and `mouse` says what the
mouse will do, in the words the tray menu uses.

## 16. Known limitations

1. **Nothing in this phase was ever executed as a window.** The Win32 layer is reviewed, arithmetic-checked
   and pinned by `size_of` assertions, and its pure parts (row→command, geometry at six scales,
   `approved_bytes`) have tests — but the first real `WM_PAINT`-equivalent for the settings window is a
   desktop. `docs/windows-overlay-notes.md`'s verify list gained five items for exactly this.
2. The settings window is laid out for one width and cannot be resized. Chosen, not left: content sized by
   its rows, a frame added by `AdjustWindowRectEx`, and no scroll state, so a resize would need either a
   scrollbar (a common-control question) or clipped rows.
3. The `Cord` and anchor rows step; they do not type. A spinner or edit box would introduce the moment at
   which text becomes a value, and that moment is where a settings window starts owning state.
4. The window is a sibling, not a child, of the overlay: it can be lost behind another app. It is not
   topmost on purpose — a settings window that floats above the user's work would be worse.
5. `About` is a modal `MessageBoxW`, so the clock's own frames pause while it is open. For a two-second
   read that is the right trade; it is not a place to put anything a user watches.
6. No `StartupApproved` *write* beyond clearing it on enable: if a user disables us in Task Manager while
   running, the checkbox is corrected at the next start, not live, because there is no notification for
   that key and polling the registry for one is exactly the per-frame work this app's design forbids.
7. Anchor drags quantise to 0.0001 of the width, which on a 3840 px display is 0.384 px. The clock cannot
   be placed finer than the file can say, and the file's unit was chosen for round-trip honesty, not for
   pointer resolution.
8. The reference model has no anchor drop: `hanglock_ref.py` places from the ratio alone, so it
   reproduces the default position exactly and nothing else. Porting the second parameter is one function,
   but it would regenerate `tests/golden/trace_drag_settle.*` and re-open every physics number published in
   Phase 1, which is a worse trade than a preview set that stops at the default.
9. Still no installer, no `CHANGELOG`, no signed binary: Phase 1's §16 items 4–6 survive untouched, and
   the release now blocks on numbers this phase changed rather than on packaging code.
10. **The infrastructure this phase depended on is fragile, and it bit twice.** The sandbox's GitHub token
    expired mid-phase (run results unreadable for several cycles) and the sandbox was later reset, which
    collapsed the local history onto the root commit while leaving the working tree intact — recovered by
    fetching the remote tip and re-committing on top of it, with no force-push and no history rewritten. Both
    are why this repo keeps `tools/ci/decode.py` and insists on measuring rather than remembering: §8's
    "10 doc tests" survived one commit before a command contradicted it.
11. The face paint costs more than the number in the budget line. CI's bench prints 2 700 µs/frame on Linux
    and 3 330–4 148 µs/frame on the Windows runners against `paint 400 us` — the 400 µs figure is the
    original design target for a digits-only repaint, and the bench repaints a whole 780×403 canvas every
    frame at full `present_rect` off. What keeps this from being a live problem is that the app does not
    paint when nothing moves: a settled rope asks for no frames (`a_settled_rope_stops_asking_for_frames`),
    and the second-tick path repaints the digits' box. Solving the cost properly is a painter decision
    (dirty-row tracking through `present_rect` on the real surface, or a smaller canvas), not a Phase 2 one,
    and no number in this report was smoothed to hide it.
12. `bool_of` in the settings reader accepts `true|1|yes|on` and reads everything else as false, so a
    hand-typed `hour12 = n0` silently means 24-hour instead of being refused. It is deliberate — the wide
    truthy list is what lets a person edit the file in Notepad — and it is now pinned by a test rather than
    discovered. The asymmetry to know about is that a *number* that does not parse falls back to that key's
    own default (a second test), so junk text is treated differently by type. If a future phase ever offers
    a text field for a value, this is the reason it must not.

## 17. Phase 3 recommendation

Phase 2 is closed: three jobs green at `dbf9cba`, 111 unit tests passing on both Windows ABIs and 104 on
Linux, both binaries inside the size gate with 5.8× and 6.6× of headroom. Nothing here is owed a verification
that CI could perform; what is owed is a desktop, which CI cannot provide.

Do the two things Phase 2 made obvious, in this order, and keep them small.

1. **A desktop session, as a gate.** Not a feature: `mvp.md` §2's list — mixed-DPI unplug/replug, a
   full-screen game on Alt+Tab, an antivirus scanner holding `settings.toml` open, Task Manager's Startup
   page, idle CPU and working set with the settings window open and closed — is now the only thing standing
   between this and `v0.1.0`. One afternoon, and the report writes itself from the run.
2. **Then Timer and Stopwatch**, which is where Phase 3 was always going: the mode plumbing already exists
   as data (`FaceText::suffix_str`, `posture`, `Command`, the row form), and a countdown is the first
   feature that needs the card itself to *show* a state, which the design has been refusing to do with
   dialogs and focus steals since day one. Per-mode persisted state through the same tolerant document,
   `schema = 2` if and only if a key has to change meaning.

Deliberately *not* next: the settings window's resize and search, a per-monitor position store, themes, and
any installer automation — each is a Phase 4+ concern dressed as an urgent one.
