# Windows desktop validation — Hanglock Phase 2.5

This is the manual pass CI cannot make. Every check below is something a person with a real desktop can
answer in one sitting; nothing here asks for a code change, a rebuild or an opinion about design. Report
what you see, including "annoying".

**Build to test:** the `hanglock-x86_64-pc-windows-msvc` artifact of the latest green run of
`.github/workflows/ci.yml` on branch `arena/01a0a5bc-hanglock` (see §0 for where to click). Do not build
from source unless you want to: the artifact is the same binary CI smoke-tested.

**Result template** — copy this block once per check that fails, and use the short form for the
ones that
pass. One line per pass is enough: `A3 pass`, or `A3 pass (minor lag, see below)`.

```text
Test:         <check id, e.g. E2>
Result:       PASS / FAIL
Notes:        <what you did, what happened, once or every time>
Screenshot:   <drag into the report, or a .png path / short .mp4>
Severity:     blocker / major / minor / cosmetic
```

**Severity, so we mean the same thing:**

| Level | Means |
|---|---|
| blocker | The app will not be used, or cannot be recovered: won't start, clock invisible, click-through you cannot escape, a crash |
| major | A feature is wrong or unusable, but the app is livable: time wrong, position lost on restart, tray item does nothing |
| minor | Noticeable and wrong, small consequences: a label that misleads, a control greyed when it shouldn't be |
| cosmetic | Looks off, works fine: spacing, shadow weight, a 1 px edge |

## 0. Get the build and set up

**Where the artifact lives.** GitHub → this repo → **Actions** → the newest run of *Phase 1: First
runnable
Hanglock prototype* on `arena/01a0a5bc-hanglock` whose conclusion is green → scroll to
**Artifacts** at the
bottom → `hanglock-x86_64-pc-windows-msvc` (a `.zip`). It contains four files:

| File | What it is |
|---|---|
| `target/x86_64-pc-windows-msvc/release/hanglock.exe` | the app, one file, no installer |
| `diag.txt` | what CI's release build printed for `--diag` — compare yours against it |
| `bench.txt` | CI's `--bench 200` numbers |
| `scene.png` | CI's headless render of the clock at default settings — **the reference for section B** |

Unzip it somewhere that will still exist tomorrow, e.g. `C:\Tools\Hanglock\`, and keep the folder layout
flat enough to type: `cd C:\Tools\Hanglock\target\x86_64-pc-windows-msvc\release`. **Use PowerShell, not
cmd**, for the diagnostic commands: this release binary is a GUI-subsystem process, and `cmd` hands back
the prompt before it has finished printing.

Two notes before the checklist. The exe must live where you will leave it, because *Start with Windows*
registers the path it finds itself at — moving it later leaves a startup entry pointing at nothing. And
there is no installer, no redistributable and no administrator rights involved: Windows 10/11 x64 is the
whole requirement. If Windows says a `VCRUNTIME140.dll` or `api-ms-win-crt-*.dll` is missing, that is a
release defect — file it as a blocker and stop, because everything below depends on the binary
launching.

```powershell
# The two commands you will use most. Neither one writes to the settings file.
.\hanglock.exe --diag
.\hanglock.exe --dump-scene $env:TEMP\scene.png; Invoke-Item $env:TEMP\scene.png
```

`--diag` is a printout of what the app *believes*: which file it read, which display it chose, where the
hang point is, and what each mode is set to. Its fields, in the order it prints them:

| Field | What to read off it |
|---|---|
| `settings file` | always `%APPDATA%\Hanglock\settings.toml`, printed unexpanded on purpose so a pasted report cannot name your account |
| `as read` | `no file yet; defaults, none written` = first run, nothing created. `loaded` = read. `recovered (…)` = the file was moved aside to `settings.toml.corrupt-<n>` |
| `displays` / `#N` | every display Win32 reported: size, origin, work area, scale, `primary` |
| `on monitor` | which of them the clock is on. If this is not the one you expect, sections H and L are the ones to run |
| `frame (device)` / `clipped` | the overlay rectangle and whether the placement had to pull the clock on-screen (`clipped=true` means it did) |
| `anchor` / `anchor (screen)` | the hang point three ways: a ratio of the work area, a logical drop in px, and the device point. This is what sections E and J are about |
| `clock` | 12/24, AM/PM on or off, seconds on or off, and the string currently drawn (`shows "10:42"`) |
| `mouse` | `Interactive (whole window)` / `Transparent areas click through` / `Fully click-through` |
| `on top`, `visible`, `autostart` | always-on-top, whether the clock is hidden, and whether the registry entry is live |
| `posture`, `hang`, `size` | how it swings, the cord length in logical and device px, and the card scale + margin |
| `state`, `canvas`, `fps cap`, `budgets` | physics state (`Settled`/`Swinging`/`Dragging`), the painted surface, the frame cap, and the per-frame budget the painter is aiming at |

