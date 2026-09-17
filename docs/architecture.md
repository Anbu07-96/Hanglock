# Hanglock architecture (proposal)

Status: proposal, to be locked at the end of Phase 0. Stack decision: [ADR-0001](decisions/0001-technology-stack.md).
Prior-art findings this is built on: [hangly-inspection.md](research/hangly-inspection.md).

---

## 1. Principles

1. **The physics and the model know nothing about Windows.** Two crates compile and run their full
   test suite on Linux CI, on a phone, in a headless container. If they can't, the boundary is wrong.
2. **Value types hold state; only the composition root holds behaviour.** Settings, clock state,
   rope state are plain `struct`s. `#[derive(Debug, Clone, Copy, PartialEq)]` on everything in `core`.
3. **One funnel per side effect.** One writer for settings. One owner of the HWND. One thing that can
   call `UpdateLayeredWindow`. When there is exactly one path, it is testable and it cannot race.
4. **Nothing runs unless something changed.** No frame clock while settled. No present while hidden.
   Every wake has a named cause: `PointerDown`, `Hover`, `SecondTicked`, `SettingsChanged`,
   `DisplayChanged`, `ResumeFromSleep`.
5. **Physics is computed; everything else is cached.** Per-frame work = one solver step + one blit.
   Gradients, cord geometry, glyph rasterisation and shadows are built on settings change, not per frame.
6. **Legibility outranks realism.** The clock is a readout that happens to hang. If a physical choice
   makes the digits hard to read, the physical choice loses (and we ship the toggle that proves we know).

## 2. Repository layout

```
Hanglock/
├── Cargo.toml                  # workspace
├── rust-toolchain.toml         # pinned, so CI and strangers agree
├── crates/
│   ├── hanglock-core/          # `#![forbid(unsafe_code)]` — pure model, no dependencies at all
│   │   src/
│   │   │   ├── lib.rs
│   │   │   ├── rope/
│   │   │   │   ├── mod.rs          # RopeSim: step/reset/wake/sleep state machine
│   │   │   │   ├── config.rs       # RopeParams: every tunable, no magic numbers elsewhere
│   │   │   │   ├── node.rs         # Node {pos, prev, inv_mass}; Verlet maths
│   │   │   │   ├── constraints.rs  # relaxation + one-sided stretch projection
│   │   │   │   ├── drag.rs         # pick, reach-clamp, target, release velocity
│   │   │   │   └── card.rs         # rigid-card attitude: angle, angular damping, legibility clamp
│   │   │   ├── clock/
│   │   │   │   ├── mod.rs          # ClockModel: now() → FaceFields, DST/sleep-safe
│   │   │   │   ├── format.rs       # 12/24 h, seconds, AM/PM rules, tabular digit strings
│   │   │   │   └── timer.rs        # TimerModel/StopwatchModel (state only, Phase 3)
│   │   │   ├── scene.rs            # Scene: what to draw (immutable, render-facing)
│   │   │   ├── placement.rs        # pure rect math in a y-down virtual-desktop space
│   │   │   └── ids.rs              # FaceId, RopeStyleId, MonitorId, ModeId
│   ├── hanglock-render/        # `#![forbid(unsafe_code)]` — Scene → premultiplied BGRA + dirty rect
│   │   src/{lib.rs, painter.rs, cord.rs, card.rs, glyphs.rs, atlas.rs, theme.rs, measure.rs}
│   ├── hanglock-platform/      # traits only, no implementation
│   │   └── src/{lib.rs, overlay.rs, presenter.rs, clock.rs, input.rs, tray.rs, display.rs, power.rs}
│   └── hanglock-win/           # the `unsafe`, the Windows imports
│       ├── src/{lib.rs, window.rs, wndproc.rs, layered.rs, hit.rs, tray.rs, dpi.rs,
│       │        displays.rs, timer.rs, hotkeys.rs, autostart.rs, appmanifest.rs}
│       └── assets/manifest.manifest   # PMv2 DPI awareness + LongPathAware
├── apps/
│   └── hanglock/               # composition root: WndProc → App → core → render → presenter
│       └── src/{main.rs, app.rs, config.rs, state.rs}
├── assets/
│   ├── fonts/                  # OFL faces + LICENSE.txt per family
│   ├── branding/               # our icon sources (SVG), generated .ico
│   └── SOURCES.md              # provenance line per file — enforced in CI
├── docs/
│   ├── architecture.md  mvp.md  roadmap.md
│   ├── decisions/0001-technology-stack.md 0002-… (one file per real decision)
│   ├── research/hangly-inspection.md ip-boundaries.md
│   ├── physics.md              # the solver, with its measurements and its graphs
│   ├── windows-overlay-notes.md# ULW/HTTRANSPARENT/DPI/tray findings (the 600 lines' worth of why)
│   └── PRIVACY.md
├── scripts/                    # build.ps1, bench-idle.ps1, installer.iss, gen-icons.ps1
├── tests/                      # cross-crate: physics invariants, golden scenes, settings migration
├── .github/workflows/          # ci.yml, release.yml
└── CHANGELOG.md CONTRIBUTING.md SECURITY.md LICENSE THIRD-PARTY-NOTICES.md README.md
```

The tree above is the plan from Phase 0 and it has drifted, as plans do; the shipped `src/` trees are
the truth. The differences worth knowing before you look for a file: `render`'s `glyphs.rs`/`atlas.rs`
became `face_data.rs`/`text.rs` (the digits are drawn from generated geometry, so there is no atlas),
`win`'s `hotkeys.rs` and `appmanifest.rs` were never needed (global hotkeys are Phase 4, and DPI
awareness is set by call, not by manifest), `platform` is one `lib.rs` plus the settings form in
`panel.rs`, and the app crate is `main.rs` / `model.rs` / `store.rs` / `app.rs`. Phase 2 added
`core/src/anchor.rs` (the hang point, and what dragging it means) and `win/src/panel.rs` (the settings
window as a renderer of rows).

Three rules the layout enforces:

* `hanglock-win` is the only crate that may `unsafe` or link a Windows API. `deny`-level clippy at
  the workspace root, `#![forbid(unsafe_code)]` in the other three. The one exception is deliberate and
  measured: the app crate's `store.rs` calls `ReplaceFileW`, because atomically replacing a file another
  process may have open is a thing only the OS can do, and the `unsafe` there is four lines inside one
  `#[cfg(windows)]` function rather than a seam with a trait under it.
