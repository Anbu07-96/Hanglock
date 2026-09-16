# ADR-0002 — no third-party dependencies (a deviation forced by the build box, then kept)

* Status: accepted, pending review
* Date: 2026-09-16
* Supersedes part of: [ADR-0001](0001-technology-stack.md) (which named `windows`, `tiny-skia`, `cosmic-text`, `serde` + `toml`)

## What happened

ADR-0001 assumed crates.io. It is not reachable from this project's build environment: only
`github.com` answers, so `cargo build` cannot fetch a single crate, and there is no Rust toolchain
installed to begin with. Implementing Phase 1 therefore meant either stopping or writing the
dependencies' parts of the job ourselves. We did the latter, and the result is worth keeping, so this
ADR records both halves honestly: what was forced, and what turned out to be a good idea.

## What changed in the design

| ADR-0001 said | Now | Why it is fine, or what it costs |
|---|---|---|
| `windows` crate for Win32 | `crates/hanglock-win/src/sys.rs`: ~40 hand-declared imports, ~12 `#[repr(C)]` structs, each with a compile-time `size_of` assertion | Costs: verbosity, and the risk that a field order typo compiles and then misbehaves. Mitigated by the asserts (a wrong layout is a build failure) and by keeping the surface to what four files actually call. **Adopting the `windows` crate later is still recommended** and is a contained swap inside `hanglock-win`. |
| `tiny-skia` for geometry | `hanglock-render`: analytic signed-distance coverage for capsules, rounded boxes, annuli and a soft shadow | Costs: no perspective/clip machinery, no gradient stops beyond what we wrote. Buys: the cord, the plate, the rim, the shadow and the *glyphs* are all one primitive (a distance and a ramp), ~450 lines total, and no per-frame filter is even possible. |
| `cosmic-text` for text | The face is authored as stroke outlines in `tools/model/hanglock_ref.py` and generated into `face_data.rs` | This is the biggest real deviation, and it has an upside: the face is original geometry with no licence, no subpixel-positioning surprises, tabular by construction, and it renders identically at any DPI. The cost is that only the glyphs we drew exist (digits, `:`, `.`, `A M P S T -`), so Phase 3's labels need more glyphs authored — and that is a chore, not a redesign. |
| `serde` + `toml` | A 200-line parser for the subset of TOML this file needs | Costs: no nested tables, no arrays. Keeps: the file on disk is still valid TOML, so switching to `toml` later is a swap in one module and no user migration. Tolerant-decode rules (missing keys → default, unknown keys → ignore, clamp ranges, quarantine unreadable) are ours and tested. |
| — | `hanglock-render::png`: an uncompressed-deflate PNG writer | Only for `--dump-scene`, so the renderer's real output can be looked at on a machine with no image library. That is how the visual design in this phase got reviewed at all. |

## Why keep it, rather than add the crates back

1. The budget in `docs/architecture.md` §8 is easier to hold with nothing else in the binary:
   the release executable target is ≤ 2 MB, and there is no `Cargo.lock` supply chain to audit.
2. A one-maintainer OSS utility with zero dependencies has no dependency-bump churn, no MSRV
   conflicts with three different crates, and no `cargo deny` policy to write.
3. Everything we would have used a crate for is now understood well enough to debug, which matters
   more for the risky parts (layered windows, hit testing, DPI) than for the fun parts.

## Standing decisions

* Add the `windows` crate when the FFI layer grows past what a person will review comfortably — the
  trigger is "a third file needs new imports", not "it would be nicer".
* Do not add an image, font or serialization crate for convenience. If a feature needs one (e.g.
  importing a wallpaper), write that ADR first.
* Keep `tools/model/hanglock_ref.py` runnable and its trace golden-tested, because it is now load-bearing:
  it is the only executable specification of the physics the shipped solver claims to implement.
