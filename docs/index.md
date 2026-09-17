# Hanglock — documentation index

**What this is.** A lightweight Windows utility that hangs a working clock (later: timer, stopwatch)
from the top edge of the desktop on a real rope — draggable, swingable, click-through everywhere it
isn't drawn, and near-zero cost while you ignore it.

**Status.** Phase 2.5 is open: the code is finished and green, and it is waiting to be *used* — the
manual checklist for a real Windows desktop is [`testing/windows-desktop-validation.md`](testing/windows-desktop-validation.md)
(108 checks, with the artifact instructions and a bug template in it). Phase 1 delivered the first prototype: the workspace, the solver, the painter, the Win32
overlay, the tray, the settings store. Phase 2 made it usable: the hang point is now a pair of numbers
the file can hold and Alt+drag can move, the mouse has three named modes with a refusal built into one of
them, the tray grew into the control surface, and a small native settings window draws the rows the
model derives. There are 111 `#[test]` functions in the workspace and no doc tests — counted with
`git ls-files '*.rs' | xargs grep -c '^[[:space:]]*#\[test\]'` rather than remembered, and CI now
publishes what it ran: `unit 104 passed` on Linux and `unit 111 passed` on each Windows ABI, the difference
being the seven tests that only compile under `cfg(windows)`. Every number in these pages is measured off the
tree or off a CI run for the same reason; the sentence here that once claimed "10 doc tests" was neither, and
`docs/reports/phase-2.md` §8 says so out loud instead of correcting itself quietly.

CI's three jobs cover Linux, Windows `x64` and Windows `arm64`: fmt and clippy `-D warnings`, the suite
including the golden trace, both generated artefacts against their generators, and on Windows the release
link, a headless run of the binary and a 2 048 KB size gate. The gate was met at 312 KB (`x64`) /
271 KB (`arm64`) by `9dcf3b9`, Phase 1's green run, and Phase 2's code moved that to 355 KB / 308 KB at
`dbf9cba`, where all three jobs are green and
[`reports/phase-2.md`](reports/phase-2.md) carries the numbers a run measured. The
zero-dependency rule that keeps the sizes cheap to hold is
[ADR-0002](decisions/0002-zero-dependencies.md). Anything that needs a desktop is owed, and listed as
owed in [`gate-a.md`](gate-a.md); read [`reports/phase-1.md`](reports/phase-1.md) and
[`reports/phase-2.md`](reports/phase-2.md) for the phases as a whole. The physics and the look were measured and reviewed in a runnable reference model, and a golden
trace keeps the shipped solver honest to it.

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
| [reports/phase-2.md](reports/phase-2.md) | What became usable, what was refused, and what is still owed on a desktop |


| `PRIVACY.md` *(with 0.1)* | No network, no telemetry, one settings file — with the command that proves it |

## Reading order for a new contributor

1. `reports/phase-2.md` — what exists now, and the one caveat that matters.
2. `mvp.md` §1 (in/out) — so you know what not to build yet.
3. `architecture.md` §1–§3 — the rules and the one-direction data flow.
4. `research/ip-boundaries.md` §1 — the short list of things that will get the repo into trouble.
5. Pick a task from `mvp.md` §3 or `roadmap.md`, and open a draft PR before writing code.

## Conventions

* One `docs/decisions/NNNN-*.md` per real decision, with the alternatives and why they lost. A
  decision that isn't written down will be re-litigated every month.
* Every claim about cost carries a measurement command. "It's fast" is not documentation.
* Every tunable lives in `core::rope::config` or `settings`, never inline in a solver or painter.
* Comments explain *why*; a physics or budget change lands with a test that would fail without it.
* No artwork without a provenance line in `assets/SOURCES.md`.