* `hanglock-render` depends on `hanglock-core` types only. It cannot see a window, a device or a clock.
* A `macos` backend later is `crates/hanglock-macos` implementing the same traits, reusing core +
  render unchanged. Nothing in core/render may grow a `#[cfg(windows)]`.

## 3. Data flow

```
                    ┌────────────── hanglock-win ──────────────┐
 WM_* ──▶ WndProc ──┤ input.rs  → Pointer{pressed,moved,released}│
 SetTimer ─▶ tick ──┤ timer.rs  → Tick{Awake|Second}            │
 WM_DPICHANGED ─────┤ dpi/displays.rs → LayoutChanged          │
 Shell_NotifyIcon ──┤ tray.rs → Command{ToggleAot,Mode,..}      │
                    └───────┬──────────────────────┬───────────┘
                            ▼                      │
                    ┌───────┴────────┐              │
                    │  App (root)    │              │
                    │ settings.write │              │
                    └───┬────────┬───┘              │
                        ▼        ▼                  │
             ┌──────────┐  ┌────────────┐          │
             │ RopeSim  │  │ ClockModel │          │
             │ + Card   │  └─────┬──────┘          │
             └────┬─────┘        │                  │
                  ▼              ▼                  │
             ┌──────────────────────┐              │
             │ Scene (immutable)    │──────────────┤
             └──────────┬───────────┘              │
                        ▼                          │
              ┌───────────────────┐                │
              │ hanglock-render   │                │
              │ paint → BGRA +    │                │
              │ dirty rect        │                │
              └─────────┬─────────┘                │
                        ▼                          ▼
              ┌───────────────────────────────────────┐
              │ Presenter (layered.rs) → UpdateLayeredWindow
              └───────────────────────────────────────┘
```

One direction, once per wake. A settings write never reaches into the window; it mutates
`Settings`, which raises a `Changed` event that the App handles by re-fitting geometry and
requesting a present. `Scene` is the single immutable handoff (Hangly's `RopeSnapshot` idea,
generalised to a full frame description).

## 4. The overlay window contract

Exactly one type owns the HWND. Its requirements and their Win32 realisation:

