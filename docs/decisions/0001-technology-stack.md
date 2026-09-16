# ADR-0001 — Technology stack for Hanglock

* Status: proposed (needs owner sign-off before Phase 1)
* Date: 2026-09-15
* Decides: language, windowing, rendering, packaging for a Windows-first always-on-top hanging clock

---

## 1. The requirement, restated as constraints

Hanglock is not "an app with a window". It is **a permanent object on the user's desktop that must
behave like part of the OS**. That gives four hard constraints and six preferences:

**Hard**
1. Borderless, frameless, non-activating top-level window with true per-pixel transparency.
2. Above normal and most floating windows, below system UI; must survive app switching, full-screen
   games (best effort), lock screen, sleep/resume.
3. Mouse input only where the object is drawn; every other pixel must pass clicks through to
   whatever is underneath — *continuously*, including while the object is mid-swing over a
   spreadsheet.
4. Must cost ~nothing while the user is not touching it, because it runs 16 h/day on a workstation.

**Preferences**
5. Smooth physics at 60–144 Hz while interacting.
6. Native-looking text and vector rendering (this is a clock: the typography *is* the UI).
7. Reliable on Windows 10 + 11, x64, at 100/125/150/175/200 % display scaling, 1–3 monitors, Intel/
   AMD/NVIDIA/whatever-iGPU drivers, RDP and VMs.
8. Small download; no runtime install step; no admin.
9. Trivial for strangers to build and test on GitHub Actions with one `cargo`/`dotnet` command.
10. Architecture that can grow a macOS backend later without a rewrite.

## 2. Options evaluated

