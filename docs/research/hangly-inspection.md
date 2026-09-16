# Hangly inspection report

Reference: <https://github.com/SharanCreatedThis/Hangly> @ `f1c5909` (2026-09-12, v1.0.0).
Inspected by shallow-cloning the repo and reading the whole source, docs and CI.
Purpose: understand the *concepts* so Hanglock can implement its own. Companion:
[ip-boundaries.md](ip-boundaries.md) (what we must not copy).

---

## 1. What Hangly actually is

| | |
|---|---|
| Product | A decorative charm hanging on a simulated rope from the macOS menu bar |
| Platform | macOS 14+, Apple Silicon only |
| Code | 10,683 lines Swift across 89 files + 3,096 lines of tests in 17 files |
| Deps | Zero third-party dependencies. Swift 6, strict concurrency |
| UI | SwiftUI for content, AppKit `NSPanel` for the window. No Dock icon (`LSUIElement`) |
| Measured cost | 0.6 % of one core settled / ~15 % swinging; 26 MB settled, 42 MB peak |
| Bundle | 5.2 MB shipped artifact; 1.14 MB stripped executable (their own report shows 3.4 MB on disk for `Production`); unsigned, so Gatekeeper warns on first launch |
| Ships | 16 charms, an import "Studio" (Vision segmentation), a charm library browser, synthesized sounds |

It is an ornament, not a tool — but it is an ornament built like a systems component, and that
is the part worth learning from. The docs (`Docs/Architecture.md`, `Physics.md`,
`DISTRIBUTION.md`) read like engineering notes rather than marketing, which is why the
inspection was cheap: every decision has a stated reason and a stated measurement.

Layout: `Hangly/{App,Physics,Models,Services,Views/{Overlay,Settings,Library,Studio},MenuBar,Utilities}`
plus `Tests/`, `Docs/`, `Scripts/`, `Assets/`, `.github/workflows/build.yml`.

---

## 2. Transparent window implementation

`OverlayPanel: NSPanel` — borderless, `.nonactivatingPanel`. Every requirement is one property:

| Requirement | Mechanism |
|---|---|
| Transparent | `isOpaque=false`, `backgroundColor=.clear`, `hasShadow=false` |
| Above everything | `level = .statusBar` (25) — above all apps, below system menus/alerts |
| Click-through | `ignoresMouseEvents`, flipped per frame (see §4) |
| Never steals focus | `.nonactivatingPanel`, `canBecomeKey/Main = false`, `orderFrontRegardless()` |
| All Spaces / full screen | `.canJoinAllSpaces` + `.stationary` + `.fullScreenAuxiliary` + `.ignoresCycle` |
| No slide animation on display change | `animationBehavior = .none` |
| Free life | Panel created lazily, **torn down entirely** when the overlay is switched off |