`--bench [n]` paints `n` frames and prints the cost of each; use it only if you want a number to compare
against CI's `bench.txt`. `--dump-scene PATH` writes one settled frame as a PNG and exits — it is the
fastest way to tell "the painter is wrong" apart from "the window is wrong", because it bypasses Win32
entirely.

**Recording:** fill the table below as you go. A check is PASS only if nothing about it needed
explaining.

| Section | Checks | Pass | Fail | Blocker | Major | Minor | Cosmetic |
|---|---|---|---|---|---|---|---|
| A First launch | 8 | | | | | | |
| B Visual quality | 7 | | | | | | |
| C Clock behaviour | 7 | | | | | | |
| D Physics | 8 | | | | | | |
| E Anchor and Alt+drag | 7 | | | | | | |
| F Tray menu | 14 | | | | | | |
| G Click-through | 6 | | | | | | |
| H Settings window | 11 | | | | | | |
| I Always on top | 4 | | | | | | |
| J Reset position | 5 | | | | | | |
| K DPI | 6 | | | | | | |
| L Multi-monitor | 6 | | | | | | |
| M Sleep and wake | 4 | | | | | | |
| N Startup with Windows | 5 | | | | | | |
| O Recovery | 5 | | | | | | |
| P Exit and restart | 5 | | | | | | |
| **Total** | **108** | | | | | | |

Skip what your hardware cannot do — K on a 100 % single display, L without a second one — and write
`n/a`
rather than a pass, so the gap is visible.

## A. First launch

- [ ] **A1 — It starts.** Double-click `hanglock.exe`. Expected: the clock appears within about a second and the process stays running. Nothing else opens.
- [ ] **A2 — No console window.** Same double-click. Expected: **no black console window** anywhere. (If one appears, that is a blocker: the release binary is meant to be GUI-subsystem.)
- [ ] **A3 — No taskbar entry, no Alt+Tab entry.** Expected: no button on the taskbar and no entry in Alt+Tab. The window is `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`, so this is structural, not a setting.
- [ ] **A4 — Nothing stole focus.** After A1, click on a document or a terminal and type. Expected: your keystrokes go where they were going. Then `Alt+Tab` and confirm Hanglock is not in the list, and that returning to your app did not raise the clock over it.
- [ ] **A5 — Tray icon.** Expected: an icon in the notification area whose tooltip says `Hanglock`, and a left-click on it does not open a window (the menu is a right-click; some Windows builds only offer the right-click at all).
- [ ] **A6 — Safe starting position.** Expected: the clock hangs near the top-centre of the primary display, fully visible, not under the taskbar, not clipped by the top edge. Note the actual position if it surprises you.
- [ ] **A7 — The right time, now.** Expected: the digits are the current wall time. If they read a fixed time (e.g. `10:42`) and never advance, the tick source is dead — major, and `--diag`'s `state`/`clock` lines will say what it thinks it is.
- [ ] **A8 — No file written on an untouched first run.** `dir $env:APPDATA\Hanglock` **before** the first launch and after an hour of running. Expected: the folder does not exist (or is empty) until you change something. This is the designed behaviour: a program that has not been told anything has nothing to remember.

## B. Visual quality

Compare against `scene.png` from the artifact — same painter, same default settings, no window server in
between. If yours and CI's disagree in shape, the bug is in the surface; if they agree but you do
not like
it, that is a design note, and both are useful.