| # | Stack | Idle CPU | Idle RAM | Installer | Transparency | Per-pixel click-through | Physics/render control | macOS later | OSS dev | Verdict |
|---|---|---|---|---|---|---|---|---|---|---|
| A | **Rust + Win32 (`windows` crate) + own compositor** | ~0 % (message loop + 1 Hz timer, no frame clock when asleep) | **10–25 MB** | **2–4 MB** | `UpdateLayeredWindow`, premultiplied BGRA | **free**: layered windows hit-test per pixel; alpha 0 lets clicks through; `WM_NCHITTEST` for explicit control | Total | Same core crate + new backend | `cargo build/test` on `windows-latest`, no runtime | **Recommended** |
| B | **C# / .NET + WPF** | ~0 % when idle (DispatcherTimer at 1 Hz) | 60–120 MB | 70–130 MB self-contained, or 2 MB + runtime | `AllowsTransparency=true` → the same layered-window path internally | Same per-pixel behaviour (WPF relies on it for popups) | High (custom `DrawingVisual`/`WriteableBitmap`) | No (WPF is Windows-only; would need an Avalonia/MAUI rewrite) | Excellent tooling, huge C# contributor pool | **Plan B** |
| C | C# / .NET + **Avalonia** | ~0 % when idle | 90–160 MB | 60–120 MB | `TransparencyLevelHint=Transparent`; historically broken in software-render mode until 11.1, and a known first-frames-transparent-while-empty flash (#19787) | Not from the framework: needs `WS_EX_TRANSPARENT` (whole window) or P/Invoke `WM_NCHITTEST` interop | Medium-high | **Yes, one codebase** | Good | Cross-platform tiebreak; heavier and more transparent-window quirks than A |
| D | **Tauri v2** (Rust + WebView2) | Needs a ~50–60 ms cursor-poll loop to fake per-region click-through (no per-region API) | 40–150 MB (webview; documented OS-driven variance: one 2026 field report measured 110 MB on macOS Tahoe vs 29 MB on Sequoia for identical code) | 3–10 MB (WebView2 evergreen on Win10/11) | Works, with caveats | `setIgnoreCursorEvents` is **whole-window only** → poll + toggle | Low: DOM/CSS animation, and per-frame IPC if physics crosses the boundary | Yes | Very good docs, big community | **Rejected for this app**: it is, literally, a webpage floating on the desktop |
| E | **Electron** | Chromium frames + poll loop | 200–350 MB | 80–120 MB | Yes | Whole-window only | Low | Yes | Fine | Rejected: fails 1, 2, 4, 8 outright |
| F | **Flutter desktop** | Reasonable | 90–180 MB | 30–60 MB | **No first-class desktop transparency.** Windows support has never landed in the embedder; Linux regressed to opaque-black in 3.22 (flutter/flutter#152154) and the macOS fix lives in a third-party plugin (leanflutter/window_manager#293), with workarounds asking apps to draw an overlay themselves (flutter/flutter#96732) | No | Medium | Yes | OK | Rejected: the one hard requirement is the one it lacks |
| G | C++20 + Win32 + Direct2D/DirectWrite | ~0 % | 8–20 MB | 1–3 MB | Same ULW or DComp path | Same | Total | Yes | Weakest: no dependency/build story that strangers like; memory-safety risk in a hand-written message loop | Rejected **for this project**, not for the domain: same footprint as A, materially higher maintenance and contributor risk |
| H | Qt 6 (C++ or PySide) / wxWidgets / Slint / egui | Qt good, others fine | 40–120 MB | 40–80 MB (Qt) | Qt supports translucent frameless windows; Slint/egui transparency on Windows is via `WS_EX_LAYERED`-style hacks | Whole-window `Qt::WindowTransparentForInput` only; per-pixel needs `SetWindowRgn` (kills AA) | Medium | Yes | LGPL compliance + `windeployqt` bloat | Rejected: platform abstraction priced in, without buying us anything we need |
| I | Python (PySide/Tk) + PyInstaller | Poor | 60–150 MB + interpreter | 25–60 MB | Via Qt | Via Qt | Low | Yes | Fast to prototype | Rejected: 16 h/day resident app in a 90 MB interpreter with per-frame Python physics. Prototype only |

Notes on the matrix: idle-CPU entries assume the same architecture in every stack (advance physics
only while moving; refresh text at 1 Hz). Where a stack *forces* a poll loop (D), that shows up in
the idle column, and that is the whole argument against it here.

## 3. Decision

**Build Hanglock in Rust on top of the `windows` crate, with our own frame loop and a
software-composited, premultiplied-alpha surface presented through `UpdateLayeredWindow`.**

Concretely:

| Concern | Choice | Notes |
|---|---|---|
| Language | Rust, edition 2024, MSRV pinned in `rust-toolchain.toml` | No `unsafe` outside `platform/win` (enforced by `#![forbid(unsafe_code)]` in every other crate) |
| Window | `windows` crate (0.6x line), direct Win32: `WS_POPUP` + `WS_EX_LAYERED \| WS_EX_TOPMOST \| WS_EX_TOOLWINDOW \| WS_EX_NOACTIVATE` | A hand-written `WndProc` is the *feature*: it is 300 lines that we own, versus a framework's opinion about what a window is |
| Transparency | `UpdateLayeredWindow` + `AC_SRC_ALPHA`, 32 bpp premultiplied BGRA DIB section | Per-pixel alpha, no GPU dependency, no device-lost handling, works on RDP/VM/broken drivers. Partial updates via `pUpdateRect` |
| Hit testing | Automatic (alpha-0 pixels let clicks through) **plus** explicit `WM_NCHITTEST` → `HTCLIENT`/`HTTRANSPARENT` | Documented layered-window behaviour; the explicit handler is what makes it deterministic and independent of anti-aliased edge pixels |
| Rendering | `tiny-skia` for geometry (cord strokes, rounded card, gradients) + `cosmic-text` (or `fontdue`) with a glyph atlas for digits | ~0.1–0.4 ms per frame at our sizes; everything cached except the digits |
| Typography | Bundled OFL font, tabular figures, medium+ weight, ≥28 logical px | **Grayscale AA only** — see §4 |
| Frame clock | `CreateWaitableTimerExW` (waitable timer) at 60/120 Hz **while awake**, 1 Hz while settled, none while hidden | A clock's steady state is one small dirty rect per second |
| Tray | `Shell_NotifyIconW` on a message-only window (`HWND_MESSAGE`) | Menu is the MVP settings surface |
| Settings | `%APPDATA%\Hanglock\settings.toml` (serde + `#[serde(default)]`, versioned, tolerant) | Port of Hangly's best lesson; see `../architecture.md` §6 |
| Autostart | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, reconciled at startup | No admin, no service |
| Tests | `cargo test` on any OS for `core`/`render`; a `windows-latest` smoke job for the platform layer | Physics is pure math — it does not need Windows to test |
| CI | `fmt`, `clippy -D warnings`, `cargo test`, `cargo build --release` for `x86_64-pc-windows-msvc` + `aarch64-pc-windows-msvc`, NSIS/Inno installer artifact | Free on GitHub-hosted runners |
| Installer | Inno Setup 6 `.exe` (per-user, no admin) + winget manifest in a follow-up PR | MSIX only if we later want Store/enterprise deployment |
| Signing | Start unsigned and honest (README + FAQ), then **SignPath Foundation** (free for eligible OSS, SmartScreen-trusted) or Azure Artifact Signing (~$10/mo, US/CA/EU/UK) | See §5; this is the Windows analogue of Hangly's Gatekeeper note |

## 4. Constraints this decision imposes on the *design* (accepted up front)

1. **No ClearType on a transparent surface.** D2D/DirectWrite fall back from ClearType to grayscale
   antialiasing whenever the render target carries alpha, and WPF disables ClearType on layered
   windows for the same reason (blending subpixel masks against an unknown background is incorrect).
   Both plans have this limit, so it must be designed for, not worked around: digits at medium or
   heavier weight, ≥28 px, hinting-free glyph raster, no hairline strokes for anything load-bearing,
   and a face chosen for legibility under grayscale AA.
2. **`UpdateLayeredWindow` copies the surface per present.** Cost scales with window area × rate.
   So: keep the window tight around the swept area, present the digits' dirty rect when only the
   time changed, and prefer 60 Hz for animation with an opt-in higher rate. Budget: a 420×320
   logical window at 150 % = 630×480 px ≈ 1.2 MB, so a full present at 60 Hz ≈ 72 MB/s of copies —
   acceptable; the same trick at 740×420 (Hangly's size) at 120 Hz is the way to lose the
   performance argument.
3. **Any pixel with alpha > 0 steals the click.** An outer drop shadow that bleeds past the card
   makes a 2–4 px halo clickable. Accepted (a 3 px halo around a clock is harmless); if it ever
   matters, the fix is to keep decorative bleed inside the hit rect or split a paint window from a
   hit window.
4. **A `WS_EX_TOPMOST` window is still beaten by exclusive-fullscreen games**, and by nothing else
   unusual. `SetWindowPos` re-asserted on `WM_WINDOWPOSCHANGING` covers the common cases. Documented
   as a known limitation, not silently promised.
5. **Per-Monitor V2 DPI awareness is mandatory** (application manifest), or every coordinate in the
   physics solver is wrong on half the installed base. Non-negotiable, and a Phase-1 exit test.

## 5. Windows distribution reality (2026)

| Step | Detail |
|---|---|
| Unsigned `.exe` today | SmartScreen "Windows protected your PC" on first download. Mitigation: honest README/FAQ wording (the Hangly playbook), SHA-256 published next to the asset, "More info → Run anyway" screenshot |
| Free reputation for OSS | **SignPath Foundation** signs eligible public-repo OSS releases with a Sectigo OV cert (publisher shows as SignPath Foundation). Costs nothing; onboarding is slower and requires MFA on every maintainer account |
| Paid | Azure Artifact Signing (formerly Trusted Signing) ≈ $9.99/mo, no hardware token, official GitHub Action, key never leaves Microsoft's HSM; individuals only in US/CA, orgs in US/CA/EU/UK. The user is in Texas, so this route is open |
| Not worth it | EV certificates ($400+/yr) no longer bypass SmartScreen since the 2024 change; Sigstore/cosign signatures are not trusted for Authenticode |
| Antivirus | Unsigned Rust binaries sometimes trip heuristic flags → submit samples to Defender for non-detection, publish reproducible build steps |
| Channels | GitHub Releases (primary), winget-community manifest, Scoop bucket; *not* the Microsoft Store for the MVP (packaging + sandbox friction for a tray utility) |

## 6. Why not the two "faster" paths

**Why not Tauri, even though it is the trendy answer.** It would give us HTML/CSS clocks (exactly
the styling surface a "many clock faces, many themes" roadmap wants) and a 3 MB installer, and
2026 field reports for transparent always-on-top overlays are otherwise flattering. It still fails
the product on three counts: (a) per-region click-through does not exist, so we must run a 50–60 ms
cursor-polling loop forever, which contradicts priority 1 for an app that lives on the desktop all
day; (b) the webview's idle memory is not ours to control and is documented to swing hard with OS
releases (110 MB vs 29 MB for identical code across two macOS versions); (c) the frame boundary we
most want tight — *physics → pixels, every 8 ms* — is exactly where a JS/DOM or IPC indirection
costs. The "feels like a webpage on the desktop" objection in the brief is not aesthetic snobbery;
it is these three problems wearing a hat.

**Why not WPF, which would be quicker.** Because "quicker" buys the first two weeks and pays for
them forever: 4–8× the memory, 20–40× the installer, a Windows-only ceiling on the macOS ambition,
and a per-app renderer we can only tune by proxy. Plan B stays on the table as a *documented
fallback* — if the Phase-1 spike shows we cannot get a clean, cheap, non-activating transparent
window, or if maintaining this alone proves too slow, we re-decide with data (the physics model,
settings schema, interaction grammar and test suite carry over; only the platform crate changes, and
in C# the translation is 1:1 because WPF uses the same layered-window mechanism underneath).

## 7. Decision in one line

Rust + Win32 + our own compositor buys the two numbers the product is judged on — **~0 % idle CPU
and ~15 MB idle RAM** — for a bounded ~600 lines of platform code we own, and keeps the physics,
model and settings layers pure and testable on any OS, which is also what makes a later macOS
backend additive rather than a rewrite.
