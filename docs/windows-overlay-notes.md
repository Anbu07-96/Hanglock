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

## The settings window, and the two kinds of dialog

There are exactly two dialogs in the app, and they are deliberately different shapes, because the two
have different amounts to lose.

**About** is a `MessageBoxW`. It blocks, it is modal, and it needs no window of ours: the text cannot
change while it is up, so there is nothing to keep in sync and nobody has to remember to close it. A
hand-rolled About box would be a second thing to lay out, to font, to position and to keep from
drifting.

**Settings** is a real window — `RegisterClassExW` with our own procedure, and `BUTTON`/`STATIC`
children — and it owns nothing. `hanglock-platform::panel::form` turns the applied `Settings` plus the
live display list into groups of rows; the window draws those rows and reports a click as
`(RowId, Step)`; the app turns that into the same `Command` the tray menu would have sent. So the
window has no draft state, no Apply button, no validation and no copy of a value, which is the whole
reason the tray and the window cannot end up describing different apps. The row set is fixed for a
release, which is what lets a click be decoded by one division (`ID_BASE + row * 4 + part`) instead of
a table that could go out of date.

What a plain window buys over the obvious alternatives, each of which was rejected:

* **`CreateDialogIndirectParamW` with a hand-built template.** Free fonts, free tab order — for a byte
  encoding of `DLGITEMTEMPLATE` with alignment padding that no Linux CI can check, and a modal or
  modeless loop that is not the one this process already runs.
* **A comctl32 property sheet.** Tabs, for three sections that fit in one column. It also drags in a
  v6 common-control manifest and an activation-context question, and `docs/research/` records what that
  class of question cost the reference project. No UI framework, no new runtime: those were rules.
* **`DefWindowProcW`'s defaults for a child's look.** Buttons draw themselves; that is the extent of
  the platform's help, and it is enough.

Three details are what make it not look like 1995, and each is a call rather than a style:

* The font comes from `SystemParametersInfoW(SPI_GETICONTITLELOGFONT)` and `CreateFontIndirectW`. The
  icon-title font is Segoe UI at the user's chosen size on every Windows this app supports.
  `GetStockObject(DEFAULT_GUI_FONT)` — the usual suggestion — is the old 8 pt ANSI face, and a window
  drawn in it reads as a bug report.
* The window's height comes from the rows: `repaint` walks the groups, accumulates a client rect, and
  hands it to `AdjustWindowRectEx` to add the caption and border. Guessing a border width breaks at
  150 % scaling, and a bottom row that is 6 px short of the frame is a user who cannot reach `Close`.
* `IsDialogMessageW` is called from the message loop, for the panel's messages only — gated on
  `GetAncestor(msg.hwnd, GA_ROOT) == panel`. That one call is the entire keyboard support: Tab between
  rows, arrows inside a radio group, Space to tick, Esc to close. It is gated because routing the
  overlay's messages through a dialog manager could swallow the one message that keeps a drag alive.

The window's state is a `Box<Panel>` parked in `GWLP_USERDATA`, the same arrangement the overlay uses
for its `Runtime` and for the same reason: a window long cannot hold a reference, and the message loop
cannot outlive the frame that owns the data. It is freed in `WM_NCDESTROY` rather than `WM_DESTROY`,
after the children that were drawn with its font are gone, and the window long is cleared before the
box is dropped so a message that arrives in between finds null and not a dangling pointer. The overlay's
own answer to "is the settings window up?" is `IsWindow`, not a flag: the user can close it with its
`x`, and no message about that reaches `Host`.

A DPI change on this window re-runs the same `repaint`, because the layout is logical pixels times
`GetDpiForWindow`: a clock that survives a monitor change while a settings window next to it does not
would be the one part of the app that is not DPI-aware, which is a strange thing to read in a file
about being DPI-aware.

## Autostart, and what a checkbox is allowed to claim

`HKCU\Software\Microsoft\Windows\CurrentVersion\Run` is the *wish*. Windows keeps a second flag
beside it: `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run`, which is
what Task Manager's Startup page writes when a user clicks Disable on an entry that still exists. A
checkbox that read only `Run` would say "on" while the system declined to start us — a lie that survives
a restart, which is the worst kind this app can tell.

So `is_enabled` is `entry exists && approved`, `apply()` answers with what a read after the write
reports rather than with `Ok(())`, and *enabling* clears the `StartupApproved` value the way Task
Manager's Enable button does. On startup `Model::on_ready` reconciles the document against the registry
in the other direction: the registry wins, because a `Run` key we deleted by hand or disabled in Task
Manager is a decision taken after the file was written, and re-adding the key at every logon would be
the app overruling a person.

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
8. That the settings window opens from the tray and from the card's menu, is keyboard-navigable without
   the mouse (Tab, arrows through a choice, Space, Esc), and that a nudge's `Less`/`More` are greyed at
   the ends of their range rather than wrapping or clamping silently.
9. That Alt+drag from the *middle of the plate* moves the hang point, that the window and the card
   disagree for the duration of the gesture on purpose (the card follows on release, via `Relayout`),
   and that the position survives a restart, a scale change and a monitor switch.
10. That `Fully click-through` is refused while the tray icon is missing, that the refusal is readable in
    the tooltip and as a greyed line in the menu, and that the mode arrives as soon as the icon appears.
11. That disabling Hanglock in Task Manager's Startup page makes the checkbox read false at the next
    start, and that ticking it there again survives our write.
12. That `settings.toml` does not exist after a first run in which nothing was changed, and that a
    corrupt file comes back as defaults plus one stderr line and a renamed `.corrupt-<ts>` original.