- [ ] **B1 — Readable at your desk distance.** Read the time from where you actually sit. Note the display size and scale you tested at.
- [ ] **B2 — Rope to card.** The two cords should meet the plate at its top edge with the clamp drawn, not cut off. Look hardest at the top few pixels.
- [ ] **B3 — Centring.** The digits should sit centred in the plate with even margins left and right, not optically low or high.
- [ ] **B4 — Shadow and edges.** No hard rectangle of dark pixels, no visible cut line where the layered surface ends.
- [ ] **B5 — No alpha artifacts.** Drag the clock over a busy, high-contrast window (a video timeline, a photo viewer, a dark IDE) and watch the card's edges. Expected: the transparency is clean; no fringing, no halos, no dirty pixels left behind when you move it.
- [ ] **B6 — Light wallpaper.** Against a bright wallpaper the digits should still read; note if the card's own fill disappears into it.
- [ ] **B7 — Dark wallpaper.** Same against a dark wallpaper, plus the AM/PM marker and (with seconds on) the seconds field: legible, aligned with the minutes, not crowding the plate's right edge.

## C. Clock behaviour

Use the tray and the settings window for these, not the file — the point is that the UI changes the
clock,
and that a format change never changes the *time*.

- [ ] **C1 — Ticking.** Watch for 60 seconds. Expected: it advances a minute at the right moment, not early or late.
- [ ] **C2 — 12-hour.** Set 12-hour. Expected: `hh:mm` with `AM`/`PM`, hour 1–12.
- [ ] **C3 — 24-hour.** Toggle to 24-hour. Expected: the same instant, hour 0–23 — **the minute must not change and the digits must not jump**. (If switching to 24-hour shows the 12-hour hour, or seconds snap to `:00`, that is the bug fixed in `83c397b`; file it as major.)
- [ ] **C4 — Seconds on.** Expected: seconds appear **immediately**, showing the real current second — not `:00` until the next minute boundary.
- [ ] **C5 — Seconds off.** Expected: seconds disappear immediately, and the layout stays centred rather than shifting by a width.
- [ ] **C6 — AM/PM off in 12-hour.** With 12-hour on, toggle the meridiem. Expected: the marker vanishes, the digits stay. Toggle it back.
- [ ] **C7 — Nothing changed the time.** Run C2–C6 in a random order for two minutes and compare with your phone at the end. Expected: still correct to the second.

## D. Physics

- [ ] **D1 — Drag.** Left-drag the card and move it around. Expected: it follows the cursor as one object, cords trailing, no lag you can point at.
- [ ] **D2 — Momentum.** Let go mid-drag. Expected: it carries on and settles after a couple of swings rather than stopping dead.
- [ ] **D3 — Hard throw.** Throw it toward the nearest edge. Expected: it swings/follows and comes back without the card detaching or spinning through the rope.
- [ ] **D4 — Settling.** Leave it swinging and watch it stop. Expected: it comes to rest level, over a second or two, without a final twitch.
- [ ] **D5 — No over-stretch.** During D3, do the cords visibly lengthen by more than a few percent? Expected: no (the solver is pinned to a 1.03× stretch bound by `tests/golden/trace_drag_settle.txt`).
- [ ] **D6 — No snapping.** Nothing should teleport: not at the start of a drag, not at the end, not when it hits a screen edge.
- [ ] **D7 — No jitter at rest.** At rest for 10 seconds, no pixel should shimmer. Then run `.\hanglock.exe --diag` and read `state`: expected `Settled`.
- [ ] **D8 — Quiet when settled.** Open Task Manager, find `hanglock.exe`, watch CPU for 30 seconds while the clock is settled with seconds off. Expected: ~0 %. A settled clock asks for no frames — that is the whole design argument, and a number here is worth recording (write it in Notes even on a pass).

## E. Anchor and Alt+drag

The hang point has two handles: the small square around the anchor at the top of the cords (the cursor
changes over it), and **Alt+drag anywhere on the card**. Both write `anchor_ratio` + `anchor_drop`
to the
file — nothing else does.

