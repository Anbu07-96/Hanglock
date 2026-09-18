# Hanglock roadmap

Phases are sequenced by what unblocks the next one, not by feature count. Each has an exit gate;
a phase is not "done" until its measurements are in the release notes.

---

## Phase 0 — Inspect and decide *(done)*

Repo skeleton, `docs/` (research, architecture, MVP plan, ADR-0001, IP boundaries), README rewrite,
licence + privacy stance chosen, CI skeleton with `fmt`/`clippy`/`test` on Linux + Windows.

**Exit:** ADR-0001 signed off (Rust/Win32 vs Plan B), skeleton builds green on both OSes.

## Phase 1 — The spike, plus the rest of the MVP's parts *(built; unmeasured)*

"Blank card": the window contract, `UpdateLayeredWindow`, PMv2 DPI, `WM_NCHITTEST`, 1 Hz present,
tray icon with a Quit item. **Gate A** in `mvp.md` §4 decides the stack for real, on hardware.

**Exit:** the six numbers of Gate A measured, and `docs/windows-overlay-notes.md` written with what
broke. Kill-or-commmit: no physics, no typography, no settings beyond a hard-coded struct.

## Phase 2 — MVP: it hangs, it swings, it tells the time *(steps 2–11 of `mvp.md` §3)*

Rope solver → paint → card attitude → drag/throw/re-anchor → tray + settings → displays/power →
face polish → Inno installer, `v0.1.0` with public measurements.

**Built:** everything up to the installer, in the order that sentence gives — with the emphasis this
phase actually needed on the last three arrows. The re-anchor is a persisted pair of numbers rather than
a pixel; the tray is the control surface, with the mouse modes, the display choice and `Reset position`
in it; the settings window draws rows the model derives, so it cannot disagree with the menu; and the
file is only written once something changes. **Owed:** the installer, the tag, and every number in
`mvp.md` §2 that needs a desktop. See [`reports/phase-2.md`](reports/phase-2.md).

**Exit:** `mvp.md` §2 satisfied. **This is the first release anyone should install.**

## Phase 3 — It's a tool, not a clock

Timer + Stopwatch as first-class modes: wheel/click or tray to switch; countdown preset chips on the
card; start/pause/reset by clicking the card (grammar extended, not replaced); completion state =
the card's own visual (a rim pulse — no dialogs, no focus steal) + an optional notification only if
the user enabled them; per-mode persisted state so a running timer survives a restart (store the
target `Instant` + drift-corrected wall clock, never "elapsed + tick counting").

Also here: reduced-motion posture, `Idle` timeout setting, first-run onboarding balloon,
DPI/per-monitor regression tests into CI, and the `--record`/`--replay` feel harness.

**Exit:** a kitchen timer you can trust from the top of the screen without opening anything, and
CPU/memory budgets unchanged in the `Idle` state (mode switching must not buy a frame clock).

## Phase 4 — Identity and choice

* **Faces:** `Hanglock Mono`, `Hanglock Condensed` (tabular numerals, distinct optical sizes),
  `Terminal` (monospaced, block caret, optional scanline-free "phosphor" tint), `Plate` (engraved
  plaque, no glow), `Minimal` (time only, hairline cord). Each is code + a golden test + a budget line.
* **Rope styles:** Braided, Wire, Leather, Ribbon, Chain, Invisible — a stroke recipe plus a
  `mass-per-length` term so a chain actually swings differently from ribbon. *That coupling is the
  feature:* style changes feel as well as look.
* **Themes:** `Slate`, `Ink`, `Ember`, plus explicit light/dark/auto and a single accent colour.
* **Card options:** size (continuous, not 5 steps), opacity, corner radius, plate vs. no plate.
* **Dual-cord mount** (`Sign` / `Plate` / `Mounted` postures) replacing the `θ_max` clamp with
  geometric stiffness — see [`ideas/dual-cord-mount.md`](ideas/dual-cord-mount.md). This is the phase
  where "Posture" stops being a damping fudge and becomes bracket width.
* **Modes UI:** a Settings window (small, native) for everything the tray menu can't hold; the menu
  stays the fast path for the 5 settings people actually change.
* Sound: opt-in, material-specific tick at low volume, engine started only while audible
  (the reference project's "stop the audio engine when quiet" lesson — a resident audio thread would
  break the idle budget).

**Exit:** a `Face` trait with 4 shipped faces and 5 rope styles, all inside budget; every style
selectable without editing files; a documented "how to add a face" page with the test you must add.

## Phase 5 — Distribution and trust

Authenticode via SignPath Foundation (or Azure Artifact Signing) wired into `release.yml`; winget +
Scoop manifests; MSI via WiX for managed IT; crash-free telemetry **not** added — instead a local
`diag.log` + a "copy diagnostics" tray item; updater decision (own updater vs winget vs none) written
as an ADR; reproducible-build notes; `aarch64-pc-windows-msvc` build published.

**Exit:** `winget install hanglock` works; a Windows 11 clean VM installs without a SmartScreen
detour; 0.2.0 assets carry signatures and hashes.

## Phase 6 — More utilities on the same rope

The product thesis pays off here: the hanging object is a **mount point for glanceable tools**.
Candidates, ranked by "does it need a window?": World clocks (a second card on a second anchor,
reusing `OverlayId`), Pomodoro (state on the timer), Next calendar event (declined in MVP for
privacy; needs a decision on CalDAV vs. Windows Calendar intake), Sticky note (likely refused —
it is a text box, i.e. focus, i.e. a different app), Focus mode (a card that hides the others),
Battery/Network glance for laptops and workstations.

**Exit criterion per addition:** zero idle CPU cost, no new resident thread, no new permission.

## Phase 7 — macOS (only if the core proves out)

`crates/hanglock-macos` implementing `hanglock-platform`: `NSPanel` + `.nonactivatingPanel`, window
level, `NSHostingView`-free (Core Graphics into a bitmap, or Metal layer), `NSStatusItem` menu,
`SMAppService` login item, notarisation. The reward for keeping `core`/`render` pure is that this is
one backend, not a rewrite — and the physics/formatting/settings tests run unchanged.

## Deliberately never

Wallpaper engine, lock-screen replacement, "widget board", transparency colour-key hacks,
`SetWindowsHookEx` global hooks, screen scraping for adaptive colour, a bundledCEF, telemetry, a
paid tier, and charm ornaments (see `research/ip-boundaries.md` §4).

---

## Sequencing notes

* Nothing in Phase 3–4 is allowed to make the MVP slower. If a feature needs a frame clock while
  idle, it is in the wrong phase.
* Faces/rope styles are *asset-free by design* (code + geometry, no image pipeline) so the whole
  repo stays trivially forkable and the artwork question never arises.
* The `Face` trait is not cut in MVP; `ModeId` and `Scene`'s payload slot are. Designing an
  extension API for one implementation is how projects end up with a legacy-shaped API on day 40.
* Measurement is a phase deliverable every time, not a background activity: each release note opens
  with a table (idle CPU, animating CPU, working set, present cost, cold start, installer size).
* The first such table is owed: `docs/gate-a.md` has the blanks and the script that fills them.
* Total MVP-to-0.2 scope is roughly one part window plumbing, three parts model and rendering, and
  one part packaging. The reference project spent about 45 % of its code (5,019 of 10,683 Swift lines)
  on the charm catalogue, the artwork pipeline, the Library and the Studio — machinery we do not have.
  That freed budget is what pays for the extra modes and the polish.
