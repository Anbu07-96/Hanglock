# Hanglock — documentation index

**What this is.** A lightweight Windows utility that hangs a working clock (later: timer, stopwatch)
from the top edge of the desktop on a real rope — draggable, swingable, click-through everywhere it
isn't drawn, and near-zero cost while you ignore it.

**Status.** Phase 1 delivered the first prototype: the workspace, the solver, the painter, the Win32
overlay, the tray, the settings store, and ~47 tests. It has **not been compiled** — this build
environment has no Rust toolchain and no crates.io ([why](decisions/0002-zero-dependencies.md)) — so
read [`reports/phase-1.md`](reports/phase-1.md) before building it. The physics and the look were
measured and reviewed in a runnable reference model, and a golden trace keeps the shipped solver honest
to it.

| Doc | What it answers |
|---|---|
| [mvp.md](mvp.md) | Exactly what ships first, in what order, and the gate each step must pass |
| [architecture.md](architecture.md) | Crates, data flow, window contract, physics design, settings, budgets, tests, seams |
| [roadmap.md](roadmap.md) | Phases 0–7 and what is deliberately never |
| [decisions/0001-technology-stack.md](decisions/0001-technology-stack.md) | Why Rust + Win32 + our own compositor; every alternative scored |
| [research/hangly-inspection.md](research/hangly-inspection.md) | What we learned from the reference project (concepts, measurements, its own bug stories) |
| [research/ip-boundaries.md](research/ip-boundaries.md) | What must not be copied: artwork, branding, prose, translated code |
| [ideas/dual-cord-mount.md](ideas/dual-cord-mount.md) | The one physics idea that makes Hanglock Hanglock: a plaque on two strands |
| [physics.md](physics.md) | The solver as built: step order, why it cannot stretch, why it settles, why a throw still carries |
| [gate-a.md](gate-a.md) | The measurement gate: what was measured, the cost geometry, what is owed on hardware |
| [windows-overlay-notes.md](windows-overlay-notes.md) | The Win32 reality: layered windows, per-pixel hit testing, DPI, the tray, and a verify list |
| [reports/phase-1.md](reports/phase-1.md) | What the first prototype is, what it does not do yet, and what to review |


| `PRIVACY.md` *(with 0.1)* | No network, no telemetry, one settings file — with the command that proves it |

## Reading order for a new contributor

1. `reports/phase-1.md` — what exists, and the one caveat that matters.
2. `mvp.md` §1 (in/out) — so you know what not to build yet.
2. `architecture.md` §1–§3 — the rules and the one-direction data flow.
3. `research/ip-boundaries.md` §1 — the short list of things that will get the repo into trouble.
4. Pick a task from `mvp.md` §3 or `roadmap.md`, and open a draft PR before writing code.

## Conventions

* One `docs/decisions/NNNN-*.md` per real decision, with the alternatives and why they lost. A
  decision that isn't written down will be re-litigated every month.
* Every claim about cost carries a measurement command. "It's fast" is not documentation.
* Every tunable lives in `core::rope::config` or `settings`, never inline in a solver or painter.
* Comments explain *why*; a physics or budget change lands with a test that would fail without it.
* No artwork without a provenance line in `assets/SOURCES.md`.