The SwiftUI tree is hosted by an `NSHostingView` with an explicitly clear layer background
("belt and braces" — an opaque hosting backing would defeat the panel's transparency).

**Why AppKit and not SwiftUI:** a `Window` scene cannot express
borderless + non-activating + click-through + always-on-top. The window belongs to AppKit;
SwiftUI only owns the pixels. **Read for Hanglock: the window layer is a solved, small,
nasty problem. Whatever we pick, it must be a thin wrapper we own, not a framework feature.**

## 3. Hanging/rope physics

`RopeSimulation` — 21 nodes / 20 links, anchored at the top, terminal node weighted by the
charm's mass. Step order is explicit and deliberate:

```
pin anchor → integrate → drive held node → relax (Gauss–Seidel) → clamp stretch → refresh cord → advance beads
```

Shipped constants (`RopeConfiguration.default`, in points and seconds):

| Constant | Value | Why (their stated reason) |
|---|---|---|
| `segmentCount` | 20 | Enough nodes to read as a continuous cord |
| `segmentLength` | 11 (`fitted(to:)` = 69 % of canvas height / 20) | Scales with the overlay |
| `gravity` | 2000 pt/s² | Canvas is y-down, so gravity is +y |
| `damping` | 0.999 | Air drag; below 1 the rope settles instead of swinging forever |
| `constraintIterations` | 256 (cap, exits early) | Corrections travel ≈1 link/pass, so the cap must exceed the segment count or the far end stays unaware and links stretch |
| `convergenceTolerance` | 0.05 pt | Settled rope exits after 1–2 passes; the big cap is free |
| `maxStretchRatio` | 1.02 | Hard one-sided ceiling; measured worst case 1.0009 free swing, 1.0115 hard flick |
| `fixedTimeStep` | 1/240 s | Behaviour identical at 60/120/ProMotion rates; asserted in a test (2×120 Hz frames == 1×60 Hz frame, 1e-9) |
| `maxFrameDuration` | 0.1 s | Accumulator clamp: a stall or wake-from-sleep cannot fire a catch-up burst |
| `maximumSpeed` | 6000 pt/s | Divergence rail; unreachable by hand |
| `maximumReachRatio` | 0.98 | Drag target clamped onto the reachable circle, leaving slack so the rope never reads rigid |
| `restSpeed` / `framesBeforeSleep` | 4 pt/s / 60 | Sleep threshold |
| `initialAngle` | 0.38 rad | On first appearance it swings into place — it looks *placed*, not *spawned* |

Design points worth stealing:

* **Verlet, not springs.** Velocity is implied by `position - previousPosition`. So (a) momentum
  after a drag release is free — stop writing the position and the gap *is* the velocity;
  (b) constraints are positional (move nodes, no stiffness term to tune into instability);
  (c) it stays stable at the constraint counts a rope needs.
* **Inverse mass as the only pinning mechanism.** Anchor is `inverseMass = 0`; the solver has no
  special case for it. A dragged node is also temporarily given inverse mass 0.
* **Stretch is a three-layer guarantee:** relax toward rest length → project any remaining
  over-long link back to the ceiling → clamp the *input* (drag target) so the solver is never
  asked for the impossible. Their note on the second layer is a good bug lesson: snapping the
  offending node onto the limit *oscillated*; sharing the correction between both ends converges.
* **Beads are a separate read-only pass** on top of the curve — "reads the rope, writes only
  beads", so decoration can never destabilise the solver.
* The cord is drawn as a quadratic spline through node midpoints, with a flattened arc-length
  lookup (`RopeCurve`, 4 samples/segment, reused buffer each step, binary search) so beads sit
  *on* the drawn line and the cord is cut exactly where the charm covers it — measured from the
  silhouette, not hard-coded.

## 4. Drag, swing, momentum, settling

* Hit test is a **disc** around the charm (`radius + 10 pt` padding), used twice: as the SwiftUI
  `contentShape` for the gesture, and as the window-level click-through gate.
* The gesture is `DragGesture(minimumDistance: 0)` — a grab registers on press, not after 4 px of
  travel. Velocity comes from the gesture value and is written into the held node each step.
* **Click-through is decided once per frame for the whole window** (`ignoresMouseEvents`), because
  AppKit cannot pass through part of a window. Cursor position is **polled** with
  `NSEvent.mouseLocation` rather than via an event monitor: no event tap, so no Accessibility
  permission. The write only happens when the value actually flips — poking the window server on
  every tick is enough to keep a settled overlay measurably busy.
* Cursor is open-hand/closed-hand, pushed/popped strictly in pairs, and only when it changes.
* Releasing above 500 pt/s plays a sound scaled by speed — the only tactile feedback in the app.
* **Settling = sleep.** After 60 still frames, `step` no-ops, no snapshot is published, so the
  observation layer never fires, so nothing invalidates and nothing repaints. The frame clock is *not*
  stopped, only dropped 120 → 30 Hz, because the same tick polls for a grab.

> Hanglock improvement available here: on Windows a per-pixel-alpha layered window hit-tests
> per pixel natively, so we get hover/grab from real `WM_MOUSEMOVE`/`WM_NCHITTEST` messages and
> can stop the frame clock entirely at rest. Zero polling, zero per-frame window-server chatter.

## 5. Rendering approach

* One SwiftUI `Canvas` (immediate mode, `rendersAsynchronously: false`) draws rope + beads + glow
  + charm + knot in a single pass. Explicitly chosen over "21 shape views" because rebuilding and
  diffing view identities 120×/s would dominate the budget.
* **No filters anywhere in the frame loop.** A first attempt with three blur/shadow filters
  measured >20 % CPU and ~100 MB; filters rasterise an offscreen layer per frame. Replaced by:
  four cord strokes (two offset low-alpha for shadow, cord, two dashed for twist, hairline
  highlight), a radial-gradient glow, and dash spacing measured along the path so the twist
  follows bends.
* The one surviving blur (the charm's drop shadow, which must follow the artwork's alpha, not a
  path) is precomputed on the CPU — `RGBABitmap.shadow`, three box passes — cached per size and
  drawn as an ordinary bitmap afterwards.
* Vector artwork is rasterised once per size at native device scale and kept in a cache; raster
  sizes are rounded up to a fixed step so a growing charm doesn't re-rasterise every frame of
  the grow.
* Artwork is built **once** per layer rather than rebuilding dozens of `CGPath`s 120×/s.
* The renderer is a pure function of an immutable `RopeSnapshot` (points, charm radius/angle,
  bead placements, max stretch, isDragging). It never touches the solver, so it can be rendered
  from a hand-written snapshot in a test or preview.

**Exactly the discipline Hanglock needs: the physics is the only thing computed per frame;
everything else is either cached geometry or a bitmap blit.**

## 6. Menu / tray behaviour

* `MenuBarExtra` with `.menu` style, so SwiftUI hands off to a real `NSMenu` and inherits the OS
  rendering, keyboard navigation and screen-reader support at no cost. Commands only — richer controls live in Settings.
* Menu holds: show/hide overlay (⌘⇧O), charm picker (built-ins + imports, checkmarked), import,
  delete, open Studio, open Settings, quit.
* The status icon and the menu's checkmark share **one** `MenuBarViewModel`, so they cannot
  disagree. Filled icon = visible, outline icon = hidden.
* A settings toggle for "launch at login" is **reconciled against the system at startup**
  (`SMAppService` is the truth), because the user can change it outside the app.
* Raising a window from an accessory app needs a deliberate sequence — activate, open the scene,
  then raise on the *next* main-actor turn, after SwiftUI has created the window
  (`SettingsWindowPresenter`). Expect to need the same dance for a tray-driven settings window.

## 7. Always-visible desktop behaviour

Window level (above normal/floating, below system UI) + all-Spaces + `fullScreenAuxiliary` +
`orderFrontRegardless` + never-key. Panel is `.stationary` (doesn't move during Mission Control)
and `.ignoresCycle` (skips ⌘-Tab). The overlay never becomes key or main, so the user's caret
stays where it was. This is the "polite overlay" contract; see §11 for the Windows equivalents.

## 8. Performance and idle/sleep

Measured and published, which is the behaviour we should copy:

| State | CPU | Memory |
|---|---|---|
| Settled | 0.56–0.60 % of one core | 23–26 MB |
| Swinging | ~15 % | 33–42 MB peak |
| Launch → on screen | ~210 ms | — |
| Solver cost | 0.02 ms/frame | — |

Their honest attribution: **what costs money while swinging is presenting a transparent window at
the display's rate, not the content of that present** — an empty canvas over the same simulation
still cost ~2/3 of it. Everything else in the app is engineered around that one fact.
`footprint`, not `rss`, is quoted (rss counts shared frameworks and the compositor's layer pool).

Other levers: sleep-when-settled; single observable write per frame; adaptive solver budgets;
no allocations in the step (reused `RopeCurve` buffer); audio engine stopped a few seconds after
quiet (a running `AVAudioEngine` keeps a render thread awake, which alone would break the budget);
debug state read 5×/s only in non-production builds and compiled out of `Production`.

## 9. Multi-monitor

Deliberately minimal in v1 and documented as such: it uses `NSScreen.screens.first` (primary),
*not* `NSScreen.main`, because `main` follows keyboard focus and the overlay would hop displays
as the user changes apps. Per-display selection is listed as Phase 2, and the reason the seam is
cheap is that all placement math is already display-agnostic:

* `ScreenPlacement.frame(for:anchor:in:offset:edgeInset:)` is pure `CGRect` arithmetic, importing
  only CoreGraphics, unit-tested in a global y-up space, so a secondary display left of the
  primary (negative origin) works with no special case — and has a test.
* `ScreenObserver` republishes on `didChangeScreenParametersNotification` (resolution, scale,
  plug/unplug, menu-bar/Dock geometry changes) and the panel re-anchors.
* Oversized rects are returned unclamped on purpose, so the caller decides.

Also note `resize(to:)` re-fits the rope *without* resetting it, so a scale/display change swings
into the new geometry rather than snapping.

## 10. Settings / preferences architecture

* Pure value types (`AppSettings`, `OverlaySettings`, …): `Codable` structs, no framework imports,
  clamped in the initialiser, `Limits` ranges shared between the UI and the decoder.
* **One funnel per side effect**: every write goes through `SettingsStore.update { … }`, which
  mutates a copy, assigns, and persists in the setter. No `save()` to forget; equality guard means
  a no-op write writes nothing.
* Storage is **one JSON document under a versioned key** (`com.hangly.settings.v1`) in
  `UserDefaults`, with `schemaVersion` written on every save.
* **Tolerant decoding is a hard requirement:** hand-written `init(from:)` with `decodeIfPresent` +
  per-field fallbacks + clamping; unknown keys from a newer build ignored; a corrupt document is
  logged, moved aside under a `.corrupt` key, and defaults are used rather than blocking launch.
  Reason given: synthesised `Codable` throws on a missing key, so *adding a field in a future
  release would silently discard every existing preference*. This is the single most valuable
  settings lesson in the repo.
* `isFirstRun` exists explicitly, so "register as login item" is applied once and never over a
  later choice; `save()` on first run writes the defaults so the next launch isn't a first run.
* Views never see the settings document: `SettingsViewModel.opacity`, not
  `store.settings.overlay.opacity`.
* Services observe settings through `ObservationStream`, a small re-arming
  `withObservationTracking` wrapper publishing newest-value-only — one mechanism for
  non-UI consumers. Because tracking covers only the keys the closure reads, an unrelated edit (the
  login-item flag, say) never wakes the window layer.

## 11. Project structure, build, distribution

* **Committed `project.pbxproj` generated by XcodeGen**, so a clean checkout builds with no
  generator step and no package manager. (Their SwiftLint phase even leaked a build machine's
  `PATH` into the committed project because `${PATH}` was brace-expanded by XcodeGen — a
  documented near-miss worth remembering for any generated-project setup.)
* Three configurations: `Debug` (`-Onone`), `Release` (`-O`, keeps the debug overlay and diagnostic
  logging — *the configuration you profile in*), `Production` (`-Osize`, whole-module, thin LTO,
  every dev surface **compiled out**, not disabled at runtime).
* `DISTRIBUTION.md` is a shipping report: bundle size, minimum OS, "no networking framework linked,
  no request code in the binary", what Production removes *with the evidence used to prove it's
  gone*, and optimisation deltas (−62 % executable, −36 % bundle, ~10 ms faster launch, memory
  unchanged). `Scripts/build-dmg.sh` produces `.app` + `.dmg`.
* CI (`macos-15`): select newest Xcode (deliberately unpinned), assert Swift ≥ 6, `swiftlint --strict`,
  Debug build, full test suite, Production build. `concurrency` group with cancel-in-progress.
* Ad-hoc signing so a clean checkout builds with no developer account; unsigned release → the
  Gatekeeper warning is documented in the README and the FAQ as an honest known cost.
* Docs culture: `Docs/Architecture.md` (per-decision rationale + a 30-row
  decision/alternative/why table), `Physics.md`, `Charm-System.md`, `SVG-Import.md`, `FAQ.md`,
  `DISTRIBUTION.md`, plus issue/PR templates, `SECURITY.md`, `CHANGELOG.md` with a
  "Known limitations" section, and screenshots rendered *from the app's own drawing code* so they
  cannot drift from the product.

## 12. Testing strategy (why their claims are believable)

`Tests/RopeSimulationTests.swift` is organised as Structure / Inextensibility / Gravity+damping /
Frame-rate independence / Dragging / Idling / Reduce Motion / Resizing, and asserts the marketing
claims as numbers: stretch ceiling under free swing and hard flick, 2×120 Hz == 1×60 Hz within
1e-9, long-run stability, momentum on release, sleep/wake, exact cursor tracking, grab targeting,
oversized-frame clamping. Only possible because the solver is pure arithmetic over `CGPoint` and
the store takes its defaults suite by injection.

Their stated bug tally from tests rather than eyeballing: drag past reach tore the links; a
single downstream projection sweep oscillated; a `for … where` (which filters iterations instead
of ending the loop) silently ran the full relaxation budget every step.

The window and render layers are **deliberately not unit-tested** — thin declarative mappings onto
`NSPanel` properties and draw commands, unverifiable without a real window server. A reasonable
line; our MVP should hold it too, and instead buy confidence with a scripted on-hardware test
matrix.

---

## 13. What Hanglock should reuse (concepts, not code)

| # | Concept | Why it earns its place in Hanglock |
|---|---|---|
| 1 | Verlet + positional constraints, fixed timestep, accumulator clamp | It is the whole reason a hanging object feels like an object. Free momentum on release, no stiffness tuning |
| 2 | Inverse-mass pinning; adaptive relaxation whose cap exceeds segment count; one-sided stretch ceiling with shared correction; reach-clamped drag target | The four things that make the rope both stiff and unbreakable, each with a documented failure mode |
| 3 | Sleep when settled, publish nothing when nothing moved | A clock is on the desktop 24 h. Idle cost is the product decision, not an optimisation |
| 4 | No filters/blur in the frame loop; precompute soft shadows; cache raster per size; allocate nothing per step | Directly transfers to a software-blitted frame buffer |
| 5 | Immediate-mode canvas over a per-view tree for per-frame geometry | Same reasoning: N nodes ≠ N view identities |
| 6 | Immutable snapshot across the physics→render boundary | Keeps rendering unable to perturb physics; makes the renderer testable |
| 7 | Pure geometry for placement, tested without a display | Multi-monitor correctness that doesn't need hardware to prove |
| 8 | Re-fit on resize instead of reset (motion preserved) | Display change and DPI change are *frequent* on Windows |
| 9 | Tolerant, versioned, single-document settings with one write funnel | Settings corruption is how utility apps lose users |
| 10 | One shared view model for tray icon state + tray menu state | Two sources of truth for "is the clock showing" will disagree |
| 11 | Reconcile OS-owned state (login item) at startup | Windows' Startup apps list is user-editable outside us |
| 12 | A `Production` build that compiles dev surfaces out + a distribution report with measurements | Cheap trust for an open-source project |
| 13 | Public "known limitations" and a measured performance table | Sets expectations instead of a support queue |
| 14 | Tests that assert the physics claims numerically | "Swings naturally" must be a number |

## 14. Where Hanglock should deliberately differ

| Hangly | Hanglock | Why |
|---|---|---|
| Point-mass charm oriented along the last link | Rigid card on a cord; later a dual-cord V-mount (see [`../ideas/dual-cord-mount.md`](../ideas/dual-cord-mount.md)) | A wide clock on one strand rotates a lot; a plaque needs its own rotational behaviour, and legibility outranks realism |
| Payload is decoration, physics is the point | Payload is *information* — the face must stay readable mid-swing | Rotation limits, contrast against any desktop, settle faster than an ornament |
| Polled cursor + per-frame click-through toggle | Per-pixel hit testing via layered-window alpha + `WM_NCHITTEST` | Windows gives us this natively: hover affordance with no polling, and a frame clock we can stop completely at rest |
| Always-on 120 Hz clock while awake | 60 Hz while swinging, **1 Hz while settled** (only the digits change), 0 Hz hidden | A clock's steady-state work is one small dirty rect per second |
| Menu-bar accessory, hangs under the menu bar | Taskbar/Dock-adjacent tray app, hangs from the physical top edge | Windows has no top bar to hide behind; the top edge is free real estate (but must respect a top-docked or auto-hidden taskbar) |
| Primary display only, "Phase 2" | Monitor index persisted + live re-anchor on `WM_DPICHANGED`/`WM_DISPLAYCHANGE` in MVP | A Windows workstation user has ≥2 monitors; being stuck on the primary is a first-week complaint |
| `NSPanel` at `.statusBar` level | `WS_POPUP` + `WS_EX_LAYERED|TOPMOST|TOOLWINDOW|NOACTIVATE|NOREDIRECTIONBITMAP?`, re-asserted on `WM_WINDOWPOSCHANGING` | Same contract, Win32 vocabulary |
| Sounds on by default | Silence by default; optional tick | A productivity utility on a shared/Teams-call desktop must never make noise unless asked |
| Charm artwork, SVG import + Vision segmentation, Studio | No import pipeline in scope; faces are code | Different product: tools, not ornaments. Their asset pipeline is ~45 % of the codebase (5,019 of 10,683 Swift lines) and none of it is in scope |
| arm64-only, `.dmg`, unsigned with a caveat | `x64` first + `arm64` windows, Inno Setup/MSI + winget, SignPath-style free OSS signing | Windows distribution is installer + reputation, not a drag-to-Applications DMG |

## 15. Unresolved in their design, worth solving for us

* Multi-display is a roadmap item, not a feature; no DPI-per-monitor story (macOS scales
  uniformly, so it doesn't bite them — it will bite us).
* The 740×420 window is much larger than what's drawn in it (justified for an ornament that
  swings wide); a clock should size its window to its swept area, and could move the window with
  the object when idle to shrink the presented surface.
* Click-through is all-or-nothing per window and only as responsive as the poll rate; there's no
  "click straight through while I'm dragging a window near you" heuristic.
* One rope, one object, one overlay — multi-instance is called out as a contained change but not
  done.
* No keyboard interaction at all (no focus, by design); a productivity utility needs shortcuts
  (toggle seconds, start timer, switch mode) without stealing focus from the active app — which
  means global hotkeys, a different permission story.
* No sleep/resume handling beyond the accumulator clamp; a machine that wakes at 3 a.m. must not
  have drifted its clock *or* its notion of "last tick".
