# Windows overlay notes

The 2.4k lines of `crates/hanglock-win`, and what they depend on. Written as an onboarding document,
because this is the knowledge that makes the next person able to change it.

> **Status: it compiles, tests, links and runs headless in CI** — clippy `-D warnings` and
> `cargo test --workspace` on Linux, Windows `x64` and Windows `arm64`, and on both Windows ABIs the
> release link plus a `--diag`/`--bench`/`--dump-scene` smoke of the built executable. What that does
> *not* buy: no runner has a desktop, so every
> claim below about what Windows *shows* — the tray icon, a present, a DPI change a person made — is still
> reasoned-from-specification rather than observed-in-a-debugger, and the verify list at the end is the
> checklist for the first run on hardware.

## The window

`WS_POPUP` with `WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`, and `WS_EX_TOPMOST` when the
user wants it. Each style is one requirement:

| Requirement | Style | Note |
|---|---|---|
| No frame, no taskbar entry, not in Alt+Tab | `WS_POPUP` + `WS_EX_TOOLWINDOW` | A tool window is also what keeps the system from animating it. |
| Per-pixel transparency | `WS_EX_LAYERED` + `UpdateLayeredWindowIndirect` | No DWM blur/round-corner requests; the plate draws its own shape. |
| Above normal and most floating windows | `WS_EX_TOPMOST` + `SetWindowPos(HWND_TOPMOST)` | Does **not** beat exclusive-fullscreen games; documented limitation, not a promise we chase. |
| Never steals focus | `WS_EX_NOACTIVATE`, and never calling `SetForegroundWindow`/`SetFocus` | The one exception is the popup menu, below. |
| Show without activating | `ShowWindow(SW_SHOWNA)` | `SW_SHOW` would activate. |

**No `WM_PAINT` exists in this design.** A layered window's pixels come from the DIB we own; there is
no paint cycle to invalidate, which is also why the class has no `CS_HREDRAW|CS_VREDRAW`.

## Transparency and input

The behaviour the whole design rests on: **hit testing on a layered window follows alpha** — pixels
with alpha 0 let mouse messages fall through to whatever is underneath. That is what makes click-through
free (no `WS_EX_TRANSPARENT` toggle, no cursor polling, no per-frame window-server traffic) and is why
`hanglock-render::tests` asserts that the corners are *exactly* zero alpha, not merely small: a pixel
at alpha 1 of 255 is invisible and yet eats the click.

`WM_NCHITTEST` answers `HTCLIENT` inside the plate's oriented rect (or the ring) and `HTTRANSPARENT`
everywhere else, from the same positions the painter used. Verify: does the anti-aliased 1 px rim steal
clicks it shouldn't? If it does, shrink the hit rect by a pixel rather than widening the paint.

## Presentation

`CreateDIBSection` with a negative height (top-down rows, so the painter's row 0 is the window's row 0)
into a 32 bpp premultiplied-BGRA buffer selected into a compatible DC, then
`UpdateLayeredWindowIndirect` with `ULW_ALPHA` and `BLENDFUNCTION{AC_SRC_OVER, 0, 255, AC_SRC_ALPHA}`,
and `prc_dirty` set to the union of what changed.

Three things worth knowing before changing this file:

1. `UpdateLayeredWindow` has **no** update-rect parameter. The sub-rect present — the difference between
   a settled clock copying ~137 KiB a second and ~72 MiB a second — requires the *Indirect* variant. The
   plain call is kept as a fallback and loses only the optimisation.
2. Alpha must be **premultiplied** or the edges fringe. The painter composites in premultiplied form for
   the same reason: `dst = src + dst·(1−srcA)` needs no divide, and no alpha-dependent colour drift.
3. A fresh DIB's contents are not reliably zeroed across drivers. `Surface::resize` zeroes it explicitly;
   if that is ever removed, the first frame after a resize shows whatever the allocator left behind.

## Text

Grayscale anti-aliasing only, by construction: ClearType blends subpixel masks against a known opaque
backdrop, and a transparent window has none — which is true of every renderer on Windows (Direct2D
switches to grayscale when the target carries alpha; WPF disables ClearType on layered windows for the
same reason). The consequence for the design is real and permanent: thin strokes turn to mush. The face
is therefore monoline at `STROKE_RATIO = 0.125` of cap height, at a 48 px cap at 100 %, with no
hairlines. Verify at 100 % on a light wallpaper and a dark one, with a magnifier.

## Timers, and where the idle cost goes

Two `SetTimer`s, and that is the whole scheduling story: `TICK` (interval from `fps_cap`, killed when the
rope settles) and `SECOND` (1000 ms, always). `timeBeginPeriod(1)` makes the 16.7 ms request actually
arrive at 16.7 ms instead of in 15.6 ms quanta, which is the difference between a smooth swing and a
visible stutter; `timeEndPeriod(1)` runs on the way out.