- [ ] **E1 — The ring handle is discoverable.** Hover the top of the cords. Expected: the cursor changes, and pressing it drags the *hang point*, not the clock.
- [ ] **E2 — Alt+drag re-anchors.** `Alt` + left-drag on the card. Expected: the anchor moves under the pointer and the clock hangs from the new point, the card lagging during the gesture and catching up when you let go (that lag is intentional: the rope is still a rope).
- [ ] **E3 — Alt is read at the press.** `Alt+drag`, then try the same gesture with Alt pressed *after* the mouse button. Expected: the second one drags the clock, not the anchor — the modifier is sampled when you press.
- [ ] **E4 — It stays on the display.** Drag the anchor far off to the left, right, and up past the top edge. Expected: it stops at a valid position on that monitor; the card is never left stranded off-screen, and `--diag` shows either `clipped=false` or a pulled-in frame.
- [ ] **E5 — The rope responds.** Re-anchor to a point well off-centre and release. Expected: it swings to hang from the new point naturally, and settles level.
- [ ] **E6 — It survives a restart.** Re-anchor somewhere clearly unusual, note `--diag`'s `anchor` line, then Exit and relaunch. Expected: the same numbers (to 4 decimals), same place on screen.
- [ ] **E7 — DPI does not lose it.** With the anchor off-centre, change the display scale (section K) and read the `anchor` line again. Expected: same ratio and same logical drop; the device point moves, which is correct.

## F. Tray menu

Right-click the tray icon. The whole list, top to bottom, is what F1–F14 check: the point is that every
item works *and* that its tickmark/greyed state reflects reality after you use the other surfaces.

- [ ] **F1 — Layout.** Expected, in order: Show clock / (separator) / Always on top / Mouse ▸ / Show seconds / 12-hour time / AM / PM / How it swings ▸ / Hang from this display ▸ / (separator) / Hang longer / Hang shorter / Bigger / Smaller / Reset position / (separator) / Settings… / Start with Windows / (separator) / About Hanglock / Exit Hanglock. No Timer, no Stopwatch. (A leading greyed line is allowed only when the tray refused something — see G5.)
- [ ] **F2 — Show clock.** Hides the clock; the item reads `Hide clock`. Expected: hiding stops painting entirely (CPU stays ~0), showing brings it back where it was.
- [ ] **F3 — Always on top** — see I1/I2 for the real test; here just confirm the tick follows the state.
- [ ] **F4 — Mouse ▸** shows the three names exactly: `Interactive (whole window)`, `Transparent areas click through`, `Fully click-through`, with a bullet on the current one.
- [ ] **F5 — Show seconds** ticks and unticks, and the clock changes on the same click.
- [ ] **F6 — 12-hour time** ticks and unticks; the digits change 12↔24 at once.
- [ ] **F7 — AM / PM is greyed while the clock is 24-hour**, live while it is 12-hour, and toggling it changes the marker only.
- [ ] **F8 — How it swings ▸** has `Swings freely`, `Swings a little`, `Rigid`, `Always level`. Pick `Always level`, throw it (D3): the card should stay level while the cords move. Then `Rigid`, then back to `Swings a little`.
- [ ] **F9 — Hang from this display ▸** lists the monitors; on a single display the entry is greyed and names that display. Expected: nothing happens when you cannot choose, and the label tells you why.
- [ ] **F10 — Hang longer / Hang shorter** step the cord and grey out at the ends (at the longest, `Hang longer` is greyed). Note: a wheel notch over the card does the same thing.
- [ ] **F11 — Bigger / Smaller** step the card size and grey at their limits.
- [ ] **F12 — Reset position** — see J1–J5.
- [ ] **F13 — Settings…** opens the window; **Start with Windows** — section N; **About Hanglock** shows a short dialog and the clock stops painting while it is open (expected: it resumes the moment you dismiss it, and the time is still right).
- [ ] **F14 — Exit Hanglock** — section P.

## G. Click-through

Put something clickable *under* the clock before you start — desktop icons, a video timeline you
can scrub,
a text editor with the caret under the card's transparent margin. Then compare the three modes at
the same
spot. `--diag`'s `mouse` line always says which mode you are in, which makes this section quick.

