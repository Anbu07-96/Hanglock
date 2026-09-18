# IP and design boundaries: what Hanglock must not copy

Hangly is a reference implementation, not a source to fork. This file is the rule list; it is
deliberately blunt because "don't blindly clone" is easy to agree with and easy to violate by
accident six weeks later when someone is porting a function.

## 1. Absolutely not

| Asset | Status | Rule |
|---|---|---|
| The eleven collection charm SVGs shipped from `Assets/Charms/` (12 files in the folder; `Emoji.svg` is not in the catalogue) | © SharanCreatedThis, all rights reserved (explicit carve-out in their LICENSE) | Do not read, trace, redraw, "get inspired by pixel-by-pixel", or bundle. Hanglock ships no charm artwork in MVP anyway |
| `Assets/Branding/Charms.ai`, app icon PNGs, `Assets/Icons/*` | Reserved | No reuse, no derivative. Commission/generate our own, keep provenance notes in `assets/SOURCES.md` |
| The name **Hangly**, its wordmark and its iconography | Reserved | Our name is Hanglock. Do not use "Hangly" in our UI, screenshots, descriptions, or as a "based on Hangly" subtitle. Mentioning it in `docs/research/` as prior art is fine and honest |
| Their `Assets/Screenshots/*` and DMG background | Reserved | Our screenshots must be captured from our own build |
| `CharmLibrary.json` strings and any prose in their README/Docs/FAQ/release notes | Their writing | Paraphrase the engineering *concepts* (as in `hangly-inspection.md`), never the sentences. No translated/renumbered copies of their doc sections |
| `Hangly/Models/Charms/*` — the charm catalogue (daruma, nazar boncuğu, maneki-neko, hamsa, himmeli, scarab, …) | Their creative selection + cultural artwork | Hanglock is not a charm app. Do not port the catalogue, the "protective charms of the world" theme, or the per-charm names/lore |

## 2. Grey zone: allowed with attribution, but we choose not to

Their source code is MIT. Legally, Hanglock could take `RopeSimulation.swift`, translate it to
Rust, keep the copyright notice, and ship. We are not doing that, because:

1. The brief is a different product with a different window system, renderer and payload.
2. A Swift→Rust line-by-line translation would inherit macOS-shaped decisions (y-down canvas,
   polling for hover, a single point-mass payload) that are wrong for us.
3. It would make the project look like a port, which costs credibility for an open-source utility
   and makes upstream changes unmaintainable.

**Rule for contributors:** implement from the *algorithms and trade-offs* (uncopyrightable ideas,
and standard numerical methods — Verlet integration and Gauss-Seidel relaxation are textbook), and
write our own code. If a future PR does port an MIT-licensed implementation, it must (a) say so in
the commit message, (b) add the upstream copyright notice to the file header, and (c) add an entry
to `THIRD-PARTY-NOTICES.md`. Do not let that become the default.

## 3. Ideas we freely take (not protected, and useful)

* Fixed-timestep Verlet rope with a clamped accumulator.
* Gauss-Seidel relaxation with an early-exit tolerance and a pass cap above the node count.
* One-sided stretch ceiling applied as a projection shared between both node ends.
* Clamping the drag target to the reachable circle rather than letting the solver eat an
  impossible input.
* Inverse mass = 0 as the single pinning mechanism.
* Sleep-when-settled, publish-nothing-when-nothing-moved.
* No per-frame filters/blur; precomputed soft shadows; cached rasterisation per size.
* An immutable snapshot type as the only thing crossing physics → render.
* Pure geometry functions for screen placement so they can be tested without a display.
* Tolerant, versioned, single-document settings with exactly one write path.
* A `Production` build that compiles out developer surfaces, and a distribution doc with real
  measurements.
* Publishing measured idle cost and a candid "known limitations" list.
* "Hide the overlay ⇒ tear the window down" so an unused feature costs nothing.
* A frame-rate-independence test (two 120 Hz frames equal one 60 Hz frame).

## 4. Our own identity (must be original, and deliberately unlike Hangly)

| | Hanglock |
|---|---|
| Category | Productivity utility: clock, timer, stopwatch |
| Payload | A legible information card on a cord (later: a dual-cord plaque) |
| Name / wordmark | "Hanglock" — lock (the fastening) + clock. Ours |
| Icon | To be drawn for us; tracked in `assets/SOURCES.md` with provenance |
| Palette / typography | Our own; see `../architecture.md` §7 (visual identity) |
| Tone | Instrument-panel: precise, quiet, technical. Not temple/charm/folk ornament |
| Licence (proposed) | MIT for code; fonts under their upstream OFL, bundled unmodified; our icon and wordmark released under CC0/MIT so forks carry no legal ambiguity — the opposite of Hangly's reserved-artwork stance, which suits a utility people are meant to extend |

## 5. Enforcement

Cheap, mechanical, reviewable:

* `.github/CI` check: `deny.toml`-style scan rejecting `Hangly`, `charm`, `nazar`, `daruma`,
  `maneki`, `hamsa`, `himmeli`, `scarab`, `sharancreatedthis` in `assets/`, plus an allowlist check
  that every file under `assets/` has a provenance line in `assets/SOURCES.md`.
* `docs/CONTRIBUTING.md` opens with this file and the one-line rule: *"No artwork, no names, no
  prose, no code lifted from the reference project. Ideas and documented trade-offs are the
  import."*
* PR template checkbox: "This PR adds no third-party artwork and no code translated from another
  project (or lists what it does, with notice)."