| Requirement | Mechanism |
|---|---|
| No frame, no title, no taskbar entry | `WS_POPUP` + `WS_EX_TOOLWINDOW`, no `WS_EX_APPWINDOW` |
| Per-pixel transparency | `WS_EX_LAYERED` + `UpdateLayeredWindow(ULW_ALPHA)` with a premultiplied-BGRA DIB section |
| Above normal windows | `WS_EX_TOPMOST` + `SetWindowPos(HWND_TOPMOST)`; re-asserted from `WM_WINDOWPOSCHANGING` |
| Never steals focus | `WS_EX_NOACTIVATE`; never call `SetForegroundWindow`/`SetFocus`; the settings window (Phase 4+) is the only thing that may activate |
| Clicks pass through everything but the object | Layered-window per-pixel hit testing (alpha 0 falls through) + `WM_NCHITTEST` returning `HTTRANSPARENT`/`HTCLIENT` against our own `hit_shape` |
| Visible on all virtual-desktop/Task View states | Tool window at topmost is not associated with a virtual desktop; verify in the matrix (Win11 Virtual Desktops is a real test case, unlike macOS Spaces) |
| Survives resolution/scale/monitor change | `WM_DPICHANGED`, `WM_DISPLAYCHANGE`, `WM_SETTINGCHANGE` → recompute `placement` + re-fit rope (preserve motion, never reset) |
| Costs nothing when hidden | `DestroyWindow` (not `ShowWindow(SW_HIDE)`) while disabled; re-created on demand |
| Single instance | Named mutex `Local\Hanglock.SingleInstance` + `WM_COPYDATA` "show me" on second launch |
| Not captured as a window by Alt-Tab | `WS_EX_TOOLWINDOW` covers it; documented as "does not appear in Alt+Tab" |

## 5. Physics design (Hanglock's own, informed by Hangly)

### 5.1 Payload differs

A charm is a lump. A clock is a **wide, flat, read-only object**: its mass is spread across a card,
it hangs from a bracket near its top edge, and rotating 25° makes it unreadable. So:

* `RopeSim` is a Verlet chain (21 nodes default) whose terminal node is the card's **attach point**,
  not its centre. `inv_mass[0] = 0` pins the anchor; `inv_mass[terminal] = 1/mass_card`.
* `CardRig` is a tiny rigid body hanging off that node: `angle` and `ang_vel` integrated from the
  torque of gravity about the attach point, with a soft restoring term toward vertical (the bracket
  is a *pivot*, not a free swivel), and `angle_limit` as a one-sided clamp:
  `|θ| ≤ θ_max` (default 14°, "Posture" setting: `Natural` = 30°, `Sign` = 6°, `Locked` = 0°).
  Legibility is the reason the clamp exists; realism is the reason it isn't 0.
* The cord is drawn to the bracket; the card paints over the last few pixels, so the joint never
  shows a gap. Same "cut at the silhouette" idea, but here the silhouette is a rect we own, so the
  cut is exact arithmetic instead of a measured row scan.

### 5.2 Single strand now, `V` later

Everything in §5.1 is the MVP: one cord, one pivot, one `θ_max` clamp. The clamp is an honest
stopgap — the real answer for a wide plate is two strands and geometric stiffness, designed in
[`ideas/dual-cord-mount.md`](ideas/dual-cord-mount.md). The solver is written so that is additive:
`RopeSim` owns an arbitrary constraint list and a set of pinned/mass-weighted nodes, so a
single-strand charm, a single-strand card, and a two-strand rigid triangle are three configurations
of one solver rather than three solvers.

### 5.3 Solver, reused as ideas