There is deliberately **no thread of ours**. `GetMessageW` blocks, so a settled clock costs a message
per second and nothing else. Verify: Task Manager's thread count for `hanglock.exe` should be ~2
(us plus the DWM-side one), and idle CPU ~0.0 %.

## The tray and the one place focus is touched

`Shell_NotifyIconW` on the overlay's own HWND. The menu is a real popup (`CreatePopupMenu` +
`TrackPopupMenuEx` with `TPM_RETURNCMD`).

`TrackPopupMenu` will not dismiss on click-away unless its owner window is foreground. So: remember
`GetForegroundWindow()`, `SetForegroundWindow(ours)`, track, `SendMessage(ours, WM_NULL, 0, 0)` (the
documented trick that makes the menu release capture cleanly), then restore the previous foreground
window. Net effect: our window is *never activated* (no `SetActiveWindow`, no caret movement), but is
briefly foreground while the menu is up. Verify: with a Notepad caret blinking, open the card's
right-click menu and dismiss it by clicking the desktop; the caret must still be where it was, and the
menu must not stick.

The tray icon's `NOTIFYICONDATAW` is hand-declared with a `size_of` assertion, and `cbSize` is set from
`size_of` rather than a literal, so a layout mistake makes the icon fail to appear in front of a
developer rather than writing past the end of a buffer on a user's machine. If the icon never appears:
that is the failure mode, and `--diag` prints `tray_installed`.

## DPI

`SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)` resolved through `GetProcAddress` at startup,
falling back to `SetProcessDPIAware()`. The dynamic lookup is not caution: a *static* import of a
function that predates Windows 10 1703 makes the executable fail to load there, with no error message
worth reading. `GetDpiForWindow` is likewise probed, falling back to `GetDeviceCaps(LOGPIXELSX)`.

There is no application manifest in this phase because there is no resource compiler in the build path
(no `.rc` tool, no `mt.exe` requirement). `--` consequences to check: the DPI-awareness bit is set early
enough that `CreateWindowExW` never runs under the wrong context (it does, because `run` sets it first),
and long-path handling is not enabled (irrelevant at these path lengths). Adding `apps/hanglock/hanglock.manifest`
via `link.exe /MANIFESTINPUT:…` in `scripts/build.ps1` is the tidy version; recorded in
`docs/reports/phase-1.md` as a follow-up rather than done blind here.

All model geometry is in **logical** px; the display scale is applied at `Rope.scale` and at
`Surface`/`Canvas` sizes, and converted once in `placement::place`. `WM_DPICHANGED` re-runs
`placement` with the *saved* ratio rather than the suggested rectangle, because the suggested rect is
sized for a normal window, not for a swept sector. Verify: 100 → 200 % while swinging must not reset
the swing and must not detach the cord from the plate. The model half of that is asserted —
`refitting_preserves_motion_instead_of_resetting` is exactly the "does not reset the swing" promise, and it
is the test that caught `refit` handing the cord a zero velocity and letting it fall dead. What is not asserted anywhere
is the arrival: that `WM_DPICHANGED` reaches us at all, and that the re-layout — which ignores the
suggested rectangle on purpose and recomputes from the saved ratio — puts the clock back where the user had
it.

## Displays

Monitor enumeration uses `MonitorFromWindow`/`GetMonitorInfo` for the window's own display and a
64-px-stepped `MonitorFromPoint` scan of the virtual screen for the rest, because a
callback-based `EnumDisplayMonitors` would have to hand a `WNDPROC`-shaped closure through FFI from a
crate that keeps its `unsafe` in four files. `rcWork` is the per-monitor work area, which is what makes
a top-docked taskbar a data point rather than a special case. Verify with: three displays, mixed scale,
one above the primary, and the clock on the one that gets removed.

## The verify list (what the first run on a desktop should check, in order)

1. **Settled by CI:** that it compiles at all — types, borrows, unused imports — `cargo clippy --workspace
   --all-targets -D warnings` on Linux, Windows `x64` and Windows `arm64`.
2. `NOTIFYICONDATAW` size assertion (992 on x64) — **settled by the same builds**, since it is a `const`
   check — and that the icon appears, which no headless runner can see.
3. `UPDATELAYEREDWINDOWINFO` size (72) — likewise a `const` check, so already proved — and that a present
   shows the plate rather than a black box, which is owed.
4. `HitShape::contains` covering the plate *and* the ring, and only those.
5. That `--background` + `RegSetKeyValueW` autostart round-trips, and `is_enabled` reconciles after the
   user disables it in Task Manager.
6. That `PresentRect` for the digits looks right when the minute rolls over `9:59 → 10:00` (the run gets
   wider: the union rect must cover the old glyphs too).
7. `ReplaceFileW` in `store.rs` while an antivirus scanner holds `settings.toml` open.
