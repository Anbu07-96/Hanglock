# Hanglock

**A working clock, hanging from the top of your screen.**

Not a widget. Not a webpage in a frameless window. A digital clock on the end of a rope, attached to
the top edge of your desktop: drag it, throw it, watch it swing and settle, and read the time while it
does. It is a productivity utility that happens to hang — clock now, timer and stopwatch next — and
it is designed to cost about nothing while you ignore it.

```
 screen top
 ─────────────────────────────────────
        │
        │  cord (real Verlet physics, 240 Hz)
        │
      ┌─┴──────────────┐
      │    10:42 PM    │   ← you can grab this
      └────────────────┘
```

**Status: the first prototype compiles, passes its tests and produces its binary in CI.** Every gate this
repo has is green, across three jobs — Linux, Windows `x64`, Windows `arm64`: fmt and clippy
`-D warnings`, 60 tests including a golden trace against the reference model, the generated artefacts
checked against their generators, and on Windows the release link, a headless run of the built executable
and a 2 048 KB size gate it meets at 312 KB (`x64`) / 271 KB (`arm64`). What CI cannot measure — a
desktop, a tray, a real DPI change, idle CPU and RAM — is stated as owed in
[docs/gate-a.md](docs/gate-a.md); the phase's record is [docs/reports/phase-1.md](docs/reports/phase-1.md).

<img src="docs/previews/01-settled-light.png" width="440" alt="Hanglock's clock plate hanging from a ceiling clamp on a twisted cord, reading 10:42 PM"> <img src="docs/previews/03-settled-dark.png" width="440" alt="The same clock on a dark desktop wallpaper, kept legible by a one-pixel rim light">

<sub>Rendered by the reference model in `tools/model/`, with the same constants and painter logic as
`hanglock-render` — design previews, not screenshots of the Windows build.</sub>

```sh
cargo build --release -p hanglock          # Windows: the overlay
cargo run -p hanglock -- --dump-scene a.png # any OS: the frame it would paint
cargo run -p hanglock -- --bench 400        # any OS: per-frame cost
cargo test --workspace                      # any OS: the solver, placement, settings
```

## What it will be

* **Hanging, physically.** A fixed-timestep Verlet rope with a stretch ceiling and momentum that
  survives release. Not a looping animation, because the eye can tell.
* **Useful.** Clock → Timer → Stopwatch, on the same object, with no window to open.
* **Polite.** Click-through everywhere it isn't drawn, never steals focus, and it stops simulating
  when it settles: one small repaint per second, not a frame clock.
* **Native.** Windows 10/11 first, `x64` + `arm64`, per-monitor DPI, tray-resident, no installer bloat.

## Planned MVP

One hanging digital clock + real rope physics + drag/swing + an Always-on-Top toggle + persisted
settings. Nothing else — faces, themes, rope styles, timer, stopwatch and analog are specified for
later phases but not built yet. See [docs/mvp.md](docs/mvp.md).

## Stack

Rust, on the Win32 API directly: `WS_POPUP` + `WS_EX_LAYERED|TOPMOST|TOOLWINDOW|NOACTIVATE`, a
self-owned software compositor (`hanglock-render`: analytic signed-distance coverage for the cord, the
plate, the rim and the shadow, with the digits drawn from generated geometry rather than a font file)
presented with `UpdateLayeredWindow`, and per-pixel hit testing so transparent pixels pass clicks through
for free. No third-party crates, in the build or in the binary
([ADR-0002](docs/decisions/0002-zero-dependencies.md)). Target: **~0 % idle CPU,
~15 MB idle RAM, ≤ 4 MB installer.** Every alternative (Tauri, Electron, Flutter, WPF, Avalonia, Qt)
is scored against those numbers in
[docs/decisions/0001-technology-stack.md](docs/decisions/0001-technology-stack.md).

## Documentation

Start at [docs/index.md](docs/index.md) — architecture, MVP plan, roadmap, the stack decision, the
prior-art inspection it grew out of, and the boundaries that keep this project's own work its own.

| | |
|---|---|
| [docs/mvp.md](docs/mvp.md) | What ships first, in what order, with the gate each step must pass |
| [docs/architecture.md](docs/architecture.md) | Crates, data flow, window contract, physics, settings, budgets |
| [docs/roadmap.md](docs/roadmap.md) | Phases 0–7, and what is deliberately never |
| [docs/physics.md](docs/physics.md) | The solver: why it cannot stretch, why it settles, why a throw carries |
| [docs/gate-a.md](docs/gate-a.md) | Measured results and the numbers still owed on hardware |
| [docs/decisions/](docs/decisions/) | One file per real decision, with the alternatives that lost |
| [docs/research/](docs/research/) | Findings from the reference project, and what we won't copy |

## Principles

1. The physics and the model must not know what an `HWND` is. If they do, the boundary is wrong.
2. Nothing runs unless something changed. A clock's steady state is one repaint per second.
3. Legibility beats realism — and we say which trade-off we made, and why, in the docs.
4. Every performance claim ships with the command that measures it.
5. Original art, original code, own name. Ideas and published trade-offs are the import.

## Contributing

Open an issue before writing code for anything larger than a bug fix. Read
[docs/index.md](docs/index.md) → *Reading order* first; it takes ten minutes and prevents most
rejected PRs. Help wanted, specifically: Win32 overlay/DPI know-how, type design for a face that has
to survive grayscale antialiasing, and someone who enjoys writing physics tests.

## License

To be declared at the end of Phase 0. Proposed: MIT for code, OFL fonts bundled unmodified, our own
icon and wordmark released permissively so forks carry no legal ambiguity — with the rationale in an
ADR, since the reference project takes the opposite stance on its artwork
([docs/research/ip-boundaries.md](docs/research/ip-boundaries.md) §4).

Privacy: no network code, no telemetry, settings in one file under `%APPDATA%\Hanglock`. That is a
design constraint, not a marketing line — it will be verifiable from the binary's imports.