Fixed timestep `1/240 s`, accumulator clamped to `max_frame_duration = 0.1 s`. Step order:
`pin anchor → integrate → drive held node → relax (cap 4× nodes, exit when max correction < 0.05 px)
→ project over-stretch (shared correction, ≤ 1.02×) → refresh card attitude`.
Reach-clamp the drag target to `0.98 × rope length` around the anchor. Damping `0.997` (higher than
Hangly's `0.999` on purpose: a tool should settle in ~2 s, not swing for 40).

### 5.4 Interaction grammar (MVP)

| Input | Result |
|---|---|
| Press on card → move → release | Drag the card; on release it carries the pointer velocity, swings, settles. Rope stays taut, never stretches |
| Press on card + `Alt`, anywhere on the plate | **Re-anchor**: the window follows the pointer, the anchor becomes `(anchor_ratio, anchor_drop)` — x as a fraction of the usable width, y as a drop below the hang line — and the card settles under it on release. Both numbers are quantised to the four decimals the file writes, so what is on screen, what is saved and what the next build reads are the same position |
| Press on the hang ring, no modifier | The same gesture, for a pointer that was already aimed at it |
| Double-click card | *Not built in v0.1*: the input has no double-click, and `WM_LBUTTONDBLCLK` would have to be claimed from the drag grammar that already works |
| Right-click card | Context menu: mode (Clock only in MVP), Always on Top, Click-through, Show/Hide, Size, Settings…, Quit |
| Wheel over card | Rope length −/+ (hang distance), 6 discrete steps; re-fits the window height |
| Wheel + `Ctrl` over card | *Not built in v0.1*: size is a tray and settings step, so no modifier state has to be read outside the re-anchor |
| Hover over card | `WM_SETCURSOR` → open hand; closed hand while dragging; `IDC_SIZEWE`-style near the bracket. No global polling, no cursor-API churn |
| `Ctrl+Alt+H` (Phase 4) | Show/hide. Global hotkeys are deliberately not MVP: they carry a different user expectation and, on macOS later, a different permission story |

Three mouse modes ship, because two of them were a trap:

| `click_through` | Menu says | What the window does |
|---|---|---|
| `solid` | Interactive (whole window) | The plate's rectangle answers every click, empty pixels included. For a card over a busy background where the anti-aliased edge kept stealing a click |
| `hover` | Transparent areas click through | The default: the plate and the hang ring take clicks, everything else in the window is `HTTRANSPARENT` |
| `always` | Fully click-through | `WS_EX_TRANSPARENT` as well, so the overlay is not a target at all — and the tray becomes the only way back, which is why the model refuses this mode while no tray icon is installed and says so in the tooltip |

The refusal is the interesting part: a setting that can make the app unreachable is a setting the app is
not allowed to persist without an exit. `Model::on_ready` re-checks it at startup, because a file copied
from a machine where the icon installed cleanly is not a promise that this one will.

### 5.5 Wake/sleep state machine

```
Hidden ──enable──▶ Armed(0 fps, no timer) ──pointer near/hover/second tick──▶ Awake(60 fps)
  ▲                    ▲                                   │ 60 still frames (~1 s)
  └──disable───────────┴──── Idle(1 fps, digits only) ◀────┘
```

* `Idle`: one present per second, dirty rect limited to the digit area. Physics not stepped.
* `Awake`: physics stepped 4× per rendered frame at 240 Hz; full-surface present at ≤ `fps_cap`.
* Hover, drag, settings change, `WM_DPICHANGED`, resume-from-suspend, and the second tick wake it.
  Nothing else does. `Hidden` destroys the window, so a disabled overlay is not "invisible but
  running" — it is gone.
* On `WM_POWERBROADCAST(PBT_APMRESUMEAUTOMATIC)`: drop the accumulator, resync the clock, and stay
  `Armed` (do not animate on wake; the user should not come back to a swinging clock).

## 6. Settings

One TOML document, `%APPDATA%\Hanglock\settings.toml`, written atomically
(temp + `ReplaceFileW`/rename-with-retry), only when the in-memory value actually differs.

The document as written, from `Settings::to_toml` with the defaults `Settings::default` holds — the
shipped keys, not a sketch of them:

```toml
# Hanglock settings. Ranges are enforced on load; unknown keys are ignored.
schema = 1

[overlay]
enabled = true
monitor_index = 0          # which display to hang from, by enumeration index
anchor_ratio = 0.5         # 0..1 across the usable width; survives resolution and scale changes
anchor_drop = 0.0          # logical px below the hang line; 0 is "from the edge"
hang = 150.0               # cord length in logical px, 70..260, six steps the tray and window share
scale = 1.0                # 0.75..1.75
opacity = 1.0              # 0.35..1.0
topmost = true
click_through = "hover"    # "solid" | "hover" | "always"
respect_taskbar = true
margin = 16.0              # the slack around the swept box

[face]
hour12 = true
seconds = false
meridiem = true
posture = "plate"          # "natural" | "plate" | "mounted" | "locked"

[general]
launch_at_login = false    # reconciled against the registry, including StartupApproved
fps_cap = 60
```

Two rules about writing it matter more than the keys. The file is not created by a first run: nothing is
written until a command changes something, so an app the user tried once and closed leaves no file
behind. And `anchor_ratio` / `anchor_drop` are stored quantised to these four decimals — `Anchor::clamped`
rounds to the writer's unit — because a position the writer cannot express is a position that moves when
the file is saved, and `from_toml(to_toml(s)) == s` is worth more than the eighth decimal of a pixel.
The `Cord` row is `hang`, and it is one of six rungs rather than a continuous value, so the tray's two
menu items and the window's two buttons can never disagree about what the next step is.

Rules (each one exists because someone will hit it):

* `Settings::from_toml` defaults every field it does not see; **missing keys must never reset a
  preference**, so adding a field is always safe. Unknown keys are ignored, so a newer build's file still
  loads on an older one. (The ADR-0001 draft said `#[serde(default = "…")]`; ADR-0002 made the reader and
  writer ours, and the rule is the same one, implemented by hand.)
* Numeric fields are clamped on the way in by the same `Limits` consts the UI uses — a corrupt or
  hand-edited file yields a sane overlay, not an off-screen one.
* A file that fails to parse is renamed `settings.toml.corrupt-<ts>`, defaults are used, and the
  tray menu says so once. We never silently discard, and we never refuse to start.
* `schema` bumped only on a breaking change, with an explicit migration function per bump.
* `launch_at_login` is **read back** from the registry at startup and the stored value corrected, so
  the toggle in our menu matches what Task Manager's Startup tab shows.

## 7. Visual identity (our own, deliberately)

* **Shape:** a thin plaque with a visible metal bracket and a single cord; the "hang" is one knot at
  top-centre. No charms, no beads, no tassels, no folk ornament.
* **Type:** one bundled OFL face with tabular figures (shortlist: `IBM Plex Mono`, `JetBrains Mono`,
  `Space Grotesk` + its own tabular set) rendered with grayscale AA at medium+ weight. Digits are the
  brand; everything else is chrome.
* **Palette:** three shipped looks in Phase 4 (`Slate`, `Ink`, `Ember`) — none of them "glassy
  widget"; the card must read against a wallpaper *and* a white document, so the card carries its
  own opaque-enough plate plus a subtle 1 px rim rather than relying on blur (blur is also exactly
  the per-frame filter cost we ruled out).
* **Colour resolution:** we sample the desktop? No. Out of scope: it needs either DWM thumbnail
  tricks or screen-capture permission, and it is the kind of feature that turns a quiet utility into
  a privacy question.
* Motion vocabulary: cord + card only. No fade-in on show (it hangs; it doesn't appear), no sound by
  default, no idle animation. The 0.38 rad initial swing on first placement is a shared idea, with
  our own value.

## 8. Performance budgets (the gates CI/release cannot skip)

Targets, to be *measured* on the reference rig defined in `scripts/bench-idle.ps1`
(1080p, 150 %, 60 Hz, iGPU, Win11, idle desktop, 60 s window):

| Budget | Target | How |
|---|---|---|
| Idle CPU, `Idle` state | ≤ 0.05 % of one core | `typeperf`/`Get-Counter` `\Process(hanglock*)\% Processor Time` over 60 s |
| Idle CPU, `Hidden` | 0.00 % | no thread awake |
| Animating CPU | ≤ 3 % of one core, 60 Hz | same, scripted drag |
| Present cost | ≤ 0.5 ms p95 per frame | internal rdtsc histogram, printed by `--bench` |
| Paint cost | ≤ 0.4 ms p95 | `hanglock-render` criterion bench, non-GUI |
| Working set, settled | ≤ 20 MB (hard fail > 30) | `Get-Process` `WorkingSet64` / `PrivateMemorySize64` |
| Commit | ≤ 35 MB | same |
| Frame-time jitter | p99 ≤ 1.5 × vsync interval | internal |
| Cold start → clock visible | ≤ 150 ms | scripted |
| Binary | ≤ 2 MB stripped, ≤ 4 MB installer | CI fails beyond |
| Handle/GDI leak | 0 growth over 1 h of scripted drag | `Get-Process` handles + user32/gdi object counts |

The `Present cost` line is the one Hangly's data points hardest at: they measured that a *transparent
window's* redraw dominated the cost of swinging, not the simulation. We budget it explicitly instead
of discovering it.

## 9. Testing strategy

| Layer | How | Where |
|---|---|---|
| `rope` | Invariants as numbers: stretch ceiling ≤ 1.02 under free swing and a 3000 px/s flick; `2×120 Hz frames == 1×60 Hz frame` within 1e-9; sleep after N still frames, wake on hover; drag-past-reach does not tear; long-run energy never grows; release velocity preserved | `cargo test`, any OS |
| `card` | Angle clamp respected; locked posture never rotates; torque settles to 0 in ≤ 3 s; cord endpoint and card top-centre coincide to <0.5 px | same |
| `clock` | Frozen `TimeSource` trait: format across 12/24 h, DST boundary, midnight rollover, year end, and a 90-minute system-sleep gap | same |
| `placement` | Pure rects: negative-origin secondary monitor, 100 vs 200 % scale, top-docked and auto-hidden taskbar, monitor smaller than the overlay | same |
| `settings` | Empty / partial / unknown-key / out-of-range / corrupt documents; round-trip lossless; atomic-write retry | same |
| `render` | Golden `.ppm` + `.svg` exports of hand-written scenes, compared in CI; glyph-atlas cache-hit assertions; dirty-rect correctness (a digit change must not dirty the cord) | same |
| `win` | **Not unit-tested.** A scripted manual matrix per release + an optional `windows-latest` "smoke" job that creates the window under a software renderer, asserts one present, asserts the ex-styles, then tears down. No window-server claims are made in `core` | CI + human |

Manual matrix per release: Win11 23H2/24H2 + Win10 22H2 · 100/125/150/200 % · one, two (side by
side), two (different scale) monitors · dark/light · top- and bottom-docked taskbar, auto-hide ·
fullscreen game (Alt+Enter) and RDP session · locked/unlocked workstation · sleep/resume · Night
light + HDR on · `msctf`/IME active for a CJK layout (proves we never steal focus).

## 10. Extension seams (cut now, paid for later)

| Later feature | Seam |
|---|---|
| Timer / Stopwatch | `ModeId` + `enum FacePayload` in `Scene`; `core::clock::timer` types already reserved; the renderer already draws "a card with fields", so a mode switch is one payload swap |
| Many faces | `trait Face { fn layout(&self, &FaceCtx) -> SceneFragment }` registered by `FaceId`; faces are code, not markup, so they stay in the test suite and the budget |
| Themes / rope styles | `theme::Palette` + `rope::Style` (width, twist, material) already consumed by the painter; a rope material is a stroke recipe |
| Custom hang points (top-left corner, off-centre) | `anchor_offset` is already in `RopeSim` (the pivot is a parameter of `CardRig`), and `core::anchor::Anchor` is the pair the file holds and the gesture produces |
| A second settings surface (another platform, or a `--set` flag) | `hanglock-platform::panel` owns the form: `form(&Settings, &[Monitor])` draws the rows and `change(&Settings, RowId, Step)` answers a click with a `Command`. A surface that builds its own row list or its own mapping is the drift this seam exists to prevent. The window is drawn from `form`; the tray menu is its own list of items, because a menu is not a form — but both end in a `Command`, and `Command` is the only vocabulary a surface has for asking, which is the half of the arrangement that actually has to hold |
| Multiple clocks (e.g. two world clocks) | `OverlayHost` is keyed by `OverlayId`; App holds a small map. Not in MVP, but the window owner must not be a global for this to stay cheap |
| macOS backend | `hanglock-macos` implementing the same traits with `NSWindow` + `NSView`/CoreGraphics; `hanglock-platform` exists so `core`/`render` never learn what an HWND is |
| Alarms / focus sessions | `core::clock` is already a state machine driven by an injected `TimeSource`; scheduling is a new service, not a new model |
| Community faces | Only once `Face` is a stable trait with a versioned `Scene` — deliberately *not* in MVP, so we don't design an API for one implementation |
| Dual-cord (V) mount for the plate | The solver's constraint list is a parameter (`RopeSim::constraints()`), so two cords + a rigid triangle is *additive*: same integrator, same timestep, same stretch ceiling, same sleep rule. See [`ideas/dual-cord-mount.md`](ideas/dual-cord-mount.md) |