- [ ] **G1 — Interactive (whole window).** Click on the empty part of the card (not a digit). Expected: it responds — you are dragging the window, not the desktop.
- [ ] **G2 — Transparent areas click through.** Same click on the transparent margin. Expected: the click goes **through** to whatever is under it; clicking a digit still drags the clock. Scrub a video timeline through the transparent area if you can.
- [ ] **G3 — Fully click-through.** Expected: the clock no longer reacts to anything at the mouse; every click lands on the desktop/app below, over the digits too. Drag is impossible in this mode, which is intended.
- [ ] **G4 — The tray still works in Fully click-through.** Right-click the tray icon and set the mode back to `Transparent areas click through`. Expected: it works, and the clock is clickable again immediately.
- [ ] **G5 — The refusal exists.** With the tray icon absent, `Fully click-through` is refused and the menu's first greyed line says a tray is needed. Testing this properly means making the tray fail, so: if you cannot reproduce it, write `n/a`, and do not try to force it by killing the icon.
- [ ] **G6 — You are never trapped.** From every mode, in under five seconds, you can get back to `Interactive` using only the tray. If any sequence leaves you unable to reach the settings, that is a blocker — write down exactly how.

## H. Settings window

Open it from the tray (`Settings…`) once, and from a right-click on the card once. Same 13 rows: Launch
at sign-in, Always on top, Hang from this display, 12-hour clock, Show seconds, AM / PM, Card size, Cord
length, Mouse, How it swings, Anchor across, Anchor drop, Reset position.

- [ ] **H1 — It opens where you can find it**, is not topmost over your work (deliberate: it can be lost behind a window — note if that annoyed you), and it has no taskbar button.
- [ ] **H2 — Painting.** Open it, resize nothing (it cannot resize), move it between two displays if you have them, and minimise/restore your other windows. Expected: no black rectangles, no text that fails to redraw, no flicker on the first frame worth mentioning.
- [ ] **H3 — No Apply, no Cancel.** Every row takes effect on the click. That is the design: the window edits nothing and holds nothing, so "close the window and lose the change" cannot happen.
- [ ] **H4 — Each checkbox does exactly its own thing.** Flip `12-hour clock`, `Show seconds`, `AM / PM`, `Always on top`, `Launch at sign-in` one at a time; after each, `--diag` and the clock should agree, and no *other* setting should move.
- [ ] **H5 — Radios tick one option only**, and switching the `Mouse` radio also changes what the mouse does (section G) at once.
- [ ] **H6 — `Cord length` and `Card size` step** with `Less`/`More`, the readout names the rung (`3 of 6`), and the value moves on the first click, not the second.
- [ ] **H7 — Buttons grey when they cannot move anything.** `Less` greyed at the shortest cord, `More` at the longest, both live in between. Now hand-edit `hang = 259.5` into the file (§O for the mechanics), restart, and check `More` is still live and moves it to the top rung. Expected: no enabled button that does nothing.
- [ ] **H8 — `Anchor across` / `Anchor drop`** move the hang point by one step per click and the clock follows immediately; `Anchor drop` should not be able to pull the clamp off the top of the screen.
- [ ] **H9 — `Reset position` inside the window** does what the tray item does (J2).
- [ ] **H10 — Reopening preserves everything.** Set three things, close the window (its `Close` button, `Esc`, or the title bar ×), reopen. Expected: still there. Then Exit and relaunch, and check with `--diag` that they survived to the file.
- [ ] **H11 — Keyboard.** Tab reaches every control exactly once in a sensible order, Space toggles/presses, Enter closes. (If a radio group eats Tab oddly, say so; it is minor.)

## I. Always on top

- [ ] **I1 — On.** Open a browser maximised. Expected: the clock stays above it.
- [ ] **I2 — Off.** Turn it off, raise the browser. Expected: the clock goes behind and stays where it was; `--diag`'s `on top` reads off.
- [ ] **I3 — It survives.** Set it off, Exit, relaunch. Expected: off, because that is what the file says (`topmost = no`). A state the file never learned is a state that comes back as default, and that is the honest behaviour: note it as *minor* only if the file disagreed with what you had set.
- [ ] **I4 — Full-screen.** Start a video full-screen (or `F11`). Expected: the clock is behind the video — always-on-top is above *windows*, not above exclusive full-screen. A fight here is worth a note either way.

## J. Reset position

