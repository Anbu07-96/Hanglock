# Hanglock MVP — scope and implementation plan

> **Phase 2 status:** steps 0–7, 9 and 10 of §3 are built and compile in CI, which is the only place
> this repo is ever compiled: the Linux job runs fmt, clippy `-D warnings`, the suite and the
> generated-artefact checks, and the two Windows jobs build and run the release binary at `x64` and
> `arm64`. Step 8 (the card's context menu) shipped in Phase 1 and grew the same items the tray has;
> step 11 (installer, `v0.1.0` tag) is deliberately not here yet, because the numbers it would publish
> are the ones Phase 2 changed.
>
> Of §1, items 1–6 and 8–10 are now true as written. Two changed shape: item 6's click-through is three
> named modes rather than one toggle, with a refusal built into the third; item 7's menu gained the
> submenus, the mode names, `AM / PM`, the display choice and `Settings…`, and lost nothing. The
> settings *window* — listed under "Out of MVP" when this plan was written — is built, because the
> tray had stopped being able to hold the settings honestly: an anchor position is not a menu item.
> The rest of §1 is unchanged, and §2's on-hardware definition of done is still owed: it needs a
> desktop, which CI does not have. See [`reports/phase-2.md`](reports/phase-2.md).

One sentence: **one hanging digital clock, real rope physics, drag and swing, an Always-on-Top
toggle** — a thing that hangs off the top of a Windows desktop and tells the time.

## 1. In / out

**In (nothing else ships if we run short)**

1. A frameless, transparent, non-activating, topmost window hanging from the top edge of a chosen monitor.
2. Verlet cord with fixed 240 Hz timestep, stretch ceiling, sleep-when-settled.
3. A clock card showing `10:42` + `PM` + optional `:07`, tabular figures, one face.
4. Drag to swing; release carries momentum; settles in ~2 s.
5. `Alt`+drag (or drag the hang ring) to re-anchor along the top edge, and below it; position persisted
   as a ratio and a drop, so it survives a scale change and a monitor switch.
6. Per-pixel click-through everywhere except the card and the ring; hover shows an open hand, the ring
   a move cursor; three named mouse modes, and the last one cannot be entered without the tray present.
7. Tray icon + menu as the primary control surface: Show/Hide, **Always on Top**, Mouse ▸, Seconds,
   12/24 h, AM/PM, How it swings ▸, Monitor ▸, the two size steps and two cord steps, **Reset
   position**, Settings…, Start with Windows, About, Exit.
8. `settings.toml` persistence with tolerant decode + atomic write.
9. 60 Hz while swinging, 1 Hz while settled, 0 Hz while hidden.
10. Per-Monitor V2 DPI awareness; re-anchor on display/DPI/taskbar changes; resync on resume.
11. `cargo build/test/clippy` green on Linux + Windows `x64` + Windows `arm64` CI *(done, three jobs)*;
    Inno Setup per-user installer *(not built: it publishes numbers this phase changed)*;
    README + CHANGELOG + LICENSE + PRIVACY + docs listed in `docs/index.md`.
12. `--bench`/`--diag` flags that print the §8 numbers of `architecture.md`, so the claims are checkable.

**Out of MVP (designed for, not built)** — modes (Timer/Stopwatch), faces beyond one, themes,
custom ropes, analog, world clocks, dual-cord mount, global hotkeys, autostart-on-first-run,
import of anything, sounds, blur/acrylic, screen-colour sampling, updater, MSIX,
Store, macOS code, i18n beyond `en` (the layout must not hard-code "PM" width — that's the only
i18n duty in MVP), Linux build, plugins.

## 2. Definition of done

* The §8 budgets in `architecture.md` are met on the reference rig, with the numbers pasted into
  the release notes (measured-in-public is the Hangly habit we most want).
* On a 125 %-scaled second monitor, right of a 100 % primary: the clock is crisp, in place after
  unplug/replug, and a full-screen game in exclusive mode is unaffected when we Alt+Tab past it.
* Left-clicking the wallpaper *through* the overlay's transparent area selects an icon. One-frame
  test: no click is ever eaten outside the card.
* Nothing steals focus: with Notepad open and a caret blinking, dragging the clock leaves the caret
  where it was (verified in the manual matrix).
* Kill the process in Task Manager → no orphan files; uninstall leaves only `%APPDATA%\Hanglock`
  (and says so), removing it resets the app fully.
* A first-time contributor can build on a fresh Windows VM with: `winget install Rustlang.Rustup` +
  `cargo run` (documented and actually tried).
* The physics invariants and settings-migration tests fail loudly if weakened (no snapshot-only tests).

## 3. Build order (each step runnable, ~1 PR each)

| # | Step | Deliverable | Exit test |
|---|---|---|---|
| 0 | Repo skeleton | workspace, 5 crates, CI, lints, `deny.toml`, templates | `cargo test` green on Linux + Windows; hello-world binary that does nothing |
| 1 | **Spike: the blank card** | `hanglock-win` window: `WS_POPUP`+layered+topmost+noactivate+toolwindow, paints a flat rounded rect via `UpdateLayeredWindow`, 1 Hz present of a static string, PMv2 manifest, `WM_NCHITTEST` returns `HTTRANSPARENT` outside the rect | **Gate A (see §4).** Idle CPU ≤ 0.05 %, WS ≤ 12 MB, clicks pass through, no focus steal |
| 2 | Frame loop + clock | waitable timer, wake/sleep state machine, `ClockModel` + `format.rs` (frozen-time tests), digits only dirty | 1 h idle at 1 Hz; digits change exactly on the second; CPU stays under Gate A numbers |
| 3 | `core::rope` (no window) | `RopeSim`, `config`, `constraints`, `drag`; invariants test suite | All physics assertions pass headless; `2×120 == 1×60` within 1e-9; stretch ceiling holds under the torture flick |
| 4 | Wire physics → paint | `Scene`, `cord.rs` (stroke stack, no filters), cord follows the solver; window sized to the swept area | 60 Hz while dragging with paint ≤ 0.4 ms p95; ≤ 0.15 MB/frame copies |
| 5 | Card + attitude | `card.rs`: pivot, torque, `θ_max` postures; `measure.rs` lays out the card | Swing reads as a hanging plaque; `Locked` posture keeps digits level; no gap at the joint |
| 6 | Interaction | hit shape, press/drag/release with capture, velocity from raw `WM_POINTER`/`WM_MOUSEMOVE`, hover cursor, wheel = length, `Alt`+drag = re-anchor | Momentum on release matches pointer velocity within 5 %; drag past reach stays taut; re-anchor persists across restart |
| 7 | Tray + settings | `Shell_NotifyIconW` on a message-only HWND, menu commands, `settings.toml` full schema + tolerant decode | Toggling **Always on Top** from the menu updates the window style live *and* survives a restart; corrupt file → `.corrupt-<ts>` + defaults + one menu warning |
| 8 | Context menu on the card | Right-click → the same commands, at the pointer, without activating us | Menu closes on click-away; menu does not fight the drag state |
| 9 | Displays + power | `WM_DPICHANGED`/`WM_DISPLAYCHANGE`/`WM_SETTINGCHANGE`/taskbar (`ABM_GETTASKBARPOS`), `PBT_APMRESUMEAUTOMATIC` | Unplug/replug and lid sleep/resume leave it correct with no reset of motion |
| 10 | Face polish | glyph atlas, subpixel positions, 3 sizes, rim + shadow (precomputed), reduced-motion posture | Golden scene tests; digits legible at 100 % on a white *and* a photo wallpaper |
| 11 | Ship | `--release` LTO+strip+`codegen-units=1`, Inno Setup, `--bench` printed, README/FAQ/CHANGELOG, v0.1.0 tag + draft release with the SHA-256 | Gate B: the §2 list, plus installer ≤ 4 MB and a clean uninstall |

## 4. Gates, and what happens if one fails

**Gate A (after step 1) — the go/no-go for ADR-0001.** Any of:
no clean per-pixel transparency at 125 % scale; alpha-0 hit-testing unreliable (edge pixels swallow
clicks we don't want, or the card stops taking them); idle CPU > 0.3 % or working set > 25 MB in the
blank-card state; the tray menu unable to appear without activating us; `UpdateLayeredWindow`
visibly tearing/juddering at 60 Hz on an iGPU.

→ **Fallback ladder:** (i) keep Rust, swap the presenter to DirectComposition + a DXGI
premultiplied swapchain with `WS_EX_NOREDIRECTIONBITMAP` (GPU path, same core); (ii) keep Rust, use
a 2-window split (opaque-alpha hit window + paint window) if hit testing is the only problem;
(iii) invoke Plan B — C#/WPF, which uses the same layered-window mechanism with a mature toolkit on
top; core physics, settings schema, interaction grammar and the test suite carry over by
translation, `hanglock-win` and `hanglock-render` are replaced by WPF drawing code. Estimated cost of
(iii): one week of porting, and ADR-0002 records why.

**Gate B (step 11)** is the release bar (§2). If budgets miss but nothing is *broken*, we ship
0.1.0 with the measured numbers and an honest "known limitations" list — Hangly's move, and the
right one for an early public utility.

## 5. Risks, ranked

| Risk | Impact | Mitigation |
|---|---|---|
| Grayscale-AA digits look soft on 100 % displays | Core legibility | Pick the face early against a real render (`--dump-scene`), heavier weights, ≥28 px; test at 100/125/150 % in Gate A. Not fixable by code — every transparent-surface renderer has this limit (see ADR-0001 §4) |
| ULW present cost on low-end iGPU/VM | Smoothness | Dirty-rect presents, `fps_cap` setting, 60 Hz default; Gate-A fallback (i) is measured on the same rig |
| `WS_EX_NOACTIVATE` + context menu conflict | Menu steals focus / closes instantly | Standard recipe: `TPM_RETURNCMD`-free menu with `SetForegroundWindow` on our hwnd then restore, or a custom layered popup window; decide in step 8, note it in `docs/windows-overlay-notes.md` |
| Fullscreen-exclusive games cover the clock | "Always on top" promise | Documented limitation + `topmost` re-assert; do not chase with `SetWindowPos` spam |
| Antivirus/SmartScreen flags an unsigned Rust exe | Adoption | Sign via SignPath Foundation after 0.1; reproducible build docs; publish hashes |
| One maintainer + hand-written Win32 | Bus factor | `docs/windows-overlay-notes.md` is the onboarding doc; keep `hanglock-win` ≤ ~900 lines and its `unsafe` in `layered.rs`+`window.rs`; clippy `unsafe_op_in_unsafe_fn` deny |
| `cosmic-text`/`tiny-skia` churn or MSRV drift | Build breakage | Pin exactly + `cargo vendor`-able CI, `deny.toml`, and a vendored-feature fallback (`fontdue`) behind `glyphs.rs` |
| Physics feels "wrong" though every number passes | The product is the feel | A scripted `--record`/`--replay` harness: capture a real drag as JSON, replay it headless, diff node paths against a committed baseline. Feel becomes a test, not a vibe |

## 6. What we explicitly refuse to optimise for

Nothing in MVP touches another process, reads outside `%APPDATA%\Hanglock`, phones home, or needs
admin. No network code at all — and, like the reference project, we will state in `docs/PRIVACY.md`
that the binary links no networking API, so the claim is checkable by `dumpbin /imports`.
