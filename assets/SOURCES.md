# Asset provenance

Every file in `assets/` must have a line here, and CI rejects an unlisted file. The rule this exists to
enforce: no third-party artwork, no ports of the reference project's art, no ambiguous fonts.

| File | Origin | Licence |
|---|---|---|
| — (none yet) | The v0.1 icon is generated at runtime by `crates/hanglock-win/src/icon.rs` from a distance-field description in code, so there is no binary asset to licence. A designed `.ico` for the installer arrives in Phase 2 and needs its own row here. | — |
| `docs/previews/*.png` | Rendered by `tools/model/hanglock_ref.py` — the reference model, which shares its constants and its painter logic with `hanglock-render`. Design-review images, **not** product screenshots; `docs/reports/phase-1.md` says so in full. | CC0 |

## The face

`crates/hanglock-render/src/face_data.rs` is generated data: geometric stroke outlines for
`0 1 2 3 4 5 6 7 8 9 : . A M P S T -` and space, authored in `tools/model/hanglock_ref.py`
(`FACE_GLYPHS`) and quantised by `tools/model/gen_face.py`. It is original geometry — no font file is
bundled, referenced, traced or derived, so no font licence is needed, because there is no font family.

If a real `.ttf` is ever bundled (Phase 4's proportional text, or wider Latin coverage), it must arrive
with its upstream licence file in `assets/fonts/`, a row in the table above, and a note on whether the
generated `face_data.rs` still exists for the headless paths.