- [ ] **J1 — From an edge.** Drag the clock until it is hard against the top-left corner of the screen, then Reset position. Expected: back to the default hang, fully visible, `clipped=false`.
- [ ] **J2 — After a re-anchor.** Re-anchor somewhere odd (E2), then Reset. Expected: `anchor_ratio 0.5000`, `drop 0.0`, and the cord length and card size restored to the file's defaults — the *display choice is left alone*, deliberately: Reset puts the clock back, not your monitor selection.
- [ ] **J3 — From a hand-broken position.** Put `anchor_ratio = 9` in the file, restart, Reset. Expected: it loads clamped to the valid band and Reset centres it; nothing off-screen at any point.
- [ ] **J4 — After a monitor change.** Hang it on the secondary display, disconnect that display (L3), let it fall back, then Reset. Expected: a safe place on the primary.
- [ ] **J5 — While hidden.** Hide the clock (F2), then Reset position from the tray. Expected: the clock comes back visible — Reset is also the recovery path when you have hidden it somewhere you cannot find it.

## K. DPI (where available)

Test at whatever subset of 100 / 125 / 150 / 200 % your display supports; use Windows Settings →
Display →
Scale, and change it with the clock visible where you can.

- [ ] **K1 — Size follows.** Expected: the card gets proportionally bigger/smaller; digits stay sharp, not stretched or blurry (a blurry card after a scale change means the surface was rescaled by DWM instead of re-painted — note it).
- [ ] **K2 — Anchor preserved.** `--diag` before and after: `ratio` and the *logical* `drop` must be identical; only the device px should change.
- [ ] **K3 — Swing survives the change.** Change scale while it is swinging. Expected: it keeps swinging; it does not freeze, go limp, or reset to level.
- [ ] **K4 — No dead clock after a change.** After each scale change, drag it once and confirm the time still ticks. `--diag`'s `state` should not be stuck.
- [ ] **K5 — Settings window scales** — same DPI, text and controls sized for it, nothing clipped at 200 %.
- [ ] **K6 — Tray unaffected.** The menu, its ticks and its submenus look normal at that scale (this is the OS's drawing, but a 200 % greyed-item or truncated-label problem is still worth noting).

## L. Multi-monitor (if available)

- [ ] **L1 — Move it between displays** with `Hang from this display` and confirm the clock lands at the equivalent spot on the other screen (top-centre of *its* work area), not at the same pixel coordinates.
- [ ] **L2 — Mixed DPI.** Re-anchor and hang on the higher-DPI display, then the other. Expected: correct size on each, and its `on monitor` + `anchor` lines agree with where it is.
- [ ] **L3 — Unplug while it hangs there** (or disable the display in Settings). Expected: it moves to a display that exists, remains fully visible, and the *saved* pair for the other monitor is not overwritten with garbage — check `--diag` then plug back in and see whether it returns sensibly. A restart here is a fair thing to try; if you do, note it.
- [ ] **L4 — Restart with the preferred display missing.** Set it to display #1, shut #1 off, restart Hanglock. Expected: a safe clock on #0, no empty window, and `clipped=false`.
- [ ] **L5 — Re-anchor across the bezel.** Alt+drag so the anchor lands on the other monitor. Expected: it goes to that monitor's hang band; it cannot straddle two screens, and `--diag` says which one it picked.
- [ ] **L6 — Taskbar edges.** If your taskbar is auto-hidden or docked left/right/top, drag near it. Expected: the clock does not hide behind it permanently, and it is still reachable.

## M. Sleep and wake

- [ ] **M1 — Sleep and resume.** Let the PC sleep for a few minutes, wake it. Expected: the clock renders immediately, shows the correct time, and does not animate furiously to catch up (`--diag` → `state Settled` within a few seconds).
- [ ] **M2 — No physics explosion.** Before sleeping, leave it mid-swing if you can (or drag it and let the PC sleep while it settles). On wake: no throw across the screen, no detached card.
- [ ] **M3 — Tray functional after wake.** Right-click the icon: menu opens, items work. If the icon vanished, that is major (and it also means the recovery route is gone: note whether the clock was still clickable).
- [ ] **M4 — Long idle.** Leave it running through an hour of ordinary work. Expected: still ~0 % CPU when settled, time still correct, no drift (compare at the end). Record the CPU figure in Notes.

## N. Start with Windows

- [ ] **N1 — Enable it** from the tray or the window, then check the registry entry:

      reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v Hanglock

  Expected: value data `"<full path to hanglock.exe>" --background`, path matching where you put
the exe.
- [ ] **N2 — Task Manager's Startup page** lists Hanglock as *Enabled*.
- [ ] **N3 — Sign out and back in** (or reboot). Expected: the clock is there, **with no console window and no flash of one**, and `--diag`'s `autostart` reads on.
- [ ] **N4 — Disable it** from the app. Expected: `reg query` reports the value not found (not an empty value), and Task Manager no longer lists it.
- [ ] **N5 — No duplicates.** Toggle it on/off/on a few times, then `reg query` once. Expected: exactly one `Hanglock` value. Also try the *other* direction: disable it in Task Manager instead, re-open the app's window, and check the checkbox shows unchecked (`StartupApproved\Run` is read, not just written).

## O. Recovery and corruption

The file is `%APPDATA%\Hanglock\settings.toml`. Close Hanglock before editing (it writes on change,
so an
edit while it runs can simply be overwritten — that is expected, not a bug).

- [ ] **O1 — Junk that is not ours.** Put `hello, this is not a config` in the file, start Hanglock. Expected: defaults on screen, the bad file renamed to `settings.toml.corrupt-1` (then `-2`…), and `--diag` saying `recovered`. You should not have lost the clock.
- [ ] **O2 — A half-written file.** Write a real file, then truncate it mid-line (delete the last few characters of `[face]`'s last line). Expected: everything that survived is still read, and the file is **not** renamed — a partial document is repaired, not confiscated. `--diag` should still show the settings you recognise.
- [ ] **O3 — An off-screen saved position.** `anchor_ratio = 9` plus `anchor_drop = -400`, start. Expected: clamped into the valid band, clock visible, no Reset needed. Then Reset position anyway (J3) and confirm it centres.
- [ ] **O4 — Escape from full click-through.** Set `Fully click-through` (§G), then use the tray to come back. Expected: one menu click. This is the "user cannot trap themselves" check; if it ever takes more than the tray, blocker.
- [ ] **O5 — A file that will not write.** `attrib +R "$env:APPDATA\Hanglock\settings.toml"` (create it first if you have none), change a setting in the window. Expected: the clock keeps running and stays correct for the session, no crash, no error dialog; the read-only file keeps its old contents. After `attrib -R`, restart and check with `--diag` whether the change is in the file or not — and that the app never *pretended* it saved.

## P. Exit and restart

- [ ] **P1 — Exit exits.** Tray → Exit Hanglock. Then: `Get-Process hanglock -ErrorAction SilentlyContinue` returns nothing, and no window, tray icon or phantom drag region is left where the clock was.
- [ ] **P2 — No orphan.** Nothing named `hanglock` or `conhost` belonging to it survives; no `*.new` / `*.corrupt-*` files left in `%APPDATA%\Hanglock` from a clean exit.
- [ ] **P3 — Settings persisted.** Before exiting, note `--diag`'s anchor/clock/mouse/posture/hang lines; after relaunching they are the same, and `as read` says `loaded`.
- [ ] **P4 — First-run-only file rule still holds.** After a *changed* setting the file exists, and it is human-readable (`type $env:APPDATA\Hanglock\settings.toml`): sections `[overlay] [face] [general]`, plain numbers, no binary junk, and a `schema` line.
- [ ] **P5 — Kill it instead.** End `hanglock.exe` in Task Manager (a crash rehearsal), then start it again. Expected: the last saved state comes back, and nothing was quarantined because the file was left mid-write (worst case after a hard kill is a truncated file, which O2 says how it behaves).

## Q. Reporting a failure

Copy this per finding — into an issue, a file, or the chat, whichever you are using:

```text
Title:
Commit:        (git rev-parse --short HEAD, or the run's head sha; 018bb26 or later)
Windows:       (e.g. 11 23H2, build from `winver`)
Architecture:  x64
DPI:           (scale %, and the display's resolution)
Monitor setup: (count, resolutions, which is primary, taskbar docked where)
Steps:
Expected:
Actual:
Severity:      blocker / major / minor / cosmetic
Diagnostics:   (paste `--diag` output — it is safe to paste: the settings path is printed unexpanded)
Screenshot:
```

Two things make a report immediately usable: whether it happens **every time or sometimes**, and what
`--diag` said **while it was misbehaving**. If you can add the `scene.png` from the artifact next to a
`--dump-scene` you took yourself, the "is it the painter or the window" question is already answered.
