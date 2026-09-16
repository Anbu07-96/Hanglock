# Contributing

Hanglock is a small utility with a narrow set of things it must be good at: hanging believably,
staying out of the way, and costing nothing while nobody looks at it. Most rejected pull requests here
will be about scope, not quality.

## Read these first

1. [`docs/mvp.md`](docs/mvp.md) §1 — in and out of the current phase, so you know what not to build.
2. [`docs/research/ip-boundaries.md`](docs/research/ip-boundaries.md) §1 — the short list of things
   that would get the repository into trouble. Summary: **no artwork, no branding, no prose, no
   translated code** from the reference project. Its documented algorithms and trade-offs are the
   import.
3. [`docs/architecture.md`](docs/architecture.md) §1–§3 — the rules and the one-directional data flow.
4. [`docs/physics.md`](docs/physics.md) and [`docs/windows-overlay-notes.md`](docs/windows-overlay-notes.md),
   for the two parts where the details matter most.

Open an issue before writing code for anything larger than a bug fix. A physics change, a new setting,
or a new window behaviour is a design change and deserves five minutes of agreement first.

## House style

* `hanglock-core` and `hanglock-render` are `#![forbid(unsafe_code)]` and dependency-free. If a change
  needs a dependency, that is [ADR-0002](docs/decisions/0002-zero-dependencies.md) territory: say why
  in the PR, and expect the answer to be "put it in `hanglock-win`" or "write the 60 lines".
* All `unsafe` lives in `crates/hanglock-win` (`sys.rs` declares, `window.rs` and `surface.rs` call).
  Every `#[repr(C)]` struct there carries a `size_of` assertion.
* Value types hold state; one funnel per side effect; no singletons — services are built once in
  `apps/hanglock` and passed down.
* Comments explain *why*, and cite the measurement or the failure that motivated them. A comment that
  restates the code is noise; a comment that says "two earlier versions of this were wrong, here is
  how" is the most useful line in the file.
* Tunables live in `rope/config.rs`, `theme.rs`, or settings — never inline in a solver or a painter.
* Every physics or budget claim in a PR description carries the command that measures it.

## Gates

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                    # runs on Linux too; the solver does not need a desktop
python3 tools/model/hanglock_ref.py metrics   # if you touched the physics
python3 tools/model/hanglock_ref.py trace tests/golden/trace_drag_settle.json
python3 tools/model/gen_face.py > crates/hanglock-render/src/face_data.rs   # if you touched the face
```

The golden trace test is not decoration: it is what keeps the shipped solver honest against the model
the feel was reviewed in. If it fails after a deliberate change, re-review the feel (`--dump-scene`,
and run it), then regenerate the trace — never widen the tolerance.

A change to how anything is presented or scheduled needs the numbers from
[`scripts/gate-a.ps1`](scripts/gate-a.ps1) in the PR, even if they miss the budget. A miss with a
written reason ships; an unmeasured change does not.

## Reading a run without the web UI

A red step is worth more than a red tick, so CI ships the captured text of the steps that can fail —
`tools/ci/run-and-annotate.sh` for the shell ones, an `emit.py` call for the PowerShell ones — as gzip
chunks in the run's annotations. `tools/ci/fetch-logs.py` puts them back into files:

```sh
python3 tools/ci/fetch-logs.py 35160471461 Linux    # -> /tmp/decoded_cargo_test.txt, one per stream
```

The reverse channel exists for the gates that *pass* and print a number worth keeping (the bench, the
size gate): `tools/ci/notice.py` folds the tail of a captured log into a single `::notice` annotation,
so the measurement lives in the run's record instead of only in its log. A notice cannot change a verdict.

## Pull request template

Keep it short and honest: what changed, what it costs (measured), what you did not test and why.
