# Asset provenance

Every file in `assets/` must have a line here, and CI rejects an unlisted file. The rule this exists to
enforce: no third-party artwork, no ports of the reference project's art, no ambiguous fonts.

| File | Origin | Licence |
|---|---|---|
| — (none yet) | The v0.1 icon is generated at runtime by `crates/hanglock-win/src/icon.rs` from a distance-field description in code, so there is no binary asset to licence. A designed `.ico` for the installer arrives in Phase 2 and needs its own row here. | — |

`docs/previews/*.png` are design-review images rather than product assets, and every one of them is
output of `tools/model/hanglock_ref.py` — the reference model, which shares its constants and its
painter logic with `hanglock-render`. They are listed file by file because that is what the check in
`.github/workflows/ci.yml` asks for: it looks for each filename, so a glob here would satisfy nobody.

| File | Origin | Licence |
|---|---|---|
| `docs/previews/01-settled-light.png` | `hanglock_ref.py` render at rest on a light desktop: clamp, ring, cord, plate, `10:42` `PM`. Not a screenshot of the app — it is what the shipped painter draws. | CC0 |
| `docs/previews/02-mid-swing.png` | `hanglock_ref.py` render mid-throw: the cord whips while the plate stays near level (`plate` posture). | CC0 |
| `docs/previews/03-settled-dark.png` | `hanglock_ref.py` render, same frame on a dark wallpaper: the rim is what keeps it an object instead of a hole. | CC0 |
| `docs/previews/04-seconds.png` | `hanglock_ref.py` render of `10:42:07`, the widest run the MVP face draws. | CC0 |
| `docs/previews/05-long-hang-natural.png` | `hanglock_ref.py` render at a 230 px hang in `natural` posture: livelier, still bounded. | CC0 |
| `docs/previews/06-held-locked.png` | `hanglock_ref.py` render held under the cursor in `locked` posture: level, for reduced-motion users. | CC0 |


## The face

`crates/hanglock-render/src/face_data.rs` is generated data: geometric stroke outlines for
`0 1 2 3 4 5 6 7 8 9 : . A M P S T -` and space, authored in `tools/model/hanglock_ref.py`
(`FACE_GLYPHS`) and quantised by `tools/model/gen_face.py`. It is original geometry — no font file is
bundled, referenced, traced or derived, so no font licence is needed, because there is no font family.

If a real `.ttf` is ever bundled (Phase 4's proportional text, or wider Latin coverage), it must arrive
with its upstream licence file in `assets/fonts/`, a row in the table above, and a note on whether the
generated `face_data.rs` still exists for the headless paths.
